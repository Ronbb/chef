//! Real operator authorization and editorial decisions against a disposable schema.
use axum::{Router, body::Body, http::Request};
use chef_engine::{
    csrf::CsrfPolicy,
    identity::{self, Backend},
};
use http_body_util::BodyExt;
use sea_orm::{ConnectOptions, ConnectionTrait, Database, DbBackend, Statement, TransactionTrait};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use tower::ServiceExt;
#[path = "support/assets.rs"]
mod assets;
struct Browser {
    app: Router,
    cookie: String,
    csrf: String,
}
#[derive(Default)]
struct MockQwen {
    calls: std::sync::Mutex<Vec<Value>>,
    status: std::sync::Mutex<String>,
    unknown: std::sync::atomic::AtomicBool,
}
#[async_trait::async_trait]
impl chef_engine::qwen::Transport for MockQwen {
    async fn create(
        &self,
        prefix: &str,
        url: &str,
    ) -> Result<chef_engine::qwen::Receipt, chef_engine::qwen::ProviderError> {
        self.calls
            .lock()
            .unwrap()
            .push(json!({"create":prefix,"url":url}));
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if self.unknown.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(chef_engine::qwen::ProviderError::Unknown);
        }
        Ok(chef_engine::qwen::Receipt {
            voice_id: format!("{}-{prefix}-test", chef_engine::qwen::MODEL),
            request_id: "create-test".into(),
        })
    }
    async fn query(
        &self,
        voice: &str,
    ) -> Result<chef_engine::qwen::Details, chef_engine::qwen::ProviderError> {
        self.calls.lock().unwrap().push(json!({"query":voice}));
        let status = self.status.lock().unwrap().clone();
        Ok(chef_engine::qwen::Details {
            model: if status == "mismatch" {
                "other-model".into()
            } else {
                chef_engine::qwen::MODEL.into()
            },
            status: if status == "mismatch" {
                "OK".into()
            } else {
                status
            },
            request_id: "query-test".into(),
        })
    }
    async fn synthesize(
        &self,
        request: &chef_engine::qwen::SpeechRequest,
    ) -> Result<chef_engine::qwen::Speech, chef_engine::qwen::ProviderError> {
        self.calls
            .lock()
            .unwrap()
            .push(json!({"synthesis":request.parameters().unwrap()}));
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if self.unknown.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(chef_engine::qwen::ProviderError::Unknown);
        }
        let mut wav = vec![0u8; 4844];
        wav[..4].copy_from_slice(b"RIFF");
        wav[4..8].copy_from_slice(&4836u32.to_le_bytes());
        wav[8..16].copy_from_slice(b"WAVEfmt ");
        wav[16..20].copy_from_slice(&16u32.to_le_bytes());
        wav[20..24].copy_from_slice(&[1, 0, 1, 0]);
        wav[24..28].copy_from_slice(&24000u32.to_le_bytes());
        wav[28..32].copy_from_slice(&48000u32.to_le_bytes());
        wav[32..36].copy_from_slice(&[2, 0, 16, 0]);
        wav[36..40].copy_from_slice(b"data");
        wav[40..44].copy_from_slice(&4800u32.to_le_bytes());
        let info = chef_engine::audio::inspect(&wav, "audio/wav").unwrap();
        Ok(chef_engine::qwen::Speech {
            provider_wav: wav.clone(),
            wav,
            info,
            request_id: "audition-test".into(),
            verification: (request.profile.voice_kind == "cloned").then(|| {
                chef_engine::qwen::Details {
                    model: chef_engine::qwen::MODEL.into(),
                    status: "OK".into(),
                    request_id: "verify-audition".into(),
                }
            }),
            input_tokens: Some(10),
            output_tokens: Some(20),
        })
    }
}
async fn settled(browser: &mut Browser, path: &str) -> Value {
    for _ in 0..100 {
        let read = browser.send("GET", path, None, true).await;
        assert_eq!(read.0, 200);
        if !["submitted", "checking"].contains(&read.1["status"].as_str().unwrap()) {
            return read.1;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("owned provider test worker did not settle");
}

#[tokio::test]
#[ignore = "set TEST_DATABASE_URL to a dedicated PostgreSQL database"]
async fn voice_reference_delivery_is_bounded_revocable_private_and_audited() {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let admin = Database::connect(&url).await.unwrap();
    let schema = format!(
        "voice_reference_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    admin
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut options = ConnectOptions::new(url);
    options.set_schema_search_path(&schema);
    let db = Database::connect(options).await.unwrap();
    brioche_migration::Migrator::up(&db, None).await.unwrap();
    let root = assets::fixture_assets(&db, &schema).await;
    let backend = Backend::new(db.clone()).await.unwrap();
    let base_app = identity::router_with_media_root(
        backend.clone(),
        CsrfPolicy::new(["http://localhost:5173".into()]).unwrap(),
        false,
        root.clone(),
    );
    let qwen = std::sync::Arc::new(MockQwen::default());
    *qwen.status.lock().unwrap() = "OK".into();
    let app = base_app.clone().layer(axum::Extension(
        chef_engine::qwen::Service::new(qwen.clone(), "https://example.test").unwrap(),
    ));
    let mut visitor = Browser::new(app.clone()).await;
    let mut operator = Browser::new(app.clone()).await;
    operator
        .register(&backend, "reference-operator@example.test", true)
        .await;
    let mut learner = Browser::new(app.clone()).await;
    learner
        .register(&backend, "reference-learner@example.test", false)
        .await;
    let check_path = "/api/v1/operator/documents/lesson/check";
    let mut check_source = chef_engine::development_source().unwrap();
    check_source["assetRefs"] = assets::fixture_refs();
    check_source["media"] = json!("placeholder replaced from registry");
    check_source["audioRefs"] = json!([]);
    check_source["audio"] = json!("placeholder replaced from registry");
    let check_request = json!({"document":serde_json::to_string(&check_source).unwrap(),"reason":"Read-only isolated preflight"});
    let counts_sql = "SELECT jsonb_build_object('lessons',(SELECT count(*) FROM lesson_revisions),'imports',(SELECT count(*) FROM lesson_import_audit),'releases',(SELECT count(*) FROM content_releases),'entries',(SELECT count(*) FROM release_entries),'assets',(SELECT count(*) FROM media_assets),'audio',(SELECT count(*) FROM audio_assets),'assetAudit',(SELECT count(*) FROM asset_import_audit),'audioAudit',(SELECT count(*) FROM audio_import_audit),'reviews',(SELECT count(*) FROM editorial_reviews),'generation',(SELECT generation FROM content_state WHERE singleton)) AS counts";
    let before_check: Value = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            counts_sql.to_owned(),
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get("", "counts")
        .unwrap();
    assert_eq!(
        visitor
            .send("POST", check_path, Some(check_request.clone()), true)
            .await
            .0,
        401
    );
    assert_eq!(
        learner
            .send("POST", check_path, Some(check_request.clone()), true)
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .send("POST", check_path, Some(check_request.clone()), false)
            .await
            .0,
        403
    );
    let (status, report) = operator
        .send("POST", check_path, Some(check_request), true)
        .await;
    assert_eq!(status, 200);
    assert_eq!(report, json!({"valid":true,"issue":null}));
    for (pointer, value, expected, message) in [
        (
            "/assetRefs/0/revision",
            json!(7991),
            "/assetRefs/0/revision",
            "图片素材未登记",
        ),
        (
            "/audioRefs",
            json!([{"assetId":"missing-recording","revision":7992}]),
            "/audioRefs/0/revision",
            "录音未登记",
        ),
        (
            "/cast/0/revision",
            json!(7993),
            "/cast/0/revision",
            "素材、角色或录音未登记",
        ),
    ] {
        let mut source = check_source.clone();
        *source.pointer_mut(pointer).unwrap() = value;
        let text = serde_json::to_string_pretty(&source)
            .unwrap()
            .replace('\n', "\r\n");
        let (status, report) = operator
            .send(
                "POST",
                check_path,
                Some(json!({"document":text,"reason":"Read-only reference mismatch"})),
                true,
            )
            .await;
        assert_eq!(status, 200);
        assert_eq!(report["valid"], false);
        assert_eq!(report["issue"]["pointer"], expected);
        assert!(
            report["issue"]["messageZh"]
                .as_str()
                .unwrap()
                .contains(message)
        );
        assert!(report["issue"]["line"].as_u64().unwrap() > 1);
        assert!(!report.to_string().contains("missing-recording"));
    }
    let asset: Value = db.query_one_raw(Statement::from_string(DbBackend::Postgres, "SELECT descriptor FROM media_assets WHERE asset_id='art-bakery-morning' AND revision=1".to_owned())).await.unwrap().unwrap().try_get("", "descriptor").unwrap();
    let media_file = root.join(format!("{}.svg", asset["sha256"].as_str().unwrap()));
    let original_media = std::fs::read(&media_file).unwrap();
    std::fs::write(&media_file, "broken private test bytes").unwrap();
    let (status, report) = operator.send("POST", check_path, Some(json!({"document":serde_json::to_string(&check_source).unwrap(),"reason":"Read-only corrupt object"})), true).await;
    std::fs::write(&media_file, original_media).unwrap();
    assert_eq!(status, 200);
    assert_eq!(report["valid"], false);
    assert_eq!(report["issue"]["pointer"], "/assetRefs/0");
    assert!(
        report["issue"]["messageZh"]
            .as_str()
            .unwrap()
            .contains("素材文件缺失、损坏")
    );
    assert!(
        !report
            .to_string()
            .contains(asset["sha256"].as_str().unwrap())
    );
    let after_check: Value = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            counts_sql.to_owned(),
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get("", "counts")
        .unwrap();
    assert_eq!(before_check, after_check);
    // Real blocked registry reads prove cancelling HTTP work cannot bypass the
    // two-job admission limit. Only this disposable schema is locked.
    let lock = db.begin().await.unwrap();
    lock.execute_unprepared("LOCK TABLE media_assets IN ACCESS EXCLUSIVE MODE")
        .await
        .unwrap();
    let request_body = json!({"document":serde_json::to_string(&check_source).unwrap(),"reason":"Read-only cancellation admission"});
    let mut waiting = Vec::new();
    for _ in 0..2 {
        let app = app.clone();
        let request = Request::builder()
            .method("POST")
            .uri(check_path)
            .header("cookie", &operator.cookie)
            .header("origin", "http://localhost:5173")
            .header("x-csrf-token", &operator.csrf)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&request_body).unwrap()))
            .unwrap();
        waiting.push(tokio::spawn(async move { app.oneshot(request).await }));
    }
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let n = db.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,
                "SELECT count(*) AS n FROM pg_locks l JOIN pg_class c ON c.oid=l.relation JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relname='media_assets' AND NOT l.granted", [schema.clone().into()]))
                .await.unwrap().unwrap().try_get::<i64>("", "n").unwrap();
            if n == 2 { break; }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }).await.unwrap();
    assert_eq!(
        operator
            .send("POST", check_path, Some(request_body.clone()), true)
            .await
            .0,
        429
    );
    for request in waiting {
        request.abort();
        assert!(request.await.unwrap_err().is_cancelled());
    }
    assert_eq!(
        operator
            .send("POST", check_path, Some(request_body.clone()), true)
            .await
            .0,
        429
    );
    lock.rollback().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let (status, report) = operator
                .send("POST", check_path, Some(request_body.clone()), true)
                .await;
            if status == 200 {
                assert_eq!(report["valid"], true);
                break;
            }
            assert_eq!(status, 429);
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    let (status, report) = operator
        .send(
            "POST",
            check_path,
            Some(json!({"document":"{bad-private-marker}","reason":"Invalid isolated preflight"})),
            true,
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(report["valid"], false);
    assert!(!report.to_string().contains("private-marker"));
    // Synthetic editorial statuses below exist only in this disposable schema;
    // the check itself never approves, imports, stages or activates content.
    for (id, status) in [
        ("check-reviewed", "reviewed"),
        ("check-draft", "draft"),
        ("check-withdrawn", "reviewed"),
    ] {
        let mut source = check_source.clone();
        source["id"] = json!(id);
        source["editorial"]["status"] = json!(status);
        chef_engine::author_import::import(
            &db,
            source,
            "isolated-test",
            "Synthetic release check fixture",
        )
        .await
        .unwrap();
    }
    let manifest = json!({"id":"check-existing-release","schemaVersion":"1.0","levels":[{
    "id":check_source["levelId"],"label":"A1 入门","units":[{
        "id":check_source["unitId"],"titleZh":"测试单元","lessons":[{"lessonId":"check-reviewed","revision":1}]
    }]}]});
    let typed = serde_json::from_value(manifest.clone()).unwrap();
    chef_engine::content::stage(
        &db,
        &typed,
        "isolated-test",
        "Synthetic release check fixture",
        &root,
    )
    .await
    .unwrap();
    chef_engine::content::withdraw(
        &db,
        "check-withdrawn",
        1,
        0,
        "isolated-test",
        "Synthetic withdrawal fixture",
    )
    .await
    .unwrap();
    let mut valid_manifest = manifest.clone();
    valid_manifest["id"] = json!("check-new-release");
    let release_check_path = "/api/v1/operator/documents/release/check";
    let mut cases = vec![
        (valid_manifest.clone(), None),
        (manifest, Some(("/id", "编号已存在"))),
    ];
    for (pointer, value, expected, message) in [
        (
            "/levels/0/units/0/lessons/0/revision",
            json!(7999),
            "/levels/0/units/0/lessons/0/revision",
            "尚未导入",
        ),
        (
            "/levels/0/units/0/lessons/0/lessonId",
            json!("check-withdrawn"),
            "/levels/0/units/0/lessons/0/revision",
            "已撤回",
        ),
        (
            "/levels/0/units/0/lessons/0/lessonId",
            json!("check-draft"),
            "/levels/0/units/0/lessons/0",
            "未满足发布条件",
        ),
        (
            "/levels/0/units/0/id",
            json!("other-unit"),
            "/levels/0/units/0/lessons/0",
            "未满足发布条件",
        ),
    ] {
        let mut manifest = valid_manifest.clone();
        *manifest.pointer_mut(pointer).unwrap() = value;
        cases.push((manifest, Some((expected, message))));
    }
    let release_counts_sql = "SELECT jsonb_build_object('business',(".to_owned()
        + counts_sql
            .trim_end_matches(" AS counts")
            .trim_start_matches("SELECT ")
        + "),'audit',(SELECT count(*) FROM content_audit),'withdrawals',(SELECT count(*) FROM content_withdrawals)) AS counts";
    let before_release: Value = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            release_counts_sql.clone(),
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get("", "counts")
        .unwrap();
    for (manifest, expected) in cases {
        let text = serde_json::to_string_pretty(&manifest)
            .unwrap()
            .replace('\n', "\r\n");
        let request = json!({"document":text,"reason":"Read-only release preflight"});
        assert_eq!(
            visitor
                .send("POST", release_check_path, Some(request.clone()), true)
                .await
                .0,
            401
        );
        assert_eq!(
            learner
                .send("POST", release_check_path, Some(request.clone()), true)
                .await
                .0,
            403
        );
        let (status, report) = operator
            .send("POST", release_check_path, Some(request), true)
            .await;
        assert_eq!(status, 200);
        if let Some((pointer, message)) = expected {
            assert_eq!(report["valid"], false);
            assert_eq!(report["issue"]["pointer"], pointer);
            assert!(
                report["issue"]["messageZh"]
                    .as_str()
                    .unwrap()
                    .contains(message)
            );
            assert!(report["issue"]["line"].as_u64().unwrap() > 1);
            assert!(!report.to_string().contains("check-reviewed"));
        } else {
            assert_eq!(report, json!({"valid":true,"issue":null}));
        }
    }
    let original_media = std::fs::read(&media_file).unwrap();
    std::fs::write(&media_file, b"corrupt release check fixture").unwrap();
    let (status, report) = operator.send("POST", release_check_path, Some(json!({"document":serde_json::to_string(&valid_manifest).unwrap(),"reason":"Read-only release media failure"})), true).await;
    std::fs::write(&media_file, original_media).unwrap();
    assert_eq!(status, 200);
    assert_eq!(report["valid"], false);
    assert_eq!(report["issue"]["pointer"], "/levels/0/units/0/lessons/0");
    assert!(
        !report
            .to_string()
            .contains(asset["sha256"].as_str().unwrap())
    );
    assert!(!report.to_string().contains("corrupt release"));
    let after_release: Value = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            release_counts_sql,
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get("", "counts")
        .unwrap();
    assert_eq!(before_release, after_release);
    // Five seconds of synthetic PCM solely for protocol validation, no real speaker/consent claim.
    let mut wav = vec![0u8; 160044];
    wav[..4].copy_from_slice(b"RIFF");
    wav[4..8].copy_from_slice(&(160036u32).to_le_bytes());
    wav[8..12].copy_from_slice(b"WAVE");
    wav[12..16].copy_from_slice(b"fmt ");
    wav[16..20].copy_from_slice(&16u32.to_le_bytes());
    wav[20..22].copy_from_slice(&1u16.to_le_bytes());
    wav[22..24].copy_from_slice(&1u16.to_le_bytes());
    wav[24..28].copy_from_slice(&16000u32.to_le_bytes());
    wav[28..32].copy_from_slice(&32000u32.to_le_bytes());
    wav[32..34].copy_from_slice(&2u16.to_le_bytes());
    wav[34..36].copy_from_slice(&16u16.to_le_bytes());
    wav[36..40].copy_from_slice(b"data");
    wav[40..44].copy_from_slice(&160000u32.to_le_bytes());
    let upload = json!({"assetId":"qa-reference-delivery","revision":1,"mimeType":"audio/wav","creditZh":"合成协议测试","source":"test:synthetic","license":"LicenseRef-TestOnly","creator":"test fixture","rightsConfirmed":true,"reason":"isolated reference file"});
    assert_eq!(
        operator
            .upload_media("/api/v1/operator/recordings", upload.clone(), &wav, true)
            .await
            .0,
        200
    );
    let seed: Value =
        serde_json::from_str(include_str!("../../../docs/characters/voices.json")).unwrap();
    let mut profile = seed["items"][0]["profile"].clone();
    profile["referenceAudio"] = json!({"assetId":"qa-reference-delivery","revision":1,"transcript":"Synthetic five second fixture","cloningPermission":"No real person; protocol test only"});
    let voice = json!({"characterId":"character-camille","characterRevision":1,"expectedVoiceRevision":0,"profile":profile,"reason":"isolated voice reference"});
    assert_eq!(
        operator
            .send("POST", "/api/v1/operator/characters", Some(voice), true)
            .await
            .0,
        200
    );
    let path = "/api/v1/operator/voice-references";
    let request = json!({"characterId":"character-camille","characterRevision":1,"voiceRevision":1,"singleSpeakerConfirmed":true,"reason":"isolated authorized delivery"});
    assert_eq!(visitor.send("GET", path, None, true).await.0, 401);
    assert_eq!(learner.send("GET", path, None, true).await.0, 403);
    assert_eq!(
        learner
            .send("POST", path, Some(request.clone()), true)
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .send("POST", path, Some(request.clone()), false)
            .await
            .0,
        403
    );
    let mut unconfirmed = request.clone();
    unconfirmed["singleSpeakerConfirmed"] = json!(false);
    assert_eq!(
        operator.send("POST", path, Some(unconfirmed), true).await.0,
        400
    );
    let mut missing = request.clone();
    missing["voiceRevision"] = json!(99);
    assert_eq!(
        operator.send("POST", path, Some(missing), true).await.0,
        404
    );
    let (status, result) = operator
        .send("POST", path, Some(request.clone()), true)
        .await;
    assert_eq!(status, 200);
    let grant_id = result["grant"]["id"].as_str().unwrap();
    let bearer = result["path"].as_str().unwrap();
    let token = bearer.rsplit('/').next().unwrap();
    assert_eq!(token.len(), 64);
    assert_eq!(
        operator
            .send("POST", path, Some(request.clone()), true)
            .await
            .0,
        409
    );
    let list = operator.send("GET", path, None, true).await;
    assert_eq!(list.1["items"][0]["assetRevision"], 1);
    assert!(!list.1.to_string().contains(token));
    assert!(!list.1.to_string().contains("tokenHash"));
    assert!(!list.1.to_string().contains("cloningPermission"));
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(bearer)
                .header("range", "bytes=0-9")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 206);
    assert_eq!(response.headers()["cache-control"], "private, no-store");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert_eq!(
        response.into_body().collect().await.unwrap().to_bytes(),
        &wav[..10]
    );
    let wrong = format!("/api/v1/voice-references/{grant_id}/{}", "0".repeat(64));
    assert_eq!(visitor.send("GET", &wrong, None, false).await.0, 404);
    assert_eq!(
        operator.send("GET", path, None, true).await.1["items"][0]["readCount"],
        1
    );
    let expired_id = "e".repeat(32);
    let expired_token = "e".repeat(64);
    use sha2::Digest;
    let expired_hash = format!("{:x}", sha2::Sha256::digest(expired_token.as_bytes()));
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO voice_reference_grants SELECT $1,$2,character_id,character_revision,voice_revision,asset_id,asset_revision,descriptor,reference,actor_id,reason,model,single_speaker_confirmed,CURRENT_TIMESTAMP-interval '30 minutes',CURRENT_TIMESTAMP-interval '20 minutes' FROM voice_reference_grants WHERE id=$3",vec![expired_id.clone().into(),expired_hash.into(),grant_id.into()])).await.unwrap();
    assert_eq!(
        visitor
            .send(
                "GET",
                &format!("/api/v1/voice-references/{expired_id}/{expired_token}"),
                None,
                false
            )
            .await
            .0,
        404
    );
    let stored = db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT token_hash,reference,actor_id FROM voice_reference_grants WHERE id=$1",
            vec![grant_id.into()],
        ))
        .await
        .unwrap()
        .unwrap();
    assert_ne!(stored.try_get::<String>("", "token_hash").unwrap(), token);
    assert_eq!(
        stored.try_get::<Value>("", "reference").unwrap()["cloningPermission"],
        profile["referenceAudio"]["cloningPermission"]
    );
    let revoke = format!("{path}/{grant_id}/revoke");
    assert_eq!(
        learner
            .send("POST", &revoke, Some(json!({"reason":"test"})), true)
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .send("POST", &revoke, Some(json!({"reason":"test"})), false)
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .send("POST", &revoke, Some(json!({"reason":"test"})), true)
            .await
            .0,
        200
    );
    assert_eq!(visitor.send("GET", bearer, None, false).await.0, 404);
    assert_eq!(
        operator
            .send("POST", &revoke, Some(json!({"reason":"test"})), true)
            .await
            .0,
        409
    );
    assert!(
        brioche_migration::Migrator::migrations()
            .into_iter()
            .find(|m| m.name() == "m20261007_000018_voice_reference_grants")
            .unwrap()
            .down(&sea_orm_migration::SchemaManager::new(&db))
            .await
            .is_err()
    );
    // Account lock serializes simultaneous issuance: one credential, one explicit conflict.
    let parallel_cookie = operator.cookie.clone();
    let parallel_csrf = operator.csrf.clone();
    let (a, b) = tokio::join!(
        operator.send("POST", path, Some(request.clone()), true),
        learner.app.clone().oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("cookie", &parallel_cookie)
                .header("origin", "http://localhost:5173")
                .header("x-csrf-token", &parallel_csrf)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&request).unwrap()))
                .unwrap()
        )
    );
    let response = b.unwrap();
    let b_status = response.status().as_u16();
    let b_result: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let mut statuses = vec![a.0, b_status];
    statuses.sort();
    assert_eq!(statuses, vec![200, 409]);
    let next = if a.0 == 200 { a.1 } else { b_result };
    let next_id = next["grant"]["id"].as_str().unwrap();
    let next_bearer = next["path"].as_str().unwrap();
    // Exhaustion is atomic and applies equally to HEAD/Range retries.
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO voice_reference_reads(grant_id) SELECT $1 FROM generate_series(1,32)",
        vec![next_id.into()],
    ))
    .await
    .unwrap();
    assert_eq!(visitor.send("GET", next_bearer, None, false).await.0, 404);
    let third_revoke = format!("{path}/{next_id}/revoke");
    assert_eq!(
        operator
            .send(
                "POST",
                &third_revoke,
                Some(json!({"reason":"after exhaustion"})),
                true
            )
            .await
            .0,
        200
    );
    let third = operator
        .send("POST", path, Some(request.clone()), true)
        .await;
    assert_eq!(third.0, 200);
    let third_path = third.1["path"].as_str().unwrap();
    let actor = stored.try_get::<i64>("", "actor_id").unwrap();
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "UPDATE product_memberships SET role='learner',version=version+1 WHERE product_id='brioche' AND user_id=$1",
        vec![actor.into()],
    ))
    .await
    .unwrap();
    assert_eq!(visitor.send("GET", third_path, None, false).await.0, 404);
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "UPDATE product_memberships SET role='operator',version=version+1 WHERE product_id='brioche' AND user_id=$1",
        vec![actor.into()],
    ))
    .await
    .unwrap();
    let sha = format!("{:x}", sha2::Sha256::digest(&wav));
    let file = root.join(format!("{sha}.wav"));
    assert!(file.exists());
    std::fs::write(&file, b"corrupt").unwrap();
    assert_eq!(visitor.send("GET", third_path, None, false).await.0, 400);
    std::fs::write(&file, &wav).unwrap();
    // Registered audio is not enough: the provider's actual minimum duration is rechecked.
    let mut short = wav[..32044].to_vec();
    short[4..8].copy_from_slice(&32036u32.to_le_bytes());
    short[40..44].copy_from_slice(&32000u32.to_le_bytes());
    let mut short_upload = upload;
    short_upload["revision"] = json!(2);
    assert_eq!(
        operator
            .upload_media("/api/v1/operator/recordings", short_upload, &short, true)
            .await
            .0,
        200
    );
    profile["referenceAudio"]["revision"] = json!(2);
    let short_voice = json!({"characterId":"character-camille","characterRevision":1,"expectedVoiceRevision":1,"profile":profile,"reason":"short reference test"});
    assert_eq!(
        operator
            .send(
                "POST",
                "/api/v1/operator/characters",
                Some(short_voice),
                true
            )
            .await
            .0,
        200
    );
    let mut short_request = request;
    short_request["voiceRevision"] = json!(2);
    assert_eq!(
        operator
            .send("POST", path, Some(short_request), true)
            .await
            .0,
        400
    );
    for i in 0..25u32 {
        db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO voice_reference_grants SELECT $1,$2,character_id,character_revision,voice_revision,asset_id,asset_revision,descriptor,reference,actor_id,reason,model,single_speaker_confirmed,CURRENT_TIMESTAMP-interval '30 minutes',CURRENT_TIMESTAMP-interval '20 minutes' FROM voice_reference_grants WHERE id=$3",vec![format!("{i:032x}").into(),format!("{i:064x}").into(),grant_id.into()])).await.unwrap();
    }
    let first = operator.send("GET", path, None, true).await.1;
    assert_eq!(first["items"].as_array().unwrap().len(), 20);
    let cursor = first["next"].as_str().unwrap();
    let second = operator
        .send("GET", &format!("{path}?afterId={cursor}"), None, true)
        .await
        .1;
    assert_eq!(second["items"].as_array().unwrap().len(), 9);
    let ids = first["items"]
        .as_array()
        .unwrap()
        .iter()
        .chain(second["items"].as_array().unwrap())
        .map(|g| g["id"].as_str().unwrap())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(ids.len(), 29);
    // Audit tables cannot be edited, including expiry, token and consent.
    assert!(
        db.execute_unprepared("UPDATE voice_reference_grants SET reason='overwrite'")
            .await
            .is_err()
    );
    assert!(
        db.execute_unprepared("DELETE FROM voice_reference_revocations")
            .await
            .is_err()
    );
    assert!(
        db.execute_unprepared("DELETE FROM voice_reference_reads")
            .await
            .is_err()
    );
    let jobs = "/api/v1/operator/voice-jobs";
    let create = json!({"grantId":third.1["grant"]["id"],"token":third_path.rsplit('/').next().unwrap(),"costConfirmed":true,"reason":"isolated enrollment"});
    assert_eq!(visitor.send("GET", jobs, None, true).await.0, 401);
    assert_eq!(learner.send("GET", jobs, None, true).await.0, 403);
    assert_eq!(
        operator
            .send("POST", jobs, Some(create.clone()), false)
            .await
            .0,
        403
    );
    assert_eq!(
        learner
            .send("POST", jobs, Some(create.clone()), true)
            .await
            .0,
        403
    );
    let mut disabled = Browser {
        app: base_app,
        cookie: operator.cookie.clone(),
        csrf: operator.csrf.clone(),
    };
    assert_eq!(
        disabled.send("GET", jobs, None, true).await.1["configured"],
        false
    );
    assert_eq!(
        disabled
            .send("POST", jobs, Some(create.clone()), true)
            .await
            .0,
        503
    );
    let mut unconfirmed = create.clone();
    unconfirmed["costConfirmed"] = json!(false);
    assert_eq!(
        operator.send("POST", jobs, Some(unconfirmed), true).await.0,
        400
    );
    let mut wrong = create.clone();
    wrong["token"] = json!("0".repeat(64));
    assert_eq!(operator.send("POST", jobs, Some(wrong), true).await.0, 404);
    let created = operator
        .send("POST", jobs, Some(create.clone()), true)
        .await;
    assert_eq!(created.0, 200);
    assert_eq!(created.1["status"], "submitted");
    let job_path = format!("{jobs}/{}", created.1["id"].as_str().unwrap());
    let mut job = settled(&mut operator, &job_path).await;
    assert_eq!(job["status"], "processing");
    assert_eq!(
        operator
            .send("POST", jobs, Some(create.clone()), true)
            .await
            .0,
        409
    );
    assert_eq!(qwen.calls.lock().unwrap().len(), 1);
    let calls = qwen.calls.lock().unwrap().clone();
    assert_eq!(calls[0]["url"], format!("https://example.test{third_path}"));
    for status in ["DEPLOYING", "mismatch", "UNDEPLOYED", "OK"] {
        *qwen.status.lock().unwrap() = status.into();
        let request = json!({"expectedVersion":job["version"],"voiceId":null,"reason":"query fixed enrollment"});
        let check_path = format!("{job_path}/check");
        assert_eq!(
            operator
                .send("POST", &check_path, Some(request.clone()), true)
                .await
                .0,
            200
        );
        job = settled(&mut operator, &job_path).await;
        assert_eq!(
            job["status"],
            match status {
                "DEPLOYING" => "processing",
                "mismatch" => "modelMismatch",
                "UNDEPLOYED" => "unavailable",
                _ => "ready",
            }
        );
        assert_eq!(
            operator
                .send("POST", &check_path, Some(request), true)
                .await
                .0,
            409
        );
    }
    let listed = operator.send("GET", jobs, None, true).await;
    assert_eq!(listed.1["configured"], true);
    assert!(!listed.1.to_string().contains(third_path));
    assert!(
        !listed
            .1
            .to_string()
            .contains(create["token"].as_str().unwrap())
    );
    assert!(!listed.1.to_string().contains("resource_link"));
    assert_eq!(visitor.send("GET", &job_path, None, true).await.0, 401);
    assert_eq!(learner.send("GET", &job_path, None, true).await.0, 403);
    // Ambiguous creation persists without a retry. Explicit recovery only queries the job's unique prefix.
    let recovery_id = "a".repeat(32);
    let recovery_token = "f".repeat(64);
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO voice_reference_grants SELECT $1,$2,character_id,character_revision,voice_revision,asset_id,asset_revision,descriptor,reference,actor_id,reason,model,single_speaker_confirmed,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP+interval '15 minutes' FROM voice_reference_grants WHERE id=$3",vec![recovery_id.clone().into(),format!("{:x}",sha2::Sha256::digest(recovery_token.as_bytes())).into(),third.1["grant"]["id"].as_str().unwrap().into()])).await.unwrap();
    qwen.unknown
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let lost=operator.send("POST",jobs,Some(json!({"grantId":recovery_id,"token":recovery_token,"costConfirmed":true,"reason":"unknown test"})),true).await;
    assert_eq!(lost.0, 200);
    let lost_path = format!("{jobs}/{}", lost.1["id"].as_str().unwrap());
    let lost = settled(&mut operator, &lost_path).await;
    assert_eq!(lost["status"], "unknown");
    let calls_before = qwen.calls.lock().unwrap().len();
    assert_eq!(operator.send("POST",&format!("{lost_path}/check"),Some(json!({"expectedVersion":lost["version"],"voiceId":"wrong-prefix","reason":"recover"})),true).await.0,400);
    let recovery_voice = format!(
        "{}-{}-found",
        chef_engine::qwen::MODEL,
        lost["prefix"].as_str().unwrap()
    );
    assert_eq!(operator.send("POST",&format!("{lost_path}/check"),Some(json!({"expectedVersion":lost["version"],"voiceId":recovery_voice,"reason":"recover"})),true).await.0,200);
    assert_eq!(settled(&mut operator, &lost_path).await["status"], "ready");
    assert_eq!(qwen.calls.lock().unwrap().len(), calls_before + 1);
    assert!(
        qwen.calls
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .get("query")
            .is_some()
    );
    assert!(
        db.execute_unprepared("UPDATE voice_clone_jobs SET reason='overwrite'")
            .await
            .is_err()
    );
    let history = operator
        .send("GET", "/api/v1/operator/history", None, true)
        .await
        .1;
    assert!(history.to_string().contains("voiceJobCreated"));
    assert!(history.to_string().contains("voiceJobCheck"));
    assert!(!history.to_string().contains(third_path));
    // Auditions use client attempt IDs, never replay a paid call, and reviews atomically append voices.
    // Earlier reference-length checks intentionally appended Camille v2. Use a separate fixed role
    // for successful acceptance; applying an older candidate over a newer profile must remain rejected.
    let ref_profile = operator
        .send(
            "GET",
            "/api/v1/operator/characters/character-camille/1/voices/1",
            None,
            true,
        )
        .await
        .1["profile"]
        .clone();
    assert_eq!(operator.send("POST","/api/v1/operator/characters",Some(json!({"characterId":"character-luc","characterRevision":1,"expectedVoiceRevision":0,"profile":ref_profile,"reason":"isolated audition source"})),true).await.0,200);
    let source_id = "f".repeat(32);
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO voice_reference_grants SELECT $1,$2,'character-luc',1,1,asset_id,asset_revision,descriptor,reference,actor_id,'isolated audition source',model,single_speaker_confirmed,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP+interval '15 minutes' FROM voice_reference_grants WHERE id=$3",vec![source_id.clone().into(),"9".repeat(64).into(),third.1["grant"]["id"].as_str().unwrap().into()])).await.unwrap();
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO voice_clone_jobs(id,grant_id,prefix,actor_id,reason) VALUES($1,$1,'auditionqa',$2,'isolated ready source')",vec![source_id.clone().into(),actor.into()])).await.unwrap();
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO voice_clone_events(job_id,version,status,voice_id,reason) VALUES($1,1,'ready',$2,'isolated ready source')",vec![source_id.clone().into(),format!("{}-auditionqa-test",chef_engine::qwen::MODEL).into()])).await.unwrap();
    let job = operator
        .send("GET", &format!("{jobs}/{source_id}"), None, true)
        .await
        .1;
    qwen.unknown
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let audition_api = "/api/v1/operator/voice-auditions";
    let audition_id = "b".repeat(32);
    let audition_path = format!("{audition_api}/{audition_id}");
    let audition_request = json!({"id":audition_id,"cloneJobId":job["id"],"expectedCloneVersion":job["version"],"text":"Bonjour !","emotion":"Warm greeting.","costConfirmed":true,"reason":"isolated audition"});
    assert_eq!(
        visitor
            .send("POST", audition_api, Some(audition_request.clone()), true)
            .await
            .0,
        401
    );
    assert_eq!(
        learner
            .send("POST", audition_api, Some(audition_request.clone()), true)
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .send("POST", audition_api, Some(audition_request.clone()), false)
            .await
            .0,
        403
    );
    let mut bad = audition_request.clone();
    bad["costConfirmed"] = json!(false);
    assert_eq!(
        operator.send("POST", audition_api, Some(bad), true).await.0,
        400
    );
    let mut bad = audition_request.clone();
    bad["expectedCloneVersion"] = json!(99999);
    assert_eq!(
        operator.send("POST", audition_api, Some(bad), true).await.0,
        409
    );
    let mut twin = Browser {
        app: operator.app.clone(),
        cookie: operator.cookie.clone(),
        csrf: operator.csrf.clone(),
    };
    let (first, second) = tokio::join!(
        operator.send("POST", audition_api, Some(audition_request.clone()), true),
        twin.send("POST", audition_api, Some(audition_request.clone()), true)
    );
    assert_eq!(first.0, 200);
    assert_eq!(second.0, 200);
    assert_eq!(first.1["id"], second.1["id"]);
    let audition = settled(&mut operator, &audition_path).await;
    assert_eq!(audition["status"], "ready");
    assert_eq!(audition["durationMs"], 100);
    let synth_count = || {
        qwen.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|v| v.get("synthesis").is_some())
            .count()
    };
    assert_eq!(synth_count(), 1);
    assert_eq!(
        operator
            .send("POST", audition_api, Some(audition_request.clone()), true)
            .await
            .0,
        200
    );
    assert_eq!(synth_count(), 1);
    let mut changed = audition_request.clone();
    changed["emotion"] = json!("Changed request");
    assert_eq!(
        operator
            .send("POST", audition_api, Some(changed), true)
            .await
            .0,
        409
    );
    let file_path = format!("{audition_path}/file");
    assert_eq!(visitor.send("GET", &file_path, None, true).await.0, 401);
    assert_eq!(learner.send("GET", &file_path, None, true).await.0, 403);
    let req = Request::builder()
        .uri(&file_path)
        .header("cookie", &operator.cookie)
        .header("range", "bytes=0-15")
        .body(Body::empty())
        .unwrap();
    let response = operator.app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), 206);
    assert_eq!(response.headers()["cache-control"], "private, no-store");
    assert_eq!(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .len(),
        16
    );
    let review_api = format!("{audition_path}/review");
    let review_request = json!({"accepted":true,"heard":false,"expectedVoiceRevision":1,"reason":"isolated audition acceptance"});
    assert_eq!(
        operator
            .send("POST", &review_api, Some(review_request.clone()), true)
            .await
            .0,
        400
    );
    let mut accepted = review_request.clone();
    accepted["heard"] = json!(true);
    let reviewed = operator
        .send("POST", &review_api, Some(accepted.clone()), true)
        .await;
    assert_eq!(reviewed.0, 200);
    assert_eq!(reviewed.1["appliedVoiceRevision"], 2);
    assert_eq!(reviewed.1["accepted"], true);
    assert_eq!(
        operator
            .send("POST", &review_api, Some(accepted), true)
            .await
            .0,
        409
    );
    let profile = operator
        .send(
            "GET",
            "/api/v1/operator/characters/character-luc/1/voices/2",
            None,
            true,
        )
        .await;
    assert_eq!(profile.0, 200);
    assert_eq!(profile.1["profile"]["voiceId"], job["voiceId"]);
    let mut another = audition_request.clone();
    another["id"] = json!("c".repeat(32));
    assert_eq!(
        operator
            .send("POST", audition_api, Some(another), true)
            .await
            .0,
        200
    );
    let second_path = format!("{audition_api}/{}", "c".repeat(32));
    assert_eq!(
        settled(&mut operator, &second_path).await["status"],
        "ready"
    );
    let rejection = json!({"accepted":false,"heard":true,"expectedVoiceRevision":1,"reason":"isolated audition rejection"});
    assert_eq!(
        operator
            .send(
                "POST",
                &format!("{second_path}/review"),
                Some(rejection),
                true
            )
            .await
            .0,
        200
    );
    let mut third_audition = audition_request.clone();
    third_audition["id"] = json!("d".repeat(32));
    assert_eq!(
        operator
            .send("POST", audition_api, Some(third_audition), true)
            .await
            .0,
        200
    );
    let third_audition_path = format!("{audition_api}/{}", "d".repeat(32));
    assert_eq!(
        settled(&mut operator, &third_audition_path).await["status"],
        "ready"
    );
    let conflict = json!({"accepted":true,"heard":true,"expectedVoiceRevision":1,"reason":"must preserve newer voice"});
    assert_eq!(
        operator
            .send(
                "POST",
                &format!("{third_audition_path}/review"),
                Some(conflict),
                true
            )
            .await
            .0,
        409
    );
    qwen.unknown
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut unknown = audition_request;
    unknown["id"] = json!("e".repeat(32));
    assert_eq!(
        operator
            .send("POST", audition_api, Some(unknown), true)
            .await
            .0,
        200
    );
    let unknown_path = format!("{audition_api}/{}", "e".repeat(32));
    assert_eq!(
        settled(&mut operator, &unknown_path).await["status"],
        "unknown"
    );
    assert_eq!(
        operator
            .send("GET", &format!("{unknown_path}/file"), None, true)
            .await
            .0,
        404
    );
    let private_list = operator.send("GET", audition_api, None, true).await;
    assert_eq!(private_list.0, 200);
    assert_eq!(private_list.1["items"].as_array().unwrap().len(), 4);
    assert!(!private_list.1.to_string().contains("Signature"));
    assert!(!private_list.1.to_string().contains("sha256"));
    assert_eq!(
        disabled.send("GET", audition_api, None, true).await.1["configured"],
        false
    );
    let missing_config = json!({"id":"8".repeat(32),"cloneJobId":job["id"],"expectedCloneVersion":job["version"],"text":"Bonjour !","emotion":"Warm greeting.","costConfirmed":true,"reason":"no configured synthesis"});
    assert_eq!(
        disabled
            .send("POST", audition_api, Some(missing_config), true)
            .await
            .0,
        503
    );
    // Actual media integrity and bounded listings are independent of successful provider receipts.
    let audio_sha_row=db.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT result->>'sha256' AS sha FROM voice_audition_events WHERE audition_id=$1 AND status='ready'",vec![audition_id.clone().into()])).await.unwrap().unwrap();
    let audio_sha: String = audio_sha_row.try_get("", "sha").unwrap();
    let stored = root.join(format!("{audio_sha}.wav"));
    let intact = std::fs::read(&stored).unwrap();
    std::fs::write(&stored, b"damaged").unwrap();
    assert_eq!(operator.send("GET", &file_path, None, true).await.0, 503);
    std::fs::write(&stored, &intact).unwrap();
    let h = operator
        .send("GET", "/api/v1/operator/history", None, true)
        .await
        .1;
    assert!(h.to_string().contains("voiceAuditionAccepted"));
    assert!(h.to_string().contains("voiceAuditionRejected"));
    for i in 0..25u32 {
        let id = format!("{i:032x}");
        db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO voice_auditions(id,clone_job_id,clone_version,profile,parameters,actor_id,reason,created_at) SELECT $1,clone_job_id,clone_version,profile,parameters,actor_id,reason,CURRENT_TIMESTAMP FROM voice_auditions WHERE id=$2",vec![id.clone().into(),audition_id.clone().into()])).await.unwrap();
        db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO voice_audition_events(audition_id,version,status,created_at) VALUES($1,1,'submitted',CURRENT_TIMESTAMP-interval '301 seconds')",vec![id.into()])).await.unwrap();
    }
    let first_auditions = operator.send("GET", audition_api, None, true).await.1;
    assert_eq!(first_auditions["items"].as_array().unwrap().len(), 20);
    assert_eq!(first_auditions["items"][0]["status"], "unknown");
    let last_auditions = operator
        .send(
            "GET",
            &format!(
                "{audition_api}?afterId={}",
                first_auditions["next"].as_str().unwrap()
            ),
            None,
            true,
        )
        .await
        .1;
    assert_eq!(last_auditions["items"].as_array().unwrap().len(), 9);
    assert_eq!(
        operator
            .send(
                "GET",
                &format!("{audition_api}?cloneJobId={source_id}"),
                None,
                true
            )
            .await
            .1["items"]
            .as_array()
            .unwrap()
            .len(),
        20
    );
    assert_eq!(
        operator
            .send(
                "GET",
                &format!("{audition_api}?afterId=invalid"),
                None,
                true
            )
            .await
            .0,
        400
    );
    assert!(
        db.execute_unprepared("UPDATE voice_auditions SET reason='overwrite'")
            .await
            .is_err()
    );
    assert!(
        db.execute_unprepared("DELETE FROM voice_audition_events")
            .await
            .is_err()
    );
    assert!(
        db.execute_unprepared("DELETE FROM voice_audition_reviews")
            .await
            .is_err()
    );
    // First system profile: independent fixed character source, durable exact paid-request retry.
    qwen.unknown
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let new_character = json!({"characterId":"character-system-qa","expectedRevision":0,"displayName":"System QA","avatarId":"avatar-lea-v1","avatarRevision":1,"reason":"isolated system audition source"});
    assert_eq!(
        operator
            .send(
                "POST",
                "/api/v1/operator/characters/revisions",
                Some(new_character),
                true
            )
            .await
            .0,
        200
    );
    let mut candidate = json!({"characterId":"character-system-qa","characterRevision":1,"expectedVoiceRevision":0,"profile":seed["items"][0]["profile"]});
    candidate["profile"]["rate"] = json!(1.0);
    let mut system = json!({"id":"f".repeat(32),"cloneJobId":null,"expectedCloneVersion":null,"candidate":candidate,"text":"Bonjour ! Je m’appelle Léa. Au revoir !","emotion":"A friendly introduction.","costConfirmed":true,"reason":"isolated system candidate"});
    let calls_before = qwen.calls.lock().unwrap().len();
    for (key, value) in [
        ("costConfirmed", json!(false)),
        ("cloneJobId", job["id"].clone()),
        ("expectedCloneVersion", json!(1)),
    ] {
        let mut invalid = system.clone();
        invalid[key] = value;
        assert_eq!(
            operator
                .send("POST", audition_api, Some(invalid), true)
                .await
                .0,
            400
        );
    }
    for voice in [
        "longanhuan_v3.6",
        "english-only",
        "qwen-audio-3.1-tts-flash-fake",
    ] {
        let mut invalid = system.clone();
        invalid["candidate"]["profile"]["voiceId"] = json!(voice);
        assert_eq!(
            operator
                .send("POST", audition_api, Some(invalid), true)
                .await
                .0,
            400
        );
    }
    let mut stale = system.clone();
    stale["candidate"]["expectedVoiceRevision"] = json!(1);
    assert_eq!(
        operator
            .send("POST", audition_api, Some(stale), true)
            .await
            .0,
        409
    );
    let mut missing = system.clone();
    missing["candidate"]["characterId"] = json!("character-missing");
    assert_eq!(
        operator
            .send("POST", audition_api, Some(missing), true)
            .await
            .0,
        404
    );
    assert_eq!(
        learner
            .send("POST", audition_api, Some(system.clone()), true)
            .await
            .0,
        403
    );
    assert_eq!(
        visitor
            .send("POST", audition_api, Some(system.clone()), true)
            .await
            .0,
        401
    );
    assert_eq!(
        operator
            .send("POST", audition_api, Some(system.clone()), false)
            .await
            .0,
        403
    );
    assert_eq!(qwen.calls.lock().unwrap().len(), calls_before);
    let local_actor = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT id FROM users WHERE email='reference-operator@example.test'",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "id")
        .unwrap();
    let local_request: brioche_course_contract::AdminAuditionRequest =
        serde_json::from_value(system.clone()).unwrap();
    let local = chef_engine::voice_auditions::submit_local(
        backend.clone(),
        local_actor,
        Some(chef_engine::qwen::Service::new(qwen.clone(), "https://example.test").unwrap()),
        root.clone(),
        local_request.clone(),
    )
    .await
    .unwrap();
    assert_eq!(local.status, "ready");
    assert!(
        local.accepted.is_none(),
        "local generation must never declare listening acceptance"
    );
    let retried = chef_engine::voice_auditions::submit_local(
        backend.clone(),
        local_actor,
        None,
        root.clone(),
        local_request.clone(),
    )
    .await
    .unwrap();
    assert_eq!(retried.id, local.id);
    let mut changed = local_request;
    changed.text = "Different French text.".into();
    assert!(
        chef_engine::voice_auditions::submit_local(
            backend.clone(),
            local_actor,
            None,
            root.clone(),
            changed
        )
        .await
        .is_err()
    );
    assert_eq!(qwen.calls.lock().unwrap().len(), calls_before + 1);
    system["reason"] = json!("[local-cli] isolated system candidate");
    let (a, b) = tokio::join!(
        operator.send("POST", audition_api, Some(system.clone()), true),
        disabled.send("POST", audition_api, Some(system.clone()), true)
    );
    // The disabled route may win the race and reject before recording; the configured route always records.
    assert_eq!(a.0, 200);
    assert!([200, 503].contains(&b.0));
    let system_path = format!("{audition_api}/{}", "f".repeat(32));
    let ready = settled(&mut operator, &system_path).await;
    assert_eq!(ready["status"], "ready");
    assert!(ready["cloneJobId"].is_null());
    assert_eq!(ready["baseVoiceRevision"], 0);
    assert_eq!(ready["profile"], candidate["profile"]);
    assert_eq!(
        disabled
            .send("POST", audition_api, Some(system.clone()), true)
            .await
            .0,
        200
    );
    let mut different = system.clone();
    different["candidate"]["profile"]["personality"] = json!("Changed candidate.");
    assert_eq!(
        operator
            .send("POST", audition_api, Some(different), true)
            .await
            .0,
        409
    );
    assert_eq!(qwen.calls.lock().unwrap().len(), calls_before + 1);
    assert_eq!(
        operator
            .send("GET", &format!("{system_path}/file"), None, true)
            .await
            .0,
        200
    );
    let filtered = operator
        .send(
            "GET",
            &format!("{audition_api}?characterId=character-system-qa&characterRevision=1"),
            None,
            true,
        )
        .await;
    assert_eq!(filtered.0, 200);
    assert_eq!(filtered.1["items"].as_array().unwrap().len(), 1);
    for query in [
        "characterId=character-system-qa",
        "characterRevision=1",
        "characterId=character-system-qa&characterRevision=0",
    ] {
        assert_eq!(
            operator
                .send("GET", &format!("{audition_api}?{query}"), None, true)
                .await
                .0,
            400
        );
    }
    let mut stale_review = system.clone();
    stale_review["id"] = json!("6".repeat(32));
    assert_eq!(
        operator
            .send("POST", audition_api, Some(stale_review), true)
            .await
            .0,
        200
    );
    let stale_path = format!("{audition_api}/{}", "6".repeat(32));
    assert_eq!(settled(&mut operator, &stale_path).await["status"], "ready");
    let approval = json!({"accepted":true,"heard":true,"expectedVoiceRevision":0,"reason":"synthetic protocol approval only"});
    let mut unheard = approval.clone();
    unheard["heard"] = json!(false);
    assert_eq!(
        operator
            .send(
                "POST",
                &format!("{system_path}/review"),
                Some(unheard),
                true
            )
            .await
            .0,
        400
    );
    assert!(
        chef_engine::voice_auditions::review_local(
            &backend,
            local_actor,
            "f".repeat(32),
            serde_json::from_value({
                let mut request = approval.clone();
                request["heard"] = json!(false);
                request
            })
            .unwrap(),
        )
        .await
        .is_err()
    );
    assert!(
        chef_engine::voice_auditions::review_local(
            &backend,
            -1,
            "f".repeat(32),
            serde_json::from_value(approval.clone()).unwrap(),
        )
        .await
        .is_err()
    );
    let approved = chef_engine::voice_auditions::review_local(
        &backend,
        local_actor,
        "f".repeat(32),
        serde_json::from_value(approval.clone()).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(approved.applied_voice_revision, Some(1));
    assert_eq!(approved.accepted, Some(true));
    assert_eq!(
        operator
            .send(
                "POST",
                &format!("{system_path}/review"),
                Some(approval),
                true
            )
            .await
            .0,
        409
    );
    let stale_approval = json!({"accepted":true,"heard":true,"expectedVoiceRevision":0,"reason":"must preserve first accepted system voice"});
    assert_eq!(
        operator
            .send(
                "POST",
                &format!("{stale_path}/review"),
                Some(stale_approval.clone()),
                true
            )
            .await
            .0,
        409
    );
    let mut rejection = stale_approval;
    rejection["accepted"] = json!(false);
    assert_eq!(
        operator
            .send(
                "POST",
                &format!("{stale_path}/review"),
                Some(rejection),
                true
            )
            .await
            .0,
        200
    );
    let applied = operator
        .send(
            "GET",
            "/api/v1/operator/characters/character-system-qa/1",
            None,
            true,
        )
        .await
        .1;
    assert_eq!(applied["profile"], candidate["profile"]);
    assert_eq!(applied["voiceRevision"], 1);
    let mut new_attempt = system.clone();
    new_attempt["id"] = json!("9".repeat(32));
    assert_eq!(
        operator
            .send("POST", audition_api, Some(new_attempt), true)
            .await
            .0,
        409
    );
    assert_eq!(qwen.calls.lock().unwrap().len(), calls_before + 2);
    assert!(db.execute_unprepared("INSERT INTO voice_auditions(id,clone_job_id,clone_version,profile,parameters,actor_id,reason,character_id,character_revision,base_voice_revision) SELECT '77777777777777777777777777777777',clone_job_id,clone_version,profile,parameters,actor_id,reason,'character-system-qa',1,0 FROM voice_auditions WHERE clone_job_id IS NOT NULL LIMIT 1").await.is_err());
    // Every job remains reachable with a bounded cursor; abandoned workers become unknown, never resent.
    for i in 0..25u32 {
        let id = format!("{i:032x}");
        db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO voice_clone_jobs(id,grant_id,prefix,actor_id,reason) VALUES($1,$1,$2,$3,'pagination fixture')",vec![id.clone().into(),format!("t{i}").into(),actor.into()])).await.unwrap();
        db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO voice_clone_events(job_id,version,status,reason,created_at) VALUES($1,1,'submitted','abandoned fixture',CURRENT_TIMESTAMP-interval '61 seconds')",vec![id.into()])).await.unwrap();
    }
    let first_jobs = operator.send("GET", jobs, None, true).await.1;
    assert_eq!(first_jobs["items"].as_array().unwrap().len(), 20);
    assert_eq!(first_jobs["items"][0]["status"], "unknown");
    let next_jobs = operator
        .send(
            "GET",
            &format!("{jobs}?afterId={}", first_jobs["next"].as_str().unwrap()),
            None,
            true,
        )
        .await
        .1;
    assert_eq!(next_jobs["items"].as_array().unwrap().len(), 8);
    assert_eq!(
        operator
            .send("GET", &format!("{jobs}?afterId=invalid"), None, true)
            .await
            .0,
        400
    );
    assert!(
        db.execute_unprepared("DELETE FROM voice_clone_events")
            .await
            .is_err()
    );
    assert!(brioche_migration::Migrator::down(&db, None).await.is_err());
    db.close().await.unwrap();
    admin
        .execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}
#[tokio::test]
#[ignore = "set TEST_DATABASE_URL to a dedicated PostgreSQL database"]
async fn pending_links_are_private_revocable_and_serialized_with_consumption() {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let admin = Database::connect(&url).await.unwrap();
    let schema = format!(
        "token_admin_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    admin
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut options = ConnectOptions::new(url);
    options.set_schema_search_path(&schema);
    let db = Database::connect(options).await.unwrap();
    brioche_migration::Migrator::up(&db, None).await.unwrap();
    let backend = Backend::new(db.clone()).await.unwrap();
    let app = identity::router_with_media_root(
        backend.clone(),
        CsrfPolicy::new(["http://localhost:5173".into()]).unwrap(),
        false,
        std::env::temp_dir(),
    );
    let mut visitor = Browser::new(app.clone()).await;
    let mut operator = Browser::new(app.clone()).await;
    operator
        .register(&backend, "operator@example.test", true)
        .await;
    let mut learner = Browser::new(app).await;
    learner
        .register(&backend, "learner@example.test", false)
        .await;
    let path = "/api/v1/operator/accounts/pending-tokens";
    assert_eq!(visitor.send("GET", path, None, true).await.0, 401);
    assert_eq!(learner.send("GET", path, None, true).await.0, 403);
    let invite = backend
        .issue_token("pending@example.test", false, false)
        .await
        .unwrap();
    let reset_token = backend
        .issue_token("learner@example.test", true, false)
        .await
        .unwrap();
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO identity_tokens(token_hash,kind,email,expires_at) VALUES($1,'invite','expired@example.test',CURRENT_TIMESTAMP-interval '1 second')",vec!["e".repeat(64).into()])).await.unwrap();
    let listed = operator.send("GET", path, None, true).await;
    assert_eq!(listed.0, 200);
    assert_eq!(listed.1["items"].as_array().unwrap().len(), 2);
    assert!(!listed.1.to_string().contains(&invite));
    assert!(!listed.1.to_string().contains("tokenHash"));
    let id = listed.1["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["email"] == "pending@example.test")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let revoke = format!("{path}/{id}/revoke");
    assert_eq!(
        learner
            .send("POST", &revoke, Some(json!({"reason":"isolated"})), true)
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .send("POST", &revoke, Some(json!({"reason":"isolated"})), false)
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .send("POST", &revoke, Some(json!({"reason":"isolated"})), true)
            .await
            .0,
        200
    );
    assert_eq!(
        operator
            .send("POST", &revoke, Some(json!({"reason":"isolated"})), true)
            .await
            .0,
        404
    );
    assert!(
        backend
            .accept_invite(brioche_course_contract::AcceptInviteRequest {
                token: invite,
                email: "pending@example.test".into(),
                display_name: "Pending".into(),
                password: "correct horse brioche fromage".into()
            })
            .await
            .is_err()
    );
    let remaining = operator
        .send("GET", &format!("{path}?kind=invite"), None, true)
        .await;
    assert_eq!(remaining.1["items"].as_array().unwrap().len(), 0);
    assert_eq!(
        operator
            .send("GET", &format!("{path}?kind=unknown"), None, true)
            .await
            .0,
        400
    );
    let reset_list = operator
        .send("GET", &format!("{path}?kind=reset"), None, true)
        .await;
    let reset_id = reset_list.1["items"][0]["id"].as_str().unwrap();
    assert_eq!(
        operator
            .send(
                "POST",
                &format!("{path}/{reset_id}/revoke"),
                Some(json!({"reason":"isolated reset revocation"})),
                true
            )
            .await
            .0,
        200
    );
    assert!(
        backend
            .reset_password(brioche_course_contract::ResetPasswordRequest {
                token: reset_token,
                password: "changed but rejected brioche password".into()
            })
            .await
            .is_err()
    );
    assert_eq!(learner.send("GET", "/api/v1/me", None, true).await.0, 200);
    assert_eq!(
        operator
            .send("GET", &format!("{path}?afterId=bad"), None, true)
            .await
            .0,
        400
    );
    let audited=db.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT count(*) AS n FROM account_admin_audit a JOIN users u ON u.id=a.actor_id WHERE a.action IN ('revokeInvite','revokeReset') AND u.email='operator@example.test'".to_owned())).await.unwrap().unwrap().try_get::<i64>("","n").unwrap();
    assert_eq!(audited, 2);
    let history = operator
        .send("GET", "/api/v1/operator/history", None, true)
        .await;
    assert!(
        history.1["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["action"] == "revokeInvite")
    );
    assert!(
        history.1["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["action"] == "revokeReset")
    );
    for i in 0..25 {
        db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO identity_tokens(token_hash,kind,email,role,expires_at) VALUES($1,'invite',$2,'learner',CURRENT_TIMESTAMP+interval '1 day')",vec![format!("{i:064x}").into(),format!("synthetic-{i}@example.test").into()])).await.unwrap();
    }
    let page = operator
        .send("GET", &format!("{path}?kind=invite"), None, true)
        .await;
    assert_eq!(page.1["items"].as_array().unwrap().len(), 20);
    let next = operator
        .send(
            "GET",
            &format!(
                "{path}?kind=invite&afterId={}",
                page.1["nextId"].as_str().unwrap()
            ),
            None,
            true,
        )
        .await;
    assert_eq!(next.1["items"].as_array().unwrap().len(), 5);
    assert_eq!(learner.send("GET", "/api/v1/me", None, true).await.0, 200);
    let race = backend
        .issue_token("race@example.test", false, false)
        .await
        .unwrap();
    // Query the synthetic identifier directly if the first page is filled by other records.
    use sha2::{Digest, Sha256};
    let record_id = format!(
        "{:x}",
        Sha256::digest(format!("{:x}", Sha256::digest(&race)))
    );
    let race_path = format!("{path}/{record_id}/revoke");
    let (revoked, accepted) = tokio::join!(
        operator.send(
            "POST",
            &race_path,
            Some(json!({"reason":"race test"})),
            true
        ),
        backend.accept_invite(brioche_course_contract::AcceptInviteRequest {
            token: race,
            email: "race@example.test".into(),
            display_name: "Race".into(),
            password: "correct horse brioche fromage".into()
        })
    );
    assert_ne!(revoked.0 == 200, accepted.is_ok());
    db.close().await.unwrap();
    admin
        .execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .unwrap();
}
impl Browser {
    async fn new(app: Router) -> Self {
        let mut result = Self {
            app,
            cookie: String::new(),
            csrf: String::new(),
        };
        assert_eq!(
            result.send("GET", "/api/v1/auth/csrf", None, true).await.0,
            200
        );
        result
    }
    async fn send(
        &mut self,
        method: &str,
        path: &str,
        body: Option<Value>,
        protect: bool,
    ) -> (u16, Value) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("cookie", &self.cookie);
        if protect {
            request = request
                .header("origin", "http://localhost:5173")
                .header("x-csrf-token", &self.csrf);
        }
        let request = request
            .header("content-type", "application/json")
            .body(Body::from(
                body.map(|v| serde_json::to_vec(&v).unwrap())
                    .unwrap_or_default(),
            ))
            .unwrap();
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status().as_u16();
        assert_eq!(response.headers()["cache-control"], "private, no-store");
        for cookie in response.headers().get_all("set-cookie") {
            self.cookie = cookie.to_str().unwrap().split(';').next().unwrap().into();
        }
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        if let Some(csrf) = value["csrfToken"].as_str() {
            self.csrf = csrf.into();
        }
        (status, value)
    }
    async fn register(&mut self, backend: &Backend, email: &str, operator: bool) {
        let token = backend.issue_token(email, false, operator).await.unwrap();
        assert_eq!(self.send("POST","/api/v1/auth/accept-invite",Some(json!({"token":token,"email":email,"displayName":"Test","password":"correct horse brioche fromage"})),true).await.0,200);
    }
    async fn upload_asset(&self, document: Value, file: &[u8], protect: bool) -> (u16, Value) {
        self.upload_media("/api/v1/operator/assets", document, file, protect)
            .await
    }
    async fn upload_media(
        &self,
        path: &str,
        document: Value,
        file: &[u8],
        protect: bool,
    ) -> (u16, Value) {
        let boundary = "brioche-test-boundary";
        let mut body=format!("--{boundary}\r\nContent-Disposition: form-data; name=\"document\"\r\n\r\n{}\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"../../untrusted.svg\"\r\nContent-Type: image/svg+xml\r\n\r\n",document).into_bytes();
        body.extend_from_slice(file);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let mut request = Request::builder()
            .method("POST")
            .uri(path)
            .header("cookie", &self.cookie)
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            );
        if protect {
            request = request
                .header("origin", "http://localhost:5173")
                .header("x-csrf-token", &self.csrf);
        }
        let response = self
            .app
            .clone()
            .oneshot(request.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        let status = response.status().as_u16();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
}
#[tokio::test]
#[ignore = "set TEST_DATABASE_URL to a dedicated PostgreSQL database"]
async fn approvals_permissions_concurrency_and_publication() {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let admin = Database::connect(&url).await.unwrap();
    let schema = format!(
        "admin_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    admin
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut options = ConnectOptions::new(url);
    options.set_schema_search_path(&schema);
    let db = Database::connect(options).await.unwrap();
    brioche_migration::Migrator::up(&db, None).await.unwrap();
    let root = assets::fixture_assets(&db, &schema).await;
    let backend = Backend::new(db.clone()).await.unwrap();
    let app = identity::router_with_media_root(
        backend.clone(),
        CsrfPolicy::new(["http://localhost:5173".into()]).unwrap(),
        false,
        root.clone(),
    );
    let mut visitor = Browser::new(app.clone()).await;
    assert_eq!(
        visitor
            .send("GET", "/api/v1/operator/overview", None, true)
            .await
            .0,
        401
    );
    let mut learner = Browser::new(app.clone()).await;
    learner
        .register(&backend, "learner@example.test", false)
        .await;
    assert_eq!(
        learner
            .send("GET", "/api/v1/operator/overview", None, true)
            .await
            .0,
        403
    );
    let mut operator = Browser::new(app.clone()).await;
    operator
        .register(&backend, "operator@example.test", true)
        .await;
    let voices_path = "/api/v1/operator/characters";
    let revisions_path = "/api/v1/operator/characters/revisions";
    let character_request = json!({"characterId":"character-qa","expectedRevision":0,"displayName":"Test original","avatarId":"avatar-camille-v1","avatarRevision":1,"reason":"isolated character creation"});
    assert_eq!(
        visitor
            .send(
                "POST",
                revisions_path,
                Some(character_request.clone()),
                true
            )
            .await
            .0,
        401
    );
    assert_eq!(
        learner
            .send(
                "POST",
                revisions_path,
                Some(character_request.clone()),
                true
            )
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .send(
                "POST",
                revisions_path,
                Some(character_request.clone()),
                false
            )
            .await
            .0,
        403
    );
    let created = operator
        .send(
            "POST",
            revisions_path,
            Some(character_request.clone()),
            true,
        )
        .await;
    assert_eq!(created.0, 200);
    assert_eq!(created.1["character"]["revision"], 1);
    assert_eq!(created.1["voiceRevision"], 0);
    assert!(created.1["profile"].is_null());
    assert_eq!(
        operator
            .send(
                "POST",
                revisions_path,
                Some(character_request.clone()),
                true
            )
            .await
            .0,
        409
    );
    let mut update = character_request.clone();
    update["expectedRevision"] = json!(1);
    update["displayName"] = json!("Test new");
    update["avatarId"] = json!("avatar-luc-v1");
    let mut peer = Browser {
        app: operator.app.clone(),
        cookie: operator.cookie.clone(),
        csrf: operator.csrf.clone(),
    };
    let (left, right) = tokio::join!(
        operator.send("POST", revisions_path, Some(update.clone()), true),
        peer.send("POST", revisions_path, Some(update.clone()), true)
    );
    let mut statuses = vec![left.0, right.0];
    statuses.sort();
    assert_eq!(statuses, vec![200, 409]);
    for path in [
        "/api/v1/operator/characters/character-qa/1",
        "/api/v1/operator/characters/character-qa/2",
    ] {
        assert_eq!(visitor.send("GET", path, None, true).await.0, 401);
        assert_eq!(learner.send("GET", path, None, true).await.0, 403);
    }
    let old = operator
        .send(
            "GET",
            "/api/v1/operator/characters/character-qa/1",
            None,
            true,
        )
        .await;
    assert_eq!(old.0, 200);
    assert_eq!(old.1["character"]["displayName"], "Test original");
    assert_eq!(old.1["character"]["avatarId"], "avatar-camille-v1");
    let latest = operator
        .send(
            "GET",
            "/api/v1/operator/characters/character-qa/2",
            None,
            true,
        )
        .await;
    assert_eq!(latest.1["character"]["displayName"], "Test new");
    assert!(latest.1["profile"].is_null());
    for (key, value) in [
        ("avatarId", json!("art-bakery-morning")),
        ("avatarId", json!("unregistered-avatar")),
        ("displayName", json!("")),
        ("reason", json!("")),
    ] {
        let mut invalid = update.clone();
        invalid["expectedRevision"] = json!(2);
        invalid[key] = value;
        assert_eq!(
            operator
                .send("POST", revisions_path, Some(invalid), true)
                .await
                .0,
            400
        );
    }
    assert!(
        db.execute_unprepared(
            "UPDATE character_revisions SET snapshot='{}' WHERE character_id='character-qa'"
        )
        .await
        .is_err()
    );
    let history = operator
        .send("GET", "/api/v1/operator/history", None, true)
        .await;
    assert!(
        history.1["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["target"] == "character-qa v2" && i["action"] == "assetImport")
    );
    let upload = json!({"assetId":"qa-web-upload","revision":1,"mimeType":"image/svg+xml","altZh":"隔离上传","creditZh":"仅测试","source":"test:synthetic","license":"LicenseRef-TestOnly","creator":"test fixture","rightsConfirmed":true,"reason":"isolated asset upload"});
    let audio_upload = json!({"assetId":"qa-web-recording","revision":1,"mimeType":"audio/mpeg","creditZh":"仅测试","source":"test:synthetic","license":"LicenseRef-TestOnly","creator":"test fixture","rightsConfirmed":true,"reason":"isolated recording upload"});
    let recording = include_bytes!("fixtures/audio/synthetic.mp3");
    let audio_path = "/api/v1/operator/recordings";
    assert_eq!(
        visitor
            .upload_media(audio_path, audio_upload.clone(), recording, true)
            .await
            .0,
        401
    );
    assert_eq!(
        learner
            .upload_media(audio_path, audio_upload.clone(), recording, true)
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .upload_media(audio_path, audio_upload.clone(), recording, false)
            .await
            .0,
        403
    );
    for (field, value) in [
        ("rightsConfirmed", json!(false)),
        ("assetId", json!("../escape")),
        ("reason", json!("")),
        ("mimeType", json!("audio/ogg")),
    ] {
        let mut bad = audio_upload.clone();
        bad[field] = value;
        assert_eq!(
            operator
                .upload_media(audio_path, bad, recording, true)
                .await
                .0,
            400
        );
    }
    assert_eq!(
        operator
            .upload_media(audio_path, audio_upload.clone(), b"not audio", true)
            .await
            .0,
        400
    );
    assert_eq!(
        operator
            .upload_media(
                audio_path,
                audio_upload.clone(),
                &recording[..recording.len() - 1],
                true
            )
            .await
            .0,
        400
    );
    let uploaded = operator
        .upload_media(audio_path, audio_upload.clone(), recording, true)
        .await;
    assert_eq!(uploaded.0, 200);
    assert_eq!(
        uploaded.1,
        json!({"assetId":"qa-web-recording","revision":1})
    );
    assert_eq!(
        operator
            .upload_media(audio_path, audio_upload.clone(), recording, true)
            .await
            .0,
        409
    );
    let row=db.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT count(*) AS n FROM audio_import_audit a JOIN users u ON a.actor_id=u.id WHERE u.email='operator@example.test' AND a.reason='isolated recording upload'".to_owned())).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 1);
    let audio_history = operator
        .send("GET", "/api/v1/operator/history", None, true)
        .await;
    assert!(
        audio_history.1["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["action"] == "audioImport"
                && i["target"] == "qa-web-recording v1"
                && i["reason"] == "isolated recording upload")
    );
    let listed = operator.send("GET", audio_path, None, true).await;
    assert_eq!(listed.1["items"][0]["asset"]["durationMs"], 1000);
    assert_eq!(listed.1["items"][0]["sampleRate"], 24000);
    let mut concurrent = audio_upload.clone();
    concurrent["assetId"] = json!("qa-concurrent-audio");
    concurrent["reason"] = json!("concurrent recording");
    let (left, right) = tokio::join!(
        operator.upload_media(audio_path, concurrent.clone(), recording, true),
        operator.upload_media(audio_path, concurrent, recording, true)
    );
    let mut outcomes = [left.0, right.0];
    outcomes.sort();
    assert_eq!(outcomes, [200, 409]);
    assert!(
        brioche_migration::Migrator::migrations()
            .into_iter()
            .find(|m| m.name() == "m20261007_000017_recording_admin")
            .unwrap()
            .down(&sea_orm_migration::SchemaManager::new(&db))
            .await
            .is_err(),
        "operator recording audit cannot be removed by rollback"
    );
    brioche_migration::Migrator::up(&db, None).await.unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/operator/recordings/qa-web-recording/1/file")
                .header("cookie", &operator.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        &response.into_body().collect().await.unwrap().to_bytes()[..],
        recording
    );
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 96 96\"><!--{}--><rect width=\"96\" height=\"96\" fill=\"red\"/></svg>",
        "x".repeat(20_000)
    );
    assert_eq!(
        visitor
            .upload_asset(upload.clone(), svg.as_bytes(), true)
            .await
            .0,
        401
    );
    assert_eq!(
        learner
            .upload_asset(upload.clone(), svg.as_bytes(), true)
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .upload_asset(upload.clone(), svg.as_bytes(), false)
            .await
            .0,
        403
    );
    let mut invalid = upload.clone();
    invalid["rightsConfirmed"] = json!(false);
    assert_eq!(
        operator.upload_asset(invalid, svg.as_bytes(), true).await.0,
        400
    );
    assert_eq!(operator.upload_asset(upload.clone(),b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\"><script>alert(1)</script></svg>",true).await.0,400);
    assert_eq!(
        operator
            .upload_asset(upload.clone(), b"not an image", true)
            .await
            .0,
        400
    );
    let first_upload = operator
        .upload_asset(upload.clone(), svg.as_bytes(), true)
        .await;
    assert_eq!(first_upload.0, 200);
    assert_eq!(
        first_upload.1,
        json!({"assetId":"qa-web-upload","revision":1})
    );
    assert_eq!(
        operator
            .upload_asset(upload.clone(), svg.as_bytes(), true)
            .await
            .0,
        409
    );
    let row=db.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT count(*) AS n FROM asset_import_audit a JOIN users u ON a.actor_id=u.id WHERE u.email='operator@example.test' AND a.reason='isolated asset upload'".to_owned())).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 1);
    let upload_history = operator
        .send("GET", "/api/v1/operator/history", None, true)
        .await;
    assert!(
        upload_history.1["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["action"] == "assetImport"
                && item["target"] == "qa-web-upload v1"
                && item["reason"] == "isolated asset upload")
    );
    let mut invalid = upload.clone();
    invalid["assetId"] = json!("../escape");
    assert_eq!(
        operator.upload_asset(invalid, svg.as_bytes(), true).await.0,
        400
    );
    let mut invalid = upload.clone();
    invalid["reason"] = json!("");
    assert_eq!(
        operator.upload_asset(invalid, svg.as_bytes(), true).await.0,
        400
    );
    let image = operator
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/operator/assets/qa-web-upload/1/file")
                .header("cookie", &operator.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(image.status(), 200);
    assert_eq!(
        image
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .as_ref(),
        svg.as_bytes()
    );
    let assets_path = "/api/v1/operator/assets";
    let file_path = "/api/v1/operator/assets/avatar-camille-v1/1/file";
    for path in [assets_path, file_path] {
        assert_eq!(visitor.send("GET", path, None, true).await.0, 401);
        assert_eq!(learner.send("GET", path, None, true).await.0, 403);
    }
    let registry = operator.send("GET", assets_path, None, true).await;
    assert_eq!(registry.0, 200);
    assert_eq!(registry.1["items"].as_array().unwrap().len(), 5);
    assert!(registry.1["items"].as_array().unwrap().iter().all(|item| {
        item.get("file").is_none()
            && item.get("provenance").is_none()
            && item["asset"]["url"]
                .as_str()
                .unwrap()
                .starts_with("/api/v1/operator/assets/")
    }));
    assert_eq!(
        operator
            .send(
                "GET",
                "/api/v1/operator/assets?afterId=avatar-camille-v1",
                None,
                true
            )
            .await
            .0,
        400
    );
    assert_eq!(
        operator
            .send(
                "GET",
                "/api/v1/operator/assets?afterId=avatar-camille-v1&afterRevision=0",
                None,
                true
            )
            .await
            .0,
        400
    );
    assert_eq!(
        operator
            .send("GET", "/api/v1/operator/assets?unknown=yes", None, true)
            .await
            .0,
        400
    );
    assert_eq!(
        operator
            .send("GET", "/api/v1/operator/assets?afterRevision=1", None, true)
            .await
            .0,
        400
    );
    // Twenty-five versions of the same ID prove a scalar ID cursor cannot work here.
    db.execute_unprepared(r#"INSERT INTO media_assets(asset_id,revision,descriptor,provenance,sha256,extension,byte_size)
        SELECT 'qa-asset',n,jsonb_set(jsonb_set(descriptor,'{assetId}','"qa-asset"'),'{revision}',to_jsonb(n)),provenance,sha256,extension,byte_size
        FROM media_assets CROSS JOIN generate_series(1,25) n WHERE asset_id='avatar-camille-v1' AND revision=1"#).await.unwrap();
    let first = operator
        .send("GET", "/api/v1/operator/assets?q=qa-asset", None, true)
        .await;
    assert_eq!(first.1["items"].as_array().unwrap().len(), 20);
    assert_eq!(first.1["next"], json!({"assetId":"qa-asset","revision":20}));
    let second = operator
        .send(
            "GET",
            "/api/v1/operator/assets?q=qa-asset&afterId=qa-asset&afterRevision=20",
            None,
            true,
        )
        .await;
    assert_eq!(second.1["items"].as_array().unwrap().len(), 5);
    assert_eq!(second.1["items"][0]["asset"]["revision"], 21);
    assert!(second.1["next"].is_null());
    assert!(
        operator
            .send("GET", "/api/v1/operator/assets?q=%25", None, true)
            .await
            .1["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        operator
            .send(
                "GET",
                "/api/v1/operator/assets/no-such-asset/1/file",
                None,
                true
            )
            .await
            .0,
        404
    );
    let response = operator
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri(file_path)
                .header("cookie", &operator.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["content-type"], "image/svg+xml");
    assert_eq!(response.headers()["cache-control"], "private, no-store");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        bytes.as_ref(),
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../docs/preview/avatars/camille.svg")
        )
        .unwrap()
    );
    assert_eq!(visitor.send("GET", voices_path, None, true).await.0, 401);
    assert_eq!(learner.send("GET", voices_path, None, true).await.0, 403);
    let listed = operator.send("GET", voices_path, None, true).await;
    assert_eq!(listed.0, 200);
    assert!(
        listed.1["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["voiceRevision"] == 0)
    );
    let seed: Value =
        serde_json::from_str(include_str!("../../../docs/characters/voices.json")).unwrap();
    let mut voice_request = json!({"characterId":"character-camille","characterRevision":1,"expectedVoiceRevision":0,"profile":seed["items"][0]["profile"],"reason":"isolated voice profile test"});
    assert_eq!(
        learner
            .send("POST", voices_path, Some(voice_request.clone()), true)
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .send("POST", voices_path, Some(voice_request.clone()), false)
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .send("POST", voices_path, Some(voice_request.clone()), true)
            .await
            .0,
        200
    );
    assert_eq!(
        operator
            .send("POST", voices_path, Some(voice_request.clone()), true)
            .await
            .0,
        409
    );
    voice_request["expectedVoiceRevision"] = json!(1);
    voice_request["profile"]["voiceKind"] = json!("cloned");
    assert_eq!(
        operator
            .send("POST", voices_path, Some(voice_request.clone()), true)
            .await
            .0,
        400
    );
    voice_request["profile"]["voiceKind"] = json!("system");
    voice_request["profile"]["referenceAudio"] = json!({"assetId":"qa-web-recording","revision":1,"transcript":"Bonjour !","cloningPermission":"Synthetic protocol fixture; no real speaker"});
    let mut missing = voice_request.clone();
    missing["profile"]["referenceAudio"]["revision"] = json!(2);
    assert_eq!(
        operator
            .send("POST", voices_path, Some(missing), true)
            .await
            .0,
        404
    );
    let mut unauthorized = voice_request.clone();
    unauthorized["profile"]["referenceAudio"]["cloningPermission"] = json!("");
    assert_eq!(
        operator
            .send("POST", voices_path, Some(unauthorized), true)
            .await
            .0,
        400
    );
    db.execute_unprepared(r#"INSERT INTO audio_assets(asset_id,revision,descriptor,provenance,sha256,extension,byte_size,duration_ms,sample_rate,channels) SELECT 'qa-long-reference',1,jsonb_set(jsonb_set(descriptor,'{assetId}','"qa-long-reference"'),'{durationMs}','31000'),provenance,sha256,extension,byte_size,31000,sample_rate,channels FROM audio_assets WHERE asset_id='qa-web-recording' AND revision=1"#).await.unwrap();
    let mut long = voice_request.clone();
    long["profile"]["referenceAudio"]["assetId"] = json!("qa-long-reference");
    assert_eq!(
        operator.send("POST", voices_path, Some(long), true).await.0,
        400
    );
    voice_request["profile"]["defaultEmotion"] = json!("Quiet and calm.");
    assert_eq!(
        operator
            .send("POST", voices_path, Some(voice_request), true)
            .await
            .0,
        200
    );
    let fixed = operator
        .send(
            "GET",
            "/api/v1/operator/characters/character-camille/1/voices/1",
            None,
            true,
        )
        .await;
    assert_eq!(fixed.0, 200);
    assert_eq!(
        fixed.1["profile"]["defaultEmotion"],
        seed["items"][0]["profile"]["defaultEmotion"]
    );
    assert!(
        db.execute_unprepared("UPDATE character_voice_profiles SET reason='mutated'")
            .await
            .is_err()
    );
    let cli_request = root.join("luc-voice.json");
    std::fs::write(&cli_request,json!({"characterId":"character-luc","characterRevision":1,"expectedVoiceRevision":0,"profile":seed["items"][1]["profile"],"reason":"CLI test"}).to_string()).unwrap();
    let separator = if std::env::var("TEST_DATABASE_URL").unwrap().contains('?') {
        "&"
    } else {
        "?"
    };
    let cli_url = format!(
        "{}{separator}options=-csearch_path%3D{schema}",
        std::env::var("TEST_DATABASE_URL").unwrap()
    );
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_chef-server"))
        .current_dir(&root)
        .env("DATABASE_URL", cli_url)
        .env("CONTENT_MODE", "database")
        .env("API_BIND", "invalid-bind-must-not-be-used")
        .args([
            "character-voice-import",
            cli_request.to_str().unwrap(),
            "operator@example.test",
            "isolated CLI profile initialization",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("registered at revision 1"));
    let mut source: Value =
        serde_json::from_str(include_str!("../../../docs/examples/a1-bakery.lesson.json")).unwrap();
    source["assetRefs"] = assets::fixture_refs();
    source["editorial"] = json!({"status":"draft","note":"synthetic draft"});
    source["summaryZh"] = json!("导入正文边界".repeat(1500));
    let import_request = json!({"document":source.to_string(),"reason":"operator import test"});
    assert!(serde_json::to_vec(&import_request).unwrap().len() > 16 * 1024);
    assert_eq!(
        learner
            .send(
                "POST",
                "/api/v1/operator/lessons/import",
                Some(import_request.clone()),
                true
            )
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .send(
                "POST",
                "/api/v1/operator/lessons/import",
                Some(import_request.clone()),
                false
            )
            .await
            .0,
        403
    );
    let mut missing_asset = source.clone();
    missing_asset["assetRefs"][0]["assetId"] = json!("unregistered-asset");
    assert_eq!(
        operator
            .send(
                "POST",
                "/api/v1/operator/lessons/import",
                Some(json!({"document":missing_asset.to_string(),"reason":"invalid asset"})),
                true
            )
            .await
            .0,
        400
    );
    assert_eq!(operator.send("POST","/api/v1/operator/lessons/import",Some(json!({"document":"{\"id\":\"a\",\"id\":\"b\"}","reason":"duplicate JSON member"})),true).await.0,400);
    assert_eq!(
        operator
            .send(
                "POST",
                "/api/v1/operator/lessons/import",
                Some(import_request.clone()),
                true
            )
            .await
            .0,
        200
    );
    assert_eq!(
        operator
            .send(
                "POST",
                "/api/v1/operator/lessons/import",
                Some(import_request.clone()),
                true
            )
            .await
            .0,
        200
    );
    let mut different = source.clone();
    different["summaryZh"] = json!("different content");
    assert_eq!(
        operator
            .send(
                "POST",
                "/api/v1/operator/lessons/import",
                Some(json!({"document":different.to_string(),"reason":"immutable conflict"})),
                true
            )
            .await
            .0,
        409
    );
    let imports = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(*) AS n FROM lesson_import_audit",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "n")
        .unwrap();
    assert_eq!(imports, 1);
    let source = chef_engine::media::hydrate_source(&db, source)
        .await
        .unwrap();
    let lesson = chef_engine::project_source(source.clone()).unwrap();
    let path = format!("/api/v1/operator/lessons/{}/revisions/1/review", lesson.id);
    let decision = json!({"version":0,"approved":true,"reason":"Test approval"});
    assert_eq!(
        learner
            .send("POST", &path, Some(decision.clone()), true)
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .send("POST", &path, Some(decision.clone()), false)
            .await
            .0,
        403
    );
    let overview = operator
        .send("GET", "/api/v1/operator/overview", None, true)
        .await;
    assert_eq!(overview.0, 200);
    assert_eq!(overview.1["lessons"][0]["approved"], false);
    let encoded = overview.1.to_string();
    assert!(
        !encoded.contains("serverOnly")
            && !encoded.contains("accepted")
            && !encoded.contains("correctOptionId")
    );
    let manifest:chef_engine::content::ReleaseManifest=serde_json::from_value(json!({"id":"admin-test","schemaVersion":"1.0","levels":[{"id":lesson.level_id,"label":"A1","units":[{"id":lesson.unit_id,"titleZh":"test","lessons":[{"lessonId":lesson.id,"revision":1}]}]}]})).unwrap();
    let stage_request = json!({"document":serde_json::to_string(&manifest).unwrap(),"reason":"stage through admin"});
    assert_eq!(
        learner
            .send(
                "POST",
                "/api/v1/operator/releases/stage",
                Some(stage_request.clone()),
                true
            )
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .send(
                "POST",
                "/api/v1/operator/releases/stage",
                Some(stage_request.clone()),
                true
            )
            .await
            .0,
        400
    );
    assert!(
        chef_engine::content::stage(&db, &manifest, "test", "test", &root)
            .await
            .is_err()
    );
    let approved = operator
        .send("POST", &path, Some(decision.clone()), true)
        .await;
    assert_eq!(approved.0, 200);
    assert_eq!(approved.1["version"], 1);
    assert_eq!(
        operator
            .send("POST", &path, Some(decision.clone()), true)
            .await
            .0,
        200
    );
    assert_eq!(
        operator
            .send(
                "POST",
                &path,
                Some(json!({"version":0,"approved":false,"reason":"stale"})),
                true
            )
            .await
            .0,
        409
    );
    assert_eq!(
        operator
            .send(
                "POST",
                "/api/v1/operator/releases/stage",
                Some(stage_request.clone()),
                true
            )
            .await
            .0,
        200
    );
    assert_eq!(
        operator
            .send(
                "POST",
                "/api/v1/operator/releases/stage",
                Some(stage_request.clone()),
                true
            )
            .await
            .0,
        409
    );
    assert_eq!(
        operator
            .send(
                "POST",
                &path,
                Some(json!({"version":1,"approved":false,"reason":"return staged course"})),
                true
            )
            .await
            .0,
        200
    );
    assert_eq!(
        operator
            .send(
                "POST",
                "/api/v1/operator/releases/activate",
                Some(json!({"releaseId":"admin-test","generation":"0","reason":"test"})),
                true
            )
            .await
            .0,
        409
    );
    assert_eq!(
        operator
            .send(
                "POST",
                &path,
                Some(json!({"version":2,"approved":true,"reason":"approve again"})),
                true
            )
            .await
            .0,
        200
    );
    assert_eq!(
        operator
            .send(
                "POST",
                "/api/v1/operator/releases/activate",
                Some(json!({"releaseId":"admin-test","generation":"0","reason":"test"})),
                true
            )
            .await
            .0,
        200
    );
    assert_eq!(
        operator
            .send(
                "POST",
                &path,
                Some(json!({"version":3,"approved":false,"reason":"published"})),
                true
            )
            .await
            .0,
        409
    );
    let saved = db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT server_document FROM lesson_revisions WHERE lesson_id=$1",
            [lesson.id.clone().into()],
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<Value>("", "server_document")
        .unwrap();
    assert_eq!(saved, source);
    let mut second_source = source.clone();
    second_source["revision"] = json!(2);
    let second_lesson = chef_engine::project_source(second_source.clone()).unwrap();
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO lesson_revisions(lesson_id,revision,published,public_document,server_document) VALUES($1,2,false,$2,$3)",[lesson.id.clone().into(),serde_json::to_value(second_lesson).unwrap().into(),second_source.into()])).await.unwrap();
    let mut second_tab = Browser {
        app: operator.app.clone(),
        cookie: operator.cookie.clone(),
        csrf: operator.csrf.clone(),
    };
    let second_path = format!("/api/v1/operator/lessons/{}/revisions/2/review", lesson.id);
    let (a, b) = tokio::join!(
        operator.send(
            "POST",
            &second_path,
            Some(json!({"version":0,"approved":true,"reason":"concurrent approval"})),
            true
        ),
        second_tab.send(
            "POST",
            &second_path,
            Some(json!({"version":0,"approved":false,"reason":"concurrent rejection"})),
            true
        )
    );
    let mut statuses = [a.0, b.0];
    statuses.sort();
    assert_eq!(statuses, [200, 409]);
    let concurrent_count = db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT count(*) AS n FROM editorial_reviews WHERE lesson_id=$1 AND revision=2",
            [lesson.id.clone().into()],
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "n")
        .unwrap();
    assert_eq!(concurrent_count, 1);
    let withdraw = format!(
        "/api/v1/operator/lessons/{}/revisions/1/withdraw",
        lesson.id
    );
    assert_eq!(
        operator
            .send(
                "POST",
                &withdraw,
                Some(json!({"generation":"0","reason":"stale"})),
                true
            )
            .await
            .0,
        409
    );
    assert_eq!(
        operator
            .send(
                "POST",
                &withdraw,
                Some(json!({"generation":"1","reason":"test withdrawal"})),
                true
            )
            .await
            .0,
        200
    );
    assert_eq!(
        operator
            .send(
                "POST",
                &path,
                Some(json!({"version":3,"approved":true,"reason":"withdrawn"})),
                true
            )
            .await
            .0,
        410
    );
    let latest = operator
        .send("GET", "/api/v1/operator/overview", None, true)
        .await;
    assert_eq!(latest.1["generation"], "2");
    for (browser, status) in [(&mut visitor, 401), (&mut learner, 403)] {
        assert_eq!(
            browser
                .send("GET", "/api/v1/operator/history", None, true)
                .await
                .0,
            status
        );
    }
    assert_eq!(
        operator
            .send("GET", "/api/v1/operator/history?beforeTime=bad", None, true)
            .await
            .0,
        400
    );
    assert_eq!(
        operator
            .send(
                "GET",
                "/api/v1/operator/history?beforeTime=bad&beforeKey=content:1",
                None,
                true
            )
            .await
            .0,
        400
    );
    let history = operator
        .send("GET", "/api/v1/operator/history", None, true)
        .await;
    assert_eq!(history.0, 200);
    let history_text = history.1.to_string();
    for action in [
        "approve", "reject", "import", "stage", "activate", "withdraw",
    ] {
        assert!(
            history.1["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["action"] == action),
            "missing {action}"
        );
    }
    assert!(!history_text.contains("serverOnly"));
    assert!(!history_text.contains("password"));
    assert!(!history_text.contains("csrfToken"));
    // Fixed-version keyset pagination crosses both revision and ID boundaries.
    for (id, revisions) in [
        (lesson.id.as_str(), (101..=125).collect::<Vec<_>>()),
        ("zz-pagination", vec![1, 2, 3]),
    ] {
        for rev in revisions {
            let mut document = source.clone();
            document["id"] = json!(id);
            document["revision"] = json!(rev);
            document["title"]["zh"] = json!(if rev == 101 {
                "Pagination 100%_'"
            } else {
                "Pagination synthetic course"
            });
            let public = chef_engine::project_source(document.clone()).unwrap();
            db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,
                "INSERT INTO lesson_revisions(lesson_id,revision,published,public_document,server_document) VALUES($1,$2,false,$3,$4)",
                vec![id.into(),rev.into(),serde_json::to_value(public).unwrap().into(),document.into()])).await.unwrap();
        }
    }
    for n in 1..=25 {
        let id = format!("pagination-{n:03}");
        let mut document = serde_json::to_value(&manifest).unwrap();
        document["id"] = json!(id);
        db.execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "INSERT INTO content_releases(id,manifest,content_hash) VALUES($1,$2,$3)",
            vec![id.clone().into(), document.into(), "a".repeat(64).into()],
        ))
        .await
        .unwrap();
        db.execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "INSERT INTO release_entries(release_id,lesson_id,revision,position) VALUES($1,$2,1,0)",
            vec![id.into(), lesson.id.clone().into()],
        ))
        .await
        .unwrap();
    }
    for query in [
        "lessonAfterId=a",
        "lessonAfterRevision=1",
        "lessonAfterId=../bad&lessonAfterRevision=1",
        "lessonAfterId=a&lessonAfterRevision=0",
        "releaseAfterId=../bad",
        "lessonQ=%0A",
        "unknown=true",
    ] {
        assert_eq!(
            operator
                .send(
                    "GET",
                    &format!("/api/v1/operator/overview?{query}"),
                    None,
                    true
                )
                .await
                .0,
            400
        );
    }
    assert_eq!(
        operator
            .send(
                "GET",
                &format!("/api/v1/operator/overview?lessonQ={}", "x".repeat(201)),
                None,
                true
            )
            .await
            .0,
        400
    );
    let first = operator
        .send(
            "GET",
            "/api/v1/operator/overview?lessonQ=pagination&releaseQ=PAGINATION",
            None,
            true,
        )
        .await;
    assert_eq!(first.0, 200);
    assert_eq!(first.1["lessons"].as_array().unwrap().len(), 20);
    assert_eq!(first.1["releases"].as_array().unwrap().len(), 20);
    assert_eq!(first.1["lessonNext"]["revision"], 106);
    assert_eq!(first.1["releaseNext"], "pagination-020");
    let second = operator.send("GET", &format!("/api/v1/operator/overview?lessonQ=pagination&releaseQ=PAGINATION&lessonAfterId={}&lessonAfterRevision=106&releaseAfterId=pagination-020",lesson.id),None,true).await;
    assert_eq!(second.0, 200);
    assert_eq!(second.1["lessons"].as_array().unwrap().len(), 8);
    assert_eq!(second.1["releases"].as_array().unwrap().len(), 5);
    assert!(second.1["lessonNext"].is_null() && second.1["releaseNext"].is_null());
    let mut seen = std::collections::BTreeSet::new();
    for item in first.1["lessons"]
        .as_array()
        .unwrap()
        .iter()
        .chain(second.1["lessons"].as_array().unwrap())
    {
        assert!(seen.insert((
            item["id"].as_str().unwrap(),
            item["revision"].as_u64().unwrap()
        )));
        assert!(!item["published"].as_bool().unwrap());
    }
    assert_eq!(seen.len(), 28);
    assert!(
        first.1["releases"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["lessonCount"] == 1)
    );
    let literal = operator
        .send(
            "GET",
            "/api/v1/operator/overview?lessonQ=100%25_%27",
            None,
            true,
        )
        .await;
    assert_eq!(literal.0, 200);
    assert_eq!(literal.1["lessons"].as_array().unwrap().len(), 1);
    assert_eq!(literal.1["lessons"][0]["revision"], 101);
    let no_match = operator
        .send(
            "GET",
            "/api/v1/operator/overview?lessonQ=no-match&releaseQ=no-match",
            None,
            true,
        )
        .await;
    assert!(
        no_match.1["lessons"].as_array().unwrap().is_empty()
            && no_match.1["releases"].as_array().unwrap().is_empty()
    );
    assert_eq!(no_match.1["generation"], "2");
    for key in ["serverOnly", "correctOptionId", "accepted", "password"] {
        assert!(!second.1.to_string().contains(key));
    }
    // One database-clock boundary remains in the future on every test date.
    let history_boundary = db.query_one_raw(Statement::from_string(DbBackend::Postgres,
        "SELECT to_char((CURRENT_TIMESTAMP + interval '1 day') AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS boundary".to_owned()
    )).await.unwrap().unwrap().try_get::<String>("", "boundary").unwrap();
    // Tie timestamps exercise the secondary key and boundaries across tables.
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,
        "INSERT INTO content_audit(action,actor,reason,generation,created_at) SELECT 'stage','pagination-test','pagination-'||n,2,$1::timestamptz FROM generate_series(1,25) n",
        vec![history_boundary.clone().into()]
    )).await.unwrap();
    let first = operator
        .send("GET", "/api/v1/operator/history", None, true)
        .await;
    assert_eq!(first.0, 200);
    assert_eq!(first.1["items"].as_array().unwrap().len(), 20);
    let mut seen = std::collections::HashSet::new();
    let mut page = first.1;
    loop {
        for item in page["items"].as_array().unwrap() {
            assert!(
                seen.insert(item["key"].as_str().unwrap().to_owned()),
                "duplicate page entry"
            );
        }
        if page["next"].is_null() {
            break;
        }
        let mut url = url::Url::parse("http://test/api/v1/operator/history").unwrap();
        url.query_pairs_mut()
            .append_pair("beforeTime", page["next"]["beforeTime"].as_str().unwrap())
            .append_pair("beforeKey", page["next"]["beforeKey"].as_str().unwrap());
        let result = operator
            .send(
                "GET",
                &format!("{}?{}", url.path(), url.query().unwrap()),
                None,
                true,
            )
            .await;
        assert_eq!(result.0, 200);
        page = result.1;
    }
    assert_eq!(
        seen.len(),
        25 + history.1["items"].as_array().unwrap().len()
    );
    assert_eq!(
        latest.1["lessons"]
            .as_array()
            .unwrap()
            .iter()
            .find(|lesson| lesson["revision"] == 1)
            .unwrap()["withdrawn"],
        true
    );
    let account_endpoint = "/api/v1/operator/accounts/token";
    let invitation = json!({"email":"new@example.test","kind":"invite","operator":false,"reason":"new learner test"});
    assert_eq!(
        learner
            .send("POST", account_endpoint, Some(invitation.clone()), true)
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .send("POST", account_endpoint, Some(invitation.clone()), false)
            .await
            .0,
        403
    );
    assert_eq!(
        visitor
            .send("GET", "/api/v1/operator/accounts", None, true)
            .await
            .0,
        401
    );
    assert_eq!(
        learner
            .send("GET", "/api/v1/operator/accounts", None, true)
            .await
            .0,
        403
    );
    let first_invite = operator
        .send("POST", account_endpoint, Some(invitation.clone()), true)
        .await;
    assert_eq!(first_invite.0, 200);
    assert_eq!(first_invite.1["expiresInSeconds"], 172800);
    let second_invite = operator
        .send("POST", account_endpoint, Some(invitation), true)
        .await;
    assert_eq!(second_invite.0, 200);
    assert_ne!(first_invite.1["token"], second_invite.1["token"]);
    let mut invitee = Browser::new(app.clone()).await;
    let mut accept = json!({"token":first_invite.1["token"],"email":"new@example.test","displayName":"New account","password":"correct horse brioche fromage"});
    assert_eq!(
        invitee
            .send(
                "POST",
                "/api/v1/auth/accept-invite",
                Some(accept.clone()),
                true
            )
            .await
            .0,
        400
    );
    accept["token"] = second_invite.1["token"].clone();
    assert_eq!(
        invitee
            .send(
                "POST",
                "/api/v1/auth/accept-invite",
                Some(accept.clone()),
                true
            )
            .await
            .0,
        200
    );
    assert_eq!(
        invitee
            .send("POST", "/api/v1/auth/accept-invite", Some(accept), true)
            .await
            .0,
        400
    );
    let reset =
        json!({"email":"new@example.test","kind":"reset","operator":false,"reason":"reset test"});
    let reset_link = operator
        .send("POST", account_endpoint, Some(reset), true)
        .await;
    assert_eq!(reset_link.0, 200);
    assert_eq!(reset_link.1["expiresInSeconds"], 1800);
    let mut password_reset = Browser::new(app.clone()).await;
    assert_eq!(password_reset.send("POST","/api/v1/auth/reset-password",Some(json!({"token":reset_link.1["token"],"password":"changed horse brioche fromage"})),true).await.0,200);
    assert_eq!(invitee.send("GET", "/api/v1/me", None, true).await.0, 401);
    let mut invalid_invite =
        json!({"email":"another@example.test","kind":"invite","operator":false,"reason":""});
    assert_eq!(
        operator
            .send("POST", account_endpoint, Some(invalid_invite.clone()), true)
            .await
            .0,
        400
    );
    invalid_invite["reason"] = json!("check");
    invalid_invite["kind"] = json!("reset");
    invalid_invite["operator"] = json!(true);
    assert_eq!(
        operator
            .send("POST", account_endpoint, Some(invalid_invite), true)
            .await
            .0,
        400
    );
    let account_history = operator
        .send("GET", "/api/v1/operator/history", None, true)
        .await;
    let audit_count = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(*) AS n FROM account_admin_audit".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "n")
        .unwrap();
    assert_eq!(audit_count, 3);
    let operator_invite=operator.send("POST",account_endpoint,Some(json!({"email":"invited-operator@example.test","kind":"invite","operator":true,"reason":"add another operator test"})),true).await;
    assert_eq!(operator_invite.0, 200);
    let mut new_operator = Browser::new(app.clone()).await;
    let accepted=new_operator.send("POST","/api/v1/auth/accept-invite",Some(json!({"token":operator_invite.1["token"],"email":"invited-operator@example.test","displayName":"Invited operator","password":"correct horse brioche fromage"})),true).await;
    assert_eq!(accepted.0, 200);
    assert_eq!(accepted.1["user"]["role"], "operator");
    assert_eq!(
        new_operator
            .send("GET", "/api/v1/operator/accounts", None, true)
            .await
            .0,
        200
    );
    let invite_role=db.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT details->>'role' AS role FROM account_admin_audit WHERE target_email='invited-operator@example.test'".to_owned())).await.unwrap().unwrap().try_get::<String>("","role").unwrap();
    assert_eq!(invite_role, "operator");
    let history_text = account_history.1.to_string();
    assert!(!history_text.contains(first_invite.1["token"].as_str().unwrap()));
    assert!(!history_text.contains(second_invite.1["token"].as_str().unwrap()));
    assert!(!history_text.contains(reset_link.1["token"].as_str().unwrap()));
    // Search and keyset pagination must not expose private user fields.
    db.execute_unprepared("INSERT INTO users(email,password_hash,display_name,role) SELECT 'page-'||n||'@example.test',password_hash,'Page account','learner' FROM users CROSS JOIN generate_series(1,25) n WHERE email='operator@example.test'").await.unwrap();
    let accounts = operator
        .send("GET", "/api/v1/operator/accounts?q=PAGE", None, true)
        .await;
    assert_eq!(accounts.0, 200);
    assert_eq!(accounts.1["items"].as_array().unwrap().len(), 20);
    assert!(!accounts.1.to_string().contains("password"));
    assert!(!accounts.1.to_string().contains("settings"));
    let next = operator
        .send(
            "GET",
            &format!(
                "/api/v1/operator/accounts?q=PAGE&afterId={}",
                accounts.1["nextId"].as_str().unwrap()
            ),
            None,
            true,
        )
        .await;
    assert_eq!(next.0, 200);
    assert_eq!(next.1["items"].as_array().unwrap().len(), 5);
    assert!(next.1["nextId"].is_null());
    assert!(accounts.1["items"].as_array().unwrap().iter().all(|first| {
        !next.1["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|second| first["id"] == second["id"])
    }));
    // Role management is authorized, optimistic, audited and protects the last operator.
    let users = operator
        .send("GET", "/api/v1/operator/accounts", None, true)
        .await;
    let lookup = |email: &str| {
        users.1["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|u| u["email"] == email)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let first_id = lookup("operator@example.test");
    let second_id = lookup("invited-operator@example.test");
    let learner_id = lookup("learner@example.test");
    // Session controls never expose cookies/auth state and isolate the target account.
    let sessions_path = format!("/api/v1/operator/accounts/{learner_id}/sessions");
    assert_eq!(visitor.send("GET", &sessions_path, None, true).await.0, 401);
    assert_eq!(learner.send("GET", &sessions_path, None, true).await.0, 403);
    let initial = operator.send("GET", &sessions_path, None, true).await;
    assert_eq!(initial.0, 200);
    assert_eq!(initial.1["items"].as_array().unwrap().len(), 1);
    let initial_key = initial.1["items"][0]["id"].as_str().unwrap().to_owned();
    let mut learner_device = Browser::new(app.clone()).await;
    assert_eq!(learner_device.send("POST","/api/v1/auth/login",Some(json!({"email":"learner@example.test","password":"correct horse brioche fromage"})),true).await.0,200);
    let listed = operator.send("GET", &sessions_path, None, true).await;
    assert_eq!(listed.1["items"].as_array().unwrap().len(), 2);
    let key = listed.1["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] != initial_key)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(key.len(), 64);
    assert!(
        !listed
            .1
            .to_string()
            .contains(learner_device.cookie.split('=').nth(1).unwrap())
    );
    for private in ["csrf", "password", "brioche.auth", "settings"] {
        assert!(!listed.1.to_string().contains(private));
    }
    let revoke = format!("{sessions_path}/{key}/revoke");
    let reason = json!({"reason":"revoke second learner device"});
    assert_eq!(
        learner
            .send("POST", &revoke, Some(reason.clone()), true)
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .send("POST", &revoke, Some(reason.clone()), false)
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .send(
                "POST",
                &format!("/api/v1/operator/accounts/{first_id}/sessions/{key}/revoke"),
                Some(reason.clone()),
                true
            )
            .await
            .0,
        404
    );
    assert_eq!(
        learner_device.send("GET", "/api/v1/me", None, true).await.0,
        200
    );
    assert_eq!(
        operator
            .send("POST", &revoke, Some(reason.clone()), true)
            .await
            .0,
        200
    );
    assert_eq!(
        operator.send("POST", &revoke, Some(reason), true).await.0,
        404
    );
    assert_eq!(
        learner_device.send("GET", "/api/v1/me", None, true).await.0,
        401
    );
    assert_eq!(learner.send("GET", "/api/v1/me", None, true).await.0, 200);
    assert_eq!(
        operator
            .send("GET", &format!("{sessions_path}?afterId=bad"), None, true)
            .await
            .0,
        400
    );
    // Synthetic opaque records test bounded paging and exclusion of expired sessions.
    db.execute_unprepared(&format!("INSERT INTO browser_sessions(id_hash,data,expires_at) SELECT lpad(to_hex(n),64,'0'),data,CURRENT_TIMESTAMP+interval '1 day' FROM browser_sessions CROSS JOIN generate_series(5000,5024) n WHERE id_hash='{initial_key}'; INSERT INTO browser_sessions(id_hash,data,expires_at) SELECT repeat('f',64),data,CURRENT_TIMESTAMP-interval '1 second' FROM browser_sessions WHERE id_hash='{initial_key}'")).await.unwrap();
    let first_page = operator.send("GET", &sessions_path, None, true).await;
    assert_eq!(first_page.1["items"].as_array().unwrap().len(), 20);
    let after = first_page.1["nextId"].as_str().unwrap();
    let second_page = operator
        .send(
            "GET",
            &format!("{sessions_path}?afterId={after}"),
            None,
            true,
        )
        .await;
    assert_eq!(second_page.1["items"].as_array().unwrap().len(), 6);
    assert!(second_page.1["nextId"].is_null());
    let all = first_page.1["items"]
        .as_array()
        .unwrap()
        .iter()
        .chain(second_page.1["items"].as_array().unwrap());
    let ids: std::collections::HashSet<_> = all.map(|s| s["id"].as_str().unwrap()).collect();
    assert_eq!(ids.len(), 26);
    assert!(!ids.contains("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"));
    let payload = json!({"expectedRole":"learner","role":"operator","reason":"promote learner"});
    let learner_path = format!("/api/v1/operator/accounts/{learner_id}/role");
    assert_eq!(
        learner
            .send("POST", &learner_path, Some(payload.clone()), true)
            .await
            .0,
        403
    );
    assert_eq!(
        visitor
            .send("POST", &learner_path, Some(payload.clone()), true)
            .await
            .0,
        401
    );
    assert_eq!(
        operator
            .send("POST", &learner_path, Some(payload.clone()), false)
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .send("POST", &learner_path, Some(payload.clone()), true)
            .await
            .0,
        200
    );
    // Existing learner session obtains current DB privileges; no re-login or token spoofing.
    assert_eq!(
        learner
            .send("GET", "/api/v1/operator/accounts", None, true)
            .await
            .0,
        200
    );
    assert_eq!(
        operator
            .send("POST", &learner_path, Some(payload), true)
            .await
            .0,
        409
    );
    let demote = json!({"expectedRole":"operator","role":"learner","reason":"demote learner"});
    assert_eq!(
        operator
            .send("POST", &learner_path, Some(demote.clone()), true)
            .await
            .0,
        200
    );
    assert_eq!(
        learner
            .send("GET", "/api/v1/operator/accounts", None, true)
            .await
            .0,
        403
    );
    // Two operators concurrently attempt self-demotion: exactly one survives.
    let first_path = format!("/api/v1/operator/accounts/{first_id}/role");
    let second_path = format!("/api/v1/operator/accounts/{second_id}/role");
    let (first, second) = tokio::join!(
        operator.send("POST", &first_path, Some(demote.clone()), true),
        new_operator.send("POST", &second_path, Some(demote.clone()), true)
    );
    assert!(matches!((first.0, second.0), (200, 409) | (409, 200)));
    let count = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(*) AS n FROM product_memberships WHERE product_id='brioche' AND role='operator'".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "n")
        .unwrap();
    assert_eq!(count, 1);
    let changed = db.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT count(*) AS n FROM account_admin_audit WHERE action='role' AND details ? 'from' AND details ? 'to'".to_owned())).await.unwrap().unwrap().try_get::<i64>("","n").unwrap();
    assert_eq!(changed, 3);
    let (survivor, removed, last_path) = if first.0 == 200 {
        (&mut new_operator, &mut operator, &second_path)
    } else {
        (&mut operator, &mut new_operator, &first_path)
    };
    assert_eq!(
        removed
            .send("POST", last_path, Some(demote.clone()), true)
            .await
            .0,
        403
    );
    assert_eq!(
        survivor.send("POST", last_path, Some(demote), true).await.0,
        409
    );
    // Earlier pagination fixtures deliberately sit in the future; skip those rows.
    let event = survivor
        .send(
            "GET",
            &format!("/api/v1/operator/history?beforeTime={history_boundary}&beforeKey=account:0"),
            None,
            true,
        )
        .await;
    assert!(
        event.1["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["action"] == "role")
    );
    let survivor_id = if first.0 == 200 {
        &second_id
    } else {
        &first_id
    };
    let own = survivor
        .send(
            "GET",
            &format!("/api/v1/operator/accounts/{survivor_id}/sessions"),
            None,
            true,
        )
        .await;
    let current = own.1["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["current"] == true)
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let revoked = survivor
        .send(
            "POST",
            &format!("/api/v1/operator/accounts/{survivor_id}/sessions/{current}/revoke"),
            Some(json!({"reason":"revoke current operator browser"})),
            true,
        )
        .await;
    assert_eq!(revoked.0, 200);
    assert_eq!(revoked.1["current"], true);
    assert_eq!(survivor.send("GET", "/api/v1/me", None, true).await.0, 401);
    let count = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(*) AS n FROM account_admin_audit WHERE action='sessions'".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "n")
        .unwrap();
    assert_eq!(count, 2);
    db.close().await.unwrap();
    admin
        .execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
#[ignore = "set TEST_DATABASE_URL to a dedicated PostgreSQL database"]
async fn course_speech_plans_are_fixed_private_idempotent_and_retained() {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let admin = Database::connect(&url).await.unwrap();
    let schema = format!(
        "speech_plan_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    admin
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut options = ConnectOptions::new(url);
    options.set_schema_search_path(&schema);
    let db = Database::connect(options).await.unwrap();
    brioche_migration::Migrator::up(&db, None).await.unwrap();
    let root = assets::fixture_assets(&db, &schema).await;
    let backend = Backend::new(db.clone()).await.unwrap();
    let app = identity::router_with_media_root(
        backend.clone(),
        CsrfPolicy::new(["http://localhost:5173".into()]).unwrap(),
        false,
        root.clone(),
    );
    let qwen = std::sync::Arc::new(MockQwen::default());
    let app = app.layer(axum::Extension(
        chef_engine::qwen::Service::new(qwen.clone(), "https://example.test").unwrap(),
    ));
    let mut visitor = Browser::new(app.clone()).await;
    let mut learner = Browser::new(app.clone()).await;
    learner
        .register(&backend, "speech-learner@example.test", false)
        .await;
    let mut operator = Browser::new(app.clone()).await;
    operator
        .register(&backend, "speech-operator@example.test", true)
        .await;
    let mut second = Browser::new(app.clone()).await;
    second
        .register(&backend, "speech-second@example.test", true)
        .await;
    let source: Value =
        serde_json::from_str(include_str!("../../../docs/examples/a1-bakery.lesson.json")).unwrap();
    let imported =
        chef_engine::author_import::import(&db, source, "isolated-test", "Synthetic plan fixture")
            .await
            .unwrap();
    let lesson_id = imported.lesson_id;
    let revision = imported.revision;
    let path = "/api/v1/operator/speech-plans";
    let options_path =
        format!("/api/v1/operator/lessons/{lesson_id}/revisions/{revision}/speech-options");
    assert_eq!(visitor.send("GET", &options_path, None, true).await.0, 401);
    assert_eq!(learner.send("GET", &options_path, None, true).await.0, 403);
    let seed: Value =
        serde_json::from_str(include_str!("../../../docs/characters/voices.json")).unwrap();
    let mut voices = vec![];
    for id in ["character-camille", "character-luc", "character-lea"] {
        let r=operator.send("POST","/api/v1/operator/characters",Some(json!({"characterId":id,"characterRevision":1,"expectedVoiceRevision":0,"profile":seed["items"][0]["profile"],"reason":"Synthetic fixed compiler test, not real voice approval"})),true).await;
        assert_eq!(r.0, 200);
        voices.push(json!({"characterId":id,"characterRevision":1,"voiceRevision":1}));
    }
    let preview_request = json!({"lessonId":lesson_id,"lessonRevision":revision,"selection":{"voices":voices,"knowledgeNarrator":voices[0],"emotions":{}}});
    let preview = operator
        .send(
            "POST",
            &format!("{path}/preview"),
            Some(preview_request.clone()),
            true,
        )
        .await;
    assert_eq!(preview.0, 200);
    assert!(preview.1["targets"].as_array().unwrap().len() > 11);
    assert!(preview.1.get("serverOnly").is_none());
    let actor = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT id FROM users WHERE email='speech-operator@example.test'",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "id")
        .unwrap();
    let learner_actor = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT id FROM users WHERE email='speech-learner@example.test'",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "id")
        .unwrap();
    let local_preview = chef_engine::admin_speech_plans::preview_for_actor(
        &backend,
        actor,
        &serde_json::from_value(preview_request.clone()).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(serde_json::to_value(local_preview).unwrap(), preview.1);
    assert!(matches!(
        chef_engine::admin_speech_plans::preview_for_actor(
            &backend,
            learner_actor,
            &serde_json::from_value(preview_request.clone()).unwrap()
        )
        .await,
        Err(chef_engine::AppError::Forbidden)
    ));
    let id = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
    let request = json!({"id":id,"preview":preview_request,"expectedPlanHash":preview.1["planHash"],"reason":"Fixed synthetic plan"});
    assert_eq!(
        learner
            .send("POST", path, Some(request.clone()), true)
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .send("POST", path, Some(request.clone()), false)
            .await
            .0,
        403
    );
    let mut wrong = request.clone();
    wrong["expectedPlanHash"] = json!("0".repeat(64));
    assert_eq!(operator.send("POST", path, Some(wrong), true).await.0, 409);
    let local_request = serde_json::from_value(request.clone()).unwrap();
    assert!(matches!(
        chef_engine::admin_speech_plans::save_local(&backend, learner_actor, local_request).await,
        Err(chef_engine::AppError::Forbidden)
    ));
    let local_saved = chef_engine::admin_speech_plans::save_local(
        &backend,
        actor,
        serde_json::from_value(request.clone()).unwrap(),
    )
    .await
    .unwrap();
    let local_retry = chef_engine::admin_speech_plans::save_local(
        &backend,
        actor,
        serde_json::from_value(request.clone()).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(
        serde_json::to_value(&local_saved).unwrap(),
        serde_json::to_value(local_retry).unwrap()
    );
    let mut request = request;
    request["reason"] = json!("[local-cli] Fixed synthetic plan");
    let saved = operator
        .send("POST", path, Some(request.clone()), true)
        .await;
    assert_eq!(saved.0, 200);
    assert_eq!(saved.1, serde_json::to_value(local_saved).unwrap());
    assert_eq!(saved.1["id"], id);
    assert_eq!(
        operator
            .send("POST", path, Some(request.clone()), true)
            .await
            .1,
        saved.1
    );
    assert_eq!(
        second
            .send("POST", path, Some(request.clone()), true)
            .await
            .0,
        409
    );
    let mut changed = request.clone();
    changed["reason"] = json!("different");
    assert_eq!(
        operator.send("POST", path, Some(changed), true).await.0,
        409
    );
    assert_eq!(
        visitor
            .send("GET", &format!("{path}/{id}"), None, true)
            .await
            .0,
        401
    );
    // Appending a newer voice never changes the persisted old plan or idempotent receipt.
    assert_eq!(operator.send("POST","/api/v1/operator/characters",Some(json!({"characterId":"character-camille","characterRevision":1,"expectedVoiceRevision":1,"profile":seed["items"][0]["profile"],"reason":"New synthetic version"})),true).await.0,200);
    assert_eq!(
        operator
            .send("GET", &format!("{path}/{id}"), None, true)
            .await
            .1,
        saved.1
    );
    assert_eq!(
        operator.send("POST", path, Some(request), true).await.1,
        saved.1
    );
    let list = operator
        .send(
            "GET",
            &format!("{path}?lessonId={lesson_id}&lessonRevision={revision}"),
            None,
            true,
        )
        .await;
    assert_eq!(list.1["items"].as_array().unwrap().len(), 1);
    assert!(
        db.execute_unprepared("UPDATE course_speech_plans SET reason='rewrite'")
            .await
            .is_err()
    );
    let row = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT actor_id,reason,plan->>'planHash' AS hash FROM course_speech_plans",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.try_get::<String>("", "reason").unwrap(),
        "[local-cli] Fixed synthetic plan"
    );
    assert_eq!(
        row.try_get::<String>("", "hash").unwrap(),
        preview.1["planHash"].as_str().unwrap()
    );
    // Course clip attempts charge once, preserve exact retry identity and reuse validated bytes.
    let clips = "/api/v1/operator/speech-clips";
    let key = saved.1["targets"][0]["generationKey"].clone();
    let first_id = "ccccccccccccccccccccccccccccccc1";
    let clip_request = json!({"id":first_id,"planId":id,"generationKey":key,"expectedPlanHash":saved.1["planHash"],"expectedPreviousId":null,"costConfirmed":true,"retryUnknownConfirmed":false,"reason":"Synthetic paid course task"});
    assert_eq!(
        visitor
            .send("POST", clips, Some(clip_request.clone()), true)
            .await
            .0,
        401
    );
    assert_eq!(
        learner
            .send("POST", clips, Some(clip_request.clone()), true)
            .await
            .0,
        403
    );
    let started = operator
        .send("POST", clips, Some(clip_request.clone()), true)
        .await;
    assert_eq!(started.0, 200, "{:?}", started.1);
    assert_eq!(started.1["status"], "submitted");
    let mut contender = clip_request.clone();
    contender["id"] = json!("ccccccccccccccccccccccccccccccc2");
    assert_eq!(
        second
            .send("POST", clips, Some(contender.clone()), true)
            .await
            .0,
        409
    );
    let clip_path = format!("{clips}/{first_id}");
    let ready = settled(&mut operator, &clip_path).await;
    assert_eq!(ready["status"], "ready");
    assert_eq!(
        visitor
            .send("GET", &format!("{clip_path}/file"), None, true)
            .await
            .0,
        401
    );
    assert_eq!(
        learner
            .send("GET", &format!("{clip_path}/file"), None, true)
            .await
            .0,
        403
    );
    let file_response = operator
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("{clip_path}/file"))
                .header("cookie", &operator.cookie)
                .header("range", "bytes=0-15")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(file_response.status().as_u16(), 206);
    assert_eq!(
        file_response.headers()["cache-control"],
        "private, no-store"
    );
    assert_eq!(file_response.headers()["content-type"], "audio/wav");
    let bytes = file_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    assert_eq!(bytes.len(), 16);
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(qwen.calls.lock().unwrap().len(), 1);
    assert_eq!(
        operator
            .send("POST", clips, Some(clip_request.clone()), true)
            .await
            .1,
        ready
    );
    assert_eq!(
        second
            .send("POST", clips, Some(clip_request.clone()), true)
            .await
            .0,
        409
    );
    let mut changed = clip_request.clone();
    changed["reason"] = json!("Different bill");
    assert_eq!(
        operator.send("POST", clips, Some(changed), true).await.0,
        409
    );
    contender["expectedPreviousId"] = json!(first_id);
    contender["costConfirmed"] = json!(false);
    // Trusted local entry uses the same cache verification and immutable request boundary.
    assert!(matches!(
        chef_engine::speech_clips::submit_local(
            backend.clone(),
            learner_actor,
            None,
            root.clone(),
            serde_json::from_value(contender.clone()).unwrap()
        )
        .await,
        Err(chef_engine::AppError::Forbidden)
    ));
    let local_reused = chef_engine::speech_clips::submit_local(
        backend.clone(),
        actor,
        None,
        root.clone(),
        serde_json::from_value(contender.clone()).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(local_reused.status, "ready");
    assert_eq!(local_reused.accepted, None);
    let local_retry = chef_engine::speech_clips::submit_local(
        backend.clone(),
        actor,
        None,
        root.clone(),
        serde_json::from_value(contender.clone()).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(
        serde_json::to_value(&local_reused).unwrap(),
        serde_json::to_value(local_retry).unwrap()
    );
    let mut local_changed = contender.clone();
    local_changed["reason"] = json!("changed local attempt");
    assert!(matches!(
        chef_engine::speech_clips::submit_local(
            backend.clone(),
            actor,
            None,
            root.clone(),
            serde_json::from_value(local_changed).unwrap()
        )
        .await,
        Err(chef_engine::AppError::Conflict)
    ));
    contender["reason"] = json!("[local-cli] Synthetic paid course task");
    let reused = operator
        .send("POST", clips, Some(contender.clone()), true)
        .await;
    assert_eq!(reused.0, 200, "{:?}", reused.1);
    assert_eq!(reused.1, serde_json::to_value(local_reused).unwrap());
    assert_eq!(reused.1["status"], "ready");
    assert_eq!(reused.1["reusedFrom"], first_id);
    assert_eq!(qwen.calls.lock().unwrap().len(), 1);
    assert_eq!(
        operator
            .send("POST", clips, Some(contender.clone()), true)
            .await
            .1,
        reused.1
    );
    let review_path = format!("{clips}/ccccccccccccccccccccccccccccccc2/review");
    let review = json!({"heard":true,"accepted":false,"reason":"Synthetic rejection, no real sound approval"});
    let mut unheard = review.clone();
    unheard["heard"] = json!(false);
    assert_eq!(
        operator
            .send("POST", &review_path, Some(unheard), true)
            .await
            .0,
        400
    );
    assert_eq!(
        operator
            .send("POST", &review_path, Some(review.clone()), true)
            .await
            .0,
        200
    );
    assert_eq!(
        operator
            .send("POST", &review_path, Some(review), true)
            .await
            .0,
        200
    );
    let mut retry = clip_request.clone();
    retry["id"] = json!("ccccccccccccccccccccccccccccccc3");
    retry["expectedPreviousId"] = contender["id"].clone();
    qwen.unknown
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let local_unknown = chef_engine::speech_clips::submit_local(
        backend.clone(),
        actor,
        Some(chef_engine::qwen::Service::new(qwen.clone(), "https://example.test").unwrap()),
        root.clone(),
        serde_json::from_value(retry.clone()).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(local_unknown.status, "unknown");
    retry["reason"] = json!("[local-cli] Synthetic paid course task");
    let unknown = settled(
        &mut operator,
        &format!("{clips}/ccccccccccccccccccccccccccccccc3"),
    )
    .await;
    assert_eq!(unknown["status"], "unknown");
    assert_eq!(unknown, serde_json::to_value(local_unknown).unwrap());
    assert_eq!(
        operator
            .send("POST", clips, Some(retry.clone()), true)
            .await
            .1,
        unknown
    );
    assert_eq!(qwen.calls.lock().unwrap().len(), 2);
    let mut new_retry = retry.clone();
    new_retry["id"] = json!("ccccccccccccccccccccccccccccccc4");
    new_retry["expectedPreviousId"] = retry["id"].clone();
    assert_eq!(
        operator
            .send("POST", clips, Some(new_retry.clone()), true)
            .await
            .0,
        409
    );
    new_retry["retryUnknownConfirmed"] = json!(true);
    qwen.unknown
        .store(false, std::sync::atomic::Ordering::SeqCst);
    // Explicit new attempt after unknown may charge; completing it never accepts hearing.
    new_retry["reason"] = json!("Synthetic paid course task");
    let local_ready = chef_engine::speech_clips::submit_local(
        backend.clone(),
        actor,
        Some(chef_engine::qwen::Service::new(qwen.clone(), "https://example.test").unwrap()),
        root.clone(),
        serde_json::from_value(new_retry.clone()).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(local_ready.status, "ready");
    assert_eq!(local_ready.accepted, None);
    assert_eq!(
        serde_json::to_value(&local_ready).unwrap(),
        serde_json::to_value(
            chef_engine::speech_clips::submit_local(
                backend.clone(),
                actor,
                None,
                root.clone(),
                serde_json::from_value(new_retry.clone()).unwrap()
            )
            .await
            .unwrap()
        )
        .unwrap()
    );
    new_retry["reason"] = json!("[local-cli] Synthetic paid course task");
    assert_eq!(
        operator.send("POST", clips, Some(new_retry), true).await.1,
        serde_json::to_value(local_ready).unwrap()
    );
    assert_eq!(
        settled(
            &mut operator,
            &format!("{clips}/ccccccccccccccccccccccccccccccc4")
        )
        .await["status"],
        "ready"
    );
    assert_eq!(qwen.calls.lock().unwrap().len(), 3);
    let latest = operator
        .send("GET", &format!("{path}/{id}/clips"), None, true)
        .await;
    assert_eq!(latest.0, 200, "{:?}", latest.1);
    assert_eq!(latest.1["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        latest.1["items"][0]["id"],
        "ccccccccccccccccccccccccccccccc4"
    );
    assert!(
        db.execute_unprepared("DELETE FROM course_speech_clips")
            .await
            .is_err()
    );
    // Competing administrators cannot both charge the same fresh generation key.
    let mut parallel_a = clip_request.clone();
    parallel_a["id"] = json!("ccccccccccccccccccccccccccccccc5");
    parallel_a["generationKey"] = saved.1["targets"][1]["generationKey"].clone();
    let mut parallel_b = parallel_a.clone();
    parallel_b["id"] = json!("ccccccccccccccccccccccccccccccc6");
    let (a, b) = tokio::join!(
        operator.send("POST", clips, Some(parallel_a), true),
        second.send("POST", clips, Some(parallel_b), true)
    );
    let mut statuses = [a.0, b.0];
    statuses.sort();
    assert_eq!(statuses, [200, 409]);
    let winner = if a.0 == 200 { a.1 } else { b.1 };
    assert_eq!(
        settled(
            &mut operator,
            &format!("{clips}/{}", winner["id"].as_str().unwrap())
        )
        .await["status"],
        "ready"
    );
    assert_eq!(qwen.calls.lock().unwrap().len(), 4);
    // Export is private, requires every distinct request's latest accepted output,
    // and contains the fixed plan, real reviewer and verified content-addressed binaries.
    let export_path = format!("{path}/{id}/export");
    assert_eq!(visitor.send("GET", &export_path, None, true).await.0, 401);
    assert_eq!(learner.send("GET", &export_path, None, true).await.0, 403);
    assert_eq!(operator.send("GET", &export_path, None, true).await.0, 409);
    let keys: std::collections::BTreeSet<String> = saved.1["targets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["generationKey"].as_str().unwrap().to_owned())
        .collect();
    for (index, generation_key) in keys.iter().enumerate() {
        let existing = operator
            .send("GET", &format!("{path}/{id}/clips"), None, true)
            .await
            .1;
        let current = existing["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["generationKey"] == *generation_key)
            .cloned();
        let ready = if let Some(c) = current {
            c
        } else {
            let mut body = clip_request.clone();
            let attempt_id = format!("{index:032x}");
            body["id"] = json!(attempt_id);
            body["generationKey"] = json!(generation_key);
            assert_eq!(operator.send("POST", clips, Some(body), true).await.0, 200);
            settled(&mut operator, &format!("{clips}/{attempt_id}")).await
        };
        if index + 1 == keys.len() {
            // The final ready clip has no hearing decision yet. Direct input
            // delivery verifies media without inventing one, while legacy export stays gated.
            assert_eq!(operator.send("GET", &export_path, None, true).await.0, 409);
            let direct = chef_engine::speech_export::export_direct_for_actor(
                &backend,
                actor,
                id.to_owned(),
                root.clone(),
            )
            .await
            .unwrap();
            let mut input = tar::Archive::new(std::io::Cursor::new(&direct));
            let mut document = None;
            for entry in input.entries().unwrap() {
                use std::io::Read;
                let mut entry = entry.unwrap();
                if entry.path().unwrap().as_ref() == std::path::Path::new("manifest.json") {
                    let mut data = Vec::new();
                    entry.read_to_end(&mut data).unwrap();
                    document = Some(serde_json::from_slice::<Value>(&data).unwrap());
                }
            }
            let document = document.unwrap();
            assert_eq!(document["publicationPolicy"], "owner-direct-publish");
            assert_eq!(document["humanListeningAsserted"], false);
            assert!(
                document["clips"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|c| c["review"].is_null())
            );
            assert_eq!(
                settled(
                    &mut operator,
                    &format!("{clips}/{}", ready["id"].as_str().unwrap())
                )
                .await["accepted"],
                Value::Null
            );
            assert!(
                chef_engine::speech_export::export_direct_for_actor(
                    &backend,
                    -1,
                    id.to_owned(),
                    root.clone()
                )
                .await
                .is_err()
            );
        }
        let review =
            json!({"heard":true,"accepted":true,"reason":"Synthetic export fixture review"});
        let clip_id = ready["id"].as_str().unwrap().to_owned();
        assert!(
            chef_engine::speech_clips::review_local(
                &backend,
                -1,
                clip_id.clone(),
                serde_json::from_value(review.clone()).unwrap(),
            )
            .await
            .is_err()
        );
        let accepted = chef_engine::speech_clips::review_local(
            &backend,
            actor,
            clip_id.clone(),
            serde_json::from_value(review.clone()).unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(accepted.accepted, Some(true));
        assert_eq!(
            chef_engine::speech_clips::review_local(
                &backend,
                actor,
                clip_id.clone(),
                serde_json::from_value(review.clone()).unwrap(),
            )
            .await
            .unwrap()
            .id,
            accepted.id
        );
        let mut http_review = review;
        http_review["reason"] = json!("[local-cli] Synthetic export fixture review");
        assert_eq!(
            operator
                .send(
                    "POST",
                    &format!("{clips}/{clip_id}/review"),
                    Some(http_review),
                    true
                )
                .await
                .0,
            200
        );
    }
    let response = operator
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&export_path)
                .header("cookie", &operator.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["content-type"], "application/x-tar");
    assert_eq!(response.headers()["cache-control"], "private, no-store");
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let local_export =
        chef_engine::speech_export::export_for_actor(&backend, actor, id.to_owned(), root.clone())
            .await
            .unwrap();
    assert_eq!(local_export.as_slice(), bytes.as_ref());
    assert!(
        chef_engine::speech_export::export_for_actor(&backend, -1, id.to_owned(), root.clone())
            .await
            .is_err()
    );
    let source_archive_hash = {
        use sha2::Digest;
        format!("{:x}", sha2::Sha256::digest(&bytes))
    };
    let mut archive = tar::Archive::new(std::io::Cursor::new(bytes));
    let mut members = std::collections::BTreeMap::new();
    for entry in archive.entries().unwrap() {
        use std::io::Read;
        let mut entry = entry.unwrap();
        assert!(entry.header().entry_type().is_file());
        assert_eq!(entry.header().mode().unwrap(), 0o600);
        let name = entry.path().unwrap().to_str().unwrap().to_owned();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        assert!(members.insert(name, data).is_none());
    }
    let manifest: Value = serde_json::from_slice(&members["manifest.json"]).unwrap();
    assert_eq!(manifest["plan"]["planHash"], saved.1["planHash"]);
    assert_eq!(manifest["clips"].as_array().unwrap().len(), keys.len());
    for clip in manifest["clips"].as_array().unwrap() {
        assert!(clip["review"]["actorId"].as_i64().unwrap() > 0);
        assert_eq!(
            clip["review"]["reason"],
            "[local-cli] Synthetic export fixture review"
        );
        for (file, hash) in [("file", "sha256"), ("providerFile", "providerSha256")] {
            let name = clip[file].as_str().unwrap();
            assert_eq!(
                name,
                format!("media/{}.wav", clip["result"][hash].as_str().unwrap())
            );
            assert_eq!(
                {
                    use sha2::Digest;
                    format!("{:x}", sha2::Sha256::digest(&members[name]))
                },
                clip["result"][hash]
            );
        }
        let key = clip["generationKey"].as_str().unwrap();
        let raw = db
            .query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT plan FROM course_speech_plans LIMIT 1",
            ))
            .await
            .unwrap()
            .unwrap();
        let fixed: Value = raw.try_get("", "plan").unwrap();
        let text = fixed["requests"][key]["parameters"]["input"]["text"]
            .as_str()
            .unwrap();
        for word in clip["words"].as_array().unwrap() {
            let start = word["start"].as_u64().unwrap() as usize;
            let end = word["end"].as_u64().unwrap() as usize;
            assert_eq!(
                text.chars()
                    .skip(start)
                    .take(end - start)
                    .collect::<String>(),
                word["text"]
            );
        }
    }
    // Synthetic alignment predictions are compared with real fixed plan/clip identities.
    let model: Value =
        serde_json::from_str(include_str!("../../../scripts/alignment/model.json")).unwrap();
    let runtime: Value =
        serde_json::from_str(include_str!("../../../scripts/alignment/runtime.json")).unwrap();
    let engine = json!({"repository":model["repository"],"revision":model["revision"],"files":model["files"],"versions":runtime,"device":"cpu","dtype":"float32","attention":"eager","transcript":"NFC source word units, apostrophes normalized; original scalar ranges retained; raw timestamp classes without interpolation"});
    let report_clips:Vec<_>=manifest["clips"].as_array().unwrap().iter().map(|clip| {
        let key=clip["generationKey"].as_str().unwrap();
        let words:Vec<_>=clip["words"].as_array().unwrap().iter().enumerate().map(|(i,w)|{
            let mut w=w.clone(); w["startMs"]=json!(i);w["endMs"]=json!(i+1);w
        }).collect();
        let raw:Vec<_>=words.iter().map(|w|json!({"text":w["text"],"startSeconds":w["startMs"].as_u64().unwrap() as f64/1000.0,"endSeconds":w["endMs"].as_u64().unwrap() as f64/1000.0})).collect();
        let targets:Vec<_>=manifest["plan"]["targets"].as_array().unwrap().iter().filter(|t|t["generationKey"]==key).map(|t|json!({"pointer":t["pointer"],"blockId":t["blockId"],"entryId":t["entryId"],"words":[],"issues":[]})).collect();
        json!({"clipId":clip["id"],"generationKey":key,"sha256":clip["result"]["sha256"],"durationMs":clip["result"]["durationMs"],"rawPredictions":raw,"words":words,"issues":[],"targets":targets})
    }).collect();
    let mut report = json!({"schemaVersion":"1.0","kind":"brioche-alignment-predictions","planId":id,"planHash":saved.1["planHash"],"sourceArchiveSha256":source_archive_hash,"engine":engine,"reviewRequired":true,"clips":report_clips});
    // Automatic assembly is independent of human alignment decisions. These
    // timestamps are synthetic protocol data, never production audio alignment.
    let direct_input = chef_engine::speech_export::export_direct_for_actor(
        &backend,
        actor,
        id.to_owned(),
        root.clone(),
    )
    .await
    .unwrap();
    let mut automatic = report.clone();
    automatic["kind"] = json!("brioche-automatic-alignment-v1");
    automatic["reviewRequired"] = json!(false);
    automatic["humanListeningAsserted"] = json!(false);
    automatic["originalPredictionReportSha256"] = json!("b".repeat(64));
    let direct_archive_hash = {
        use sha2::Digest;
        format!("{:x}", sha2::Sha256::digest(&direct_input))
    };
    automatic["sourceArchiveSha256"] = json!(direct_archive_hash);
    for clip in automatic["clips"].as_array_mut().unwrap() {
        let words = clip["words"].clone();
        for target in clip["targets"].as_array_mut().unwrap() {
            let original = manifest["plan"]["targets"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["pointer"] == target["pointer"])
                .unwrap();
            target["words"] = json!(
                original["words"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|unit| {
                        let word = words
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|w| {
                                w["start"] == unit["entryStart"]
                                    && w["end"] == unit["entryEnd"]
                                    && w["text"] == unit["text"]
                            })
                            .unwrap();
                        let mut unit = unit.clone();
                        unit["startMs"] = word["startMs"].clone();
                        unit["endMs"] = word["endMs"].clone();
                        unit
                    })
                    .collect::<Vec<_>>()
            );
        }
    }
    let automatic_request = |report: &Value| {
        use sha2::Digest;
        brioche_course_contract::AdminSpeechPackageRequest {
            expected_report_hash: format!(
                "{:x}",
                sha2::Sha256::digest(serde_json::to_vec(report).unwrap())
            ),
            lesson_revision: revision + 1,
            gap_ms: 250,
            rights_confirmed: true,
            source: "Synthetic fixed protocol audio".into(),
            license: "LicenseRef-TestOnly".into(),
            creator: "Brioche test".into(),
            credit_zh: "测试录音".into(),
            reason: "Synthetic owner authorization; no human hearing claimed".into(),
        }
    };
    let automatic_bytes = chef_engine::speech_automatic::assemble_for_actor(
        &backend,
        actor,
        root.clone(),
        automatic.clone(),
        automatic_request(&automatic),
    )
    .await
    .unwrap();
    let mut automatic_archive = tar::Archive::new(std::io::Cursor::new(automatic_bytes));
    let mut automatic_files = std::collections::BTreeMap::new();
    for entry in automatic_archive.entries().unwrap() {
        use std::io::Read;
        let mut entry = entry.unwrap();
        let name = entry.path().unwrap().to_str().unwrap().to_owned();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        automatic_files.insert(name, data);
    }
    let automatic_source: Value = serde_json::from_slice(&automatic_files["lesson.json"]).unwrap();
    let automatic_manifest: Value =
        serde_json::from_slice(&automatic_files["manifest.json"]).unwrap();
    assert_eq!(automatic_source["editorial"]["status"], "reviewed");
    assert_eq!(
        automatic_manifest["assembly"]["humanListeningAsserted"],
        false
    );
    assert_eq!(automatic_manifest["assembly"]["approvalRequired"], false);
    assert_eq!(
        automatic_manifest["assembly"]["finalListeningRequired"],
        false
    );
    assert_eq!(automatic_manifest["automaticAlignment"], automatic);
    assert!(
        chef_engine::speech_automatic::assemble_for_actor(
            &backend,
            -1,
            root.clone(),
            automatic.clone(),
            automatic_request(&automatic)
        )
        .await
        .is_err()
    );
    for case in 0..4 {
        let mut invalid = automatic.clone();
        match case {
            0 => invalid["clips"][0]["issues"] = json!(["invalidTimeRange"]),
            1 => invalid["clips"][0]["clipId"] = json!("invalid-latest-clip"),
            2 => invalid["sourceArchiveSha256"] = json!("0".repeat(64)),
            3 => invalid["humanListeningAsserted"] = json!(true),
            _ => unreachable!(),
        }
        assert!(
            chef_engine::speech_automatic::assemble_for_actor(
                &backend,
                actor,
                root.clone(),
                invalid.clone(),
                automatic_request(&invalid)
            )
            .await
            .is_err(),
            "automatic case {case}"
        );
    }
    let corrected = report["clips"][0]["words"].clone();
    report["clips"][0]["words"] = json!([]);
    report["clips"][0]["issues"] = json!(["invalidTimeRange"]);
    let alignment_id = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeee1";
    let alignment_path = "/api/v1/operator/speech-alignments";
    let import_request = json!({"id":alignment_id,"planId":id,"expectedPlanHash":saved.1["planHash"],"reportJson":serde_json::to_string(&report).unwrap(),"reason":"Synthetic alignment import, not a production hearing"});
    assert_eq!(
        visitor
            .send("POST", alignment_path, Some(import_request.clone()), true)
            .await
            .0,
        401
    );
    assert_eq!(
        learner
            .send("POST", alignment_path, Some(import_request.clone()), true)
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .send("POST", alignment_path, Some(import_request.clone()), false)
            .await
            .0,
        403
    );
    let local_import = chef_engine::speech_alignments::import_local(
        &backend,
        actor,
        root.clone(),
        serde_json::from_value(import_request.clone()).unwrap(),
    )
    .await
    .unwrap();
    assert!(
        local_import
            .clips
            .iter()
            .all(|clip| clip.accepted.is_none())
    );
    assert_eq!(
        chef_engine::speech_alignments::import_local(
            &backend,
            actor,
            root.clone(),
            serde_json::from_value(import_request.clone()).unwrap(),
        )
        .await
        .unwrap()
        .id,
        local_import.id
    );
    assert!(
        chef_engine::speech_alignments::import_local(
            &backend,
            -1,
            root.clone(),
            serde_json::from_value(import_request.clone()).unwrap(),
        )
        .await
        .is_err()
    );
    let mut import_request = import_request;
    import_request["reason"] =
        json!("[local-cli] Synthetic alignment import, not a production hearing");
    let imported_alignment = operator
        .send("POST", alignment_path, Some(import_request.clone()), true)
        .await;
    assert_eq!(imported_alignment.0, 200, "{:?}", imported_alignment.1);
    assert_eq!(
        imported_alignment.1["clips"][0]["words"][0]["startMs"],
        Value::Null
    );
    assert_eq!(
        operator
            .send("POST", alignment_path, Some(import_request.clone()), true)
            .await
            .1,
        imported_alignment.1
    );
    assert_eq!(
        second
            .send("POST", alignment_path, Some(import_request.clone()), true)
            .await
            .0,
        409
    );
    let mut changed = import_request.clone();
    changed["reason"] = json!("Different immutable body");
    assert_eq!(
        operator
            .send("POST", alignment_path, Some(changed), true)
            .await
            .0,
        409
    );
    for engine_change in [
        json!({"versions": {"qwen-asr":"0.0.6","torch":"2.10.0+cpu","transformers":"4.57.6","numpy":"2.5.3"}}),
        json!({"revision":"c7cbfc2048c462b0d63a45797104fc9db3ad62b7"}),
        json!({"transcript":"NFC source word units, apostrophes normalized; original scalar ranges retained"}),
    ] {
        let mut incompatible = report.clone();
        for (key, value) in engine_change.as_object().unwrap() {
            incompatible["engine"][key] = value.clone();
        }
        let mut request = import_request.clone();
        request["id"] = json!("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeee9");
        request["reportJson"] = json!(serde_json::to_string(&incompatible).unwrap());
        assert_eq!(
            operator
                .send("POST", alignment_path, Some(request), true)
                .await
                .0,
            400
        );
    }
    let mut corrupt_report = report.clone();
    corrupt_report["sourceArchiveSha256"] = json!("f".repeat(64));
    let mut invalid_archive = import_request.clone();
    invalid_archive["id"] = json!("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeea");
    invalid_archive["reportJson"] = json!(serde_json::to_string(&corrupt_report).unwrap());
    assert_eq!(
        operator
            .send("POST", alignment_path, Some(invalid_archive), true)
            .await
            .0,
        409
    );
    let mut duplicate_json = import_request.clone();
    duplicate_json["id"] = json!("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeb");
    duplicate_json["reportJson"] = json!(format!(
        "{{\"schemaVersion\":\"1.0\",{}",
        &serde_json::to_string(&report).unwrap()[1..]
    ));
    assert_eq!(
        operator
            .send("POST", alignment_path, Some(duplicate_json), true)
            .await
            .0,
        400
    );
    let mut corrupt_report = report.clone();
    corrupt_report["clips"][0]["sha256"] = json!("f".repeat(64));
    let mut invalid_import = import_request.clone();
    invalid_import["id"] = json!("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeee2");
    invalid_import["reportJson"] = json!(serde_json::to_string(&corrupt_report).unwrap());
    assert_eq!(
        operator
            .send("POST", alignment_path, Some(invalid_import), true)
            .await
            .0,
        409
    );
    let list_path = format!("{path}/{id}/alignments");
    let listing = operator.send("GET", &list_path, None, true).await;
    assert_eq!(listing.0, 200, "{:?}", listing.1);
    assert_eq!(listing.1["items"].as_array().unwrap().len(), 1);
    assert!(listing.1["items"][0].get("report").is_none());
    let read_alignment = format!("{alignment_path}/{alignment_id}");
    assert_eq!(
        learner.send("GET", &read_alignment, None, true).await.0,
        403
    );
    let alignment_clip = report["clips"][0]["clipId"].as_str().unwrap();
    let review_path = format!("{read_alignment}/clips/{alignment_clip}/review");
    let mut decision = json!({"expectedReportHash":imported_alignment.1["reportHash"],"heard":true,"timingsChecked":true,"accepted":true,"words":corrected,"reason":"Synthetic timing correction fixture"});
    decision["heard"] = json!(false);
    assert_eq!(
        operator
            .send("POST", &review_path, Some(decision.clone()), true)
            .await
            .0,
        400
    );
    decision["heard"] = json!(true);
    decision["timingsChecked"] = json!(false);
    assert_eq!(
        operator
            .send("POST", &review_path, Some(decision.clone()), true)
            .await
            .0,
        400
    );
    decision["timingsChecked"] = json!(true);
    let mut overlap = decision.clone();
    overlap["words"][0]["endMs"] = json!(999);
    assert_eq!(
        operator
            .send("POST", &review_path, Some(overlap), true)
            .await
            .0,
        400
    );
    let accepted = operator
        .send("POST", &review_path, Some(decision.clone()), true)
        .await;
    assert_eq!(accepted.0, 200, "{:?}", accepted.1);
    assert_eq!(accepted.1["clips"][0]["accepted"], true);
    assert_eq!(accepted.1["clips"][0]["words"], decision["words"]);
    assert_eq!(
        operator
            .send("POST", &review_path, Some(decision.clone()), true)
            .await
            .1,
        accepted.1
    );
    assert_eq!(
        second
            .send("POST", &review_path, Some(decision.clone()), true)
            .await
            .0,
        409
    );
    decision["reason"] = json!("Cannot overwrite timing review");
    assert_eq!(
        operator
            .send("POST", &review_path, Some(decision), true)
            .await
            .0,
        409
    );
    let package_path = format!("{read_alignment}/package");
    let mut package_request = json!({"expectedReportHash":imported_alignment.1["reportHash"],
        "lessonRevision":revision+1,"gapMs":250,"rightsConfirmed":true,
        "source":"Synthetic protocol recording","license":"Synthetic fixture permission only",
        "creator":"Isolated test","creditZh":"Synthetic fixture","reason":"Synthetic assembly test, not a real hearing"});
    assert_eq!(
        visitor
            .send("POST", &package_path, Some(package_request.clone()), true)
            .await
            .0,
        401
    );
    assert_eq!(
        learner
            .send("POST", &package_path, Some(package_request.clone()), true)
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .send("POST", &package_path, Some(package_request.clone()), false)
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .send("POST", &package_path, Some(package_request.clone()), true)
            .await
            .0,
        409
    );
    let mut invalid = package_request.clone();
    invalid["rightsConfirmed"] = json!(false);
    assert_eq!(
        operator
            .send("POST", &package_path, Some(invalid), true)
            .await
            .0,
        400
    );
    for clip in report["clips"].as_array().unwrap().iter().skip(1) {
        let request = json!({"expectedReportHash":imported_alignment.1["reportHash"],"heard":true,"timingsChecked":true,
            "accepted":true,"words":clip["words"],"reason":"Synthetic full-package timing fixture"});
        let endpoint = format!(
            "{read_alignment}/clips/{}/review",
            clip["clipId"].as_str().unwrap()
        );
        let result = operator.send("POST", &endpoint, Some(request), true).await;
        assert_eq!(result.0, 200, "{:?}", result.1);
    }
    let calls_before_package = qwen.calls.lock().unwrap().len();
    let mut package_bytes = None;
    for _ in 0..2 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&package_path)
                    .header("cookie", &operator.cookie)
                    .header("origin", "http://localhost:5173")
                    .header("x-csrf-token", &operator.csrf)
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&package_request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["content-type"], "application/x-tar");
        assert_eq!(response.headers()["cache-control"], "private, no-store");
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        if let Some(previous) = &package_bytes {
            assert_eq!(previous, &bytes);
        } else {
            package_bytes = Some(bytes);
        }
    }
    assert_eq!(qwen.calls.lock().unwrap().len(), calls_before_package);
    let mut package_members = std::collections::BTreeMap::new();
    let mut archive = tar::Archive::new(std::io::Cursor::new(package_bytes.unwrap()));
    let package_root = root.join("assembled-package");
    std::fs::create_dir_all(&package_root).unwrap();
    for entry in archive.entries().unwrap() {
        use std::io::Read;
        let mut entry = entry.unwrap();
        assert!(entry.header().entry_type().is_file());
        assert_eq!(entry.header().mode().unwrap(), 0o600);
        let path = entry.path().unwrap().to_path_buf();
        assert!(
            path.components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
        );
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        let destination = package_root.join(&path);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::write(destination, &bytes).unwrap();
        assert!(
            package_members
                .insert(path.to_str().unwrap().to_owned(), bytes)
                .is_none()
        );
    }
    let draft: Value = serde_json::from_slice(&package_members["lesson.json"]).unwrap();
    assert_eq!(draft["revision"], revision + 1);
    assert_eq!(draft["editorial"]["status"], "draft");
    let draft_public = chef_engine::project_source(draft.clone()).unwrap();
    draft_public.validate().unwrap();
    assert!(draft_public.audio_tracks.len() >= 2);
    assert!(
        draft_public
            .knowledge
            .vocabulary
            .iter()
            .all(|v| v.recording.is_some())
    );
    assert!(
        draft_public
            .knowledge
            .grammar
            .iter()
            .flat_map(|g| &g.examples)
            .all(|e| e.recording.is_some())
    );
    let package_manifest: Value =
        serde_json::from_slice(&package_members["manifest.json"]).unwrap();
    for track in &draft_public.audio_tracks {
        let asset = draft_public
            .audio
            .iter()
            .find(|a| a.asset_id == track.asset_id)
            .unwrap();
        let bytes = &package_members[&format!("recordings/{}.wav", asset.sha256)];
        assert_eq!(
            chef_engine::audio::inspect(bytes, "audio/wav")
                .unwrap()
                .duration_ms,
            asset.duration_ms
        );
        let entries: Vec<_> = track
            .cues
            .iter()
            .filter(|c| c.segment_id.is_none())
            .collect();
        assert_eq!(entries[0].start_ms, 0);
        for pair in entries.windows(2) {
            assert_eq!(pair[1].start_ms - pair[0].end_ms, 250);
        }
        for entry in entries {
            let target = package_manifest["plan"]["targets"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["blockId"] == track.block_id && t["entryId"] == entry.entry_id)
                .unwrap();
            let clip = package_manifest["clips"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["generationKey"] == target["generationKey"])
                .unwrap();
            for word in target["words"].as_array().unwrap() {
                let reviewed = clip["words"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|w| w["start"] == word["entryStart"] && w["end"] == word["entryEnd"])
                    .unwrap();
                let cue = track
                    .cues
                    .iter()
                    .find(|c| {
                        c.entry_id == entry.entry_id
                            && c.segment_id.as_deref() == word["segmentId"].as_str()
                            && c.word_range.as_ref().is_some_and(|r| {
                                Some(u64::from(r.start)) == word["segmentStart"].as_u64()
                                    && Some(u64::from(r.end)) == word["segmentEnd"].as_u64()
                            })
                    })
                    .unwrap();
                assert_eq!(
                    u64::from(cue.start_ms),
                    u64::from(entry.start_ms) + reviewed["startMs"].as_u64().unwrap()
                );
                assert_eq!(
                    u64::from(cue.end_ms),
                    u64::from(entry.start_ms) + reviewed["endMs"].as_u64().unwrap()
                );
            }
        }
    }
    assert!(package_manifest["assembly"]["actorId"].as_i64().unwrap() > 0);
    assert_eq!(package_manifest["assembly"]["publicationRequired"], true);
    for clip in package_manifest["clips"].as_array().unwrap() {
        assert!(clip["review"]["actorId"].as_i64().unwrap() > 0);
        assert!(clip["speechReview"]["actorId"].as_i64().unwrap() > 0);
        assert!(package_members.contains_key(clip["file"].as_str().unwrap()));
        assert!(package_members.contains_key(clip["providerFile"].as_str().unwrap()));
    }
    let bundle: chef_engine::recording::AudioBundle =
        serde_json::from_slice(&package_members["audio-bundle.json"]).unwrap();
    chef_engine::recording::check_bundle(&bundle, &package_root).unwrap();
    chef_engine::recording::import_bundle(
        &db,
        bundle,
        &package_root,
        &root,
        "isolated-assembly-test",
    )
    .await
    .unwrap();
    let imported = chef_engine::author_import::import(
        &db,
        draft,
        "isolated-assembly-test",
        "Synthetic assembled draft",
    )
    .await
    .unwrap();
    assert_eq!(imported.revision, revision + 1);
    for a in &draft_public.audio {
        assert_eq!(visitor.send("GET", &a.url, None, false).await.0, 404);
    }
    let fixed=db.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,
        "SELECT published,server_document->'editorial'->>'status' AS status FROM lesson_revisions WHERE lesson_id=$1 AND revision=$2",
        vec![lesson_id.clone().into(),(imported.revision as i32).into()])).await.unwrap().unwrap();
    assert!(!fixed.try_get::<bool>("", "published").unwrap());
    assert_eq!(fixed.try_get::<String>("", "status").unwrap(), "draft");
    assert_eq!(
        operator
            .send("POST", &package_path, Some(package_request.clone()), true)
            .await
            .0,
        409
    );
    package_request["lessonRevision"] = json!(revision + 2);
    let package_import_path = format!("{package_path}/import");
    let package_records_path = format!("{read_alignment}/packages");
    let mut package_import = json!({"id":"ab".repeat(16),"package":package_request});
    package_import["package"]["creditZh"] = json!("Synthetic atomic import attribution");
    assert_eq!(
        visitor
            .send(
                "POST",
                &package_import_path,
                Some(package_import.clone()),
                true
            )
            .await
            .0,
        401
    );
    assert_eq!(
        learner
            .send(
                "POST",
                &package_import_path,
                Some(package_import.clone()),
                true
            )
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .send(
                "POST",
                &package_import_path,
                Some(package_import.clone()),
                false
            )
            .await
            .0,
        403
    );
    assert_eq!(
        visitor
            .send("GET", &package_records_path, None, false)
            .await
            .0,
        401
    );
    assert_eq!(
        learner
            .send("GET", &package_records_path, None, false)
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .send(
                "GET",
                &format!("{package_records_path}?after=invalid"),
                None,
                false
            )
            .await
            .0,
        400
    );
    assert_eq!(
        operator
            .send("GET", &package_records_path, None, false)
            .await
            .1["items"],
        json!([])
    );
    let count_sql = "SELECT (SELECT count(*) FROM audio_assets) AS audio_count,(SELECT count(*) FROM audio_import_audit) AS audio_audits,(SELECT count(*) FROM lesson_import_audit) AS lesson_audits,(SELECT count(*) FROM speech_package_imports) AS packages";
    let counts_before = db
        .query_one_raw(Statement::from_string(DbBackend::Postgres, count_sql))
        .await
        .unwrap()
        .unwrap();
    // Fail after registration SQL to prove the shared transaction rolls back all
    // registry/audit rows when draft insertion fails. This is our isolated schema.
    db.execute_unprepared(&format!("CREATE FUNCTION reject_package_test() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.revision={} THEN RAISE EXCEPTION 'controlled package import failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_package_test BEFORE INSERT ON lesson_revisions FOR EACH ROW EXECUTE FUNCTION reject_package_test()",revision+2)).await.unwrap();
    assert_eq!(
        operator
            .send(
                "POST",
                &package_import_path,
                Some(package_import.clone()),
                true
            )
            .await
            .0,
        503
    );
    let counts_failed = db
        .query_one_raw(Statement::from_string(DbBackend::Postgres, count_sql))
        .await
        .unwrap()
        .unwrap();
    for column in ["audio_count", "audio_audits", "lesson_audits", "packages"] {
        assert_eq!(
            counts_before.try_get::<i64>("", column).unwrap(),
            counts_failed.try_get::<i64>("", column).unwrap()
        );
    }
    db.execute_unprepared(
        "DROP TRIGGER reject_package_test ON lesson_revisions; DROP FUNCTION reject_package_test()",
    )
    .await
    .unwrap();
    let mut concurrent_operator = Browser {
        app: app.clone(),
        cookie: operator.cookie.clone(),
        csrf: operator.csrf.clone(),
    };
    let (first, concurrent) = tokio::join!(
        concurrent_operator.send(
            "POST",
            &package_import_path,
            Some(package_import.clone()),
            true
        ),
        operator.send(
            "POST",
            &package_import_path,
            Some(package_import.clone()),
            true
        )
    );
    assert!([200, 429].contains(&first.0) && [200, 429].contains(&concurrent.0));
    assert!(first.0 == 200 || concurrent.0 == 200);
    let saved = operator
        .send(
            "POST",
            &package_import_path,
            Some(package_import.clone()),
            true,
        )
        .await;
    assert_eq!(saved.0, 200, "{:?}", saved.1);
    assert_eq!(saved.1["revision"], revision + 2);
    assert_eq!(saved.1["lessonId"], lesson_id);
    assert!(saved.1["recordingCount"].as_u64().unwrap() > 0);
    let replayed = operator
        .send(
            "POST",
            &package_import_path,
            Some(package_import.clone()),
            true,
        )
        .await;
    assert_eq!(saved, replayed);
    assert_eq!(
        second
            .send(
                "POST",
                &package_import_path,
                Some(package_import.clone()),
                true
            )
            .await
            .0,
        409
    );
    let counts_after = db
        .query_one_raw(Statement::from_string(DbBackend::Postgres, count_sql))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(counts_after.try_get::<i64>("", "packages").unwrap(), 1);
    assert_eq!(
        counts_after.try_get::<i64>("", "audio_audits").unwrap(),
        counts_before.try_get::<i64>("", "audio_audits").unwrap() + 1
    );
    assert_eq!(
        counts_after.try_get::<i64>("", "lesson_audits").unwrap(),
        counts_before.try_get::<i64>("", "lesson_audits").unwrap() + 1
    );
    let records = operator
        .send("GET", &package_records_path, None, false)
        .await;
    assert_eq!(records.0, 200);
    assert_eq!(records.1["items"], json!([saved.1]));
    assert!(
        !serde_json::to_string(&records.1)
            .unwrap()
            .contains("serverOnly")
    );
    let mut changed = package_import.clone();
    changed["package"]["reason"] = json!("Changed immutable request");
    assert_eq!(
        operator
            .send("POST", &package_import_path, Some(changed), true)
            .await
            .0,
        409
    );
    let fixed=db.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,
        "SELECT published,server_document->'editorial'->>'status' AS status FROM lesson_revisions WHERE lesson_id=$1 AND revision=$2",
        vec![lesson_id.clone().into(),((revision+2) as i32).into()])).await.unwrap().unwrap();
    assert!(!fixed.try_get::<bool>("", "published").unwrap());
    assert_eq!(fixed.try_get::<String>("", "status").unwrap(), "draft");
    assert!(
        db.execute_unprepared("UPDATE speech_package_imports SET reason='changed'")
            .await
            .is_err()
    );
    assert!(
        db.execute_unprepared("DELETE FROM speech_package_imports")
            .await
            .is_err()
    );
    for index in 0..21 {
        let mut extra = package_import.clone();
        extra["id"] = json!(format!("ad{index:030x}"));
        extra["package"]["lessonRevision"] = json!(revision + 3 + index);
        assert_eq!(
            operator
                .send("POST", &package_import_path, Some(extra), true)
                .await
                .0,
            200
        );
    }
    let page_one = operator
        .send("GET", &package_records_path, None, false)
        .await;
    assert_eq!(page_one.1["items"].as_array().unwrap().len(), 20);
    let cursor = page_one.1["next"].as_str().unwrap();
    let page_two = operator
        .send(
            "GET",
            &format!("{package_records_path}?after={cursor}"),
            None,
            false,
        )
        .await;
    assert_eq!(page_two.1["items"].as_array().unwrap().len(), 2);
    assert!(page_two.1["next"].is_null());
    let ids: std::collections::BTreeSet<_> = page_one.1["items"]
        .as_array()
        .unwrap()
        .iter()
        .chain(page_two.1["items"].as_array().unwrap())
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), 22);
    package_request["lessonRevision"] = json!(revision + 24);
    assert!(
        db.execute_unprepared("UPDATE speech_alignments SET reason='overwrite'")
            .await
            .is_err()
    );
    assert!(
        db.execute_unprepared("DELETE FROM speech_alignment_reviews")
            .await
            .is_err()
    );
    assert!(
        db.execute_unprepared("DO $$ BEGIN IF EXISTS(SELECT 1 FROM speech_package_imports) THEN RAISE EXCEPTION 'speech package audit must be retained'; END IF; END $$; DROP TABLE speech_package_imports;")
            .await
            .is_err()
    );
    let calls_after_export = qwen.calls.lock().unwrap().len();
    // A corrupt cached object fails closed rather than silently issuing another paid call.
    let raw=db.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT result->>'sha256' AS hash FROM course_speech_clip_events WHERE status='ready' LIMIT 1")).await.unwrap().unwrap();
    let hash = raw.try_get::<String>("", "hash").unwrap();
    assert!(hash.bytes().all(|b| b.is_ascii_hexdigit()) && hash.len() == 64);
    std::fs::write(root.join(format!("{hash}.wav")), b"corrupt").unwrap();
    let mut corrupt_retry = clip_request.clone();
    corrupt_retry["id"] = json!("ccccccccccccccccccccccccccccccc7");
    corrupt_retry["expectedPreviousId"] = json!("ccccccccccccccccccccccccccccccc4");
    assert_eq!(
        operator
            .send("POST", clips, Some(corrupt_retry), true)
            .await
            .0,
        503
    );
    assert_eq!(qwen.calls.lock().unwrap().len(), calls_after_export);
    assert_eq!(operator.send("GET", &export_path, None, true).await.0, 503);
    assert_eq!(
        operator
            .send("POST", &package_path, Some(package_request.clone()), true)
            .await
            .0,
        503
    );
    let fresh_package = json!({"id":"ac".repeat(16),"package":package_request});
    assert_eq!(
        operator
            .send(
                "POST",
                &package_import_path,
                Some(fresh_package.clone()),
                true
            )
            .await
            .0,
        503
    );
    assert_eq!(
        operator
            .send(
                "POST",
                &package_import_path,
                Some(package_import.clone()),
                true
            )
            .await,
        saved
    );
    let mut broken_import = import_request.clone();
    broken_import["id"] = json!("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeee3");
    assert_eq!(
        operator
            .send("POST", alignment_path, Some(broken_import), true)
            .await
            .0,
        503
    );
    // Real withdrawal hides existing plan text and prevents further preview/creation.
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO content_withdrawals(lesson_id,revision) VALUES($1,$2)",
        vec![lesson_id.clone().into(), (revision as i32).into()],
    ))
    .await
    .unwrap();
    assert_eq!(
        operator
            .send("POST", &package_import_path, Some(fresh_package), true)
            .await
            .0,
        404
    );
    assert_eq!(
        operator
            .send("POST", &package_import_path, Some(package_import), true)
            .await,
        saved
    );
    assert_eq!(
        operator
            .send("GET", &format!("{path}/{id}"), None, true)
            .await
            .0,
        404
    );
    assert_eq!(
        operator
            .send(
                "POST",
                &format!("{path}/preview"),
                Some(preview_request),
                true
            )
            .await
            .0,
        404
    );
    assert_eq!(operator.send("GET", &export_path, None, true).await.0, 404);
    assert_eq!(
        operator
            .send("POST", &package_path, Some(package_request), true)
            .await
            .0,
        404
    );
    assert_eq!(
        operator.send("GET", &read_alignment, None, true).await.0,
        404
    );
    assert_eq!(
        operator
            .send("POST", alignment_path, Some(import_request), true)
            .await
            .0,
        404
    );
    assert_eq!(operator.send("GET", &clip_path, None, true).await.0, 404);
    assert_eq!(
        operator
            .send("POST", clips, Some(clip_request), true)
            .await
            .0,
        404
    );
    db.close().await.unwrap();
    admin
        .execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .unwrap();
    std::fs::remove_dir_all(root).unwrap();
}
