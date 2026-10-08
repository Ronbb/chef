//! Real schema relocation and both service roles; only disposable test data.
use axum::{Router, body::Body, http::Request};
use chef_engine::{
    csrf::CsrfPolicy,
    identity::Backend,
    identity_service::{self, ServiceConfig},
    product::ProductId,
};
use http_body_util::BodyExt;
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
    TransactionTrait,
};
use sea_orm_migration::MigratorTrait;
use sha2::{Digest, Sha256};
use tower::ServiceExt;
#[path = "support/assets.rs"]
mod assets;
#[path = "support/product_content.rs"]
mod product_content;
#[path = "support/product_facts.rs"]
mod product_facts;
#[path = "support/product_recordings.rs"]
mod product_recordings;
#[path = "support/product_speech_work.rs"]
mod product_speech_work;
#[path = "support/product_visuals.rs"]
mod product_visuals;
#[path = "support/product_voice_work.rs"]
mod product_voice_work;
#[path = "support/product_voices.rs"]
mod product_voices;
mod support;
const KEY: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const ORIGIN: &str = "http://brioche.example.test";
struct ExportCli {
    db_url: String,
    schema: String,
    root: std::path::PathBuf,
    identity_url: String,
}
impl ExportCli {
    async fn run(
        &self,
        command: &str,
        id: &str,
        email: &str,
        session: &str,
        output: &str,
    ) -> std::process::Output {
        let db_url = self.db_url.clone();
        let schema = self.schema.clone();
        let root = self.root.clone();
        let identity_url = self.identity_url.clone();
        let args = [
            command.to_owned(),
            id.to_owned(),
            email.to_owned(),
            output.to_owned(),
        ];
        let session = session.to_owned();
        tokio::task::spawn_blocking(move || {
            let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_chef-server"));
            command
                .env_clear()
                .env("DATABASE_URL", db_url)
                .env("DATABASE_SCHEMA", schema)
                .env("CHEF_PRODUCT", "brioche")
                .env("PUBLIC_APP_URL", ORIGIN)
                .env("IDENTITY_INTERNAL_URL", identity_url)
                .env("IDENTITY_INTERNAL_KEY", KEY)
                .env("CHEF_OPERATOR_SESSION_FILE", root.join(session))
                .env("MEDIA_ROOT", &root)
                .current_dir(&root)
                .args(args);
            if let Ok(system) = std::env::var("SystemRoot") {
                command.env("SystemRoot", system);
            }
            command.output().unwrap()
        })
        .await
        .unwrap()
    }
}
#[derive(Default)]
struct EnrollmentFixture {
    creates: std::sync::atomic::AtomicUsize,
    queries: std::sync::atomic::AtomicUsize,
    syntheses: std::sync::atomic::AtomicUsize,
}
#[async_trait::async_trait]
impl chef_engine::qwen::Transport for EnrollmentFixture {
    async fn create(
        &self,
        prefix: &str,
        _: &str,
    ) -> Result<chef_engine::qwen::Receipt, chef_engine::qwen::ProviderError> {
        self.creates
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(chef_engine::qwen::Receipt {
            voice_id: format!("{}-{prefix}-fixture", chef_engine::qwen::MODEL),
            request_id: "fixture-create".into(),
        })
    }
    async fn query(
        &self,
        _: &str,
    ) -> Result<chef_engine::qwen::Details, chef_engine::qwen::ProviderError> {
        self.queries
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(chef_engine::qwen::Details {
            model: chef_engine::qwen::MODEL.into(),
            status: "OK".into(),
            request_id: "fixture-query".into(),
        })
    }
    async fn synthesize(
        &self,
        request: &chef_engine::qwen::SpeechRequest,
    ) -> Result<chef_engine::qwen::Speech, chef_engine::qwen::ProviderError> {
        self.syntheses
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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
            request_id: "fixture-synthesis".into(),
            input_tokens: None,
            output_tokens: None,
            verification: (request.profile.voice_kind == "cloned").then(|| {
                chef_engine::qwen::Details {
                    model: chef_engine::qwen::MODEL.into(),
                    status: "OK".into(),
                    request_id: "fixture-verification".into(),
                }
            }),
        })
    }
}
async fn history_pages(
    app: &Router,
    cookie: &mut String,
    csrf: &mut String,
) -> Vec<serde_json::Value> {
    let mut pages = Vec::new();
    let mut path = "/api/v1/operator/history".to_owned();
    for _ in 0..100 {
        let (status, page) = request(app, "GET", &path, None, cookie, csrf).await;
        assert_eq!(status, 200, "{page}");
        let next = page["next"].clone();
        pages.push(page);
        if next.is_null() {
            return pages;
        }
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("beforeTime", next["beforeTime"].as_str().unwrap())
            .append_pair("beforeKey", next["beforeKey"].as_str().unwrap())
            .finish();
        path = format!("/api/v1/operator/history?{query}");
    }
    panic!("History pagination did not terminate");
}
async fn settled_job(
    app: &Router,
    path: &str,
    cookie: &mut String,
    csrf: &mut String,
) -> serde_json::Value {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let (status, value) = request(app, "GET", path, None, cookie, csrf).await;
        assert_eq!(status, 200, "{value}");
        if !["submitted", "checking"].contains(&value["status"].as_str().unwrap()) {
            return value;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "Enrollment worker did not settle"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}
fn json_write(path: &str, document: &serde_json::Value, cookie: &str, csrf: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("origin", ORIGIN)
        .header("cookie", cookie)
        .header("x-csrf-token", csrf)
        .header("content-type", "application/json")
        .body(Body::from(document.to_string()))
        .unwrap()
}
fn asset_upload(id: &str, cookie: &str, csrf: &str) -> Request<Body> {
    let document = serde_json::json!({"assetId":id,"revision":1,"mimeType":"image/svg+xml","altZh":"测试图片","creditZh":"隔离测试","source":"test:svg","license":"LicenseRef-TestOnly","creator":"test fixture","rightsConfirmed":true,"reason":"Independent asset upload"});
    let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 96 96\"><rect width=\"96\" height=\"96\" fill=\"red\"/></svg>";
    Request::builder().method("POST").uri("/api/v1/operator/assets")
        .header("origin", ORIGIN).header("cookie", cookie).header("x-csrf-token",csrf)
        .header("content-type","multipart/form-data; boundary=chef-test-upload")
        .body(Body::from(format!("--chef-test-upload\r\nContent-Disposition: form-data; name=\"document\"\r\n\r\n{document}\r\n--chef-test-upload\r\nContent-Disposition: form-data; name=\"file\"; filename=\"image.svg\"\r\nContent-Type: image/svg+xml\r\n\r\n{svg}\r\n--chef-test-upload--\r\n"))).unwrap()
}
fn recording_upload(id: &str, cookie: &str, csrf: &str) -> Request<Body> {
    let document = serde_json::json!({"assetId":id,"revision":1,"mimeType":"audio/mpeg","creditZh":"隔离测试","source":"test:synthetic-audio","license":"LicenseRef-TestOnly","creator":"test fixture","rightsConfirmed":true,"reason":"Independent recording upload"});
    let mut body = format!("--chef-test-recording\r\nContent-Disposition: form-data; name=\"document\"\r\n\r\n{document}\r\n--chef-test-recording\r\nContent-Disposition: form-data; name=\"file\"; filename=\"recording.mp3\"\r\nContent-Type: audio/mpeg\r\n\r\n").into_bytes();
    body.extend_from_slice(include_bytes!("fixtures/audio/synthetic.mp3"));
    body.extend_from_slice(b"\r\n--chef-test-recording--\r\n");
    Request::builder()
        .method("POST")
        .uri("/api/v1/operator/recordings")
        .header("origin", ORIGIN)
        .header("cookie", cookie)
        .header("x-csrf-token", csrf)
        .header(
            "content-type",
            "multipart/form-data; boundary=chef-test-recording",
        )
        .body(Body::from(body))
        .unwrap()
}
fn reference_upload(cookie: &str, csrf: &str) -> Request<Body> {
    let document = serde_json::json!({"assetId":"split-reference","revision":1,"mimeType":"audio/wav","creditZh":"合成测试","source":"test:synthetic","license":"LicenseRef-TestOnly","creator":"test fixture","rightsConfirmed":true,"reason":"Independent reference file"});
    let mut wav = vec![0u8; 160044];
    wav[..4].copy_from_slice(b"RIFF");
    wav[4..8].copy_from_slice(&160036u32.to_le_bytes());
    wav[8..16].copy_from_slice(b"WAVEfmt ");
    wav[16..20].copy_from_slice(&16u32.to_le_bytes());
    wav[20..24].copy_from_slice(&[1, 0, 1, 0]);
    wav[24..28].copy_from_slice(&16000u32.to_le_bytes());
    wav[28..32].copy_from_slice(&32000u32.to_le_bytes());
    wav[32..36].copy_from_slice(&[2, 0, 16, 0]);
    wav[36..40].copy_from_slice(b"data");
    wav[40..44].copy_from_slice(&160000u32.to_le_bytes());
    let mut body=format!("--chef-reference\r\nContent-Disposition: form-data; name=\"document\"\r\n\r\n{document}\r\n--chef-reference\r\nContent-Disposition: form-data; name=\"file\"; filename=\"reference.wav\"\r\nContent-Type: audio/wav\r\n\r\n").into_bytes();
    body.extend_from_slice(&wav);
    body.extend_from_slice(b"\r\n--chef-reference--\r\n");
    Request::builder()
        .method("POST")
        .uri("/api/v1/operator/recordings")
        .header("origin", ORIGIN)
        .header("cookie", cookie)
        .header("x-csrf-token", csrf)
        .header(
            "content-type",
            "multipart/form-data; boundary=chef-reference",
        )
        .body(Body::from(body))
        .unwrap()
}
const TABLES: [&str; 7] = [
    "users",
    "browser_sessions",
    "identity_tokens",
    "auth_throttle",
    "product_memberships",
    "product_membership_audit",
    "account_admin_audit",
];

async fn connect(url: &str, schema: &str) -> DatabaseConnection {
    let mut options = ConnectOptions::new(url);
    chef_engine::database_scope::apply(&mut options, Some(schema)).unwrap();
    options.sqlx_logging(false);
    Database::connect(options).await.unwrap()
}
fn app(db: &DatabaseConnection) -> impl std::future::Future<Output = Router> {
    let db = db.clone();
    async move {
        identity_service::router(
            Backend::new(db).await.unwrap(),
            CsrfPolicy::new([ORIGIN.into()]).unwrap(),
            false,
            ServiceConfig::new(ProductId::Brioche, KEY).unwrap(),
        )
    }
}
async fn request(
    app: &Router,
    method: &str,
    path: &str,
    body: Option<serde_json::Value>,
    cookie: &mut String,
    csrf: &mut String,
) -> (u16, serde_json::Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("origin", ORIGIN)
        .header("cookie", cookie.as_str())
        .header("x-csrf-token", csrf.as_str())
        .header("content-type", "application/json");
    if path.starts_with("/internal/") {
        req = req
            .header("authorization", format!("Bearer {KEY}"))
            .header("x-chef-product", "brioche");
    }
    let response = app
        .clone()
        .oneshot(
            req.body(Body::from(
                body.map(|body| serde_json::to_vec(&body).unwrap())
                    .unwrap_or_default(),
            ))
            .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    if let Some(header) = response.headers().get("set-cookie") {
        *cookie = header.to_str().unwrap().split(';').next().unwrap().into();
    }
    let value = serde_json::from_slice::<serde_json::Value>(
        &response.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap_or(serde_json::Value::Null);
    if let Some(token) = value["csrfToken"].as_str() {
        *csrf = token.into();
    }
    (status, value)
}
async fn fingerprints(db: &DatabaseConnection, schema: &str) -> Vec<(i64, String)> {
    let mut result = Vec::new();
    for table in TABLES {
        let row=db.query_one_raw(Statement::from_string(DbBackend::Postgres,format!("SELECT count(*)::bigint AS n, md5(coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text)::text,'')) AS hash FROM \"{schema}\".\"{table}\" t"))).await.unwrap().unwrap();
        result.push((
            row.try_get("", "n").unwrap(),
            row.try_get("", "hash").unwrap(),
        ));
    }
    result
}
#[tokio::test]
#[ignore = "set TEST_DATABASE_URL to a dedicated PostgreSQL database"]
async fn identity_schema_moves_preserving_sessions_and_learning_foreign_keys() {
    let url = std::env::var("TEST_DATABASE_URL").expect("dedicated database required");
    let admin = Database::connect(&url).await.unwrap();
    let source = format!(
        "split_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let target = format!("{source}_id");
    let collision = format!("{source}_collision");
    admin
        .execute_unprepared(&format!(
            "CREATE SCHEMA {source}; CREATE SCHEMA {collision}"
        ))
        .await
        .unwrap();
    let owner = connect(&url, &source).await;
    brioche_migration::Migrator::up(&owner, None).await.unwrap();
    let backend = Backend::new(owner.clone()).await.unwrap();
    let invite = backend
        .issue_token("split@example.test", false, true)
        .await
        .unwrap();
    let pending = backend
        .issue_token("next@example.test", false, false)
        .await
        .unwrap();
    let before = app(&owner).await;
    let mut cookie = String::new();
    let mut csrf = String::new();
    assert_eq!(
        request(
            &before,
            "GET",
            "/api/v1/auth/csrf",
            None,
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        200
    );
    let (status,created)=request(&before,"POST","/api/v1/auth/accept-invite",Some(serde_json::json!({"token":invite,"email":"split@example.test","displayName":"Split","password":"correct horse croissant fromage"})),&mut cookie,&mut csrf).await;
    assert_eq!(status, 200);
    let account = created["user"]["id"]
        .as_str()
        .unwrap()
        .parse::<i64>()
        .unwrap();
    let old_cookie = cookie.clone();
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO auth_throttle(key_hash,attempts,resets_at) VALUES(repeat('a',64),1,CURRENT_TIMESTAMP+interval '1 hour');",[])).await.unwrap();
    for sql in [
        "INSERT INTO account_admin_audit(action,actor_id,target_email,reason) VALUES('invite',$1,'next@example.test','Synthetic prior audit')",
        "INSERT INTO product_membership_audit(product_id,actor_id,target_id,old_role,new_role,old_version,new_version,reason) VALUES('brioche',$1,$1,'learner','operator',0,1,'Synthetic prior grant')",
    ] {
        owner
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                sql,
                [account.into()],
            ))
            .await
            .unwrap();
    }
    let source_document = chef_engine::development_source().unwrap();
    let lesson = chef_engine::project_source(source_document.clone()).unwrap();
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO lesson_revisions(lesson_id,revision,published,public_document,server_document) VALUES($1,$2,true,$3,$4)",[lesson.id.clone().into(),(lesson.revision as i32).into(),serde_json::to_value(&lesson).unwrap().into(),source_document.into()])).await.unwrap();
    support::fixture_release(&owner).await;
    let root = assets::fixture_assets(&owner, &source).await;
    product_recordings::seed(&owner, &root).await;
    product_voices::seed(&owner, account).await;
    product_voice_work::seed(&owner).await;
    product_speech_work::seed(&owner, account).await;
    let speech_work_snapshot = product_speech_work::snapshot(&owner).await;
    assert!(speech_work_snapshot.iter().all(|(n, _)| *n > 0));
    let voice_work_snapshot = product_voice_work::snapshot(&owner).await;
    assert!(voice_work_snapshot.iter().all(|(n, _)| *n > 0));
    let voice_snapshot = product_voices::snapshot(&owner).await;
    assert!(voice_snapshot.iter().all(|(n, _)| *n > 0));
    let recording_snapshot = product_recordings::snapshot(&owner).await;
    assert!(recording_snapshot.iter().all(|(n, _)| *n > 0));
    let visual_snapshot = product_visuals::snapshot(&owner).await;
    assert!(visual_snapshot.iter().all(|(n, _)| *n > 0));
    product_facts::seed(&owner, &lesson.id, lesson.revision as i32).await;
    let fact_snapshot = product_facts::snapshot(&owner).await;
    let content_snapshot = product_content::snapshot(&owner).await;
    assert!(content_snapshot.iter().filter(|(n, _)| *n > 0).count() >= 3);
    assert!(fact_snapshot.iter().all(|(n, _)| *n > 0));
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,
        "INSERT INTO product_user_settings(product_id,user_id,settings,version) VALUES('brioche',$1,$2,2)",
        [account.into(),serde_json::to_value(brioche_course_contract::UserSettings::default()).unwrap().into()])).await.unwrap();
    let snapshot = fingerprints(&owner, &source).await;
    assert!(snapshot.iter().all(|(n, _)| *n > 0));
    assert!(
        chef_engine::schema_split::relocate(&owner, &source, &collision)
            .await
            .is_err()
    );
    assert_eq!(fingerprints(&owner, &source).await, snapshot);
    chef_engine::schema_split::require_combined(&owner)
        .await
        .unwrap();
    drop(before);
    drop(backend);
    let work = std::env::temp_dir().join(format!("chef-schema-cli-{source}"));
    std::fs::create_dir(&work).unwrap();
    std::fs::write(work.join(".env"), "").unwrap();
    let invoke = |args: &[&str]| {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_chef-server"));
        command
            .env_clear()
            .env("DATABASE_URL", &url)
            .env("DATABASE_SCHEMA", &source)
            .env("APP_ENV", "development")
            .env("CONTENT_MODE", "database")
            .env("PUBLIC_APP_URL", ORIGIN)
            .current_dir(&work)
            .args(args);
        if let Ok(root) = std::env::var("SystemRoot") {
            command.env("SystemRoot", root);
        }
        command.output().unwrap()
    };
    let output = invoke(&["split-identity-schema", &source, &target]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fingerprints(&owner, &target).await, snapshot);
    // The separate dispatcher rolls both scoped DDL and its ledger back together.
    owner
        .execute_unprepared("CREATE INDEX chef_attempt_owner_time ON exercise_attempts(user_id)")
        .await
        .unwrap();
    let output = invoke(&["migrate-layout", &source]);
    assert!(!output.status.success());
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,format!("SELECT to_regclass('{target}.chef_throttle_expiry') IS NULL AND to_regclass('{source}.chef_layout_migrations') IS NULL AS rolled_back"))).await.unwrap().unwrap();
    assert!(row.try_get::<bool>("", "rolled_back").unwrap());
    owner
        .execute_unprepared("DROP INDEX chef_attempt_owner_time")
        .await
        .unwrap();
    // A late failure inside the product step must roll back its earlier columns and indexes too.
    owner.execute_unprepared("CREATE FUNCTION chef_protect_learning_product() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NEW; END $$").await.unwrap();
    assert!(!invoke(&["migrate-layout", &source]).status.success());
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT NOT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema=$1 AND table_name='learning_sessions' AND column_name='product_id') AND to_regclass($2) IS NULL AND to_regclass($3) IS NULL AS rolled_back",[source.clone().into(),format!("{source}.chef_layout_migrations").into(),format!("{target}.chef_throttle_expiry").into()])).await.unwrap().unwrap();
    assert!(row.try_get::<bool>("", "rolled_back").unwrap());
    assert_eq!(product_facts::snapshot(&owner).await, fact_snapshot);
    owner
        .execute_unprepared("DROP FUNCTION chef_protect_learning_product()")
        .await
        .unwrap();
    owner.execute_unprepared("CREATE TRIGGER chef_content_owner BEFORE UPDATE ON content_releases FOR EACH ROW EXECUTE FUNCTION reject_content_edit()").await.unwrap();
    assert!(!invoke(&["migrate-layout", &source]).status.success());
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT NOT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema=$1 AND table_name IN ('lesson_revisions','content_releases','learning_sessions') AND column_name='product_id') AND to_regclass($2) IS NULL AS rolled_back",[source.clone().into(),format!("{source}.chef_layout_migrations").into()])).await.unwrap().unwrap();
    assert!(row.try_get::<bool>("", "rolled_back").unwrap());
    assert_eq!(product_content::snapshot(&owner).await, content_snapshot);
    owner
        .execute_unprepared("DROP TRIGGER chef_content_owner ON content_releases")
        .await
        .unwrap();
    owner
        .execute_unprepared(
            "ALTER TABLE saved_items ADD CONSTRAINT chef_saved_product_lesson CHECK(true)",
        )
        .await
        .unwrap();
    assert!(!invoke(&["migrate-layout", &source]).status.success());
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT NOT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema=$1 AND table_name='learning_sessions' AND column_name='product_id') AND to_regclass($2) IS NULL AS rolled_back",[source.clone().into(),format!("{source}.chef_layout_migrations").into()])).await.unwrap().unwrap();
    assert!(row.try_get::<bool>("", "rolled_back").unwrap());
    assert_eq!(product_facts::snapshot(&owner).await, fact_snapshot);
    owner
        .execute_unprepared("ALTER TABLE saved_items DROP CONSTRAINT chef_saved_product_lesson")
        .await
        .unwrap();
    owner.execute_unprepared("ALTER TABLE character_revisions ADD CONSTRAINT chef_character_product_avatar CHECK(true)").await.unwrap();
    assert!(!invoke(&["migrate-layout", &source]).status.success());
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT NOT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema=$1 AND table_name IN ('media_assets','character_revisions','asset_import_audit','lesson_revisions','learning_sessions') AND column_name='product_id') AND to_regclass($2) IS NULL AS rolled_back",[source.clone().into(),format!("{source}.chef_layout_migrations").into()])).await.unwrap().unwrap();
    assert!(row.try_get::<bool>("", "rolled_back").unwrap());
    assert_eq!(product_visuals::snapshot(&owner).await, visual_snapshot);
    owner
        .execute_unprepared(
            "ALTER TABLE character_revisions DROP CONSTRAINT chef_character_product_avatar",
        )
        .await
        .unwrap();
    owner
        .execute_unprepared("CREATE INDEX chef_recording_product ON audio_assets(asset_id)")
        .await
        .unwrap();
    assert!(!invoke(&["migrate-layout", &source]).status.success());
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT NOT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema=$1 AND table_name IN ('audio_assets','audio_import_audit','media_assets','lesson_revisions','learning_sessions') AND column_name='product_id') AND to_regclass($2) IS NULL AS rolled_back",[source.clone().into(),format!("{source}.chef_layout_migrations").into()])).await.unwrap().unwrap();
    assert!(row.try_get::<bool>("", "rolled_back").unwrap());
    assert_eq!(
        product_recordings::snapshot(&owner).await,
        recording_snapshot
    );
    owner
        .execute_unprepared("DROP INDEX chef_recording_product")
        .await
        .unwrap();
    owner.execute_unprepared("ALTER TABLE voice_reference_reads ADD CONSTRAINT chef_reference_read_product_grant CHECK(true)").await.unwrap();
    assert!(!invoke(&["migrate-layout", &source]).status.success());
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT NOT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema=$1 AND table_name IN ('character_voice_profiles','voice_reference_grants','voice_reference_reads','audio_assets','learning_sessions') AND column_name IN ('product_id','reference_asset_id','reference_asset_revision')) AND to_regclass($2) IS NULL AS rolled_back",[source.clone().into(),format!("{source}.chef_layout_migrations").into()])).await.unwrap().unwrap();
    assert!(row.try_get::<bool>("", "rolled_back").unwrap());
    assert_eq!(product_voices::snapshot(&owner).await, voice_snapshot);
    owner
        .execute_unprepared(
            "ALTER TABLE voice_reference_reads DROP CONSTRAINT chef_reference_read_product_grant",
        )
        .await
        .unwrap();
    owner.execute_unprepared("ALTER TABLE voice_audition_reviews ADD CONSTRAINT chef_audition_review_product_voice CHECK(true)").await.unwrap();
    assert!(!invoke(&["migrate-layout", &source]).status.success());
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT NOT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema=$1 AND table_name IN ('voice_clone_jobs','voice_auditions','voice_audition_reviews','character_voice_profiles','learning_sessions') AND column_name IN ('product_id','base_profile_revision','reference_asset_id','reference_asset_revision')) AND to_regclass($2) IS NULL AS rolled_back",[source.clone().into(),format!("{source}.chef_layout_migrations").into()])).await.unwrap().unwrap();
    assert!(row.try_get::<bool>("", "rolled_back").unwrap());
    assert_eq!(
        product_voice_work::snapshot(&owner).await,
        voice_work_snapshot
    );
    owner
        .execute_unprepared(
            "ALTER TABLE voice_audition_reviews DROP CONSTRAINT chef_audition_review_product_voice",
        )
        .await
        .unwrap();
    owner.execute_unprepared("ALTER TABLE speech_package_imports ADD CONSTRAINT chef_package_product_lesson CHECK(true)").await.unwrap();
    assert!(!invoke(&["migrate-layout", &source]).status.success());
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT NOT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema=$1 AND table_name IN ('course_speech_plans','course_speech_clips','speech_alignments','speech_package_imports','voice_auditions','learning_sessions') AND column_name IN ('product_id','base_profile_revision')) AND to_regclass($2) IS NULL AS rolled_back",[source.clone().into(),format!("{source}.chef_layout_migrations").into()])).await.unwrap().unwrap();
    assert!(row.try_get::<bool>("", "rolled_back").unwrap());
    assert_eq!(
        product_speech_work::snapshot(&owner).await,
        speech_work_snapshot
    );
    owner
        .execute_unprepared(
            "ALTER TABLE speech_package_imports DROP CONSTRAINT chef_package_product_lesson",
        )
        .await
        .unwrap();
    // An unrecognized legacy dependency must fail the final step atomically.
    owner.execute_unprepared("CREATE TABLE extra_lesson_edge(lesson_id TEXT,revision INTEGER,FOREIGN KEY(lesson_id,revision) REFERENCES lesson_revisions(lesson_id,revision))").await.unwrap();
    let rejected = invoke(&["migrate-layout", &source]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("Layout migration not confirmed"));
    let dependency_error = brioche_migration::layout::up(&owner, &source, &target)
        .await
        .unwrap_err();
    assert!(
        dependency_error
            .to_string()
            .contains("Unverified legacy lesson dependency")
    );
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT NOT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema=$1 AND table_name='lesson_revisions' AND column_name='product_id') AND to_regclass($2) IS NULL AS rolled_back",[source.clone().into(),format!("{source}.chef_layout_migrations").into()])).await.unwrap().unwrap();
    assert!(row.try_get::<bool>("", "rolled_back").unwrap());
    assert_eq!(product_content::snapshot(&owner).await, content_snapshot);
    assert_eq!(
        product_speech_work::snapshot(&owner).await,
        speech_work_snapshot
    );
    owner
        .execute_unprepared("DROP TABLE extra_lesson_edge")
        .await
        .unwrap();
    owner
        .execute_unprepared(
            "ALTER TABLE lesson_import_audit ADD CONSTRAINT chef_local_import_primary CHECK(true)",
        )
        .await
        .unwrap();
    assert!(!invoke(&["migrate-layout", &source]).status.success());
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT NOT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema=$1 AND table_name='lesson_revisions' AND column_name='product_id') AND to_regclass($2) IS NULL AS rolled_back",[source.clone().into(),format!("{source}.chef_layout_migrations").into()])).await.unwrap().unwrap();
    assert!(row.try_get::<bool>("", "rolled_back").unwrap());
    assert_eq!(product_content::snapshot(&owner).await, content_snapshot);
    owner
        .execute_unprepared(
            "ALTER TABLE lesson_import_audit DROP CONSTRAINT chef_local_import_primary",
        )
        .await
        .unwrap();
    owner
        .execute_unprepared(
            "CREATE TABLE extra_release_edge(release_id TEXT REFERENCES content_releases(id))",
        )
        .await
        .unwrap();
    assert!(!invoke(&["migrate-layout", &source]).status.success());
    let dependency_error = brioche_migration::layout::up(&owner, &source, &target)
        .await
        .unwrap_err();
    assert!(
        dependency_error
            .to_string()
            .contains("Unverified legacy release dependency")
    );
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT NOT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema=$1 AND table_name='content_releases' AND column_name='product_id') AND to_regclass($2) IS NULL AS rolled_back",[source.clone().into(),format!("{source}.chef_layout_migrations").into()])).await.unwrap().unwrap();
    assert!(row.try_get::<bool>("", "rolled_back").unwrap());
    assert_eq!(product_content::snapshot(&owner).await, content_snapshot);
    owner.execute_unprepared("DROP TABLE extra_release_edge; ALTER TABLE release_entries ADD CONSTRAINT chef_local_release_position CHECK(true)").await.unwrap();
    assert!(!invoke(&["migrate-layout", &source]).status.success());
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT NOT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema=$1 AND table_name='content_releases' AND column_name='product_id') AND to_regclass($2) IS NULL AS rolled_back",[source.clone().into(),format!("{source}.chef_layout_migrations").into()])).await.unwrap().unwrap();
    assert!(row.try_get::<bool>("", "rolled_back").unwrap());
    assert_eq!(product_content::snapshot(&owner).await, content_snapshot);
    owner
        .execute_unprepared(
            "ALTER TABLE release_entries DROP CONSTRAINT chef_local_release_position",
        )
        .await
        .unwrap();
    owner.execute_unprepared("CREATE TABLE extra_character_edge(character_id TEXT,revision INTEGER,FOREIGN KEY(character_id,revision) REFERENCES character_revisions(character_id,revision))").await.unwrap();
    assert!(!invoke(&["migrate-layout", &source]).status.success());
    let dependency_error = brioche_migration::layout::up(&owner, &source, &target)
        .await
        .unwrap_err();
    assert!(
        dependency_error
            .to_string()
            .contains("Unverified legacy visual dependency")
    );
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT NOT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema=$1 AND table_name='media_assets' AND column_name='product_id') AND to_regclass($2) IS NULL AS rolled_back",[source.clone().into(),format!("{source}.chef_layout_migrations").into()])).await.unwrap().unwrap();
    assert!(row.try_get::<bool>("", "rolled_back").unwrap());
    assert_eq!(product_visuals::snapshot(&owner).await, visual_snapshot);
    owner.execute_unprepared("DROP TABLE extra_character_edge; ALTER TABLE character_revisions ADD CONSTRAINT chef_local_character_primary CHECK(true)").await.unwrap();
    assert!(!invoke(&["migrate-layout", &source]).status.success());
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT NOT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema=$1 AND table_name='media_assets' AND column_name='product_id') AND to_regclass($2) IS NULL AS rolled_back",[source.clone().into(),format!("{source}.chef_layout_migrations").into()])).await.unwrap().unwrap();
    assert!(row.try_get::<bool>("", "rolled_back").unwrap());
    assert_eq!(product_visuals::snapshot(&owner).await, visual_snapshot);
    owner
        .execute_unprepared(
            "ALTER TABLE character_revisions DROP CONSTRAINT chef_local_character_primary",
        )
        .await
        .unwrap();
    owner.execute_unprepared("CREATE TABLE extra_voice_edge(character_id TEXT,character_revision INTEGER,revision INTEGER,FOREIGN KEY(character_id,character_revision,revision) REFERENCES character_voice_profiles(character_id,character_revision,revision))").await.unwrap();
    assert!(!invoke(&["migrate-layout", &source]).status.success());
    let dependency_error = brioche_migration::layout::up(&owner, &source, &target)
        .await
        .unwrap_err();
    assert!(
        dependency_error
            .to_string()
            .contains("Unverified legacy voice dependency")
    );
    assert_eq!(product_voices::snapshot(&owner).await, voice_snapshot);
    owner.execute_unprepared("DROP TABLE extra_voice_edge; ALTER TABLE character_voice_profiles ADD CONSTRAINT chef_local_voice_primary CHECK(true)").await.unwrap();
    assert!(!invoke(&["migrate-layout", &source]).status.success());
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT NOT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema=$1 AND table_name='character_voice_profiles' AND column_name='product_id') AND to_regclass($2) IS NULL AS rolled_back",[source.clone().into(),format!("{source}.chef_layout_migrations").into()])).await.unwrap().unwrap();
    assert!(row.try_get::<bool>("", "rolled_back").unwrap());
    assert_eq!(product_voices::snapshot(&owner).await, voice_snapshot);
    owner
        .execute_unprepared(
            "ALTER TABLE character_voice_profiles DROP CONSTRAINT chef_local_voice_primary",
        )
        .await
        .unwrap();
    owner.execute_unprepared("CREATE TABLE extra_recording_edge(asset_id TEXT,revision INTEGER,FOREIGN KEY(asset_id,revision) REFERENCES audio_assets(asset_id,revision))").await.unwrap();
    assert!(!invoke(&["migrate-layout", &source]).status.success());
    let dependency_error = brioche_migration::layout::up(&owner, &source, &target)
        .await
        .unwrap_err();
    assert!(
        dependency_error
            .to_string()
            .contains("Unverified legacy recording dependency")
    );
    assert_eq!(
        product_recordings::snapshot(&owner).await,
        recording_snapshot
    );
    owner.execute_unprepared("DROP TABLE extra_recording_edge; ALTER TABLE audio_assets ADD CONSTRAINT chef_local_recording_primary CHECK(true)").await.unwrap();
    assert!(!invoke(&["migrate-layout", &source]).status.success());
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT NOT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema=$1 AND table_name='audio_assets' AND column_name='product_id') AND to_regclass($2) IS NULL AS rolled_back",[source.clone().into(),format!("{source}.chef_layout_migrations").into()])).await.unwrap().unwrap();
    assert!(row.try_get::<bool>("", "rolled_back").unwrap());
    assert_eq!(
        product_recordings::snapshot(&owner).await,
        recording_snapshot
    );
    owner
        .execute_unprepared("ALTER TABLE audio_assets DROP CONSTRAINT chef_local_recording_primary")
        .await
        .unwrap();
    owner.execute_unprepared("CREATE TABLE extra_voice_work_edge(id TEXT,FOREIGN KEY(id) REFERENCES voice_clone_jobs(id))").await.unwrap();
    assert!(!invoke(&["migrate-layout", &source]).status.success());
    let error = brioche_migration::layout::up(&owner, &source, &target)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Unverified legacy voice work dependency"),
        "{error}"
    );
    assert_eq!(
        product_voice_work::snapshot(&owner).await,
        voice_work_snapshot
    );
    owner.execute_unprepared("DROP TABLE extra_voice_work_edge; ALTER TABLE voice_audition_reviews ADD CONSTRAINT chef_local_audition_review_primary CHECK(true)").await.unwrap();
    assert!(!invoke(&["migrate-layout", &source]).status.success());
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT NOT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema=$1 AND table_name='voice_auditions' AND column_name='product_id') AND to_regclass($2) IS NULL AS rolled_back",[source.clone().into(),format!("{source}.chef_layout_migrations").into()])).await.unwrap().unwrap();
    assert!(row.try_get::<bool>("", "rolled_back").unwrap());
    assert_eq!(
        product_voice_work::snapshot(&owner).await,
        voice_work_snapshot
    );
    owner
        .execute_unprepared(
            "ALTER TABLE voice_audition_reviews DROP CONSTRAINT chef_local_audition_review_primary",
        )
        .await
        .unwrap();
    owner.execute_unprepared("CREATE TABLE extra_speech_work_edge(id TEXT,FOREIGN KEY(id) REFERENCES course_speech_clips(id))").await.unwrap();
    assert!(!invoke(&["migrate-layout", &source]).status.success());
    let error = brioche_migration::layout::up(&owner, &source, &target)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Unverified legacy speech work dependency"),
        "{error}"
    );
    assert_eq!(
        product_speech_work::snapshot(&owner).await,
        speech_work_snapshot
    );
    owner.execute_unprepared("DROP TABLE extra_speech_work_edge; ALTER TABLE speech_package_imports ADD CONSTRAINT chef_local_package_lesson CHECK(true)").await.unwrap();
    assert!(!invoke(&["migrate-layout", &source]).status.success());
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT NOT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema=$1 AND table_name='course_speech_plans' AND column_name='product_id') AND to_regclass($2) IS NULL AS rolled_back",[source.clone().into(),format!("{source}.chef_layout_migrations").into()])).await.unwrap().unwrap();
    assert!(row.try_get::<bool>("", "rolled_back").unwrap());
    assert_eq!(
        product_speech_work::snapshot(&owner).await,
        speech_work_snapshot
    );
    owner
        .execute_unprepared(
            "ALTER TABLE speech_package_imports DROP CONSTRAINT chef_local_package_lesson",
        )
        .await
        .unwrap();
    let output = invoke(&["migrate-layout", &source]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(invoke(&["migrate-layout", &source]).status.success());
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,format!("SELECT to_regclass('{target}.chef_throttle_expiry') IS NOT NULL AND to_regclass('{source}.chef_attempt_owner_time') IS NOT NULL AND to_regclass('{source}.chef_throttle_expiry') IS NULL AND (SELECT count(*)=24 FROM chef_layout_migrations) AS correct"))).await.unwrap().unwrap();
    assert!(row.try_get::<bool>("", "correct").unwrap());
    assert_eq!(product_facts::snapshot(&owner).await, fact_snapshot);
    product_facts::verify(&owner).await;
    assert_eq!(product_content::snapshot(&owner).await, content_snapshot);
    product_content::verify(&owner, &lesson.id, lesson.revision as i32).await;
    product_content::verify_local_lesson_keys(&owner, &lesson.id, lesson.revision as i32).await;
    product_content::verify_local_release_keys(&owner, &lesson.id, lesson.revision as i32).await;
    product_content::verify_local_lesson_records(
        &owner,
        &lesson.id,
        lesson.revision as i32,
        account,
    )
    .await;
    assert_eq!(product_visuals::snapshot(&owner).await, visual_snapshot);
    product_visuals::verify(&owner).await;
    product_visuals::verify_local_keys(&owner).await;
    assert_eq!(
        product_recordings::snapshot(&owner).await,
        recording_snapshot
    );
    product_recordings::verify(&owner).await;
    assert_eq!(product_voices::snapshot(&owner).await, voice_snapshot);
    product_voices::verify(&owner).await;
    assert_eq!(
        product_voice_work::snapshot(&owner).await,
        voice_work_snapshot
    );
    product_voice_work::verify(&owner).await;
    product_voice_work::verify_local_keys(&owner).await;
    assert_eq!(
        product_speech_work::snapshot(&owner).await,
        speech_work_snapshot
    );
    product_speech_work::verify(&owner, account).await;
    product_speech_work::verify_local_keys(&owner, account).await;
    assert_eq!(
        product_speech_work::snapshot(&owner).await,
        speech_work_snapshot
    );
    assert_eq!(
        product_voice_work::snapshot(&owner).await,
        voice_work_snapshot
    );
    assert_eq!(product_voices::snapshot(&owner).await, voice_snapshot);
    assert_eq!(
        product_recordings::snapshot(&owner).await,
        recording_snapshot
    );
    assert_eq!(product_visuals::snapshot(&owner).await, visual_snapshot);
    assert_eq!(product_content::snapshot(&owner).await, content_snapshot);
    assert_eq!(product_facts::snapshot(&owner).await, fact_snapshot);
    assert_eq!(fingerprints(&owner, &target).await, snapshot);
    assert!(
        brioche_migration::layout::up(&owner, &source, &collision)
            .await
            .is_err()
    );
    owner
        .execute_unprepared(
            "UPDATE chef_layout_migrations SET definition='changed' WHERE scope='identity'",
        )
        .await
        .unwrap();
    assert!(!invoke(&["migrate-layout", &source]).status.success());
    owner.execute_unprepared("UPDATE chef_layout_migrations SET definition='CREATE INDEX chef_throttle_expiry ON auth_throttle(resets_at)' WHERE scope='identity'").await.unwrap();
    for args in [&["migrate"][..], &["serve"][..]] {
        let output = invoke(args);
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("Split schema requires independent identity mode")
        );
    }
    std::fs::remove_file(work.join(".env")).unwrap();
    std::fs::remove_dir(&work).unwrap();
    assert!(
        chef_engine::schema_split::require_combined(&owner)
            .await
            .is_err()
    );
    assert!(
        chef_engine::schema_split::relocate(&owner, &source, &target)
            .await
            .is_err()
    );
    assert!(
        brioche_migration::Migrator::down(&owner, Some(1))
            .await
            .unwrap_err()
            .to_string()
            .contains("preserving layout")
    );
    assert_eq!(fingerprints(&owner, &target).await, snapshot);

    let id_role = format!("{source}_id_login");
    let learning_role = format!("{source}_learn_login");
    let content_role = format!("{source}_content_login");
    owner
        .execute_unprepared(&format!(
            "CREATE ROLE {content_role} LOGIN NOINHERIT NOSUPERUSER NOCREATEDB NOCREATEROLE"
        ))
        .await
        .unwrap();
    owner.execute_unprepared(&format!("CREATE ROLE {id_role} LOGIN NOINHERIT NOSUPERUSER NOCREATEDB NOCREATEROLE; CREATE ROLE {learning_role} LOGIN NOINHERIT NOSUPERUSER NOCREATEDB NOCREATEROLE")).await.unwrap();
    for (template, schema, role) in [
        (
            include_str!("../../../infra/database/learning-product-grants.sql"),
            source.as_str(),
            learning_role.as_str(),
        ),
        (
            include_str!("../../../infra/database/identity-grants.sql"),
            target.as_str(),
            id_role.as_str(),
        ),
        (
            include_str!("../../../infra/database/learning-grants.sql"),
            source.as_str(),
            learning_role.as_str(),
        ),
        (
            include_str!("../../../infra/database/content-grants.sql"),
            source.as_str(),
            content_role.as_str(),
        ),
    ] {
        let grants = template
            .lines()
            .filter(|line| !line.starts_with('\\'))
            .collect::<Vec<_>>()
            .join("\n")
            .replace(":\"schema\"", &format!("\"{schema}\""))
            .replace(":\"identity_schema\"", &format!("\"{target}\""))
            .replace(":\"learning_schema\"", &format!("\"{source}\""))
            .replace(":\"role\"", &format!("\"{role}\""));
        owner.execute_unprepared(&grants).await.unwrap();
    }
    let role_url = |role: &str| {
        let mut url = url::Url::parse(&url).unwrap();
        url.set_username(role).unwrap();
        url.set_password(None).unwrap();
        url.to_string()
    };
    let identity = connect(&role_url(&id_role), &target).await;
    let learning = connect(&role_url(&learning_role), &source).await;
    let content = connect(&role_url(&content_role), &source).await;
    assert!(
        identity
            .execute_unprepared(&format!(
                "SELECT {source}.chef_lock_product_lesson('brioche','missing',1)"
            ))
            .await
            .is_err()
    );
    let locked = owner.begin().await.unwrap();
    locked
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT 1 FROM lesson_revisions WHERE lesson_id=$1 AND revision=$2 FOR UPDATE",
            [lesson.id.clone().into(), (lesson.revision as i32).into()],
        ))
        .await
        .unwrap()
        .unwrap();
    let reader = learning.begin().await.unwrap();
    reader
        .execute_unprepared("SET LOCAL lock_timeout='100ms'")
        .await
        .unwrap();
    let no_lock = reader
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT chef_lock_product_lesson('hargow',$1,$2) AS found",
            [lesson.id.clone().into(), (lesson.revision as i32).into()],
        ))
        .await
        .unwrap()
        .unwrap();
    assert!(!no_lock.try_get::<bool>("", "found").unwrap());
    let error = reader
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT chef_lock_product_lesson('brioche',$1,$2) AS found",
            [lesson.id.clone().into(), (lesson.revision as i32).into()],
        ))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("lock timeout"), "{error}");
    reader.rollback().await.unwrap();
    locked.rollback().await.unwrap();
    let found = learning
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT chef_lock_product_lesson('brioche',$1,$2) AS found",
            [lesson.id.clone().into(), (lesson.revision as i32).into()],
        ))
        .await
        .unwrap()
        .unwrap();
    assert!(found.try_get::<bool>("", "found").unwrap());
    for restricted in [&identity, &learning, &content] {
        assert!(
            brioche_migration::layout::up(restricted, &source, &target)
                .await
                .is_err()
        );
    }
    for sql in [
        format!("SELECT * FROM \"{target}\".users LIMIT 0"),
        format!("SELECT * FROM \"{target}\".product_memberships LIMIT 0"),
        "SELECT * FROM product_user_settings LIMIT 0".into(),
        "UPDATE lesson_revisions SET server_document='{}' WHERE false".into(),
    ] {
        assert!(content.execute_unprepared(&sql).await.is_err());
    }
    assert!(
        identity
            .execute_unprepared(&format!(
                "SELECT * FROM \"{source}\".learning_sessions LIMIT 0"
            ))
            .await
            .is_err()
    );
    assert!(
        learning
            .execute_unprepared(&format!("SELECT * FROM \"{target}\".users LIMIT 0"))
            .await
            .is_err()
    );
    assert!(
        chef_engine::schema_split::relocate(&learning, &source, &format!("{source}_denied"))
            .await
            .is_err()
    );
    let after = app(&identity).await;
    let (status, session) = request(
        &after,
        "GET",
        "/internal/v1/session",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(session["account"]["id"], account.to_string());
    assert_eq!(cookie, old_cookie);
    assert_eq!(
        request(
            &after,
            "PATCH",
            "/api/v1/account",
            Some(serde_json::json!({"expectedAccountVersion":1,"displayName":"Preserved"})),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        200
    );
    let mut next_cookie = String::new();
    let mut next_csrf = String::new();
    request(
        &after,
        "GET",
        "/api/v1/auth/csrf",
        None,
        &mut next_cookie,
        &mut next_csrf,
    )
    .await;
    let (status,next)=request(&after,"POST","/api/v1/auth/accept-invite",Some(serde_json::json!({"token":pending,"email":"next@example.test","displayName":"Next","password":"correct horse croissant fromage"})),&mut next_cookie,&mut next_csrf).await;
    assert_eq!(status, 200);
    assert!(next["user"]["id"].as_str().unwrap().parse::<i64>().unwrap() > account);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, after).await.unwrap() });
    let client = chef_engine::learning_identity::Client::new(
        &format!("http://{address}"),
        KEY,
        ProductId::Brioche,
        false,
    )
    .unwrap();

    let content_app =
        chef_engine::admin::independent_router(content.clone(), client.clone(), root.clone())
            .unwrap();
    let (_, configuration) = request(
        &content_app,
        "GET",
        "/api/v1/operator/voice-jobs",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(configuration["configured"], false);
    let enrollment = std::sync::Arc::new(EnrollmentFixture::default());
    let service = chef_engine::qwen::Service::new(
        enrollment.clone(),
        "https://provider-fixture.example.test",
    )
    .unwrap();
    let content_app = content_app.layer(axum::Extension(service));
    let character = serde_json::json!({"characterId":"split-character","expectedRevision":0,"displayName":"Test character","avatarId":"avatar-camille-v1","avatarRevision":1,"reason":"Independent character registration"});
    for (session, token, expected) in [
        (next_cookie.as_str(), next_csrf.as_str(), 403),
        (cookie.as_str(), "bad-csrf", 403),
        (cookie.as_str(), csrf.as_str(), 200),
    ] {
        let response = content_app
            .clone()
            .oneshot(json_write(
                "/api/v1/operator/characters/revisions",
                &character,
                session,
                token,
            ))
            .await
            .unwrap();
        let status = response.status().as_u16();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(status, expected, "{}", String::from_utf8_lossy(&body));
    }
    assert_eq!(
        request(
            &content_app,
            "POST",
            "/api/v1/operator/characters/revisions",
            Some(character.clone()),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    let mut new_character = character.clone();
    new_character["expectedRevision"] = 1.into();
    new_character["displayName"] = "Updated character".into();
    assert_eq!(
        request(
            &content_app,
            "POST",
            "/api/v1/operator/characters/revisions",
            Some(new_character),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        200
    );
    let (status, old) = request(
        &content_app,
        "GET",
        "/api/v1/operator/characters/split-character/1",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(old["character"]["displayName"], "Test character");
    let voice = serde_json::json!({"characterId":"split-character","characterRevision":2,"expectedVoiceRevision":0,"profile":{"personality":"Warm and patient.","speakingStyle":"Natural conversation.","defaultEmotion":"Friendly.","provider":"qwen","model":"qwen-audio-3.1-tts-flash","voiceId":"test-voice","voiceKind":"system","locale":"fr-FR","rate":1.0,"referenceAudio":null},"reason":"Independent voice direction"});
    for (session, token, expected) in [
        (next_cookie.as_str(), next_csrf.as_str(), 403),
        (cookie.as_str(), "bad-csrf", 403),
        (cookie.as_str(), csrf.as_str(), 200),
    ] {
        let response = content_app
            .clone()
            .oneshot(json_write(
                "/api/v1/operator/characters",
                &voice,
                session,
                token,
            ))
            .await
            .unwrap();
        let status = response.status().as_u16();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(status, expected, "{}", String::from_utf8_lossy(&body));
    }
    assert_eq!(
        request(
            &content_app,
            "POST",
            "/api/v1/operator/characters",
            Some(voice.clone()),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    let (status, stored) = request(
        &content_app,
        "GET",
        "/api/v1/operator/characters/split-character/2/voices/1",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(stored["profile"], voice["profile"]);
    let response = content_app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/operator/characters/split-character/2/avatar")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    assert!(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .starts_with(b"<svg")
    );
    for (session, token, expected) in [
        (next_cookie.as_str(), next_csrf.as_str(), 403),
        (cookie.as_str(), "bad-csrf", 403),
        (cookie.as_str(), csrf.as_str(), 200),
    ] {
        let response = content_app
            .clone()
            .oneshot(asset_upload("split-upload", session, token))
            .await
            .unwrap();
        let status = response.status().as_u16();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(status, expected, "{}", String::from_utf8_lossy(&body));
    }
    assert_eq!(
        content_app
            .clone()
            .oneshot(asset_upload("split-upload", &cookie, &csrf))
            .await
            .unwrap()
            .status()
            .as_u16(),
        409
    );
    let (_, registry) = request(
        &content_app,
        "GET",
        "/api/v1/operator/assets?q=split-upload",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(registry["items"][0]["asset"]["assetId"], "split-upload");
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT product_id,(SELECT count(*) FROM asset_import_audit WHERE product_id='brioche' AND target='split-upload v1')::bigint AS audit_count FROM media_assets WHERE asset_id='split-upload' AND revision=1")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<String>("", "product_id").unwrap(), "brioche");
    assert_eq!(row.try_get::<i64>("", "audit_count").unwrap(), 1);
    let response = content_app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/operator/assets/split-upload/1/file")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    assert!(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .starts_with(b"<svg")
    );
    for (session, token, expected) in [
        (next_cookie.as_str(), next_csrf.as_str(), 403),
        (cookie.as_str(), "bad-csrf", 403),
        (cookie.as_str(), csrf.as_str(), 200),
    ] {
        let response = content_app
            .clone()
            .oneshot(recording_upload("split-recording", session, token))
            .await
            .unwrap();
        let status = response.status().as_u16();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(status, expected, "{}", String::from_utf8_lossy(&body));
    }
    assert_eq!(
        content_app
            .clone()
            .oneshot(recording_upload("split-recording", &cookie, &csrf))
            .await
            .unwrap()
            .status()
            .as_u16(),
        409
    );
    let (status, registry) = request(
        &content_app,
        "GET",
        "/api/v1/operator/recordings?q=split-recording",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(registry["items"][0]["asset"]["assetId"], "split-recording");
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT product_id,(SELECT count(*) FROM audio_import_audit WHERE product_id='brioche' AND target='split-recording v1')::bigint AS n FROM audio_assets WHERE asset_id='split-recording' AND revision=1")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<String>("", "product_id").unwrap(), "brioche");
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 1);
    let response = content_app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/operator/recordings/split-recording/1/file")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(
        &response.into_body().collect().await.unwrap().to_bytes()[..],
        include_bytes!("fixtures/audio/synthetic.mp3")
    );
    let remote = chef_engine::learning_identity::router(learning.clone(), client)
        .unwrap()
        .merge(chef_engine::independent_learning_router(
            chef_engine::AppState {
                db: Some(learning.clone()),
                fixture: None,
            },
        ));
    assert_eq!(
        request(&remote, "GET", "/api/ready", None, &mut cookie, &mut csrf)
            .await
            .0,
        200
    );
    let (status, profile) =
        request(&remote, "GET", "/api/v1/me", None, &mut cookie, &mut csrf).await;
    assert_eq!(status, 200);
    assert_eq!(profile["displayName"], "Preserved");
    assert_eq!(profile["version"], 2);
    assert_eq!(
        request(
            &remote,
            "PATCH",
            "/api/v1/me/settings",
            Some(serde_json::json!({"version":2,"showTranslation":true})),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        200
    );
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO learning_operations(product_id,user_id,scope,idempotency_key,request_hash,result) VALUES('hargow',$1,'start','split-start-learning-01',$2,$3)",[account.into(),"b".repeat(64).into(),serde_json::json!({"marker":"other product only"}).into()])).await.unwrap();
    let mut other_lesson = lesson.clone();
    other_lesson.id = "hargow-learning-fixture".into();
    let mut other_source = chef_engine::development_source().unwrap();
    other_source["id"] = other_lesson.id.clone().into();
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO lesson_revisions(product_id,lesson_id,revision,published,public_document,server_document) VALUES('hargow',$1,$2,true,$3,$4)",[other_lesson.id.clone().into(),(lesson.revision as i32).into(),serde_json::to_value(&other_lesson).unwrap().into(),other_source.into()])).await.unwrap();
    let other_session = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO learning_sessions(id,product_id,user_id,lesson_id,revision,schema_version) VALUES($1,'hargow',$2,$3,$4,'1.0')",[other_session.into(),account.into(),other_lesson.id.clone().into(),(lesson.revision as i32).into()])).await.unwrap();
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO lesson_progress(product_id,user_id,lesson_id,last_session_id,first_completed_at,latest_completed_revision) VALUES('hargow',$1,$2,$3,'2026-01-01T00:00:00Z',$4)",[account.into(),other_lesson.id.clone().into(),other_session.into(),(lesson.revision as i32).into()])).await.unwrap();
    let start_body = serde_json::json!({"lessonId":lesson.id,"schemaVersion":"1.0","idempotencyKey":"split-start-learning-01"});
    owner
        .execute_unprepared(
            "INSERT INTO content_state(product_id,singleton) VALUES('hargow',false)",
        )
        .await
        .unwrap();
    assert!(
        identity
            .execute_unprepared(&format!(
                "SELECT {source}.chef_lock_product_release_state('brioche')"
            ))
            .await
            .is_err()
    );
    let start = Request::builder()
        .method("POST")
        .uri("/api/v1/learning-sessions")
        .header("origin", ORIGIN)
        .header("cookie", &cookie)
        .header("x-csrf-token", &csrf)
        .header("content-type", "application/json")
        .header("idempotency-key", "split-start-learning-01")
        .body(Body::from(
            serde_json::to_vec(
                &serde_json::json!({"lessonId":lesson.id,"schemaVersion":"1.0","idempotencyKey":"split-start-learning-01"}),
            )
            .unwrap(),
        ))
        .unwrap();
    let response = remote.clone().oneshot(start).await.unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let started: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_ne!(started["progress"]["id"], other_session);
    assert!(started["progress"]["firstCompletedAt"].is_null());
    let other_path = format!("/api/v1/learning-sessions/{other_session}");
    for (method, path, body) in [
        ("GET", other_path.clone(), None),
        (
            "PUT",
            format!("{other_path}/steps/unknown"),
            Some(serde_json::json!({"version":1,"idempotencyKey":"other-step-test-01"})),
        ),
        (
            "POST",
            format!("{other_path}/attempts"),
            Some(
                serde_json::json!({"version":1,"idempotencyKey":"other-attempt-test-01","exerciseId":"unknown","answer":{"kind":"text","text":"Bonjour"}}),
            ),
        ),
        (
            "POST",
            format!("{other_path}/hints/unknown"),
            Some(serde_json::json!({"version":1,"idempotencyKey":"other-hint-test-01"})),
        ),
        (
            "POST",
            format!("{other_path}/complete"),
            Some(serde_json::json!({"version":1,"idempotencyKey":"other-complete-test-01"})),
        ),
    ] {
        assert_eq!(
            request(&remote, method, &path, body, &mut cookie, &mut csrf)
                .await
                .0,
            404,
            "{method} {path}"
        );
    }
    let (status, history) = request(
        &remote,
        "GET",
        "/api/v1/me/learning",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(history["completedLessons"], 0);
    assert_eq!(history["items"].as_array().unwrap().len(), 1);
    assert_eq!(history["items"][0]["sessionId"], started["progress"]["id"]);
    let untouched=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT s.version,s.completed_at IS NULL AS active,p.first_completed_at='2026-01-01T00:00:00Z'::timestamptz AS preserved,(SELECT count(*) FROM learning_sessions WHERE user_id=$1 AND (lesson_id=$2 OR id=$3) AND completed_at IS NULL)::bigint AS active_count FROM learning_sessions s JOIN lesson_progress p ON p.product_id=s.product_id AND p.last_session_id=s.id WHERE s.id=$3",[account.into(),lesson.id.clone().into(),other_session.into()])).await.unwrap().unwrap();
    assert_eq!(untouched.try_get::<i32>("", "version").unwrap(), 1);
    assert!(untouched.try_get::<bool>("", "active").unwrap());
    assert!(untouched.try_get::<bool>("", "preserved").unwrap());
    assert_eq!(untouched.try_get::<i64>("", "active_count").unwrap(), 2);
    let (status, replayed) = request(
        &remote,
        "POST",
        "/api/v1/learning-sessions",
        Some(start_body.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(started, replayed);
    let mut changed = start_body.clone();
    changed["lessonId"] = "changed-source".into();
    assert_eq!(
        request(
            &remote,
            "POST",
            "/api/v1/learning-sessions",
            Some(changed),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT count(*)::bigint AS n,count(DISTINCT request_hash)::bigint AS hashes FROM learning_operations WHERE user_id=$1 AND scope='start' AND idempotency_key='split-start-learning-01'",[account.into()])).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 2);
    assert_eq!(row.try_get::<i64>("", "hashes").unwrap(), 2);
    let session_id = started["progress"]["id"].as_str().unwrap();
    let first_step = &lesson.steps[0].id;
    let (status, confirmed) = request(
        &remote,
        "PUT",
        &format!("/api/v1/learning-sessions/{session_id}/steps/{first_step}"),
        Some(serde_json::json!({"version":1,"idempotencyKey":"split-confirm-step-01"})),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{confirmed}");
    assert_eq!(confirmed["confirmedStepIds"][0], *first_step);
    let stored = owner
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT product_id FROM step_progress WHERE session_id=$1 AND step_id=$2",
            [session_id.into(), first_step.clone().into()],
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.try_get::<String>("", "product_id").unwrap(),
        "brioche"
    );
    let knowledge = &lesson.knowledge.vocabulary[0];
    let other_saved = "dddddddddddddddddddddddddddddddd";
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO saved_items(product_id,id,user_id,knowledge_id,source_lesson_id,source_revision,snapshot,saved) VALUES('hargow',$1,$2,$3,$4,$5,$6,true)",[other_saved.into(),account.into(),knowledge.id.clone().into(),other_lesson.id.clone().into(),(lesson.revision as i32).into(),serde_json::to_value(knowledge).unwrap().into()])).await.unwrap();
    let saved_path = format!("/api/v1/me/saved-items/{}", knowledge.id);
    assert_eq!(
        request(&remote, "GET", &saved_path, None, &mut cookie, &mut csrf)
            .await
            .0,
        404
    );
    let saved_body = serde_json::json!({"sourceLessonId":lesson.id,"sourceRevision":lesson.revision,"saved":true,"version":0,"idempotencyKey":"split-save-expression-01"});
    let (status, saved) = request(
        &remote,
        "PUT",
        &saved_path,
        Some(saved_body.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{saved}");
    assert_ne!(saved["id"], other_saved);
    assert_eq!(
        request(
            &remote,
            "PUT",
            &saved_path,
            Some(saved_body),
            &mut cookie,
            &mut csrf
        )
        .await
        .1,
        saved
    );
    let (status, page) = request(
        &remote,
        "GET",
        "/api/v1/me/saved-items",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    assert_eq!(page["items"][0]["id"], saved["id"]);
    let (status,unsaved)=request(&remote,"PUT",&saved_path,Some(serde_json::json!({"sourceLessonId":lesson.id,"sourceRevision":lesson.revision,"saved":false,"version":1,"idempotencyKey":"split-unsave-expression-01"})),&mut cookie,&mut csrf).await;
    assert_eq!(status, 200, "{unsaved}");
    assert_eq!(unsaved["version"], 2);
    assert_eq!(unsaved["saved"], false);
    let untouched = owner
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT saved,version FROM saved_items WHERE id=$1",
            [other_saved.into()],
        ))
        .await
        .unwrap()
        .unwrap();
    assert!(untouched.try_get::<bool>("", "saved").unwrap());
    assert_eq!(untouched.try_get::<i32>("", "version").unwrap(), 1);
    let other_card = "cccccccccccccccccccccccccccccccc";
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO review_cards(product_id,id,user_id,knowledge_id,source_lesson_id,source_revision,snapshot) VALUES('hargow',$1,$2,$3,$4,$5,$6)",[other_card.into(),account.into(),knowledge.id.clone().into(),other_lesson.id.clone().into(),(lesson.revision as i32).into(),serde_json::to_value(knowledge).unwrap().into()])).await.unwrap();
    let other_path = format!("/api/v1/me/reviews/{other_card}");
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO review_attempts(product_id,id,card_id,user_id,rating,old_stage,new_stage,old_version,new_version,due_at,reviewed_at,time_zone) VALUES('hargow','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',$1,$2,'again',-1,0,1,2,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP,'Asia/Hong_Kong')",[other_card.into(),account.into()])).await.unwrap();
    let (status, history) = request(
        &remote,
        "GET",
        "/api/v1/me/review-history",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200);
    assert!(history["items"].as_array().unwrap().is_empty());
    for (method, path, body) in [
        ("GET", other_path.clone(), None),
        (
            "POST",
            format!("{other_path}/attempts"),
            Some(
                serde_json::json!({"cardVersion":1,"idempotencyKey":"other-review-attempt-01","rating":"remembered"}),
            ),
        ),
        (
            "PUT",
            format!("{other_path}/preferences"),
            Some(
                serde_json::json!({"cardVersion":1,"idempotencyKey":"other-review-suspend-01","suspended":true}),
            ),
        ),
    ] {
        assert_eq!(
            request(&remote, method, &path, body, &mut cookie, &mut csrf)
                .await
                .0,
            404,
            "{method} {path}"
        );
    }
    let (status, queue) = request(
        &remote,
        "GET",
        "/api/v1/me/reviews",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(queue["dueCount"], 0);
    assert!(queue["items"].as_array().unwrap().is_empty());
    let review_enrollment = serde_json::json!({"knowledgeId":knowledge.id,"sourceLessonId":lesson.id,"sourceRevision":lesson.revision,"idempotencyKey":"split-review-enroll-01"});
    let (status, card) = request(
        &remote,
        "POST",
        "/api/v1/me/review-enrollments",
        Some(review_enrollment.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{card}");
    assert_ne!(card["id"], other_card);
    assert_eq!(
        request(
            &remote,
            "POST",
            "/api/v1/me/review-enrollments",
            Some(review_enrollment),
            &mut cookie,
            &mut csrf
        )
        .await
        .1,
        card
    );
    let card_id = card["id"].as_str().unwrap();
    let (status, cards) = request(
        &remote,
        "GET",
        "/api/v1/me/review-cards",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(cards["items"].as_array().unwrap().len(), 1);
    assert_eq!(cards["items"][0]["id"], card["id"]);
    let (status, queue) = request(
        &remote,
        "GET",
        "/api/v1/me/reviews",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(queue["dueCount"], 1);
    let review_body = serde_json::json!({"cardVersion":1,"idempotencyKey":"split-review-attempt-01","rating":"remembered"});
    let review_path = format!("/api/v1/me/reviews/{card_id}/attempts");
    let (status, rated) = request(
        &remote,
        "POST",
        &review_path,
        Some(review_body.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{rated}");
    assert_eq!(rated["card"]["version"], 2);
    assert_eq!(rated["timeZone"], queue["timeZone"]);
    assert_eq!(
        request(
            &remote,
            "POST",
            &review_path,
            Some(review_body),
            &mut cookie,
            &mut csrf
        )
        .await
        .1,
        rated
    );
    let (status, history) = request(
        &remote,
        "GET",
        "/api/v1/me/review-history",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(history["items"].as_array().unwrap().len(), 1);
    assert_eq!(history["items"][0]["cardId"], card["id"]);
    let (status,paused)=request(&remote,"PUT",&format!("/api/v1/me/reviews/{card_id}/preferences"),Some(serde_json::json!({"cardVersion":2,"idempotencyKey":"split-review-suspend-01","suspended":true})),&mut cookie,&mut csrf).await;
    assert_eq!(status, 200, "{paused}");
    assert_eq!(paused["version"], 3);
    let untouched=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT version,suspended,stage,(SELECT count(*) FROM review_attempts WHERE card_id=$1)::bigint AS attempts FROM review_cards WHERE id=$1",[other_card.into()])).await.unwrap().unwrap();
    assert_eq!(untouched.try_get::<i32>("", "version").unwrap(), 1);
    assert!(!untouched.try_get::<bool>("", "suspended").unwrap());
    assert_eq!(untouched.try_get::<i16>("", "stage").unwrap(), -1);
    assert_eq!(untouched.try_get::<i64>("", "attempts").unwrap(), 1);
    owner
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "INSERT INTO step_progress(product_id,session_id,step_id) VALUES('hargow',$1,$2);",
            [other_session.into(), first_step.clone().into()],
        ))
        .await
        .unwrap();
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO exercise_attempts(product_id,id,session_id,user_id,exercise_id,attempt_index,answer,result,hint_used) VALUES('hargow','bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',$1,$2,'other-product-exercise',1,'{}','{}',false)",[other_session.into(),account.into()])).await.unwrap();
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"UPDATE lesson_progress SET first_completed_at=CURRENT_TIMESTAMP WHERE product_id='hargow' AND user_id=$1 AND lesson_id=$2",[account.into(),other_lesson.id.clone().into()])).await.unwrap();
    let (status, dashboard) = request(
        &remote,
        "GET",
        "/api/v1/me/dashboard",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{dashboard}");
    assert_eq!(dashboard["completedLessons"], 0);
    assert_eq!(dashboard["dueReviews"], 0);
    assert!(dashboard["nextReviewAt"].is_null());
    assert_eq!(dashboard["courseStates"].as_array().unwrap().len(), 1);
    assert_eq!(dashboard["resume"]["sessionId"], started["progress"]["id"]);
    assert!(dashboard["resume"]["firstCompletedAt"].is_null());
    assert_eq!(dashboard["allAvailableCompleted"], false);
    assert_eq!(dashboard["recommendedLesson"]["id"], lesson.id);
    assert_eq!(dashboard["timeZone"], queue["timeZone"]);
    let days = dashboard["days"].as_array().unwrap();
    assert_eq!(days.len(), 7);
    for (field, expected) in [
        ("confirmedSteps", 1),
        ("exerciseAttempts", 0),
        ("reviewAttempts", 1),
        ("completedLessons", 0),
    ] {
        assert_eq!(
            days.iter().map(|d| d[field].as_u64().unwrap()).sum::<u64>(),
            expected,
            "{field}"
        );
    }
    assert_eq!(dashboard["activeDays"], 1);
    owner
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE review_cards SET due_at=CURRENT_TIMESTAMP+interval '7 days' WHERE id=$1",
            [other_card.into()],
        ))
        .await
        .unwrap();
    let (status, dashboard) = request(
        &remote,
        "GET",
        "/api/v1/me/dashboard",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200);
    assert!(dashboard["nextReviewAt"].is_null());
    let invalid = learning.execute_unprepared("INSERT INTO product_user_settings(product_id,user_id,settings) VALUES('brioche',999999,'{}')")
        .await.unwrap_err();
    assert!(invalid.to_string().contains("foreign key constraint"));
    for path in [
        "/api/v1/operator/overview",
        "/api/v1/operator/history",
        "/api/v1/operator/assets",
        "/api/v1/operator/recordings",
        "/api/v1/operator/characters",
    ] {
        assert_eq!(
            request(&content_app, "GET", path, None, &mut cookie, &mut csrf)
                .await
                .0,
            200
        );
        assert_eq!(
            request(
                &content_app,
                "GET",
                path,
                None,
                &mut next_cookie,
                &mut next_csrf
            )
            .await
            .0,
            403
        );
    }
    assert_eq!(
        request(
            &content_app,
            "GET",
            "/api/v1/operator/accounts",
            None,
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        404
    );
    let mut imported_source = chef_engine::development_source().unwrap();
    imported_source["id"] = "split-admin-lesson".into();
    imported_source["assetRefs"] = assets::fixture_refs();
    imported_source["editorial"]["status"] = "draft".into();
    let document = serde_json::json!({"document":serde_json::to_string(&imported_source).unwrap(),"reason":"Independent content import"});
    let mut bad_csrf = "wrong-csrf".to_owned();
    assert_eq!(
        request(
            &content_app,
            "POST",
            "/api/v1/operator/lessons/import",
            Some(document.clone()),
            &mut cookie,
            &mut bad_csrf
        )
        .await
        .0,
        403
    );
    let (status, result) = request(
        &content_app,
        "POST",
        "/api/v1/operator/lessons/import",
        Some(document.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{result}");
    let (status, retried) = request(
        &content_app,
        "POST",
        "/api/v1/operator/lessons/import",
        Some(document.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{retried}");
    assert_eq!(retried, result);
    let imported_row = owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT product_id,(SELECT count(*) FROM lesson_import_audit WHERE lesson_id='split-admin-lesson' AND revision=1 AND product_id='brioche')::bigint AS n FROM lesson_revisions WHERE lesson_id='split-admin-lesson' AND revision=1")).await.unwrap().unwrap();
    assert_eq!(
        imported_row.try_get::<String>("", "product_id").unwrap(),
        "brioche"
    );
    assert_eq!(imported_row.try_get::<i64>("", "n").unwrap(), 1);
    let (status,result)=request(&content_app,"POST","/api/v1/operator/lessons/split-admin-lesson/revisions/1/review",Some(serde_json::json!({"version":0,"approved":true,"reason":"Independent editorial approval"})),&mut cookie,&mut csrf).await;
    assert_eq!(status, 200, "{result}");
    let manifest = serde_json::json!({"id":"split-admin-release","schemaVersion":"1.0","levels":[{"id":imported_source["levelId"],"label":"A1","units":[{"id":imported_source["unitId"],"titleZh":"Breakfast","lessons":[{"lessonId":"split-admin-lesson","revision":1}]}]}]});
    let (status, result) = request(
        &content_app,
        "POST",
        "/api/v1/operator/releases/stage",
        Some(serde_json::json!({"document":manifest.to_string(),"reason":"Independent staging"})),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{result}");
    let (_, overview) = request(
        &content_app,
        "GET",
        "/api/v1/operator/overview",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    let (status,result)=request(&content_app,"POST","/api/v1/operator/releases/activate",Some(serde_json::json!({"releaseId":"split-admin-release","generation":overview["generation"],"reason":"Independent activation"})),&mut cookie,&mut csrf).await;
    assert_eq!(status, 200, "{result}");
    let (status, result) = request(
        &content_app,
        "POST",
        "/api/v1/operator/lessons/split-admin-lesson/revisions/1/withdraw",
        Some(serde_json::json!({"generation":result,"reason":"Independent withdrawal"})),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(
        content_app
            .clone()
            .oneshot(reference_upload(&cookie, &csrf))
            .await
            .unwrap()
            .status()
            .as_u16(),
        200
    );
    let mut reference_voice = voice.clone();
    reference_voice["expectedVoiceRevision"] = 1.into();
    reference_voice["profile"]["referenceAudio"] = serde_json::json!({"assetId":"split-reference","revision":1,"transcript":"Synthetic five second fixture","cloningPermission":"No real speaker; isolated test only"});
    assert_eq!(
        request(
            &content_app,
            "POST",
            "/api/v1/operator/characters",
            Some(reference_voice),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        200
    );
    let grant_request = serde_json::json!({"characterId":"split-character","characterRevision":2,"voiceRevision":2,"singleSpeakerConfirmed":true,"reason":"Independent reference authorization"});
    let grant_route = "/api/v1/operator/voice-references";
    assert_eq!(
        request(
            &content_app,
            "POST",
            grant_route,
            Some(grant_request.clone()),
            &mut next_cookie,
            &mut next_csrf
        )
        .await
        .0,
        403
    );
    let mut wrong = "wrong-csrf".to_owned();
    assert_eq!(
        request(
            &content_app,
            "POST",
            grant_route,
            Some(grant_request.clone()),
            &mut cookie,
            &mut wrong
        )
        .await
        .0,
        403
    );
    let (status, issued) = request(
        &content_app,
        "POST",
        grant_route,
        Some(grant_request.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{issued}");
    assert_eq!(
        request(
            &content_app,
            "POST",
            grant_route,
            Some(grant_request.clone()),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    let bearer = issued["path"].as_str().unwrap();
    let response = content_app
        .clone()
        .oneshot(Request::builder().uri(bearer).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert!(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .starts_with(b"RIFF")
    );
    let (_, listed) = request(
        &content_app,
        "GET",
        grant_route,
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(listed["items"][0]["readCount"], 1);
    let revoke_route = format!(
        "{grant_route}/{}/revoke",
        issued["grant"]["id"].as_str().unwrap()
    );
    let revoke_request = serde_json::json!({"reason":"Independent reference revocation"});
    assert_eq!(
        request(
            &content_app,
            "POST",
            &revoke_route,
            Some(revoke_request.clone()),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        200
    );
    assert_eq!(
        content_app
            .clone()
            .oneshot(Request::builder().uri(bearer).body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status()
            .as_u16(),
        404
    );
    let (_, live) = request(
        &content_app,
        "POST",
        grant_route,
        Some(grant_request.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    let live_revoke = format!(
        "{grant_route}/{}/revoke",
        live["grant"]["id"].as_str().unwrap()
    );
    let job_request = serde_json::json!({"grantId":live["grant"]["id"],"token":live["path"].as_str().unwrap().rsplit('/').next().unwrap(),"costConfirmed":true,"reason":"Independent enrollment attempt"});
    let job_route = "/api/v1/operator/voice-jobs";
    assert_eq!(
        request(
            &content_app,
            "POST",
            job_route,
            Some(job_request.clone()),
            &mut next_cookie,
            &mut next_csrf
        )
        .await
        .0,
        403
    );
    assert_eq!(
        request(
            &content_app,
            "POST",
            job_route,
            Some(job_request.clone()),
            &mut cookie,
            &mut wrong
        )
        .await
        .0,
        403
    );
    let (status, submitted) = request(
        &content_app,
        "POST",
        job_route,
        Some(job_request.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{submitted}");
    assert_eq!(submitted["status"], "submitted");
    let job_path = format!("{job_route}/{}", submitted["id"].as_str().unwrap());
    let processing = settled_job(&content_app, &job_path, &mut cookie, &mut csrf).await;
    assert_eq!(processing["status"], "processing");
    assert_eq!(processing["version"], 2);
    assert_eq!(
        request(
            &content_app,
            "POST",
            job_route,
            Some(job_request.clone()),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    let job_check = format!("{job_path}/check");
    let check_request =
        serde_json::json!({"expectedVersion":2,"reason":"Independent enrollment query"});
    assert_eq!(
        request(
            &content_app,
            "POST",
            &job_check,
            Some(check_request.clone()),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        200
    );
    let ready = settled_job(&content_app, &job_path, &mut cookie, &mut csrf).await;
    assert_eq!(ready["status"], "ready");
    assert_eq!(ready["version"], 4);
    assert_eq!(
        request(
            &content_app,
            "POST",
            &job_check,
            Some(check_request),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    let audition_route = "/api/v1/operator/voice-auditions";
    let audition_request = serde_json::json!({"id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","cloneJobId":submitted["id"],"expectedCloneVersion":4,"text":"Bonjour !","emotion":"Friendly.","costConfirmed":true,"reason":"Independent synthetic audition"});
    assert_eq!(
        request(
            &content_app,
            "POST",
            audition_route,
            Some(audition_request.clone()),
            &mut next_cookie,
            &mut next_csrf
        )
        .await
        .0,
        403
    );
    assert_eq!(
        request(
            &content_app,
            "POST",
            audition_route,
            Some(audition_request.clone()),
            &mut cookie,
            &mut wrong
        )
        .await
        .0,
        403
    );
    let (status, result) = request(
        &content_app,
        "POST",
        audition_route,
        Some(audition_request.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{result}");
    let audition_path = format!("{audition_route}/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    let audition = settled_job(&content_app, &audition_path, &mut cookie, &mut csrf).await;
    assert_eq!(audition["status"], "ready");
    assert_eq!(
        request(
            &content_app,
            "POST",
            audition_route,
            Some(audition_request.clone()),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        200
    );
    let mut changed = audition_request.clone();
    changed["text"] = "Changed.".into();
    assert_eq!(
        request(
            &content_app,
            "POST",
            audition_route,
            Some(changed),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    let response = content_app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("{audition_path}/file"))
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    assert!(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .starts_with(b"RIFF")
    );
    let audition_review = format!("{audition_path}/review");
    // Synthetic protocol decision only; no human listening assertion about production media.
    let review_request = serde_json::json!({"accepted":true,"heard":true,"expectedVoiceRevision":2,"reason":"Synthetic adoption protocol"});
    let mut invalid = review_request.clone();
    invalid["heard"] = false.into();
    assert_eq!(
        request(
            &content_app,
            "POST",
            &audition_review,
            Some(invalid),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        400
    );
    let (status, result) = request(
        &content_app,
        "POST",
        &audition_review,
        Some(review_request.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["appliedVoiceRevision"], 3);
    assert_eq!(
        request(
            &content_app,
            "POST",
            &audition_review,
            Some(review_request.clone()),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    let (_, profile) = request(
        &content_app,
        "GET",
        "/api/v1/operator/characters/split-character/2/voices/3",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(profile["profile"]["voiceKind"], "cloned");
    // Course plans use fixed content voices and never call the provider.
    let mut plan_voices = Vec::new();
    let plan_seed: serde_json::Value =
        serde_json::from_str(include_str!("../../../docs/characters/voices.json")).unwrap();
    for cast in &lesson.cast {
        let direction = serde_json::json!({"characterId":cast.character_id,"characterRevision":cast.revision,"expectedVoiceRevision":0,"profile":plan_seed["items"][0]["profile"],"reason":"Synthetic course compiler direction"});
        let (status, value) = request(
            &content_app,
            "POST",
            "/api/v1/operator/characters",
            Some(direction),
            &mut cookie,
            &mut csrf,
        )
        .await;
        assert_eq!(status, 200, "{value}");
        plan_voices.push(serde_json::json!({"characterId":cast.character_id,"characterRevision":cast.revision,"voiceRevision":1}));
    }
    let options_path = format!(
        "/api/v1/operator/lessons/{}/revisions/{}/speech-options",
        lesson.id, lesson.revision
    );
    let (status, options) = request(
        &content_app,
        "GET",
        &options_path,
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{options}");
    assert_eq!(
        options["voices"].as_array().unwrap().len(),
        plan_voices.len()
    );
    let plan_route = "/api/v1/operator/speech-plans";
    let preview_route = format!("{plan_route}/preview");
    let preview_request = serde_json::json!({"lessonId":lesson.id,"lessonRevision":lesson.revision,"selection":{"voices":plan_voices,"knowledgeNarrator":plan_voices[0],"emotions":{}}});
    let (status, preview) = request(
        &content_app,
        "POST",
        &preview_route,
        Some(preview_request.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{preview}");
    assert!(!preview["targets"].as_array().unwrap().is_empty());
    let plan_request = serde_json::json!({"id":"cccccccccccccccccccccccccccccccc","preview":preview_request,"expectedPlanHash":preview["planHash"],"reason":"Independent immutable course plan"});
    for path in [plan_route, preview_route.as_str()] {
        let document = if path == plan_route {
            &plan_request
        } else {
            &preview_request
        };
        for (session, token) in [
            (next_cookie.as_str(), next_csrf.as_str()),
            (cookie.as_str(), "bad-csrf"),
        ] {
            assert_eq!(
                content_app
                    .clone()
                    .oneshot(json_write(path, document, session, token))
                    .await
                    .unwrap()
                    .status()
                    .as_u16(),
                403
            );
        }
    }
    let mut wrong_plan = plan_request.clone();
    wrong_plan["expectedPlanHash"] = "0".repeat(64).into();
    assert_eq!(
        request(
            &content_app,
            "POST",
            plan_route,
            Some(wrong_plan),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    let (status, saved_plan) = request(
        &content_app,
        "POST",
        plan_route,
        Some(plan_request.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{saved_plan}");
    let (status, retry) = request(
        &content_app,
        "POST",
        plan_route,
        Some(plan_request.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(retry, saved_plan);
    let mut changed_plan = plan_request.clone();
    changed_plan["reason"] = "Changed immutable request".into();
    assert_eq!(
        request(
            &content_app,
            "POST",
            plan_route,
            Some(changed_plan),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    let (status, fixed_plan) = request(
        &content_app,
        "GET",
        &format!("{plan_route}/cccccccccccccccccccccccccccccccc"),
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(fixed_plan, saved_plan);
    let (status, plans) = request(
        &content_app,
        "GET",
        &format!(
            "{plan_route}?lessonId={}&lessonRevision={}",
            lesson.id, lesson.revision
        ),
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{plans}");
    assert_eq!(plans["items"], serde_json::json!([saved_plan]));
    let clip_route = "/api/v1/operator/speech-clips";
    let clip_id = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
    let clip_path = format!("{clip_route}/{clip_id}");
    let clip_request = serde_json::json!({"id":clip_id,"planId":"cccccccccccccccccccccccccccccccc","generationKey":preview["targets"][0]["generationKey"],"expectedPlanHash":preview["planHash"],"expectedPreviousId":null,"costConfirmed":true,"retryUnknownConfirmed":false,"reason":"Independent synthetic course clip"});
    for (session, token) in [
        (next_cookie.as_str(), next_csrf.as_str()),
        (cookie.as_str(), "bad-csrf"),
    ] {
        assert_eq!(
            content_app
                .clone()
                .oneshot(json_write(clip_route, &clip_request, session, token))
                .await
                .unwrap()
                .status()
                .as_u16(),
            403
        );
    }
    let mut unconfirmed = clip_request.clone();
    unconfirmed["costConfirmed"] = false.into();
    assert_eq!(
        request(
            &content_app,
            "POST",
            clip_route,
            Some(unconfirmed),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        400
    );
    let (status, result) = request(
        &content_app,
        "POST",
        clip_route,
        Some(clip_request.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{result}");
    let clip = settled_job(&content_app, &clip_path, &mut cookie, &mut csrf).await;
    assert_eq!(clip["status"], "ready");
    assert_eq!(
        request(
            &content_app,
            "POST",
            clip_route,
            Some(clip_request.clone()),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        200
    );
    let mut changed = clip_request.clone();
    changed["reason"] = "Changed immutable attempt".into();
    assert_eq!(
        request(
            &content_app,
            "POST",
            clip_route,
            Some(changed),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    let response = content_app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("{clip_path}/file"))
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    assert!(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .starts_with(b"RIFF")
    );
    let clip_review_path = format!("{clip_path}/review");
    // Only a synthetic decision in an isolated fixture, not production human listening.
    let clip_review =
        serde_json::json!({"accepted":true,"heard":true,"reason":"Synthetic clip decision"});
    for (session, token) in [
        (next_cookie.as_str(), next_csrf.as_str()),
        (cookie.as_str(), "bad-csrf"),
    ] {
        assert_eq!(
            content_app
                .clone()
                .oneshot(json_write(&clip_review_path, &clip_review, session, token))
                .await
                .unwrap()
                .status()
                .as_u16(),
            403
        );
    }
    let mut unheard = clip_review.clone();
    unheard["heard"] = false.into();
    assert_eq!(
        request(
            &content_app,
            "POST",
            &clip_review_path,
            Some(unheard),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        400
    );
    let (status, accepted) = request(
        &content_app,
        "POST",
        &clip_review_path,
        Some(clip_review.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{accepted}");
    assert_eq!(accepted["accepted"], true);
    assert_eq!(
        request(
            &content_app,
            "POST",
            &clip_review_path,
            Some(clip_review.clone()),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        200
    );
    let mut changed_review = clip_review.clone();
    changed_review["accepted"] = false.into();
    assert_eq!(
        request(
            &content_app,
            "POST",
            &clip_review_path,
            Some(changed_review),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    let mut reused = clip_request.clone();
    reused["id"] = "ffffffffffffffffffffffffffffffff".into();
    reused["expectedPreviousId"] = clip_id.into();
    reused["costConfirmed"] = false.into();
    let (status, receipt) = request(
        &content_app,
        "POST",
        clip_route,
        Some(reused),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{receipt}");
    assert_eq!(receipt["status"], "ready");
    assert_eq!(receipt["reusedFrom"], clip_id);
    assert_eq!(receipt["accepted"], true);
    let (status, clips) = request(
        &content_app,
        "GET",
        "/api/v1/operator/speech-plans/cccccccccccccccccccccccccccccccc/clips",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{clips}");
    assert_eq!(clips["configured"], true);
    assert_eq!(clips["items"], serde_json::json!([receipt]));
    let plan_id = "cccccccccccccccccccccccccccccccc";
    let export_path = format!("{plan_route}/{plan_id}/export");
    // All unique requests need ready reviewed clips before private export.
    let keys: std::collections::BTreeSet<_> = preview["targets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["generationKey"].as_str().unwrap().to_owned())
        .collect();
    for (index, key) in keys
        .iter()
        .filter(|k| **k != clip["generationKey"].as_str().unwrap())
        .enumerate()
    {
        let id = format!("{index:032x}");
        let body = serde_json::json!({"id":id,"planId":plan_id,"generationKey":key,"expectedPlanHash":preview["planHash"],"expectedPreviousId":null,"costConfirmed":true,"retryUnknownConfirmed":false,"reason":"Synthetic export completion"});
        let (status, value) = request(
            &content_app,
            "POST",
            clip_route,
            Some(body),
            &mut cookie,
            &mut csrf,
        )
        .await;
        assert_eq!(status, 200, "{value}");
        assert_eq!(
            settled_job(
                &content_app,
                &format!("{clip_route}/{id}"),
                &mut cookie,
                &mut csrf
            )
            .await["status"],
            "ready"
        );
        assert_eq!(
            request(
                &content_app,
                "POST",
                &format!("{clip_route}/{id}/review"),
                Some(clip_review.clone()),
                &mut cookie,
                &mut csrf
            )
            .await
            .0,
            200
        );
    }
    assert_eq!(
        request(
            &content_app,
            "GET",
            &export_path,
            None,
            &mut next_cookie,
            &mut next_csrf
        )
        .await
        .0,
        403
    );
    let response = content_app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&export_path)
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(response.headers()["cache-control"], "private, no-store");
    assert_eq!(response.headers()["content-type"], "application/x-tar");
    let archive_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let mut archive = tar::Archive::new(std::io::Cursor::new(&archive_bytes));
    let mut members = std::collections::BTreeMap::new();
    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        let path = entry.path().unwrap().to_string_lossy().into_owned();
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut bytes).unwrap();
        assert!(members.insert(path, bytes).is_none());
    }
    let manifest: serde_json::Value = serde_json::from_slice(&members["manifest.json"]).unwrap();
    assert_eq!(manifest["plan"]["planHash"], preview["planHash"]);
    assert_eq!(manifest["clips"].as_array().unwrap().len(), keys.len());
    for exported in manifest["clips"].as_array().unwrap() {
        assert_eq!(exported["review"]["actorId"], account);
        for (file, hash) in [("file", "sha256"), ("providerFile", "providerSha256")] {
            assert_eq!(
                format!(
                    "{:x}",
                    Sha256::digest(&members[exported[file].as_str().unwrap()])
                ),
                exported["result"][hash]
            );
        }
    }
    let model: serde_json::Value =
        serde_json::from_str(include_str!("../../../scripts/alignment/model.json")).unwrap();
    let runtime: serde_json::Value =
        serde_json::from_str(include_str!("../../../scripts/alignment/runtime.json")).unwrap();
    let engine = serde_json::json!({"repository":model["repository"],"revision":model["revision"],"files":model["files"],"versions":runtime,"device":"cpu","dtype":"float32","attention":"eager","transcript":"NFC source word units, apostrophes normalized; original scalar ranges retained; raw timestamp classes without interpolation"});
    // Synthetic prediction times test protocol validation, never actual model accuracy.
    let report_clips: Vec<_> = manifest["clips"].as_array().unwrap().iter().map(|c| {
        let words:Vec<_> = c["words"].as_array().unwrap().iter().enumerate().map(|(i,w)|{let mut w=w.clone();w["startMs"]=serde_json::json!(i);w["endMs"]=serde_json::json!(i+1);w}).collect();
        let raw:Vec<_> = words.iter().map(|w|serde_json::json!({"text":w["text"],"startSeconds":w["startMs"].as_u64().unwrap() as f64 / 1000.0,"endSeconds":w["endMs"].as_u64().unwrap() as f64 / 1000.0})).collect();
        let targets:Vec<_> = manifest["plan"]["targets"].as_array().unwrap().iter().filter(|t|t["generationKey"]==c["generationKey"]).map(|t|serde_json::json!({"pointer":t["pointer"],"blockId":t["blockId"],"entryId":t["entryId"],"words":[],"issues":[]})).collect();
        serde_json::json!({"clipId":c["id"],"generationKey":c["generationKey"],"sha256":c["result"]["sha256"],"durationMs":c["result"]["durationMs"],"words":words,"rawPredictions":raw,"issues":[],"targets":targets})
    }).collect();
    let report = serde_json::json!({"schemaVersion":"1.0","kind":"brioche-alignment-predictions","planId":plan_id,"planHash":preview["planHash"],"sourceArchiveSha256":format!("{:x}",Sha256::digest(&archive_bytes)),"engine":engine,"reviewRequired":true,"clips":report_clips});
    let alignment_route = "/api/v1/operator/speech-alignments";
    let direct_export_path = format!("/api/v1/operator/speech-plans/{plan_id}/export-direct");
    let response = content_app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&direct_export_path)
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "private, no-store");
    let direct_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let mut automatic = report.clone();
    automatic["kind"] = serde_json::json!("brioche-automatic-alignment-v1");
    automatic["reviewRequired"] = false.into();
    automatic["humanListeningAsserted"] = false.into();
    automatic["originalPredictionReportSha256"] = serde_json::json!("b".repeat(64));
    automatic["sourceArchiveSha256"] = format!("{:x}", Sha256::digest(&direct_bytes)).into();
    for clip in automatic["clips"].as_array_mut().unwrap() {
        let words = clip["words"].clone();
        for target in clip["targets"].as_array_mut().unwrap() {
            let original = manifest["plan"]["targets"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["pointer"] == target["pointer"])
                .unwrap();
            target["words"] = serde_json::json!(
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
    let automatic_path = "/api/v1/operator/speech-packages/automatic";
    let automatic_request = serde_json::json!({"reportJson":automatic.to_string(),"package":{"expectedReportHash":format!("{:x}",Sha256::digest(serde_json::to_vec(&automatic).unwrap())),"lessonRevision":lesson.revision+1,"gapMs":250,"rightsConfirmed":true,"source":"Synthetic protocol recording","license":"Synthetic fixture permission only","creator":"Isolated test","creditZh":"Synthetic fixture","reason":"Synthetic automatic assembly; no human hearing"}});
    for (session, token, expected) in [
        (&cookie, &csrf, 200),
        (&next_cookie, &next_csrf, 403),
        (&cookie, &"bad-csrf".to_owned(), 403),
    ] {
        let response = content_app
            .clone()
            .oneshot(json_write(
                automatic_path,
                &automatic_request,
                session,
                token,
            ))
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), expected);
        if expected == 200 {
            assert_eq!(response.headers()["cache-control"], "private, no-store");
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let mut archive = tar::Archive::new(std::io::Cursor::new(bytes));
            let mut found = false;
            for entry in archive.entries().unwrap() {
                let mut entry = entry.unwrap();
                if entry.path().unwrap().to_string_lossy() == "manifest.json" {
                    let mut bytes = Vec::new();
                    std::io::Read::read_to_end(&mut entry, &mut bytes).unwrap();
                    let result: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                    assert_eq!(result["assembly"]["humanListeningAsserted"], false);
                    assert_eq!(result["assembly"]["approvalRequired"], false);
                    assert_eq!(result["automaticAlignment"], automatic);
                    found = true;
                }
            }
            assert!(found);
        }
    }
    let alignment_id = "88888888888888888888888888888888";
    let alignment_request = serde_json::json!({"id":alignment_id,"planId":plan_id,"expectedPlanHash":preview["planHash"],"reportJson":report.to_string(),"reason":"Independent synthetic alignment"});
    for (session, token) in [
        (next_cookie.as_str(), next_csrf.as_str()),
        (cookie.as_str(), "bad-csrf"),
    ] {
        assert_eq!(
            content_app
                .clone()
                .oneshot(json_write(
                    alignment_route,
                    &alignment_request,
                    session,
                    token
                ))
                .await
                .unwrap()
                .status()
                .as_u16(),
            403
        );
    }
    let (status, alignment) = request(
        &content_app,
        "POST",
        alignment_route,
        Some(alignment_request.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{alignment}");
    let (status, retry) = request(
        &content_app,
        "POST",
        alignment_route,
        Some(alignment_request.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(retry, alignment);
    let mut changed = alignment_request.clone();
    changed["reason"] = "Changed immutable alignment".into();
    assert_eq!(
        request(
            &content_app,
            "POST",
            alignment_route,
            Some(changed),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    let first = &alignment["clips"][0];
    let alignment_review_path = format!(
        "{alignment_route}/{alignment_id}/clips/{}/review",
        first["clipId"].as_str().unwrap()
    );
    let alignment_review = serde_json::json!({"expectedReportHash":alignment["reportHash"],"accepted":true,"heard":true,"timingsChecked":true,"words":first["words"],"reason":"Synthetic alignment protocol decision"});
    for (session, token) in [
        (next_cookie.as_str(), next_csrf.as_str()),
        (cookie.as_str(), "bad-csrf"),
    ] {
        assert_eq!(
            content_app
                .clone()
                .oneshot(json_write(
                    &alignment_review_path,
                    &alignment_review,
                    session,
                    token
                ))
                .await
                .unwrap()
                .status()
                .as_u16(),
            403
        );
    }
    let mut unchecked = alignment_review.clone();
    unchecked["timingsChecked"] = false.into();
    assert_eq!(
        request(
            &content_app,
            "POST",
            &alignment_review_path,
            Some(unchecked),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        400
    );
    let (status, reviewed) = request(
        &content_app,
        "POST",
        &alignment_review_path,
        Some(alignment_review.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{reviewed}");
    assert_eq!(reviewed["clips"][0]["accepted"], true);
    assert_eq!(
        request(
            &content_app,
            "POST",
            &alignment_review_path,
            Some(alignment_review.clone()),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        200
    );
    let (status, read) = request(
        &content_app,
        "GET",
        &format!("{alignment_route}/{alignment_id}"),
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(read, reviewed);
    let (status, list) = request(
        &content_app,
        "GET",
        &format!("{plan_route}/{plan_id}/alignments"),
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{list}");
    assert_eq!(list["items"][0]["acceptedCount"], 1);
    // Complete only synthetic timing decisions for the isolated assembly fixture.
    for c in alignment["clips"].as_array().unwrap().iter().skip(1) {
        let body = serde_json::json!({"expectedReportHash":alignment["reportHash"],"accepted":true,"heard":true,"timingsChecked":true,"words":c["words"],"reason":"Synthetic alignment protocol decision"});
        let path = format!(
            "{alignment_route}/{alignment_id}/clips/{}/review",
            c["clipId"].as_str().unwrap()
        );
        assert_eq!(
            request(
                &content_app,
                "POST",
                &path,
                Some(body),
                &mut cookie,
                &mut csrf
            )
            .await
            .0,
            200
        );
    }
    let package_path = format!("{alignment_route}/{alignment_id}/package");
    let mut package_request = serde_json::json!({"expectedReportHash":alignment["reportHash"],"lessonRevision":lesson.revision+1,"gapMs":250,"rightsConfirmed":true,"source":"Synthetic protocol recording","license":"Synthetic fixture permission only","creator":"Isolated test","creditZh":"Synthetic fixture","reason":"Independent package assembly"});
    for (session, token) in [
        (next_cookie.as_str(), next_csrf.as_str()),
        (cookie.as_str(), "bad-csrf"),
    ] {
        assert_eq!(
            content_app
                .clone()
                .oneshot(json_write(&package_path, &package_request, session, token))
                .await
                .unwrap()
                .status()
                .as_u16(),
            403
        );
    }
    let response = content_app
        .clone()
        .oneshot(json_write(&package_path, &package_request, &cookie, &csrf))
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(response.headers()["cache-control"], "private, no-store");
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let mut archive = tar::Archive::new(std::io::Cursor::new(&bytes));
    let mut package_members = std::collections::BTreeMap::new();
    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        let name = entry.path().unwrap().to_string_lossy().into_owned();
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut bytes).unwrap();
        assert!(package_members.insert(name, bytes).is_none());
    }
    let assembled: serde_json::Value =
        serde_json::from_slice(&package_members["lesson.json"]).unwrap();
    assert_eq!(assembled["revision"], lesson.revision + 1);
    assert_eq!(assembled["editorial"]["status"], "draft");
    let assembled_lesson = chef_engine::project_source(assembled).unwrap();
    assembled_lesson.validate().unwrap();
    assert!(!assembled_lesson.audio.is_empty());
    package_request["lessonRevision"] = (lesson.revision + 2).into();
    let import_path = format!("{package_path}/import");
    let package_import =
        serde_json::json!({"id":"66666666666666666666666666666666","package":package_request});
    for (session, token) in [
        (next_cookie.as_str(), next_csrf.as_str()),
        (cookie.as_str(), "bad-csrf"),
    ] {
        assert_eq!(
            content_app
                .clone()
                .oneshot(json_write(&import_path, &package_import, session, token))
                .await
                .unwrap()
                .status()
                .as_u16(),
            403
        );
    }
    let (status, imported) = request(
        &content_app,
        "POST",
        &import_path,
        Some(package_import.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{imported}");
    assert!(imported["recordingCount"].as_u64().unwrap() > 0);
    assert_eq!(imported["revision"], lesson.revision + 2);
    let (status, retry) = request(
        &content_app,
        "POST",
        &import_path,
        Some(package_import.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(retry, imported);
    let mut changed = package_import.clone();
    changed["package"]["reason"] = "Changed immutable package".into();
    assert_eq!(
        request(
            &content_app,
            "POST",
            &import_path,
            Some(changed),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    let (status, packages) = request(
        &content_app,
        "GET",
        &format!("{alignment_route}/{alignment_id}/packages"),
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(packages["items"], serde_json::json!([imported]));
    let audio_path = format!(
        "/api/v1/operator/lessons/{}/revisions/{}/audio-review",
        lesson.id,
        lesson.revision + 2
    );
    let (status, audio) = request(
        &content_app,
        "GET",
        &audio_path,
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{audio}");
    assert_eq!(audio["published"], false);
    assert_eq!(audio["required"], true);
    let direct_path = format!(
        "/api/v1/operator/lessons/{}/revisions/{}/direct-publication",
        lesson.id,
        lesson.revision + 2
    );
    let direct_request = serde_json::json!({"expectedLessonHash":audio["lessonHash"],"reason":"Synthetic owner authorization, no listening assertion","evidence":{"kind":"isolated-test-only"}});
    let audio_request = serde_json::json!({"expectedLessonHash":audio["lessonHash"],"version":0,"accepted":false,"heard":false,"reason":"Synthetic subsequent rejection"});
    for (path, body) in [
        (&direct_path, &direct_request),
        (&audio_path, &audio_request),
    ] {
        for (session, token) in [
            (next_cookie.as_str(), next_csrf.as_str()),
            (cookie.as_str(), "bad-csrf"),
        ] {
            assert_eq!(
                content_app
                    .clone()
                    .oneshot(json_write(path, body, session, token))
                    .await
                    .unwrap()
                    .status()
                    .as_u16(),
                403
            );
        }
    }
    let (status, authorized) = request(
        &content_app,
        "POST",
        &direct_path,
        Some(direct_request.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{authorized}");
    assert_eq!(authorized["directAuthorized"], true);
    assert_eq!(authorized["accepted"], true);
    assert_eq!(
        request(
            &content_app,
            "POST",
            &direct_path,
            Some(direct_request.clone()),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        200
    );
    let (status, rejected) = request(
        &content_app,
        "POST",
        &audio_path,
        Some(audio_request.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{rejected}");
    assert_eq!(rejected["version"], 1);
    let (_, current) = request(
        &content_app,
        "GET",
        &audio_path,
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(current["directAuthorized"], false);
    assert_eq!(current["accepted"], false);
    let audio_accept = serde_json::json!({"expectedLessonHash":audio["lessonHash"],"version":1,"accepted":true,"heard":true,"reason":"Synthetic final audio decision"});
    let mut invalid = audio_accept.clone();
    invalid["heard"] = false.into();
    assert_eq!(
        request(
            &content_app,
            "POST",
            &audio_path,
            Some(invalid),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        400
    );
    let (status, accepted) = request(
        &content_app,
        "POST",
        &audio_path,
        Some(audio_accept.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{accepted}");
    assert_eq!(accepted["accepted"], true);
    assert_eq!(accepted["version"], 2);
    assert_eq!(
        request(
            &content_app,
            "POST",
            &audio_path,
            Some(audio_accept.clone()),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        200
    );
    assert_eq!(
        request(
            &content_app,
            "POST",
            &audio_path,
            Some(audio_request.clone()),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    // Revoke after the first HTTP verification while the write waits on the same
    // database advisory lock used by identity membership mutations.
    let preview_path = format!(
        "/api/v1/operator/lessons/{}/revisions/{}",
        lesson.id,
        lesson.revision + 2
    );
    let (status, preview_lesson) = request(
        &content_app,
        "GET",
        &preview_path,
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{preview_lesson}");
    assert_eq!(preview_lesson["revision"], lesson.revision + 2);
    assert!(preview_lesson.get("serverOnly").is_none());
    assert!(preview_lesson.get("editorial").is_none());
    for asset in preview_lesson["media"]
        .as_array()
        .unwrap()
        .iter()
        .chain(preview_lesson["audio"].as_array().unwrap())
    {
        assert!(
            asset["url"]
                .as_str()
                .unwrap()
                .starts_with(&format!("{preview_path}/"))
        );
    }
    let media_path = preview_lesson["media"][0]["url"]
        .as_str()
        .unwrap()
        .to_owned();
    let recording_path = preview_lesson["audio"][0]["url"]
        .as_str()
        .unwrap()
        .to_owned();
    for path in [&preview_path, &media_path, &recording_path] {
        assert_eq!(
            request(
                &content_app,
                "GET",
                path,
                None,
                &mut next_cookie,
                &mut next_csrf
            )
            .await
            .0,
            403
        );
        assert_eq!(
            content_app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status()
                .as_u16(),
            401
        );
    }
    let response = content_app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&media_path)
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    assert!(
        response.headers()["cache-control"]
            .to_str()
            .unwrap()
            .contains("no-store")
    );
    assert!(
        String::from_utf8_lossy(&response.into_body().collect().await.unwrap().to_bytes())
            .contains("<svg")
    );
    let response = content_app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&recording_path)
                .header("cookie", &cookie)
                .header("range", "bytes=0-11")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 206);
    assert!(
        response.headers()["cache-control"]
            .to_str()
            .unwrap()
            .contains("no-store")
    );
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(bytes.len(), 12);
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WAVE");
    let exercise = preview_lesson["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["exerciseType"] == "single-choice")
        .unwrap();
    let grade_path = format!("{preview_path}/grade");
    let grade_request = serde_json::json!({"revision":lesson.revision+2,"exerciseId":exercise["id"],"answer":{"kind":"choice","optionId":exercise["options"][0]["id"]}});
    let progress_sql = "SELECT (SELECT count(*) FROM learning_sessions) AS sessions,(SELECT count(*) FROM exercise_attempts) AS attempts";
    let progress = owner
        .query_one_raw(Statement::from_string(DbBackend::Postgres, progress_sql))
        .await
        .unwrap()
        .unwrap();
    for (session, token) in [
        (next_cookie.as_str(), next_csrf.as_str()),
        (cookie.as_str(), "bad-csrf"),
    ] {
        assert_eq!(
            content_app
                .clone()
                .oneshot(json_write(&grade_path, &grade_request, session, token))
                .await
                .unwrap()
                .status()
                .as_u16(),
            403
        );
    }
    let (status, grade) = request(
        &content_app,
        "POST",
        &grade_path,
        Some(grade_request.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{grade}");
    assert_eq!(grade["exerciseId"], exercise["id"]);
    assert!(grade["correct"].is_boolean());
    let after = owner
        .query_one_raw(Statement::from_string(DbBackend::Postgres, progress_sql))
        .await
        .unwrap()
        .unwrap();
    for key in ["sessions", "attempts"] {
        assert_eq!(
            progress.try_get::<i64>("", key).unwrap(),
            after.try_get::<i64>("", key).unwrap()
        );
    }
    assert_eq!(
        request(
            &content_app,
            "GET",
            "/api/v1/operator/lessons/split-admin-lesson/revisions/1",
            None,
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        410
    );
    let (status, release_preview) = request(
        &content_app,
        "GET",
        "/api/v1/operator/releases/split-admin-release",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{release_preview}");
    assert_eq!(
        release_preview["withdrawnLessonIds"],
        serde_json::json!(["split-admin-lesson"])
    );
    imported_source["id"] = "split-revoked-import".into();
    let course_request = Request::builder().method("POST").uri("/api/v1/operator/lessons/import")
        .header("origin", ORIGIN).header("cookie", &cookie).header("x-csrf-token", &csrf)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::json!({"document":imported_source.to_string(),"reason":"Must not retain revoked authorization"}).to_string())).unwrap();
    // Test each write separately: concurrent parsing is intentionally capped at two.
    let mut revoked_character = character.clone();
    revoked_character["characterId"] = "split-revoked-character".into();
    let mut revoked_voice = voice.clone();
    revoked_voice["expectedVoiceRevision"] = 3.into();
    let mut revoked_audition = audition_request.clone();
    revoked_audition["id"] = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into();
    let mut revoked_plan = plan_request.clone();
    revoked_plan["id"] = "dddddddddddddddddddddddddddddddd".into();
    let mut revoked_clip = clip_request.clone();
    revoked_clip["id"] = "99999999999999999999999999999999".into();
    revoked_clip["expectedPreviousId"] = "ffffffffffffffffffffffffffffffff".into();
    let mut revoked_alignment = alignment_request.clone();
    revoked_alignment["id"] = "77777777777777777777777777777777".into();
    let mut revoked_package = package_import.clone();
    revoked_package["id"] = "55555555555555555555555555555555".into();
    revoked_package["package"]["lessonRevision"] = (lesson.revision + 3).into();
    let revoked_package_export = revoked_package["package"].clone();
    for pending in [
        course_request,
        asset_upload("split-revoked-upload", &cookie, &csrf),
        recording_upload("split-revoked-recording", &cookie, &csrf),
        json_write(
            "/api/v1/operator/characters/revisions",
            &revoked_character,
            &cookie,
            &csrf,
        ),
        json_write(
            "/api/v1/operator/characters",
            &revoked_voice,
            &cookie,
            &csrf,
        ),
        json_write(grant_route, &grant_request, &cookie, &csrf),
        json_write(&live_revoke, &revoke_request, &cookie, &csrf),
        json_write(job_route, &job_request, &cookie, &csrf),
        json_write(
            &job_check,
            &serde_json::json!({"expectedVersion":4,"reason":"Must not query after revocation"}),
            &cookie,
            &csrf,
        ),
        json_write(audition_route, &revoked_audition, &cookie, &csrf),
        json_write(&audition_review, &review_request, &cookie, &csrf),
        json_write(&preview_route, &preview_request, &cookie, &csrf),
        json_write(plan_route, &revoked_plan, &cookie, &csrf),
        json_write(clip_route, &revoked_clip, &cookie, &csrf),
        json_write(&clip_review_path, &clip_review, &cookie, &csrf),
        Request::builder()
            .uri(&export_path)
            .header("cookie", &cookie)
            .body(Body::empty())
            .unwrap(),
        json_write(alignment_route, &revoked_alignment, &cookie, &csrf),
        json_write(&alignment_review_path, &alignment_review, &cookie, &csrf),
        json_write(&package_path, &revoked_package_export, &cookie, &csrf),
        json_write(&import_path, &revoked_package, &cookie, &csrf),
        json_write(&audio_path, &audio_accept, &cookie, &csrf),
        json_write(&direct_path, &direct_request, &cookie, &csrf),
        json_write(&grade_path, &grade_request, &cookie, &csrf),
        json_write(automatic_path, &automatic_request, &cookie, &csrf),
        Request::builder()
            .uri(&direct_export_path)
            .header("cookie", &cookie)
            .body(Body::empty())
            .unwrap(),
    ] {
        let held = owner.begin().await.unwrap();
        held.execute_unprepared(
            "SELECT pg_advisory_xact_lock(hashtextextended('account-admin',0))",
        )
        .await
        .unwrap();
        let copy = content_app.clone();
        let blocked = tokio::spawn(async move { copy.oneshot(pending).await.unwrap() });
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,
            "SELECT count(*)::bigint AS waiting FROM pg_locks l JOIN pg_stat_activity a ON a.pid=l.pid WHERE l.locktype='advisory' AND NOT l.granted AND a.usename=$1",[content_role.clone().into()])).await.unwrap().unwrap();
            if row.try_get::<i64>("", "waiting").unwrap() == 1 {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "Content write did not reach authorization lock"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        held.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,
        format!("UPDATE \"{target}\".product_memberships SET role='learner',version=version+1 WHERE product_id='brioche' AND user_id=$1"),[account.into()])).await.unwrap();
        held.commit().await.unwrap();
        assert_eq!(blocked.await.unwrap().status().as_u16(), 403);
        assert_eq!(
            content_app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(live["path"].as_str().unwrap())
                        .body(Body::empty())
                        .unwrap()
                )
                .await
                .unwrap()
                .status()
                .as_u16(),
            404
        );
        let restore = owner.begin().await.unwrap();
        restore
            .execute_unprepared("SELECT pg_advisory_xact_lock(hashtextextended('account-admin',0))")
            .await
            .unwrap();
        restore.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,format!("UPDATE \"{target}\".product_memberships SET role='operator',version=version+1 WHERE product_id='brioche' AND user_id=$1"),[account.into()])).await.unwrap();
        restore.commit().await.unwrap();
    }
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT count(*)::bigint AS n FROM audio_assets WHERE asset_id='split-revoked-recording'")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 0);
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT count(*)::bigint AS n FROM course_speech_plans WHERE id='cccccccccccccccccccccccccccccccc' AND actor_id=$1 AND reason='Independent immutable course plan'",[account.into()])).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 1);
    let row = owner
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(*)::bigint AS n FROM course_speech_plans",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.try_get::<i64>("", "n").unwrap(),
        speech_work_snapshot[0].0 + 1
    );
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM character_revisions WHERE character_id='split-revoked-character') + (SELECT count(*) FROM character_voice_profiles WHERE character_id='split-character' AND revision=4) AS n")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 0);
    let row = owner
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(*)::bigint AS n FROM voice_reference_grants WHERE character_id='split-character'",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 2);
    let row = owner
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(*)::bigint AS n FROM voice_reference_revocations r JOIN voice_reference_grants g ON g.id=r.grant_id WHERE g.character_id='split-character'",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 1);
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT count(*)::bigint AS n FROM character_voice_profiles WHERE character_id='split-character' AND revision=1 AND actor_id=$1 AND reason='Independent voice direction'",[account.into()])).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 1);
    assert!(
        content
            .execute_unprepared("UPDATE character_voice_profiles SET profile='{}' WHERE false")
            .await
            .is_err()
    );
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT (SELECT count(*) FROM asset_import_audit WHERE actor_id=$1 AND target='split-upload v1') + (SELECT count(*) FROM audio_import_audit WHERE actor_id=$1 AND target='split-recording v1') AS n",[account.into()])).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 2);
    assert!(
        content
            .execute_unprepared("UPDATE media_assets SET descriptor='{}' WHERE false")
            .await
            .is_err()
    );
    assert!(
        content
            .execute_unprepared("DELETE FROM audio_import_audit WHERE false")
            .await
            .is_err()
    );
    let row = owner
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(*)::bigint AS n FROM media_assets WHERE asset_id='split-revoked-upload'",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 0);
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT count(*)::bigint AS n FROM lesson_revisions WHERE lesson_id='split-revoked-import'")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 0);
    let row = owner
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT count(*)::bigint AS n FROM voice_clone_events WHERE job_id=$1",
            [submitted["id"].as_str().unwrap().into()],
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 4);
    assert_eq!(
        enrollment.creates.load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    assert_eq!(
        enrollment.queries.load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    assert!(
        content
            .execute_unprepared("DELETE FROM voice_clone_events WHERE false")
            .await
            .is_err()
    );
    // A different valid operator cannot consume a revoked issuer's reference.
    let next_account = next["user"]["id"].as_str().unwrap().parse::<i64>().unwrap();
    let held = owner.begin().await.unwrap();
    held.execute_unprepared("SELECT pg_advisory_xact_lock(hashtextextended('account-admin',0))")
        .await
        .unwrap();
    held.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,format!("UPDATE \"{target}\".product_memberships SET role=CASE WHEN user_id=$1 THEN 'learner' ELSE 'operator' END,version=version+1 WHERE product_id='brioche' AND user_id IN($1,$2)"),[account.into(),next_account.into()])).await.unwrap();
    held.commit().await.unwrap();
    assert_eq!(
        request(
            &content_app,
            "POST",
            job_route,
            Some(job_request.clone()),
            &mut next_cookie,
            &mut next_csrf
        )
        .await
        .0,
        404
    );
    let restore = owner.begin().await.unwrap();
    restore
        .execute_unprepared("SELECT pg_advisory_xact_lock(hashtextextended('account-admin',0))")
        .await
        .unwrap();
    restore.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,format!("UPDATE \"{target}\".product_memberships SET role=CASE WHEN user_id=$1 THEN 'operator' ELSE 'learner' END,version=version+1 WHERE product_id='brioche' AND user_id IN($1,$2)"),[account.into(),next_account.into()])).await.unwrap();
    restore.commit().await.unwrap();
    assert_eq!(
        enrollment.creates.load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    assert_eq!(
        enrollment
            .syntheses
            .load(std::sync::atomic::Ordering::SeqCst),
        keys.len() + 1
    );
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT count(*)::bigint AS n FROM course_speech_clips WHERE id='eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee' AND actor_id=$1 AND reason='Independent synthetic course clip'",[account.into()])).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 1);
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT count(*)::bigint AS n FROM course_speech_clip_reviews WHERE clip_id='eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee' AND actor_id=$1 AND reason='Synthetic clip decision'",[account.into()])).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 1);
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM course_speech_clips) AS clips,(SELECT count(*) FROM course_speech_clip_events) AS events,(SELECT count(*) FROM course_speech_clip_reviews) AS reviews")).await.unwrap().unwrap();
    assert_eq!(
        row.try_get::<i64>("", "clips").unwrap(),
        keys.len() as i64 + 1 + speech_work_snapshot[1].0
    );
    assert_eq!(
        row.try_get::<i64>("", "events").unwrap(),
        2 * keys.len() as i64 + 1 + speech_work_snapshot[2].0
    );
    assert_eq!(
        row.try_get::<i64>("", "reviews").unwrap(),
        keys.len() as i64 + speech_work_snapshot[3].0
    );
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT count(*)::bigint AS n FROM speech_alignments WHERE id='88888888888888888888888888888888' AND actor_id=$1 AND reason='Independent synthetic alignment'",[account.into()])).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 1);
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM speech_alignments) AS reports,(SELECT count(*) FROM speech_alignment_reviews) AS reviews")).await.unwrap().unwrap();
    assert_eq!(
        row.try_get::<i64>("", "reports").unwrap(),
        speech_work_snapshot[4].0 + 1
    );
    assert_eq!(
        row.try_get::<i64>("", "reviews").unwrap(),
        keys.len() as i64 + speech_work_snapshot[5].0
    );
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM speech_package_imports) AS packages,(SELECT count(*) FROM lesson_audio_reviews) AS decisions,(SELECT count(*) FROM lesson_direct_publications) AS direct")).await.unwrap().unwrap();
    assert_eq!(
        row.try_get::<i64>("", "packages").unwrap(),
        speech_work_snapshot[6].0 + 1
    );
    assert_eq!(row.try_get::<i64>("", "decisions").unwrap(), 2);
    assert_eq!(row.try_get::<i64>("", "direct").unwrap(), 1);
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM voice_auditions WHERE id='bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb') + (SELECT count(*) FROM voice_audition_events WHERE audition_id='bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb') AS n")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 0);
    let before_b = chef_engine::content::catalog_matching_for_product(
        &learning,
        Some(ProductId::Brioche),
        &[],
    )
    .await
    .unwrap();
    assert!(
        chef_engine::content::catalog_matching_for_product(&learning, Some(ProductId::Hargow), &[])
            .await
            .unwrap()
            .levels
            .is_empty()
    );
    let (status, visual_before) = request(
        &content_app,
        "GET",
        "/api/v1/operator/assets",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{visual_before}");
    // Poisoned foreign descriptors would fail parsing if the product filter were
    // absent; 25 early IDs also detect foreign records consuming pagination.
    owner.execute_unprepared("INSERT INTO media_assets(product_id,asset_id,revision,descriptor,provenance,sha256,extension,byte_size) SELECT 'hargow','aaa-foreign-visual-'||i,1,'{}',provenance,sha256,extension,byte_size FROM media_assets CROSS JOIN generate_series(1,25) i WHERE asset_id='art-bakery-morning' AND revision=1").await.unwrap();
    let (status, visual_after) = request(
        &content_app,
        "GET",
        "/api/v1/operator/assets",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{visual_after}");
    assert_eq!(visual_after, visual_before);
    let (status, filtered) = request(
        &content_app,
        "GET",
        "/api/v1/operator/assets?q=foreign-visual",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{filtered}");
    assert!(filtered["items"].as_array().unwrap().is_empty());
    assert!(filtered["next"].is_null());
    let (status, body) = request(
        &content_app,
        "GET",
        "/api/v1/operator/assets/aaa-foreign-visual-1/1/file",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(
        request(
            &content_app,
            "GET",
            "/api/v1/operator/assets?product=hargow",
            None,
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        400
    );
    let (status, characters_before) = request(
        &content_app,
        "GET",
        "/api/v1/operator/characters",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{characters_before}");
    owner.execute_unprepared("INSERT INTO character_revisions(product_id,character_id,revision,snapshot,avatar_id,avatar_revision) SELECT 'hargow','aaa-foreign-character-'||i,1,'{}','aaa-foreign-visual-1',1 FROM generate_series(1,25) i").await.unwrap();
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO character_voice_profiles(product_id,character_id,character_revision,revision,profile,actor_id,reason) SELECT 'hargow',character_id,1,1,'{}',$1,'Poisoned foreign fixture' FROM character_revisions WHERE product_id='hargow' AND character_id LIKE 'aaa-foreign-character-%'",[account.into()])).await.unwrap();
    let (status, characters_after) = request(
        &content_app,
        "GET",
        "/api/v1/operator/characters",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{characters_after}");
    assert_eq!(
        characters_before, characters_after,
        "foreign records cannot consume pagination or enter profile parsing"
    );
    for path in [
        "/api/v1/operator/characters/aaa-foreign-character-1/1",
        "/api/v1/operator/characters/aaa-foreign-character-1/1/avatar",
        "/api/v1/operator/characters/aaa-foreign-character-1/1/voices/1",
    ] {
        let (status, body) = request(&content_app, "GET", path, None, &mut cookie, &mut csrf).await;
        assert_eq!(status, 404, "{path}: {body}");
    }
    for path in [
        "/api/v1/operator/characters?product=hargow",
        "/api/v1/operator/characters/split-character/2?product=hargow",
        "/api/v1/operator/characters/split-character/2/avatar?product=hargow",
        "/api/v1/operator/characters/split-character/2/voices/3?product=hargow",
    ] {
        let (status, body) = request(&content_app, "GET", path, None, &mut cookie, &mut csrf).await;
        assert_eq!(status, 400, "{path}: {body}");
    }
    let before=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM character_revisions)+(SELECT count(*) FROM character_voice_profiles)+(SELECT count(*) FROM asset_import_audit) AS n")).await.unwrap().unwrap().try_get::<i64>("","n").unwrap();
    let mut foreign_voice = voice.clone();
    foreign_voice["characterId"] = "aaa-foreign-character-1".into();
    foreign_voice["characterRevision"] = 1.into();
    foreign_voice["expectedVoiceRevision"] = 1.into();
    let (status, body) = request(
        &content_app,
        "POST",
        "/api/v1/operator/characters",
        Some(foreign_voice),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 404, "{body}");
    let mut foreign_character = character.clone();
    foreign_character["characterId"] = "aaa-foreign-character-1".into();
    foreign_character["expectedRevision"] = 0.into();
    foreign_character["avatarId"] = "aaa-foreign-visual-1".into();
    let (status, body) = request(
        &content_app,
        "POST",
        "/api/v1/operator/characters/revisions",
        Some(foreign_character),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 400, "{body}");
    let after=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM character_revisions)+(SELECT count(*) FROM character_voice_profiles)+(SELECT count(*) FROM asset_import_audit) AS n")).await.unwrap().unwrap().try_get::<i64>("","n").unwrap();
    assert_eq!(
        before, after,
        "cross-product character/voice writes and import audit remain unchanged"
    );
    owner.execute_unprepared("INSERT INTO media_assets(product_id,asset_id,revision,descriptor,provenance,sha256,extension,byte_size) SELECT 'hargow','foreign-course-illustration',1,jsonb_set(descriptor,'{assetId}','\"foreign-course-illustration\"'),provenance,sha256,extension,byte_size FROM media_assets WHERE asset_id='art-bakery-morning' AND revision=1").await.unwrap();
    let mut foreign_visual_source: serde_json::Value = serde_json::from_str(
        &imported_source
            .to_string()
            .replace("art-bakery-morning", "foreign-course-illustration"),
    )
    .unwrap();
    foreign_visual_source["id"] = "brioche-foreign-visual-source".into();
    chef_engine::validate_source_schema(foreign_visual_source.clone()).unwrap();
    let hydrated = chef_engine::media::hydrate_source(&owner, foreign_visual_source.clone())
        .await
        .unwrap();
    let mixed_lesson = chef_engine::project_source(hydrated.clone()).unwrap();
    // The descriptor and on-disk object are valid in the legacy global registry:
    // only product ownership should prevent this course from being accepted.
    chef_engine::media::validate_lesson(&owner, &mixed_lesson, &root)
        .await
        .unwrap();
    let visual_document = serde_json::json!({"document":foreign_visual_source.to_string(),"reason":"Foreign visual source rejected"});
    let (status, report) = request(
        &content_app,
        "POST",
        "/api/v1/operator/documents/lesson/check",
        Some(visual_document.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{report}");
    assert_eq!(report["valid"], false);
    assert_eq!(report["issue"]["pointer"], "/assetRefs/0/revision");
    assert_eq!(
        request(
            &content_app,
            "POST",
            "/api/v1/operator/lessons/import",
            Some(visual_document),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        400
    );
    let mut embedded = hydrated;
    embedded.as_object_mut().unwrap().remove("assetRefs");
    embedded["id"] = "brioche-foreign-embedded-visual".into();
    chef_engine::validate_source_schema(embedded.clone()).unwrap();
    let (status, report) = request(&content_app,"POST","/api/v1/operator/documents/lesson/check",Some(serde_json::json!({"document":embedded.to_string(),"reason":"Embedded foreign visual rejected"})),&mut cookie,&mut csrf).await;
    assert_eq!(status, 200, "{report}");
    assert_eq!(report["valid"], false);
    assert_eq!(report["issue"]["pointer"], "/media/0/revision");
    assert_eq!(request(&content_app,"POST","/api/v1/operator/lessons/import",Some(serde_json::json!({"document":embedded.to_string(),"reason":"Embedded foreign import rejected"})),&mut cookie,&mut csrf).await.0,400);
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM lesson_revisions WHERE lesson_id IN ('brioche-foreign-visual-source','brioche-foreign-embedded-visual'))+(SELECT count(*) FROM lesson_import_audit WHERE lesson_id IN ('brioche-foreign-visual-source','brioche-foreign-embedded-visual')) AS n")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 0);
    owner.execute_unprepared("INSERT INTO media_assets(product_id,asset_id,revision,descriptor,provenance,sha256,extension,byte_size) SELECT 'hargow','foreign-cast-avatar',1,jsonb_set(descriptor,'{assetId}','\"foreign-cast-avatar\"'),provenance,sha256,extension,byte_size FROM media_assets WHERE asset_id='avatar-camille-v1' AND revision=1").await.unwrap();
    owner.execute_unprepared("INSERT INTO character_revisions(product_id,character_id,revision,snapshot,avatar_id,avatar_revision) SELECT 'hargow','foreign-cast-member',1,jsonb_set(jsonb_set(snapshot,'{characterId}','\"foreign-cast-member\"'),'{avatarId}','\"foreign-cast-avatar\"'),'foreign-cast-avatar',1 FROM character_revisions WHERE character_id='character-camille' AND revision=1").await.unwrap();
    let mut foreign_cast_source: serde_json::Value = serde_json::from_str(
        &imported_source
            .to_string()
            .replace("\"character-camille\"", "\"foreign-cast-member\""),
    )
    .unwrap();
    foreign_cast_source["id"] = "brioche-foreign-cast-source".into();
    chef_engine::validate_source_schema(foreign_cast_source.clone()).unwrap();
    chef_engine::project_source(foreign_cast_source.clone()).unwrap();
    let cast_document = serde_json::json!({"document":foreign_cast_source.to_string(),"reason":"Foreign cast source rejected"});
    let (status, report) = request(
        &content_app,
        "POST",
        "/api/v1/operator/documents/lesson/check",
        Some(cast_document.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{report}");
    assert_eq!(report["valid"], false);
    assert_eq!(report["issue"]["pointer"], "/cast/0/revision");
    assert_eq!(
        request(
            &content_app,
            "POST",
            "/api/v1/operator/lessons/import",
            Some(cast_document),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        400
    );
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM lesson_revisions WHERE lesson_id='brioche-foreign-cast-source')+(SELECT count(*) FROM lesson_import_audit WHERE lesson_id='brioche-foreign-cast-source') AS n")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 0);
    let (status, recordings_before) = request(
        &content_app,
        "GET",
        "/api/v1/operator/recordings",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{recordings_before}");
    owner.execute_unprepared("INSERT INTO audio_assets(product_id,asset_id,revision,descriptor,provenance,sha256,extension,byte_size,duration_ms,sample_rate,channels) SELECT 'hargow','aaa-foreign-recording-'||i,1,'{}',provenance,sha256,extension,byte_size,duration_ms,sample_rate,channels FROM audio_assets CROSS JOIN generate_series(1,25) i WHERE asset_id='layout-recording-fixture' AND revision=1").await.unwrap();
    let (status, recordings_after) = request(
        &content_app,
        "GET",
        "/api/v1/operator/recordings",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{recordings_after}");
    assert_eq!(recordings_before, recordings_after);
    let (status, filtered) = request(
        &content_app,
        "GET",
        "/api/v1/operator/recordings?q=foreign-recording",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{filtered}");
    assert!(filtered["items"].as_array().unwrap().is_empty());
    assert!(filtered["next"].is_null());
    let (status, body) = request(
        &content_app,
        "GET",
        "/api/v1/operator/recordings/aaa-foreign-recording-1/1/file",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(
        request(
            &content_app,
            "GET",
            "/api/v1/operator/recordings?product=hargow",
            None,
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        400
    );
    let before = owner
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(*)::bigint AS n FROM audio_import_audit",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "n")
        .unwrap();
    owner.execute_unprepared("ALTER TABLE audio_assets DROP CONSTRAINT chef_local_recording_primary; ALTER TABLE audio_assets ADD CONSTRAINT legacy_recording_primary_fixture PRIMARY KEY(asset_id,revision)").await.unwrap();
    let recording_collision = content_app
        .clone()
        .oneshot(recording_upload("aaa-foreign-recording-1", &cookie, &csrf))
        .await
        .unwrap();
    assert_eq!(recording_collision.status().as_u16(), 409);
    owner.execute_unprepared("ALTER TABLE audio_assets DROP CONSTRAINT legacy_recording_primary_fixture; ALTER TABLE audio_assets ADD CONSTRAINT chef_local_recording_primary PRIMARY KEY(product_id,asset_id,revision)").await.unwrap();
    let after = owner
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(*)::bigint AS n FROM audio_import_audit",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "n")
        .unwrap();
    assert_eq!(before, after);
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT product_id,descriptor FROM audio_assets WHERE asset_id='aaa-foreign-recording-1' AND revision=1")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<String>("", "product_id").unwrap(), "hargow");
    assert_eq!(
        row.try_get::<serde_json::Value>("", "descriptor").unwrap(),
        serde_json::json!({})
    );
    owner.execute_unprepared("INSERT INTO audio_assets(product_id,asset_id,revision,descriptor,provenance,sha256,extension,byte_size,duration_ms,sample_rate,channels) SELECT 'hargow','foreign-course-recording',1,jsonb_set(descriptor,'{assetId}','\"foreign-course-recording\"'),jsonb_set(provenance,'{assetId}','\"foreign-course-recording\"'),sha256,extension,byte_size,duration_ms,sample_rate,channels FROM audio_assets WHERE asset_id='layout-recording-fixture' AND revision=1").await.unwrap();
    let (status, references_before) = request(
        &content_app,
        "GET",
        "/api/v1/operator/voice-references",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{references_before}");
    let foreign_token = "a".repeat(64);
    let foreign_hash = format!("{:x}", Sha256::digest(foreign_token.as_bytes()));
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO voice_reference_grants(product_id,id,token_hash,character_id,character_revision,voice_revision,asset_id,asset_revision,descriptor,reference,actor_id,reason,model,single_speaker_confirmed,expires_at) SELECT 'hargow',lpad(to_hex(i),32,'0'),CASE WHEN i=1 THEN $1 ELSE md5(i::text)||md5(i::text) END,'aaa-foreign-character-'||i,1,1,'foreign-course-recording',1,'{}','{}',$2,'Poisoned foreign reference','qwen-audio-3.1-tts-flash',true,CURRENT_TIMESTAMP+interval '5 minutes' FROM generate_series(1,25) i",[foreign_hash.into(),account.into()])).await.unwrap();
    owner.execute_unprepared("INSERT INTO voice_reference_reads(product_id,grant_id) VALUES('hargow',lpad(to_hex(1),32,'0'))").await.unwrap();
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO voice_reference_revocations(product_id,grant_id,actor_id,reason) VALUES('hargow',lpad(to_hex(2),32,'0'),$1,'Synthetic foreign revocation')",[account.into()])).await.unwrap();
    let (status, references_after) = request(
        &content_app,
        "GET",
        "/api/v1/operator/voice-references",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{references_after}");
    assert_eq!(
        references_before, references_after,
        "foreign grants cannot consume pagination or affect own read/revoke projection"
    );
    let (status, body) = request(
        &content_app,
        "GET",
        "/api/v1/operator/voice-references?product=hargow",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 400, "{body}");
    let before=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM voice_reference_grants)+(SELECT count(*) FROM voice_reference_revocations)+(SELECT count(*) FROM voice_reference_reads) AS n")).await.unwrap().unwrap().try_get::<i64>("","n").unwrap();
    let (status,body)=request(&content_app,"POST","/api/v1/operator/voice-references",Some(serde_json::json!({"characterId":"aaa-foreign-character-1","characterRevision":1,"voiceRevision":1,"singleSpeakerConfirmed":true,"reason":"Must not issue a foreign reference"})),&mut cookie,&mut csrf).await;
    assert_eq!(status, 404, "{body}");
    let foreign_path = format!("/api/v1/voice-references/{:032x}/{foreign_token}", 1);
    // The valid foreign token must fail before acquiring the account-admin lock.
    let held = owner.begin().await.unwrap();
    held.execute_unprepared("SELECT pg_advisory_xact_lock(hashtextextended('account-admin',0))")
        .await
        .unwrap();
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        content_app.clone().oneshot(
            Request::builder()
                .uri(&foreign_path)
                .body(Body::empty())
                .unwrap(),
        ),
    )
    .await
    .expect("foreign bearer precheck must not wait on account administration")
    .unwrap();
    assert_eq!(response.status().as_u16(), 404);
    held.rollback().await.unwrap();
    let (status, body) = request(
        &content_app,
        "POST",
        &format!("/api/v1/operator/voice-references/{:032x}/revoke", 1),
        Some(serde_json::json!({"reason":"Must not revoke a foreign reference"})),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 404, "{body}");
    let response = content_app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("{foreign_path}?product=hargow"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 400);
    let after=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM voice_reference_grants)+(SELECT count(*) FROM voice_reference_revocations)+(SELECT count(*) FROM voice_reference_reads) AS n")).await.unwrap().unwrap().try_get::<i64>("","n").unwrap();
    assert_eq!(
        before, after,
        "foreign issue, revoke and download leave authorization/read audit unchanged"
    );
    let (status, jobs_before) = request(
        &content_app,
        "GET",
        "/api/v1/operator/voice-jobs",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{jobs_before}");
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO voice_clone_jobs(product_id,id,grant_id,prefix,actor_id,reason) SELECT 'hargow',lpad(to_hex(100+i),32,'0'),lpad(to_hex(i),32,'0'),'hf'||i,$1,'Synthetic foreign clone' FROM generate_series(1,25) i",[account.into()])).await.unwrap();
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO voice_clone_events(product_id,job_id,version,status,voice_id,actor_id,reason) SELECT 'hargow',id,1,'ready','qwen-audio-3.1-tts-flash-'||prefix||'-fixture',$1,'Synthetic foreign clone' FROM voice_clone_jobs WHERE product_id='hargow'",[account.into()])).await.unwrap();
    let (status, jobs_after) = request(
        &content_app,
        "GET",
        "/api/v1/operator/voice-jobs",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{jobs_after}");
    assert_eq!(
        jobs_before, jobs_after,
        "foreign jobs do not consume pagination or appear in the list"
    );
    let foreign_job = format!("{:032x}", 101);
    let job_path = format!("/api/v1/operator/voice-jobs/{foreign_job}");
    let (status, body) =
        request(&content_app, "GET", &job_path, None, &mut cookie, &mut csrf).await;
    assert_eq!(status, 404, "{body}");
    for path in [
        "/api/v1/operator/voice-jobs?product=hargow".to_owned(),
        format!("{job_path}?product=hargow"),
    ] {
        let (status, body) =
            request(&content_app, "GET", &path, None, &mut cookie, &mut csrf).await;
        assert_eq!(status, 400, "{path}: {body}");
    }
    let before=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM voice_clone_jobs)+(SELECT count(*) FROM voice_clone_events) AS n")).await.unwrap().unwrap().try_get::<i64>("","n").unwrap();
    let calls_before = [
        enrollment.creates.load(std::sync::atomic::Ordering::SeqCst),
        enrollment.queries.load(std::sync::atomic::Ordering::SeqCst),
        enrollment
            .syntheses
            .load(std::sync::atomic::Ordering::SeqCst),
    ];
    let (status,body)=request(&content_app,"POST","/api/v1/operator/voice-jobs",Some(serde_json::json!({"grantId":format!("{:032x}",1),"token":foreign_token,"costConfirmed":true,"reason":"Must not enroll foreign reference"})),&mut cookie,&mut csrf).await;
    assert_eq!(status, 404, "{body}");
    let (status, body) = request(
        &content_app,
        "POST",
        &format!("{job_path}/check"),
        Some(serde_json::json!({"expectedVersion":1,"reason":"Must not query foreign voice"})),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 404, "{body}");
    let after=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM voice_clone_jobs)+(SELECT count(*) FROM voice_clone_events) AS n")).await.unwrap().unwrap().try_get::<i64>("","n").unwrap();
    assert_eq!(
        before, after,
        "foreign enrollment and recovery leave jobs/events unchanged"
    );
    assert_eq!(
        calls_before,
        [
            enrollment.creates.load(std::sync::atomic::Ordering::SeqCst),
            enrollment.queries.load(std::sync::atomic::Ordering::SeqCst),
            enrollment
                .syntheses
                .load(std::sync::atomic::Ordering::SeqCst)
        ],
        "foreign requests never reach the provider"
    );

    let (status, auditions_before) = request(
        &content_app,
        "GET",
        audition_route,
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{auditions_before}");
    let history_before = history_pages(&content_app, &mut cookie, &mut csrf).await;
    let own_alignment_list = format!("{plan_route}/{plan_id}/alignments");
    let (status, alignments_before) = request(
        &content_app,
        "GET",
        &own_alignment_list,
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{alignments_before}");
    let own_package_list = format!("{alignment_route}/{alignment_id}/packages");
    let (status, packages_before) = request(
        &content_app,
        "GET",
        &own_package_list,
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{packages_before}");
    product_speech_work::seed_foreign(&owner, account).await;
    let (status, packages_after) = request(
        &content_app,
        "GET",
        &own_package_list,
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{packages_after}");
    assert_eq!(
        packages_before, packages_after,
        "foreign package receipts do not affect own list"
    );
    let (status, alignments_after) = request(
        &content_app,
        "GET",
        &own_alignment_list,
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{alignments_after}");
    assert_eq!(
        alignments_before, alignments_after,
        "foreign reports and decisions do not change own list"
    );
    let foreign_plan_path = format!("{plan_route}/{}", "4".repeat(32));
    let foreign_options =
        "/api/v1/operator/lessons/layout-h-speech-lesson/revisions/1/speech-options";
    for path in [foreign_plan_path.as_str(), foreign_options] {
        let (status, body) = request(&content_app, "GET", path, None, &mut cookie, &mut csrf).await;
        assert_eq!(status, 404, "{path}: {body}");
    }
    let (status, body) = request(
        &content_app,
        "GET",
        &format!("{plan_route}?lessonId=layout-h-speech-lesson&lessonRevision=1"),
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert!(body["items"].as_array().unwrap().is_empty());
    for path in [
        format!("{foreign_plan_path}?product=hargow"),
        format!("{foreign_options}?product=hargow"),
        format!("{plan_route}?lessonId=layout-h-speech-lesson&lessonRevision=1&product=hargow"),
    ] {
        let (status, body) =
            request(&content_app, "GET", &path, None, &mut cookie, &mut csrf).await;
        assert_eq!(status, 400, "{path}: {body}");
    }
    let before = owner
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(*)::bigint AS n FROM course_speech_plans",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "n")
        .unwrap();
    let calls_before = [
        enrollment.creates.load(std::sync::atomic::Ordering::SeqCst),
        enrollment.queries.load(std::sync::atomic::Ordering::SeqCst),
        enrollment
            .syntheses
            .load(std::sync::atomic::Ordering::SeqCst),
    ];
    let mut foreign_preview = preview_request.clone();
    foreign_preview["lessonId"] = "layout-h-speech-lesson".into();
    foreign_preview["lessonRevision"] = 1.into();
    let (status, body) = request(
        &content_app,
        "POST",
        &preview_route,
        Some(foreign_preview.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 404, "{body}");
    let mut foreign_save = plan_request.clone();
    foreign_save["id"] = format!("{:032x}", 401).into();
    foreign_save["preview"] = foreign_preview;
    let (status, body) = request(
        &content_app,
        "POST",
        plan_route,
        Some(foreign_save),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 404, "{body}");
    owner.execute_unprepared("ALTER TABLE course_speech_plans DROP CONSTRAINT chef_local_speech_plan_primary; ALTER TABLE course_speech_plans ADD CONSTRAINT legacy_speech_plan_primary_fixture PRIMARY KEY(id)").await.unwrap();
    let mut foreign_retry = plan_request.clone();
    foreign_retry["id"] = "4".repeat(32).into();
    let (status, body) = request(
        &content_app,
        "POST",
        plan_route,
        Some(foreign_retry),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 404, "{body}");
    owner.execute_unprepared("ALTER TABLE course_speech_plans DROP CONSTRAINT legacy_speech_plan_primary_fixture; ALTER TABLE course_speech_plans ADD CONSTRAINT chef_local_speech_plan_primary PRIMARY KEY(product_id,id)").await.unwrap();
    let mut foreign_voice_preview = preview_request.clone();
    foreign_voice_preview["selection"]["voices"][0] = serde_json::json!({"characterId":"aaa-foreign-character-1","characterRevision":1,"voiceRevision":1});
    let (status, body) = request(
        &content_app,
        "POST",
        &preview_route,
        Some(foreign_voice_preview),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 400, "{body}");
    let after = owner
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(*)::bigint AS n FROM course_speech_plans",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "n")
        .unwrap();
    assert_eq!(
        before, after,
        "foreign plan compile/save/retry has no writes"
    );
    assert_eq!(
        calls_before,
        [
            enrollment.creates.load(std::sync::atomic::Ordering::SeqCst),
            enrollment.queries.load(std::sync::atomic::Ordering::SeqCst),
            enrollment
                .syntheses
                .load(std::sync::atomic::Ordering::SeqCst)
        ]
    );
    let row = owner
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT product_id FROM course_speech_plans WHERE id=repeat('c',32)",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<String>("", "product_id").unwrap(), "brioche");

    for page in &history_before {
        let text = page.to_string();
        for forbidden in [
            "Poisoned foreign",
            "Synthetic foreign revocation",
            "Synthetic foreign clone",
        ] {
            assert!(!text.contains(forbidden), "Foreign history leaked: {page}");
        }
    }
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO voice_clone_events(product_id,job_id,version,status,voice_id,actor_id,reason) SELECT 'hargow',id,2,'checking','qwen-audio-3.1-tts-flash-'||prefix||'-fixture',$1,'Foreign query fixture' FROM voice_clone_jobs WHERE product_id='hargow'",[account.into()])).await.unwrap();

    // Ready foreign auditions use real test WAV receipts: file rejection must precede reading the media.
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO voice_auditions(product_id,id,character_id,character_revision,base_voice_revision,profile,parameters,actor_id,reason) SELECT 'hargow',lpad(to_hex(200+i),32,'0'),'aaa-foreign-character-'||i,1,0,jsonb_set(a.profile,'{referenceAudio}','null'),a.parameters,$1,'Foreign audition fixture' FROM generate_series(1,25) i CROSS JOIN voice_auditions a WHERE a.id=repeat('a',32)",[account.into()])).await.unwrap();
    owner.execute_unprepared("INSERT INTO voice_audition_events(product_id,audition_id,version,status,result) SELECT 'hargow',a.id,1,'ready',e.result FROM voice_auditions a CROSS JOIN voice_audition_events e WHERE a.product_id='hargow' AND e.audition_id=repeat('a',32) AND e.version=2").await.unwrap();
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO voice_audition_reviews(product_id,audition_id,accepted,character_id,character_revision,voice_revision,actor_id,reason) SELECT 'hargow',id,false,character_id,character_revision,NULL,$1,'Foreign audition decision fixture' FROM voice_auditions WHERE product_id='hargow'",[account.into()])).await.unwrap();
    let (status, auditions_after) = request(
        &content_app,
        "GET",
        audition_route,
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{auditions_after}");
    assert_eq!(
        auditions_before, auditions_after,
        "foreign auditions cannot consume pagination"
    );
    assert_eq!(
        history_before,
        history_pages(&content_app, &mut cookie, &mut csrf).await,
        "foreign auditions do not alter any history page"
    );

    let clip_list_path = format!("{plan_route}/cccccccccccccccccccccccccccccccc/clips");
    let (status, clips_before) = request(
        &content_app,
        "GET",
        &clip_list_path,
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{clips_before}");
    let previous = clips_before["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["generationKey"] == clip_request["generationKey"])
        .unwrap()
        .clone();
    let foreign_clip_id = format!("{:032x}", 501);
    // A newer foreign ready clip has the same generation key and a genuine synthetic WAV receipt.
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO course_speech_clips(product_id,id,plan_id,generation_key,request,actor_id,reason) SELECT 'hargow',$1,repeat('4',32),generation_key,request,$2,'Foreign shared-key clip' FROM course_speech_clips WHERE id=repeat('e',32)",[foreign_clip_id.clone().into(),account.into()])).await.unwrap();
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO course_speech_clip_events(product_id,clip_id,version,status,result) SELECT 'hargow',$1,1,'ready',result FROM course_speech_clip_events WHERE clip_id=repeat('e',32) AND version=2",[foreign_clip_id.clone().into()])).await.unwrap();
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO course_speech_clip_reviews(product_id,clip_id,accepted,actor_id,reason) VALUES('hargow',$1,true,$2,'Foreign shared-key decision')",[foreign_clip_id.clone().into(),account.into()])).await.unwrap();
    let (status, clips_after) = request(
        &content_app,
        "GET",
        &clip_list_path,
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{clips_after}");
    assert_eq!(
        clips_before, clips_after,
        "newer foreign recording cannot replace own latest clip"
    );
    let foreign_clip_path = format!("{clip_route}/{foreign_clip_id}");
    let foreign_clips_path = format!("{plan_route}/{}/clips", "4".repeat(32));
    for path in [
        foreign_clip_path.clone(),
        format!("{foreign_clip_path}/file"),
        foreign_clips_path.clone(),
    ] {
        let (status, body) =
            request(&content_app, "GET", &path, None, &mut cookie, &mut csrf).await;
        assert_eq!(status, 404, "{path}: {body}");
    }
    for path in [
        format!("{foreign_clip_path}?product=hargow"),
        format!("{foreign_clip_path}/file?product=hargow"),
        format!("{foreign_clips_path}?product=hargow"),
    ] {
        let (status, body) =
            request(&content_app, "GET", &path, None, &mut cookie, &mut csrf).await;
        assert_eq!(status, 400, "{path}: {body}");
    }
    let before=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM course_speech_clips)+(SELECT count(*) FROM course_speech_clip_events)+(SELECT count(*) FROM course_speech_clip_reviews) AS n")).await.unwrap().unwrap().try_get::<i64>("","n").unwrap();
    let calls_before = [
        enrollment.creates.load(std::sync::atomic::Ordering::SeqCst),
        enrollment.queries.load(std::sync::atomic::Ordering::SeqCst),
        enrollment
            .syntheses
            .load(std::sync::atomic::Ordering::SeqCst),
    ];
    let mut foreign_plan_clip = clip_request.clone();
    foreign_plan_clip["id"] = format!("{:032x}", 503).into();
    foreign_plan_clip["planId"] = "4".repeat(32).into();
    let mut foreign_retry = clip_request.clone();
    foreign_retry["id"] = foreign_clip_id.clone().into();
    let mut foreign_previous = clip_request.clone();
    foreign_previous["id"] = format!("{:032x}", 504).into();
    foreign_previous["expectedPreviousId"] = foreign_clip_id.clone().into();
    owner.execute_unprepared("ALTER TABLE course_speech_clips DROP CONSTRAINT chef_local_speech_clip_primary; ALTER TABLE course_speech_clips ADD CONSTRAINT legacy_speech_clip_primary_fixture PRIMARY KEY(id)").await.unwrap();
    for (body, expected) in [
        (foreign_plan_clip, 404),
        (foreign_retry, 404),
        (foreign_previous, 409),
    ] {
        let (status, result) = request(
            &content_app,
            "POST",
            clip_route,
            Some(body),
            &mut cookie,
            &mut csrf,
        )
        .await;
        assert_eq!(status, expected, "{result}");
    }
    owner.execute_unprepared("ALTER TABLE course_speech_clips DROP CONSTRAINT legacy_speech_clip_primary_fixture; ALTER TABLE course_speech_clips ADD CONSTRAINT chef_local_speech_clip_primary PRIMARY KEY(product_id,id)").await.unwrap();
    let (status,body)=request(&content_app,"POST",&format!("{foreign_clip_path}/review"),Some(serde_json::json!({"accepted":true,"heard":true,"reason":"Synthetic foreign clip adoption rejected"})),&mut cookie,&mut csrf).await;
    assert_eq!(status, 404, "{body}");
    let after=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM course_speech_clips)+(SELECT count(*) FROM course_speech_clip_events)+(SELECT count(*) FROM course_speech_clip_reviews) AS n")).await.unwrap().unwrap().try_get::<i64>("","n").unwrap();
    assert_eq!(
        before, after,
        "foreign clip attempts/retries/reviews have no writes"
    );
    let mut own_reuse = clip_request.clone();
    own_reuse["id"] = format!("{:032x}", 502).into();
    own_reuse["expectedPreviousId"] = previous["id"].clone();
    own_reuse["costConfirmed"] = false.into();
    let (status, reused) = request(
        &content_app,
        "POST",
        clip_route,
        Some(own_reuse.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{reused}");
    assert_eq!(reused["status"], "ready");
    assert_eq!(reused["reusedFrom"], serde_json::json!("e".repeat(32)));
    assert_eq!(reused["accepted"], true);
    let (status, receipt) = request(
        &content_app,
        "POST",
        clip_route,
        Some(own_reuse),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{receipt}");
    assert_eq!(receipt, reused);
    let own_review =
        serde_json::json!({"accepted":true,"heard":true,"reason":"Synthetic scoped reuse review"});
    let review_path = format!("{clip_route}/{:032x}/review", 502);
    for _ in 0..2 {
        let (status, body) = request(
            &content_app,
            "POST",
            &review_path,
            Some(own_review.clone()),
            &mut cookie,
            &mut csrf,
        )
        .await;
        assert_eq!(status, 200, "{body}");
    }
    assert_eq!(
        calls_before,
        [
            enrollment.creates.load(std::sync::atomic::Ordering::SeqCst),
            enrollment.queries.load(std::sync::atomic::Ordering::SeqCst),
            enrollment
                .syntheses
                .load(std::sync::atomic::Ordering::SeqCst)
        ],
        "own ready reuse and foreign requests do not contact provider"
    );
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM course_speech_clips WHERE id=lpad(to_hex(502),32,'0') AND product_id='brioche') AS clips,(SELECT count(*) FROM course_speech_clip_events WHERE clip_id=lpad(to_hex(502),32,'0') AND product_id='brioche') AS events,(SELECT count(*) FROM course_speech_clip_reviews WHERE clip_id=lpad(to_hex(502),32,'0') AND product_id='brioche') AS reviews,(SELECT count(*) FROM course_speech_clip_events WHERE clip_id=repeat('e',32) AND product_id='brioche') AS generated_events")).await.unwrap().unwrap();
    for key in ["clips", "events", "reviews"] {
        assert_eq!(row.try_get::<i64>("", key).unwrap(), 1);
    }
    assert_eq!(row.try_get::<i64>("", "generated_events").unwrap(), 2);

    let grants = include_str!("../../../infra/database/author-grants.sql")
        .lines()
        .filter(|line| !line.starts_with('\\'))
        .collect::<Vec<_>>()
        .join("\n")
        .replace(":\"schema\"", &format!("\"{source}\""))
        .replace(":\"role\"", &format!("\"{content_role}\""));
    owner.execute_unprepared(&grants).await.unwrap();
    let export_cli = ExportCli {
        db_url: role_url(&content_role),
        schema: source.clone(),
        root: root.clone(),
        identity_url: format!("http://{address}"),
    };
    std::fs::write(root.join(".env"), "").unwrap();
    for (name, session, token) in [
        ("operator-session.json", cookie.as_str(), csrf.as_str()),
        (
            "learner-session.json",
            next_cookie.as_str(),
            next_csrf.as_str(),
        ),
        ("bad-csrf-session.json", cookie.as_str(), "bad-csrf"),
    ] {
        std::fs::write(
            root.join(name),
            serde_json::to_vec(&serde_json::json!({"cookie":session,"csrfToken":token})).unwrap(),
        )
        .unwrap();
    }
    std::fs::write(root.join("other-product-session.json"),serde_json::to_vec(&serde_json::json!({"cookie":cookie.replace("brioche.sid=","hargow.sid="),"csrfToken":csrf})).unwrap()).unwrap();
    std::fs::write(root.join("unknown-session.json"),serde_json::to_vec(&serde_json::json!({"cookie":cookie,"csrfToken":csrf,"product":"hargow","secret":"DO_NOT_LOG_PRIVATE_CREDENTIAL"})).unwrap()).unwrap();
    // Export must choose the current product's latest reviewed clip even when
    // another product has a newer ready receipt with the exact generation key.
    for suffix in ["export", "export-direct"] {
        let foreign_export = format!("{plan_route}/{}/{suffix}", "4".repeat(32));
        let (status, body) = request(
            &content_app,
            "GET",
            &foreign_export,
            None,
            &mut cookie,
            &mut csrf,
        )
        .await;
        assert_eq!(status, 404, "{foreign_export}: {body}");
        let path = format!("{plan_route}/{plan_id}/{suffix}");
        let (status, body) = request(
            &content_app,
            "GET",
            &format!("{path}?product=hargow"),
            None,
            &mut cookie,
            &mut csrf,
        )
        .await;
        assert_eq!(status, 400, "{body}");
        let response = content_app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(&path)
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "{path}");
        assert_eq!(response.headers()["cache-control"], "private, no-store");
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let command = if suffix == "export" {
            "speech-plan-export"
        } else {
            "speech-plan-export-direct"
        };
        let output_name = format!("cli-{suffix}.tar");
        let result = export_cli
            .run(
                command,
                plan_id,
                "split@example.test",
                "operator-session.json",
                &output_name,
            )
            .await;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let receipt: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(receipt["published"], false);
        assert_eq!(receipt["planId"], plan_id);
        assert!(
            std::fs::read(root.join(&output_name)).unwrap().as_slice() == bytes.as_ref(),
            "CLI archive must match verified HTTP export"
        );
        for secret in [&cookie, &csrf] {
            assert!(!String::from_utf8_lossy(&result.stderr).contains(secret));
        }
        let result = export_cli
            .run(
                command,
                plan_id,
                "split@example.test",
                "operator-session.json",
                &output_name,
            )
            .await;
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("new writable file"));
        assert!(
            std::fs::read(root.join(&output_name)).unwrap().as_slice() == bytes.as_ref(),
            "CLI archive must match verified HTTP export"
        );
        let foreign_id = "4".repeat(32);
        for (id, email, session, name) in [
            (
                plan_id,
                "other@example.test",
                "operator-session.json",
                "wrong-actor",
            ),
            (
                plan_id,
                "next@example.test",
                "learner-session.json",
                "learner",
            ),
            (
                plan_id,
                "split@example.test",
                "bad-csrf-session.json",
                "csrf",
            ),
            (
                plan_id,
                "split@example.test",
                "other-product-session.json",
                "other-product-cookie",
            ),
            (
                plan_id,
                "split@example.test",
                "unknown-session.json",
                "unknown-credentials",
            ),
            (
                foreign_id.as_str(),
                "split@example.test",
                "operator-session.json",
                "foreign",
            ),
        ] {
            let name = format!("cli-{suffix}-{name}.tar");
            let result = export_cli.run(command, id, email, session, &name).await;
            assert!(!result.status.success());
            assert!(!root.join(name).exists());
            let error = String::from_utf8_lossy(&result.stderr);
            assert!(!error.contains("DO_NOT_LOG_PRIVATE_CREDENTIAL"));
            assert!(!error.contains("SELECT ") && !error.contains("INSERT INTO"));
        }
        let mut archive = tar::Archive::new(std::io::Cursor::new(bytes));
        let mut members = std::collections::BTreeMap::new();
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            let path = entry.path().unwrap().to_string_lossy().into_owned();
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut bytes).unwrap();
            assert!(members.insert(path, bytes).is_none());
        }
        let exported: serde_json::Value =
            serde_json::from_slice(&members["manifest.json"]).unwrap();
        assert_eq!(exported["planId"], plan_id);
        assert_eq!(exported["plan"]["planHash"], preview["planHash"]);
        let clips = exported["clips"].as_array().unwrap();
        assert_eq!(clips.len(), keys.len());
        assert!(clips.iter().any(|c| c["id"] == format!("{:032x}", 502)));
        assert!(clips.iter().all(|c| c["id"] != foreign_clip_id));
        for clip in clips {
            for (file, hash) in [("file", "sha256"), ("providerFile", "providerSha256")] {
                assert_eq!(
                    format!(
                        "{:x}",
                        Sha256::digest(&members[clip[file].as_str().unwrap()])
                    ),
                    clip["result"][hash]
                );
            }
            if suffix == "export" {
                assert_eq!(clip["review"]["actorId"], account);
            } else {
                assert!(clip["review"].is_null());
            }
        }
        if suffix == "export-direct" {
            assert_eq!(exported["humanListeningAsserted"], false);
        }
    }

    let foreign_alignment_path = format!("{alignment_route}/{}", "4".repeat(32));
    let own_alignment_path = format!("{alignment_route}/{alignment_id}");
    let foreign_alignment_list = format!("{plan_route}/{}/alignments", "4".repeat(32));
    for path in [
        foreign_alignment_path.clone(),
        foreign_alignment_list.clone(),
    ] {
        let (status, body) =
            request(&content_app, "GET", &path, None, &mut cookie, &mut csrf).await;
        assert_eq!(status, 404, "{path}: {body}");
    }
    for path in [
        format!("{own_alignment_path}?product=hargow"),
        format!("{plan_route}/{plan_id}/alignments?product=hargow"),
    ] {
        let (status, body) =
            request(&content_app, "GET", &path, None, &mut cookie, &mut csrf).await;
        assert_eq!(status, 400, "{body}");
    }
    let alignment_calls_before = [
        enrollment.creates.load(std::sync::atomic::Ordering::SeqCst),
        enrollment.queries.load(std::sync::atomic::Ordering::SeqCst),
        enrollment
            .syntheses
            .load(std::sync::atomic::Ordering::SeqCst),
    ];
    let before=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM speech_alignments)+(SELECT count(*) FROM speech_alignment_reviews) AS n")).await.unwrap().unwrap().try_get::<i64>("","n").unwrap();
    let original_alignment_report: serde_json::Value =
        serde_json::from_str(alignment_request["reportJson"].as_str().unwrap()).unwrap();
    let mut foreign_report = original_alignment_report.clone();
    foreign_report["planId"] = "4".repeat(32).into();
    let mut foreign_alignment_request = alignment_request.clone();
    foreign_alignment_request["id"] = format!("{:032x}", 601).into();
    foreign_alignment_request["planId"] = "4".repeat(32).into();
    foreign_alignment_request["reportJson"] = foreign_report.to_string().into();
    let mut foreign_alignment_retry = alignment_request.clone();
    foreign_alignment_retry["id"] = "4".repeat(32).into();
    owner.execute_unprepared("ALTER TABLE speech_alignments DROP CONSTRAINT chef_local_alignment_primary; ALTER TABLE speech_alignments ADD CONSTRAINT legacy_alignment_primary_fixture PRIMARY KEY(id)").await.unwrap();
    for body in [foreign_alignment_request, foreign_alignment_retry] {
        let (status, result) = request(
            &content_app,
            "POST",
            alignment_route,
            Some(body),
            &mut cookie,
            &mut csrf,
        )
        .await;
        assert_eq!(status, 404, "{result}");
    }
    owner.execute_unprepared("ALTER TABLE speech_alignments DROP CONSTRAINT legacy_alignment_primary_fixture; ALTER TABLE speech_alignments ADD CONSTRAINT chef_local_alignment_primary PRIMARY KEY(product_id,id)").await.unwrap();
    let (status, body) = request(
        &content_app,
        "POST",
        &format!("{foreign_alignment_path}/clips/{}/review", "4".repeat(32)),
        Some(alignment_review.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 404, "{body}");
    let mut foreign_clip_report = original_alignment_report.clone();
    let reported_clip = foreign_clip_report["clips"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["generationKey"] == clip_request["generationKey"])
        .unwrap();
    reported_clip["clipId"] = foreign_clip_id.clone().into();
    let mut foreign_clip_import = alignment_request.clone();
    foreign_clip_import["id"] = format!("{:032x}", 602).into();
    foreign_clip_import["reportJson"] = foreign_clip_report.to_string().into();
    let (status, body) = request(
        &content_app,
        "POST",
        alignment_route,
        Some(foreign_clip_import),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(
        status, 409,
        "foreign shared-key clip cannot enter own report: {body}"
    );
    let after=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM speech_alignments)+(SELECT count(*) FROM speech_alignment_reviews) AS n")).await.unwrap().unwrap().try_get::<i64>("","n").unwrap();
    assert_eq!(
        before, after,
        "foreign alignment import/retry/review writes nothing"
    );
    assert_eq!(
        alignment_calls_before,
        [
            enrollment.creates.load(std::sync::atomic::Ordering::SeqCst),
            enrollment.queries.load(std::sync::atomic::Ordering::SeqCst),
            enrollment
                .syntheses
                .load(std::sync::atomic::Ordering::SeqCst)
        ],
        "alignment requests do not contact providers"
    );
    // Native successful imports and decisions explicitly retain Brioche ownership.
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM speech_alignments WHERE id=repeat('8',32) AND product_id='brioche') AS reports,(SELECT count(*) FROM speech_alignment_reviews WHERE alignment_id=repeat('8',32) AND product_id='brioche') AS reviews")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "reports").unwrap(), 1);
    assert_eq!(
        row.try_get::<i64>("", "reviews").unwrap(),
        keys.len() as i64
    );

    let foreign_package_path = format!("{alignment_route}/{}/package", "4".repeat(32));
    let (status, body) = request(
        &content_app,
        "GET",
        &format!("{foreign_package_path}s"),
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 404, "{body}");
    let (status, body) = request(
        &content_app,
        "GET",
        &format!("{own_package_list}?product=hargow"),
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 400, "{body}");
    let before=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM speech_package_imports)+(SELECT count(*) FROM audio_assets)+(SELECT count(*) FROM audio_import_audit)+(SELECT count(*) FROM lesson_revisions)+(SELECT count(*) FROM lesson_import_audit) AS n")).await.unwrap().unwrap().try_get::<i64>("","n").unwrap();
    let (status, body) = request(
        &content_app,
        "POST",
        &foreign_package_path,
        Some(package_import["package"].clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 404, "foreign export: {body}");
    let mut foreign_import = package_import.clone();
    foreign_import["id"] = format!("{:032x}", 701).into();
    let (status, body) = request(
        &content_app,
        "POST",
        &format!("{foreign_package_path}/import"),
        Some(foreign_import),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 404, "foreign import: {body}");
    owner.execute_unprepared("ALTER TABLE speech_package_imports DROP CONSTRAINT chef_local_package_primary; ALTER TABLE speech_package_imports ADD CONSTRAINT legacy_package_primary_fixture PRIMARY KEY(id)").await.unwrap();
    let mut foreign_retry = package_import.clone();
    foreign_retry["id"] = "4".repeat(32).into();
    let (status, body) = request(
        &content_app,
        "POST",
        &import_path,
        Some(foreign_retry),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 404, "foreign import id retry: {body}");
    owner.execute_unprepared("ALTER TABLE speech_package_imports DROP CONSTRAINT legacy_package_primary_fixture; ALTER TABLE speech_package_imports ADD CONSTRAINT chef_local_package_primary PRIMARY KEY(product_id,id)").await.unwrap();
    // A local receipt PK with the old global lesson key is still incomplete.
    owner.execute_unprepared("ALTER TABLE speech_package_imports DROP CONSTRAINT chef_local_package_lesson; ALTER TABLE speech_package_imports ADD CONSTRAINT legacy_package_lesson_fixture UNIQUE(lesson_id,revision)").await.unwrap();
    let mut partial_retry = package_import.clone();
    partial_retry["id"] = "4".repeat(32).into();
    let (status, body) = request(
        &content_app,
        "POST",
        &import_path,
        Some(partial_retry),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 404, "partial local package layout: {body}");
    owner.execute_unprepared("ALTER TABLE speech_package_imports DROP CONSTRAINT legacy_package_lesson_fixture; ALTER TABLE speech_package_imports ADD CONSTRAINT chef_local_package_lesson UNIQUE(product_id,lesson_id,revision)").await.unwrap();
    let after=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM speech_package_imports)+(SELECT count(*) FROM audio_assets)+(SELECT count(*) FROM audio_import_audit)+(SELECT count(*) FROM lesson_revisions)+(SELECT count(*) FROM lesson_import_audit) AS n")).await.unwrap().unwrap().try_get::<i64>("","n").unwrap();
    assert_eq!(
        before, after,
        "foreign package requests have no registration/import writes"
    );
    let (status, retry) = request(
        &content_app,
        "POST",
        &import_path,
        Some(package_import.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{retry}");
    assert_eq!(
        retry, imported,
        "own precise import retry remains stable with foreign receipts"
    );
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT count(*) AS n FROM speech_package_imports p JOIN lesson_revisions l ON (l.lesson_id,l.revision,l.product_id)=(p.lesson_id,p.revision,p.product_id) JOIN lesson_import_audit a ON (a.lesson_id,a.revision,a.product_id)=(p.lesson_id,p.revision,p.product_id) WHERE p.id=repeat('6',32) AND p.product_id='brioche'")).await.unwrap().unwrap();
    assert_eq!(
        row.try_get::<i64>("", "n").unwrap(),
        1,
        "native package/source/import audit all belong to Brioche"
    );
    assert_eq!(
        alignment_calls_before,
        [
            enrollment.creates.load(std::sync::atomic::Ordering::SeqCst),
            enrollment.queries.load(std::sync::atomic::Ordering::SeqCst),
            enrollment
                .syntheses
                .load(std::sync::atomic::Ordering::SeqCst)
        ],
        "package delivery/retries do not contact providers"
    );

    // Automatic delivery also selects own latest receipts when another product
    // shares the generation key. Synthetic predictions test binding, not accuracy.
    let tar_members = |bytes: &[u8]| {
        let mut archive = tar::Archive::new(std::io::Cursor::new(bytes));
        let mut members = std::collections::BTreeMap::new();
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            let path = entry.path().unwrap().to_string_lossy().into_owned();
            let mut data = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut data).unwrap();
            assert!(members.insert(path, data).is_none());
        }
        members
    };
    let response = content_app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&direct_export_path)
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let current_input_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let current_input = tar_members(&current_input_bytes);
    let current_manifest: serde_json::Value =
        serde_json::from_slice(&current_input["manifest.json"]).unwrap();
    let mut own_automatic: serde_json::Value =
        serde_json::from_str(automatic_request["reportJson"].as_str().unwrap()).unwrap();
    own_automatic["sourceArchiveSha256"] =
        format!("{:x}", Sha256::digest(&current_input_bytes)).into();
    for clip in own_automatic["clips"].as_array_mut().unwrap() {
        let current = current_manifest["clips"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["generationKey"] == clip["generationKey"])
            .unwrap();
        clip["clipId"] = current["id"].clone();
    }
    let mut own_automatic_request = automatic_request.clone();
    own_automatic_request["reportJson"] = own_automatic.to_string().into();
    own_automatic_request["package"]["expectedReportHash"] = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&own_automatic).unwrap())
    )
    .into();
    own_automatic_request["package"]["lessonRevision"] = (lesson.revision + 10).into();
    let before=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM speech_package_imports)+(SELECT count(*) FROM lesson_revisions)+(SELECT count(*) FROM audio_assets)+(SELECT count(*) FROM speech_alignment_reviews)+(SELECT count(*) FROM course_speech_clip_reviews)+(SELECT count(*) FROM lesson_direct_publications) AS n")).await.unwrap().unwrap().try_get::<i64>("","n").unwrap();
    let mut foreign_automatic = own_automatic.clone();
    foreign_automatic["planId"] = "4".repeat(32).into();
    let mut foreign_clip_automatic = own_automatic.clone();
    foreign_clip_automatic["clips"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["generationKey"] == clip_request["generationKey"])
        .unwrap()["clipId"] = foreign_clip_id.clone().into();
    for (report, expected) in [(foreign_automatic, 404), (foreign_clip_automatic, 409)] {
        let mut body = own_automatic_request.clone();
        body["reportJson"] = report.to_string().into();
        body["package"]["expectedReportHash"] =
            format!("{:x}", Sha256::digest(serde_json::to_vec(&report).unwrap())).into();
        let (status, result) = request(
            &content_app,
            "POST",
            automatic_path,
            Some(body),
            &mut cookie,
            &mut csrf,
        )
        .await;
        assert_eq!(status, expected, "{result}");
    }
    let response = content_app
        .clone()
        .oneshot(json_write(
            automatic_path,
            &own_automatic_request,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "private, no-store");
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let members = tar_members(&bytes);
    let output: serde_json::Value = serde_json::from_slice(&members["manifest.json"]).unwrap();
    assert_eq!(output["planId"], plan_id);
    assert_eq!(output["automaticAlignment"], own_automatic);
    assert_eq!(output["assembly"]["humanListeningAsserted"], false);
    assert_eq!(output["assembly"]["approvalRequired"], false);
    assert!(
        output["clips"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == format!("{:032x}", 502))
    );
    assert!(
        output["clips"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["id"] != foreign_clip_id)
    );
    let automatic_source: serde_json::Value =
        serde_json::from_slice(&members["lesson.json"]).unwrap();
    assert_eq!(automatic_source["id"], lesson.id);
    assert_eq!(automatic_source["revision"], lesson.revision + 10);
    chef_engine::project_source(automatic_source)
        .unwrap()
        .validate()
        .unwrap();
    let after=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM speech_package_imports)+(SELECT count(*) FROM lesson_revisions)+(SELECT count(*) FROM audio_assets)+(SELECT count(*) FROM speech_alignment_reviews)+(SELECT count(*) FROM course_speech_clip_reviews)+(SELECT count(*) FROM lesson_direct_publications) AS n")).await.unwrap().unwrap().try_get::<i64>("","n").unwrap();
    assert_eq!(
        before, after,
        "automatic delivery neither imports nor inserts hearing/publication decisions"
    );
    assert_eq!(
        alignment_calls_before,
        [
            enrollment.creates.load(std::sync::atomic::Ordering::SeqCst),
            enrollment.queries.load(std::sync::atomic::Ordering::SeqCst),
            enrollment
                .syntheses
                .load(std::sync::atomic::Ordering::SeqCst)
        ],
        "automatic delivery never calls paid providers"
    );

    let foreign_audition = format!("{:032x}", 201);
    let foreign_path = format!("{audition_route}/{foreign_audition}");
    for path in [foreign_path.clone(), format!("{foreign_path}/file")] {
        let (status, body) =
            request(&content_app, "GET", &path, None, &mut cookie, &mut csrf).await;
        assert_eq!(status, 404, "{path}: {body}");
    }
    for path in [
        format!("{audition_route}?product=hargow"),
        format!("{foreign_path}?product=hargow"),
        format!("{foreign_path}/file?product=hargow"),
    ] {
        let (status, body) =
            request(&content_app, "GET", &path, None, &mut cookie, &mut csrf).await;
        assert_eq!(status, 400, "{path}: {body}");
    }
    let before=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM voice_auditions)+(SELECT count(*) FROM voice_audition_events)+(SELECT count(*) FROM voice_audition_reviews)+(SELECT count(*) FROM character_voice_profiles) AS n")).await.unwrap().unwrap().try_get::<i64>("","n").unwrap();
    let calls_before = [
        enrollment.creates.load(std::sync::atomic::Ordering::SeqCst),
        enrollment.queries.load(std::sync::atomic::Ordering::SeqCst),
        enrollment
            .syntheses
            .load(std::sync::atomic::Ordering::SeqCst),
    ];
    let mut foreign_clone_request = audition_request.clone();
    foreign_clone_request["id"] = format!("{:032x}", 301).into();
    foreign_clone_request["cloneJobId"] = foreign_job.clone().into();
    foreign_clone_request["expectedCloneVersion"] = 1.into();
    let mut system_profile = voice["profile"].clone();
    system_profile["voiceId"] = brioche_course_contract::QWEN_FRENCH_SYSTEM_VOICES[0]
        .0
        .into();
    let foreign_system_request = serde_json::json!({"id":format!("{:032x}",302),"candidate":{"characterId":"aaa-foreign-character-1","characterRevision":1,"expectedVoiceRevision":1,"profile":system_profile},"text":"Bonjour !","emotion":"Friendly.","costConfirmed":true,"reason":"Must not synthesize foreign character"});
    let mut foreign_retry = audition_request.clone();
    foreign_retry["id"] = foreign_audition.clone().into();
    // Old/partial layouts still refuse an ID owned by a different product.
    owner.execute_unprepared("ALTER TABLE voice_auditions DROP CONSTRAINT chef_local_audition_primary; ALTER TABLE voice_auditions ADD CONSTRAINT legacy_audition_primary_fixture PRIMARY KEY(id)").await.unwrap();
    for body in [foreign_clone_request, foreign_system_request, foreign_retry] {
        let (status, result) = request(
            &content_app,
            "POST",
            audition_route,
            Some(body),
            &mut cookie,
            &mut csrf,
        )
        .await;
        assert_eq!(status, 404, "{result}");
    }
    owner.execute_unprepared("ALTER TABLE voice_auditions DROP CONSTRAINT legacy_audition_primary_fixture; ALTER TABLE voice_auditions ADD CONSTRAINT chef_local_audition_primary PRIMARY KEY(product_id,id)").await.unwrap();
    let (status,body)=request(&content_app,"POST",&format!("{foreign_path}/review"),Some(serde_json::json!({"accepted":true,"heard":true,"expectedVoiceRevision":0,"reason":"Synthetic foreign adoption rejected"})),&mut cookie,&mut csrf).await;
    assert_eq!(status, 404, "{body}");
    let after=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM voice_auditions)+(SELECT count(*) FROM voice_audition_events)+(SELECT count(*) FROM voice_audition_reviews)+(SELECT count(*) FROM character_voice_profiles) AS n")).await.unwrap().unwrap().try_get::<i64>("","n").unwrap();
    assert_eq!(before, after, "foreign create/retry/adoption has no writes");
    assert_eq!(
        calls_before,
        [
            enrollment.creates.load(std::sync::atomic::Ordering::SeqCst),
            enrollment.queries.load(std::sync::atomic::Ordering::SeqCst),
            enrollment
                .syntheses
                .load(std::sync::atomic::Ordering::SeqCst)
        ],
        "foreign auditions do not call provider"
    );
    // Same-product system candidate exercises its explicit product writes and asynchronous result.
    let system_request = serde_json::json!({"id":format!("{:032x}",303),"candidate":{"characterId":"split-character","characterRevision":2,"expectedVoiceRevision":3,"profile":system_profile},"text":"Bonjour !","emotion":"Friendly.","costConfirmed":true,"reason":"Independent synthetic system audition"});
    let (status, body) = request(
        &content_app,
        "POST",
        audition_route,
        Some(system_request.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let system_path = format!("{audition_route}/{:032x}", 303);
    let settled = settled_job(&content_app, &system_path, &mut cookie, &mut csrf).await;
    assert_eq!(settled["status"], "ready");
    let calls = enrollment
        .syntheses
        .load(std::sync::atomic::Ordering::SeqCst);
    let (status, body) = request(
        &content_app,
        "POST",
        audition_route,
        Some(system_request),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        calls,
        enrollment
            .syntheses
            .load(std::sync::atomic::Ordering::SeqCst)
    );
    let (status,body)=request(&content_app,"POST",&format!("{system_path}/review"),Some(serde_json::json!({"accepted":false,"heard":true,"expectedVoiceRevision":3,"reason":"Synthetic rejection protocol"})),&mut cookie,&mut csrf).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["accepted"], false);
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM voice_auditions WHERE id=lpad(to_hex(303),32,'0') AND product_id='brioche') AS attempts,(SELECT count(*) FROM voice_audition_events WHERE audition_id=lpad(to_hex(303),32,'0') AND product_id='brioche') AS events,(SELECT count(*) FROM voice_audition_reviews WHERE audition_id=lpad(to_hex(303),32,'0') AND product_id='brioche') AS reviews")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "attempts").unwrap(), 1);
    assert_eq!(row.try_get::<i64>("", "events").unwrap(), 2);
    assert_eq!(row.try_get::<i64>("", "reviews").unwrap(), 1);

    let mut foreign_reference = voice.clone();
    foreign_reference["expectedVoiceRevision"] = 3.into();
    foreign_reference["profile"]["referenceAudio"] = serde_json::json!({"assetId":"foreign-course-recording","revision":1,"transcript":"Bonjour.","cloningPermission":"Synthetic fixture only"});
    let (status, body) = request(
        &content_app,
        "POST",
        "/api/v1/operator/characters",
        Some(foreign_reference),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 404, "{body}");
    let count=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT count(*)::bigint AS n FROM character_voice_profiles WHERE character_id='split-character' AND character_revision=2 AND revision=4")).await.unwrap().unwrap().try_get::<i64>("","n").unwrap();
    assert_eq!(
        count, 0,
        "foreign recording cannot append a voice direction"
    );
    let mut foreign_audio_source = imported_source.clone();
    foreign_audio_source["id"] = "brioche-foreign-audio-source".into();
    foreign_audio_source["audioRefs"] =
        serde_json::json!([{"assetId":"foreign-course-recording","revision":1}]);
    chef_engine::validate_source_schema(foreign_audio_source.clone()).unwrap();
    let hydrated = chef_engine::media::hydrate_source(&owner, foreign_audio_source.clone())
        .await
        .unwrap();
    let mut hydrated = chef_engine::recording::hydrate_source(&owner, hydrated)
        .await
        .unwrap();
    hydrated["knowledge"]["vocabulary"][0]["recording"] = serde_json::json!({
        "asset": hydrated["audio"][0], "startMs": 0, "endMs": 100
    });
    let mixed_audio = chef_engine::project_source(hydrated.clone()).unwrap();
    chef_engine::media::validate_lesson(&owner, &mixed_audio, &root)
        .await
        .unwrap();
    let audio_document = serde_json::json!({"document":foreign_audio_source.to_string(),"reason":"Foreign recording reference rejected"});
    let (status, report) = request(
        &content_app,
        "POST",
        "/api/v1/operator/documents/lesson/check",
        Some(audio_document.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{report}");
    assert_eq!(report["valid"], false);
    assert_eq!(report["issue"]["pointer"], "/audioRefs/0/revision");
    assert_eq!(
        request(
            &content_app,
            "POST",
            "/api/v1/operator/lessons/import",
            Some(audio_document),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        400
    );
    let mut embedded_audio = hydrated;
    embedded_audio.as_object_mut().unwrap().remove("audioRefs");
    embedded_audio["id"] = "brioche-foreign-embedded-audio".into();
    chef_engine::validate_source_schema(embedded_audio.clone()).unwrap();
    let audio_document = serde_json::json!({"document":embedded_audio.to_string(),"reason":"Embedded foreign recording rejected"});
    let (status, report) = request(
        &content_app,
        "POST",
        "/api/v1/operator/documents/lesson/check",
        Some(audio_document.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{report}");
    assert_eq!(report["valid"], false);
    assert_eq!(report["issue"]["pointer"], "/audio/0/revision");
    assert_eq!(
        request(
            &content_app,
            "POST",
            "/api/v1/operator/lessons/import",
            Some(audio_document),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        400
    );
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM lesson_revisions WHERE lesson_id IN ('brioche-foreign-audio-source','brioche-foreign-embedded-audio'))+(SELECT count(*) FROM lesson_import_audit WHERE lesson_id IN ('brioche-foreign-audio-source','brioche-foreign-embedded-audio')) AS n")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 0);
    assert!(
        learning
            .execute_unprepared("UPDATE audio_assets SET byte_size=byte_size WHERE false")
            .await
            .is_err(),
        "public playback role cannot mutate recordings"
    );
    // Public transports use minimal-role connections and fixed product assembly.
    // Synthetic registry/file fixtures are not Cantonese curriculum evidence.
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8" fill="#73a5ca"/></svg>"##;
    let visual_hash = format!("{:x}", Sha256::digest(svg));
    std::fs::write(root.join(format!("{visual_hash}.svg")), svg).unwrap();
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT descriptor FROM media_assets WHERE asset_id='foreign-course-illustration' AND revision=1")).await.unwrap().unwrap();
    let mut visual: serde_json::Value = row.try_get("", "descriptor").unwrap();
    visual["assetId"] = "public-hargow-image".into();
    visual["sha256"] = visual_hash.clone().into();
    visual["url"] = format!("/api/media/{visual_hash}.svg").into();
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO media_assets(product_id,asset_id,revision,descriptor,provenance,sha256,extension,byte_size) SELECT 'hargow','public-hargow-image',1,$1,provenance,$2,'svg',$3 FROM media_assets WHERE asset_id='foreign-course-illustration' AND revision=1",[visual.clone().into(),visual_hash.into(),(svg.len() as i32).into()])).await.unwrap();
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
    wav[44..46].copy_from_slice(&25i16.to_le_bytes());
    let decoded = chef_engine::audio::inspect(&wav, "audio/wav").unwrap();
    let audio_hash = format!("{:x}", Sha256::digest(&wav));
    std::fs::write(root.join(format!("{audio_hash}.wav")), &wav).unwrap();
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT descriptor FROM audio_assets WHERE asset_id='foreign-course-recording' AND revision=1")).await.unwrap().unwrap();
    let mut audio: serde_json::Value = row.try_get("", "descriptor").unwrap();
    audio["assetId"] = "public-hargow-audio".into();
    audio["sha256"] = audio_hash.clone().into();
    audio["url"] = format!("/api/audio/{audio_hash}.wav").into();
    audio["mimeType"] = "audio/wav".into();
    audio["durationMs"] = decoded.duration_ms.into();
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO audio_assets(product_id,asset_id,revision,descriptor,provenance,sha256,extension,byte_size,duration_ms,sample_rate,channels) SELECT 'hargow','public-hargow-audio',1,$1,provenance,$2,'wav',$3,$4,$5,$6 FROM audio_assets WHERE asset_id='foreign-course-recording' AND revision=1",[audio.clone().into(),audio_hash.into(),(wav.len() as i32).into(),(decoded.duration_ms as i32).into(),(decoded.sample_rate as i32).into(),(decoded.channels as i32).into()])).await.unwrap();
    let mut public_fixture = mixed_audio.clone();
    public_fixture
        .media
        .push(serde_json::from_value(visual.clone()).unwrap());
    public_fixture.audio = vec![serde_json::from_value(audio.clone()).unwrap()];
    let mut public_document = serde_json::to_value(&public_fixture).unwrap();
    public_document["knowledge"]["vocabulary"][0]["recording"]["asset"] = audio.clone();
    let public_fixture: brioche_course_contract::PublicLesson =
        serde_json::from_value(public_document.clone()).unwrap();
    public_fixture.validate().unwrap();
    for (product, id) in [
        ("hargow", "public-hargow-media"),
        ("brioche", "public-brioche-foreign-media"),
    ] {
        owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO lesson_revisions(product_id,lesson_id,revision,published,public_document,server_document) VALUES($1,$2,1,false,$3,$4)",[product.into(),id.into(),public_document.clone().into(),serde_json::json!({"editorial":{"status":"draft"}}).into()])).await.unwrap();
    }
    let b_media =
        chef_engine::media::product_router(learning.clone(), root.clone(), ProductId::Brioche)
            .merge(chef_engine::recording::product_router(
                learning.clone(),
                root.clone(),
                ProductId::Brioche,
            ));
    let h_media =
        chef_engine::media::product_router(learning.clone(), root.clone(), ProductId::Hargow)
            .merge(chef_engine::recording::product_router(
                learning.clone(),
                root.clone(),
                ProductId::Hargow,
            ));
    let visual_path = visual["url"].as_str().unwrap();
    let audio_path = audio["url"].as_str().unwrap();
    for path in [visual_path, audio_path] {
        let response = h_media
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 404, "unpublished {path}");
    }
    // A published B document cannot grant access to a foreign registry object.
    owner.execute_unprepared("UPDATE lesson_revisions SET published=true WHERE lesson_id='public-brioche-foreign-media'").await.unwrap();
    for app in [&b_media, &h_media] {
        for path in [visual_path, audio_path] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(
                response.status().as_u16(),
                404,
                "foreign-only publication {path}"
            );
        }
    }
    owner
        .execute_unprepared(
            "UPDATE lesson_revisions SET published=true WHERE lesson_id='public-hargow-media'",
        )
        .await
        .unwrap();
    for (path, bytes) in [(visual_path, svg.as_slice()), (audio_path, wav.as_slice())] {
        let response = h_media
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(
            response.headers()["cross-origin-resource-policy"],
            "same-origin"
        );
        assert_eq!(
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .as_ref(),
            bytes
        );
        let response = b_media
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .header("x-product-id", "hargow")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 404);
        let response = h_media
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("{path}?product=brioche"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 400);
    }
    // H published documents referencing B registry IDs cannot publish B files.
    let response = h_media
        .clone()
        .oneshot(
            Request::builder()
                .uri(&mixed_audio.media[0].url)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 404, "reverse product boundary");
    let response = h_media
        .clone()
        .oneshot(
            Request::builder()
                .uri(audio_path)
                .header("range", "bytes=0-15")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 206);
    assert_eq!(response.headers()["content-range"], "bytes 0-15/4844");
    assert_eq!(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .as_ref(),
        &wav[..16]
    );
    owner.execute_unprepared("INSERT INTO content_withdrawals(product_id,lesson_id,revision) VALUES('hargow','public-hargow-media',1)").await.unwrap();
    for path in [visual_path, audio_path] {
        let response = h_media
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 404, "withdrawn {path}");
    }
    let response = b_media
        .clone()
        .oneshot(
            Request::builder()
                .uri(&mixed_audio.media[0].url)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status().as_u16(),
        200,
        "own published B image remains public"
    );
    let mut h_lesson = lesson.clone();
    let (status, admin_before) = request(
        &content_app,
        "GET",
        "/api/v1/operator/overview",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{admin_before}");
    let (status, history_before) = request(
        &content_app,
        "GET",
        "/api/v1/operator/history",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{history_before}");
    h_lesson.id = "hargow-catalog-fixture".into();
    h_lesson.title.fr = "Hargow catalogue sentinel".into();
    h_lesson.title.zh = "Hargow 合成目录测试".into();
    h_lesson.validate().unwrap();
    let manifest = serde_json::json!({"id":"hargow-catalog-release","schemaVersion":"1.0","levels":[{"id":h_lesson.level_id,"label":"Synthetic Hargow fixture","units":[{"id":h_lesson.unit_id,"titleZh":"合成测试单元","lessons":[{"lessonId":h_lesson.id,"revision":h_lesson.revision}]}]}]});
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO lesson_revisions(product_id,lesson_id,revision,published,public_document,server_document) VALUES('hargow',$1,$2,true,$3,'{}')",[h_lesson.id.clone().into(),(h_lesson.revision as i32).into(),serde_json::to_value(&h_lesson).unwrap().into()])).await.unwrap();
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO content_releases(product_id,id,manifest,content_hash) VALUES('hargow','hargow-catalog-release',$1,repeat('a',64))",[manifest.into()])).await.unwrap();
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO release_entries(product_id,release_id,lesson_id,revision,position) VALUES('hargow','hargow-catalog-release',$1,$2,0)",[h_lesson.id.clone().into(),(h_lesson.revision as i32).into()])).await.unwrap();
    owner.execute_unprepared("UPDATE content_state SET active_release='hargow-catalog-release' WHERE product_id='hargow'").await.unwrap();
    owner.execute_unprepared("INSERT INTO content_audit(product_id,action,release_id,actor,reason,generation) VALUES('hargow','activate','hargow-catalog-release','synthetic','Foreign course audit sentinel',1)").await.unwrap();
    let (status, admin_after) = request(
        &content_app,
        "GET",
        "/api/v1/operator/overview",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{admin_after}");
    assert_eq!(admin_after, admin_before);
    let (status, history_after) = request(
        &content_app,
        "GET",
        "/api/v1/operator/history",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{history_after}");
    assert_eq!(history_after, history_before);
    let (status, filtered) = request(
        &content_app,
        "GET",
        "/api/v1/operator/overview?lessonQ=sentinel&releaseQ=hargow",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{filtered}");
    assert!(filtered["lessons"].as_array().unwrap().is_empty());
    assert!(filtered["releases"].as_array().unwrap().is_empty());
    let foreign_preview = format!(
        "/api/v1/operator/lessons/{}/revisions/{}",
        h_lesson.id, h_lesson.revision
    );
    let mut private_paths = vec![
        foreign_preview.clone(),
        "/api/v1/operator/releases/hargow-catalog-release".to_owned(),
    ];
    for asset in &h_lesson.media {
        private_paths.push(format!(
            "{foreign_preview}/media/{}",
            asset.url.rsplit('/').next().unwrap()
        ));
    }
    for asset in &h_lesson.audio {
        private_paths.push(format!(
            "{foreign_preview}/audio/{}",
            asset.url.rsplit('/').next().unwrap()
        ));
    }
    // Include both transports even when the synthetic source has no audio.
    private_paths.push(format!("{foreign_preview}/audio/unknown.wav"));
    for path in private_paths {
        let (status, body) =
            request(&content_app, "GET", &path, None, &mut cookie, &mut csrf).await;
        assert_eq!(status, 404, "{path}: {body}");
    }
    let mut foreign_grade = grade_request.clone();
    foreign_grade["revision"] = h_lesson.revision.into();
    let (status, body) = request(
        &content_app,
        "POST",
        &format!("{foreign_preview}/grade"),
        Some(foreign_grade),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 404, "{body}");
    let audio_counts_sql = "SELECT (SELECT count(*) FROM lesson_audio_reviews)::bigint AS reviews,(SELECT count(*) FROM lesson_direct_publications)::bigint AS direct";
    let audio_counts_before = owner
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            audio_counts_sql,
        ))
        .await
        .unwrap()
        .unwrap();
    for (method, path, body) in [
        ("GET", format!("{foreign_preview}/audio-review"), None),
        (
            "POST",
            format!("{foreign_preview}/audio-review"),
            Some(audio_request.clone()),
        ),
        (
            "POST",
            format!("{foreign_preview}/direct-publication"),
            Some(direct_request.clone()),
        ),
    ] {
        let (status, result) =
            request(&content_app, method, &path, body, &mut cookie, &mut csrf).await;
        assert_eq!(status, 404, "{path}: {result}");
    }
    let audio_counts_after = owner
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            audio_counts_sql,
        ))
        .await
        .unwrap()
        .unwrap();
    for column in ["reviews", "direct"] {
        assert_eq!(
            audio_counts_before.try_get::<i64>("", column).unwrap(),
            audio_counts_after.try_get::<i64>("", column).unwrap()
        );
    }
    let foreign_review = format!(
        "/api/v1/operator/lessons/{}/revisions/{}/review",
        h_lesson.id, h_lesson.revision
    );
    for published in [true, false] {
        owner
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "UPDATE lesson_revisions SET published=$3 WHERE lesson_id=$1 AND revision=$2",
                [
                    h_lesson.id.clone().into(),
                    (h_lesson.revision as i32).into(),
                    published.into(),
                ],
            ))
            .await
            .unwrap();
        assert_eq!(request(&content_app,"POST",&foreign_review,Some(serde_json::json!({"version":0,"approved":false,"reason":"Foreign review rejected"})),&mut cookie,&mut csrf).await.0,404);
    }
    owner
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE lesson_revisions SET published=true WHERE lesson_id=$1 AND revision=$2",
            [
                h_lesson.id.clone().into(),
                (h_lesson.revision as i32).into(),
            ],
        ))
        .await
        .unwrap();
    let count=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT count(*)::bigint AS n FROM editorial_reviews WHERE lesson_id=$1 AND revision=$2",[h_lesson.id.clone().into(),(h_lesson.revision as i32).into()])).await.unwrap().unwrap();
    assert_eq!(count.try_get::<i64>("", "n").unwrap(), 0);
    let withdrawal_snapshot = "SELECT md5(jsonb_build_object('states',(SELECT jsonb_agg(to_jsonb(s) ORDER BY product_id) FROM content_state s),'lesson',(SELECT to_jsonb(r) FROM lesson_revisions r WHERE lesson_id=$1 AND revision=$2),'withdrawals',(SELECT count(*) FROM content_withdrawals),'audit',(SELECT count(*) FROM content_audit))::text) AS hash";
    let before_withdraw = owner
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            withdrawal_snapshot,
            [
                h_lesson.id.clone().into(),
                (h_lesson.revision as i32).into(),
            ],
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "hash")
        .unwrap();
    assert_eq!(request(&content_app,"POST","/api/v1/operator/releases/activate",Some(serde_json::json!({"releaseId":"hargow-catalog-release","generation":admin_before["generation"],"reason":"Foreign activation rejected"})),&mut cookie,&mut csrf).await.0,404);
    let after_activate = owner
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            withdrawal_snapshot,
            [
                h_lesson.id.clone().into(),
                (h_lesson.revision as i32).into(),
            ],
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "hash")
        .unwrap();
    assert_eq!(after_activate, before_withdraw);
    let foreign_manifest = serde_json::json!({"id":"brioche-foreign-course-release","schemaVersion":"1.0","levels":[{"id":h_lesson.level_id,"label":"Synthetic","units":[{"id":h_lesson.unit_id,"titleZh":"合成单元","lessons":[{"lessonId":h_lesson.id,"revision":h_lesson.revision}]}]}]});
    let foreign_document = serde_json::json!({"document":foreign_manifest.to_string(),"reason":"Foreign staging rejected"});
    assert_eq!(
        request(
            &content_app,
            "POST",
            "/api/v1/operator/releases/stage",
            Some(foreign_document.clone()),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        404
    );
    let (status, report) = request(
        &content_app,
        "POST",
        "/api/v1/operator/documents/release/check",
        Some(foreign_document),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{report}");
    assert_eq!(report["valid"], false);
    let count = owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM content_releases WHERE id='brioche-foreign-course-release')+(SELECT count(*) FROM release_entries WHERE release_id='brioche-foreign-course-release')+(SELECT count(*) FROM content_audit WHERE release_id='brioche-foreign-course-release') AS n")).await.unwrap().unwrap();
    assert_eq!(count.try_get::<i64>("", "n").unwrap(), 0);
    assert_eq!(request(&content_app,"POST",&format!("/api/v1/operator/lessons/{}/revisions/{}/withdraw",h_lesson.id,h_lesson.revision),Some(serde_json::json!({"generation":admin_before["generation"],"reason":"Foreign withdrawal rejected"})),&mut cookie,&mut csrf).await.0,404);
    let after_withdraw = owner
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            withdrawal_snapshot,
            [
                h_lesson.id.clone().into(),
                (h_lesson.revision as i32).into(),
            ],
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "hash")
        .unwrap();
    assert_eq!(after_withdraw, before_withdraw);
    let h_catalog =
        chef_engine::content::catalog_matching_for_product(&learning, Some(ProductId::Hargow), &[])
            .await
            .unwrap();
    assert_eq!(h_catalog.levels[0].units[0].lessons[0].id, h_lesson.id);
    assert_eq!(
        serde_json::to_value(&before_b).unwrap(),
        serde_json::to_value(
            chef_engine::content::catalog_matching_for_product(
                &learning,
                Some(ProductId::Brioche),
                &[]
            )
            .await
            .unwrap()
        )
        .unwrap()
    );
    let terms = chef_engine::content::search_terms("sentinel").unwrap();
    assert_eq!(
        chef_engine::content::catalog_matching_for_product(
            &learning,
            Some(ProductId::Hargow),
            &terms
        )
        .await
        .unwrap()
        .levels[0]
            .units[0]
            .lessons[0]
            .id,
        h_lesson.id
    );
    assert!(
        chef_engine::content::catalog_matching_for_product(
            &learning,
            Some(ProductId::Brioche),
            &terms
        )
        .await
        .unwrap()
        .levels
        .is_empty()
    );
    let (status, dashboard) = request(
        &remote,
        "GET",
        "/api/v1/me/dashboard",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{dashboard}");
    assert_eq!(
        dashboard["catalog"],
        serde_json::to_value(&before_b).unwrap()
    );
    assert_ne!(dashboard["recommendedLesson"]["id"], h_lesson.id);
    let h_public = chef_engine::independent_product_router(
        chef_engine::AppState {
            db: Some(learning.clone()),
            fixture: None,
        },
        ProductId::Hargow,
    );
    let (status, h_public_catalog) = request(
        &h_public,
        "GET",
        "/api/catalog",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(h_public_catalog, serde_json::to_value(&h_catalog).unwrap());
    for (app, foreign) in [
        (&remote, h_lesson.id.as_str()),
        (&h_public, lesson.id.as_str()),
    ] {
        for path in [
            format!("/api/lessons/{foreign}"),
            format!("/api/lessons/{foreign}?revision={}", lesson.revision),
        ] {
            assert_eq!(
                request(app, "GET", &path, None, &mut cookie, &mut csrf)
                    .await
                    .0,
                404,
                "{path}"
            );
        }
    }
    let (status, h_detail) = request(
        &h_public,
        "GET",
        &format!("/api/lessons/{}", h_lesson.id),
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{h_detail}");
    assert_eq!(h_detail, serde_json::to_value(&h_lesson).unwrap());
    assert!(h_detail.get("serverOnly").is_none());
    assert_eq!(
        request(
            &h_public,
            "GET",
            &format!(
                "/api/lessons/{}?revision={}",
                h_lesson.id, h_lesson.revision
            ),
            None,
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        200
    );
    assert_eq!(
        request(
            &remote,
            "GET",
            "/api/catalog?product=hargow",
            None,
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        400
    );
    assert_eq!(request(&h_public,"POST",&format!("/api/demo/lessons/{}/grade",h_lesson.id),Some(serde_json::json!({"revision":h_lesson.revision,"exerciseId":"unknown","answer":{"kind":"text","text":"test"}})),&mut cookie,&mut csrf).await.0,404);
    let spoofed = remote
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/catalog")
                .header("x-chef-product", "hargow")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(spoofed.status().as_u16(), 200);
    let spoofed: serde_json::Value =
        serde_json::from_slice(&spoofed.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(spoofed, serde_json::to_value(&before_b).unwrap());
    // A real, published foreign source must not create facts in this product.
    let foreign_knowledge = &h_lesson.knowledge.vocabulary[1];
    let row = owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,
        "SELECT (SELECT count(*) FROM saved_items WHERE product_id='brioche' AND user_id=$1 AND knowledge_id=$2)+(SELECT count(*) FROM review_cards WHERE product_id='brioche' AND user_id=$1 AND knowledge_id=$2) AS n",
        [account.into(),foreign_knowledge.id.clone().into()])).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 0);
    for (method, path, body) in [
        (
            "PUT",
            format!("/api/v1/me/saved-items/{}", foreign_knowledge.id),
            serde_json::json!({"sourceLessonId":h_lesson.id,"sourceRevision":h_lesson.revision,"saved":true,"version":0,"idempotencyKey":"split-foreign-source-save"}),
        ),
        (
            "POST",
            "/api/v1/me/review-enrollments".into(),
            serde_json::json!({"knowledgeId":foreign_knowledge.id,"sourceLessonId":h_lesson.id,"sourceRevision":h_lesson.revision,"idempotencyKey":"split-foreign-source-enroll"}),
        ),
        (
            "POST",
            "/api/v1/learning-sessions".into(),
            serde_json::json!({"lessonId":h_lesson.id,"schemaVersion":h_lesson.schema_version,"idempotencyKey":"split-foreign-source-start"}),
        ),
    ] {
        assert_eq!(
            request(&remote, method, &path, Some(body), &mut cookie, &mut csrf)
                .await
                .0,
            404,
            "{path}"
        );
    }
    let row = owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,
        "SELECT (SELECT count(*) FROM saved_items WHERE product_id='brioche' AND user_id=$1 AND knowledge_id=$2)+(SELECT count(*) FROM review_cards WHERE product_id='brioche' AND user_id=$1 AND knowledge_id=$2)+(SELECT count(*) FROM learning_sessions WHERE product_id='brioche' AND user_id=$1 AND lesson_id=$3) AS n",
        [account.into(),foreign_knowledge.id.clone().into(),h_lesson.id.clone().into()])).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 0);
    let tx = owner.begin().await.unwrap();
    tx.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO content_withdrawals(product_id,lesson_id,revision) VALUES('hargow',$1,$2)",
        [
            h_lesson.id.clone().into(),
            (h_lesson.revision as i32).into(),
        ],
    ))
    .await
    .unwrap();
    assert!(
        chef_engine::content::catalog_matching_for_product(&tx, Some(ProductId::Hargow), &[])
            .await
            .unwrap()
            .levels
            .is_empty()
    );
    assert_eq!(
        serde_json::to_value(&before_b).unwrap(),
        serde_json::to_value(
            chef_engine::content::catalog_matching_for_product(&tx, Some(ProductId::Brioche), &[])
                .await
                .unwrap()
        )
        .unwrap()
    );
    tx.rollback().await.unwrap();
    // Owning an identifier in H does not reserve it globally: import a valid B source.
    let mut local_import_source = imported_source.clone();
    local_import_source["id"] = h_lesson.id.clone().into();
    local_import_source["revision"] = h_lesson.revision.into();
    let local_import = serde_json::json!({"document":local_import_source.to_string(),"reason":"Own product-local source"});
    // A partial/legacy layout still rejects the foreign global identity; schema errors do not opt in.
    owner.execute_unprepared("ALTER TABLE lesson_import_audit DROP CONSTRAINT chef_local_import_primary; ALTER TABLE lesson_import_audit ADD CONSTRAINT legacy_import_primary_fixture PRIMARY KEY(lesson_id,revision)").await.unwrap();
    let (status, legacy_check) = request(
        &content_app,
        "POST",
        "/api/v1/operator/documents/lesson/check",
        Some(local_import.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{legacy_check}");
    assert_eq!(legacy_check["valid"], false, "{legacy_check}");
    assert_eq!(
        request(
            &content_app,
            "POST",
            "/api/v1/operator/lessons/import",
            Some(local_import.clone()),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        404
    );
    owner.execute_unprepared("ALTER TABLE lesson_import_audit DROP CONSTRAINT legacy_import_primary_fixture; ALTER TABLE lesson_import_audit ADD CONSTRAINT chef_local_import_primary PRIMARY KEY(product_id,lesson_id,revision)").await.unwrap();

    let h_snapshot=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT md5(to_jsonb(l)::text) AS hash FROM lesson_revisions l WHERE product_id='hargow' AND lesson_id=$1 AND revision=$2",[h_lesson.id.clone().into(),(h_lesson.revision as i32).into()])).await.unwrap().unwrap().try_get::<String>("", "hash").unwrap();
    let (status, check) = request(
        &content_app,
        "POST",
        "/api/v1/operator/documents/lesson/check",
        Some(local_import.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{check}");
    assert_eq!(check["valid"], true, "{check}");
    for _ in 0..2 {
        let (status, result) = request(
            &content_app,
            "POST",
            "/api/v1/operator/lessons/import",
            Some(local_import.clone()),
            &mut cookie,
            &mut csrf,
        )
        .await;
        assert_eq!(status, 200, "{result}");
        assert_eq!(result["lessonId"], h_lesson.id);
    }
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT md5(to_jsonb(l)::text) AS hash,(SELECT count(*) FROM lesson_revisions WHERE product_id='brioche' AND lesson_id=$1 AND revision=$2)::bigint AS lessons,(SELECT count(*) FROM lesson_import_audit WHERE product_id='brioche' AND lesson_id=$1 AND revision=$2)::bigint AS audits,(SELECT count(*) FROM lesson_import_audit WHERE product_id='hargow' AND lesson_id=$1 AND revision=$2)::bigint AS foreign_audits FROM lesson_revisions l WHERE l.product_id='hargow' AND l.lesson_id=$1 AND l.revision=$2",[h_lesson.id.clone().into(),(h_lesson.revision as i32).into()])).await.unwrap().unwrap();
    assert_eq!(row.try_get::<String>("", "hash").unwrap(), h_snapshot);
    assert_eq!(row.try_get::<i64>("", "lessons").unwrap(), 1);
    assert_eq!(row.try_get::<i64>("", "audits").unwrap(), 1);
    assert_eq!(row.try_get::<i64>("", "foreign_audits").unwrap(), 0);
    let (status, detail) = request(
        &h_public,
        "GET",
        &format!("/api/lessons/{}", h_lesson.id),
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(detail, serde_json::to_value(&h_lesson).unwrap());
    local_import_source["title"]["zh"] = "Changed immutable own course".into();
    let changed = serde_json::json!({"document":local_import_source.to_string(),"reason":"Immutable conflict"});
    assert_eq!(
        request(
            &content_app,
            "POST",
            "/api/v1/operator/lessons/import",
            Some(changed),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    // Same release ID is legitimate in B; H's staged graph must remain immutable and unchanged.
    owner.execute_unprepared("INSERT INTO content_audit(product_id,action,release_id,actor,reason,generation) VALUES('hargow','stage','hargow-catalog-release','synthetic','Isolated staging sentinel',0)").await.unwrap();
    let release_snapshot_sql = "SELECT md5(jsonb_build_array(to_jsonb(r),(SELECT jsonb_agg(to_jsonb(e) ORDER BY position) FROM release_entries e WHERE e.product_id=r.product_id AND e.release_id=r.id),(SELECT jsonb_agg(to_jsonb(a) ORDER BY id) FROM content_audit a WHERE a.product_id=r.product_id AND a.release_id=r.id),(SELECT to_jsonb(s) FROM content_state s WHERE s.product_id=r.product_id))::text) AS hash FROM content_releases r WHERE product_id='hargow' AND id='hargow-catalog-release'";
    let h_release_before = owner
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            release_snapshot_sql,
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "hash")
        .unwrap();
    let version=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT coalesce(max(version),0)::integer AS version FROM editorial_reviews WHERE product_id='brioche' AND lesson_id=$1 AND revision=$2",[h_lesson.id.clone().into(),(h_lesson.revision as i32).into()])).await.unwrap().unwrap().try_get::<i32>("", "version").unwrap();
    let (status,review)=request(&content_app,"POST",&format!("/api/v1/operator/lessons/{}/revisions/{}/review",h_lesson.id,h_lesson.revision),Some(serde_json::json!({"version":version,"approved":true,"reason":"Isolated protocol fixture editorial decision"})),&mut cookie,&mut csrf).await;
    assert_eq!(status, 200, "{review}");
    let local_manifest = serde_json::json!({"id":"hargow-catalog-release","schemaVersion":"1.0","levels":[{"id":lesson.level_id,"label":"Synthetic B fixture","units":[{"id":lesson.unit_id,"titleZh":"合成B目录","lessons":[{"lessonId":h_lesson.id,"revision":h_lesson.revision}]}]}]});
    let local_stage = serde_json::json!({"document":local_manifest.to_string(),"reason":"Own product-local release"});
    // An old position key means the partial layout must still keep the global ID guard.
    owner.execute_unprepared("ALTER TABLE release_entries DROP CONSTRAINT chef_local_release_position; ALTER TABLE release_entries ADD CONSTRAINT legacy_release_position_fixture UNIQUE(release_id,position)").await.unwrap();
    let (status, legacy_check) = request(
        &content_app,
        "POST",
        "/api/v1/operator/documents/release/check",
        Some(local_stage.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{legacy_check}");
    assert_eq!(legacy_check["valid"], false);
    assert_eq!(
        request(
            &content_app,
            "POST",
            "/api/v1/operator/releases/stage",
            Some(local_stage.clone()),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    owner.execute_unprepared("ALTER TABLE release_entries DROP CONSTRAINT legacy_release_position_fixture; ALTER TABLE release_entries ADD CONSTRAINT chef_local_release_position UNIQUE(product_id,release_id,position)").await.unwrap();
    let (status, checked) = request(
        &content_app,
        "POST",
        "/api/v1/operator/documents/release/check",
        Some(local_stage.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{checked}");
    assert_eq!(checked["valid"], true, "{checked}");
    let (status, staged) = request(
        &content_app,
        "POST",
        "/api/v1/operator/releases/stage",
        Some(local_stage.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{staged}");
    assert_eq!(
        request(
            &content_app,
            "POST",
            "/api/v1/operator/releases/stage",
            Some(local_stage),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    let b_generation = owner
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT generation FROM content_state WHERE product_id='brioche'",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "generation")
        .unwrap();
    let (status,activated)=request(&content_app,"POST","/api/v1/operator/releases/activate",Some(serde_json::json!({"releaseId":"hargow-catalog-release","generation":b_generation.to_string(),"reason":"Own product-local activation"})),&mut cookie,&mut csrf).await;
    assert_eq!(status, 200, "{activated}");
    assert_eq!(activated, (b_generation + 1).to_string());
    let h_release_after = owner
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            release_snapshot_sql,
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "hash")
        .unwrap();
    assert_eq!(h_release_after, h_release_before);
    let (status, b_body) = request(
        &remote,
        "GET",
        &format!("/api/lessons/{}", h_lesson.id),
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{b_body}");
    assert_eq!(b_body["id"], h_lesson.id);
    assert_ne!(b_body, serde_json::to_value(&h_lesson).unwrap());
    let (status, h_body) = request(
        &h_public,
        "GET",
        &format!("/api/lessons/{}", h_lesson.id),
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{h_body}");
    assert_eq!(h_body, serde_json::to_value(&h_lesson).unwrap());

    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM content_releases WHERE product_id='brioche' AND id='hargow-catalog-release')::bigint AS releases,(SELECT count(*) FROM release_entries WHERE product_id='brioche' AND release_id='hargow-catalog-release')::bigint AS entries,(SELECT count(*) FROM content_audit WHERE product_id='brioche' AND release_id='hargow-catalog-release')::bigint AS audits")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "releases").unwrap(), 1);
    assert_eq!(row.try_get::<i64>("", "entries").unwrap(), 1);
    assert_eq!(row.try_get::<i64>("", "audits").unwrap(), 2);
    // Registry IDs are local too: B imports its own bytes and character over H-only names.
    owner.execute_unprepared("INSERT INTO media_assets(product_id,asset_id,revision,descriptor,provenance,sha256,extension,byte_size) SELECT 'hargow','local-product-upload',1,jsonb_set(descriptor,'{assetId}','\"local-product-upload\"'),provenance,sha256,extension,byte_size FROM media_assets WHERE product_id='brioche' AND asset_id='art-bakery-morning' AND revision=1").await.unwrap();
    let foreign_visual_hash=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT md5(to_jsonb(m)::text) AS hash FROM media_assets m WHERE product_id='hargow' AND asset_id='local-product-upload' AND revision=1")).await.unwrap().unwrap().try_get::<String>("", "hash").unwrap();
    owner.execute_unprepared("ALTER TABLE media_assets DROP CONSTRAINT chef_local_visual_primary; ALTER TABLE media_assets ADD CONSTRAINT legacy_visual_primary_fixture PRIMARY KEY(asset_id,revision)").await.unwrap();
    assert_eq!(
        content_app
            .clone()
            .oneshot(asset_upload("local-product-upload", &cookie, &csrf))
            .await
            .unwrap()
            .status()
            .as_u16(),
        409
    );
    owner.execute_unprepared("ALTER TABLE media_assets DROP CONSTRAINT legacy_visual_primary_fixture; ALTER TABLE media_assets ADD CONSTRAINT chef_local_visual_primary PRIMARY KEY(product_id,asset_id,revision)").await.unwrap();
    assert_eq!(
        content_app
            .clone()
            .oneshot(asset_upload("local-product-upload", &cookie, &csrf))
            .await
            .unwrap()
            .status()
            .as_u16(),
        200
    );
    assert_eq!(
        content_app
            .clone()
            .oneshot(asset_upload("local-product-upload", &cookie, &csrf))
            .await
            .unwrap()
            .status()
            .as_u16(),
        409
    );
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT md5(to_jsonb(m)::text) AS hash,(SELECT count(*) FROM media_assets WHERE product_id='brioche' AND asset_id='local-product-upload' AND revision=1)::bigint AS own FROM media_assets m WHERE product_id='hargow' AND asset_id='local-product-upload' AND revision=1")).await.unwrap().unwrap();
    assert_eq!(
        row.try_get::<String>("", "hash").unwrap(),
        foreign_visual_hash
    );
    assert_eq!(row.try_get::<i64>("", "own").unwrap(), 1);
    let file = content_app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/operator/assets/local-product-upload/1/file")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(file.status().as_u16(), 200);
    assert!(
        file.headers()["cache-control"]
            .to_str()
            .unwrap()
            .contains("no-store")
    );
    assert!(
        String::from_utf8(
            file.into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec()
        )
        .unwrap()
        .contains("fill=\"red\"")
    );
    let h_character_hash=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT md5(to_jsonb(c)::text) AS hash FROM character_revisions c WHERE product_id='hargow' AND character_id='aaa-foreign-character-1' AND revision=1")).await.unwrap().unwrap().try_get::<String>("", "hash").unwrap();
    let mut own_character = character.clone();
    own_character["characterId"] = "aaa-foreign-character-1".into();
    own_character["expectedRevision"] = 0.into();
    let (status, created) = request(
        &content_app,
        "POST",
        "/api/v1/operator/characters/revisions",
        Some(own_character.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{created}");
    assert_eq!(
        request(
            &content_app,
            "POST",
            "/api/v1/operator/characters/revisions",
            Some(own_character),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT md5(to_jsonb(c)::text) AS hash,(SELECT count(*) FROM character_revisions WHERE product_id='brioche' AND character_id='aaa-foreign-character-1')::bigint AS own FROM character_revisions c WHERE product_id='hargow' AND character_id='aaa-foreign-character-1' AND revision=1")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<String>("", "hash").unwrap(), h_character_hash);
    assert_eq!(row.try_get::<i64>("", "own").unwrap(), 1);
    // Same fixed character/voice revision in two products, through the real operator endpoint.
    let h_voice_hash=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT md5(to_jsonb(v)::text) AS hash FROM character_voice_profiles v WHERE product_id='hargow' AND character_id='aaa-foreign-character-1' AND character_revision=1 AND revision=1")).await.unwrap().unwrap().try_get::<String>("", "hash").unwrap();
    let mut own_voice = voice.clone();
    own_voice["characterId"] = "aaa-foreign-character-1".into();
    own_voice["characterRevision"] = 1.into();
    own_voice["expectedVoiceRevision"] = 0.into();
    let (status, created_voice) = request(
        &content_app,
        "POST",
        "/api/v1/operator/characters",
        Some(own_voice.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{created_voice}");
    assert_eq!(created_voice["voiceRevision"], 1);
    assert_eq!(
        request(
            &content_app,
            "POST",
            "/api/v1/operator/characters",
            Some(own_voice),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    let (status, read_voice) = request(
        &content_app,
        "GET",
        "/api/v1/operator/characters/aaa-foreign-character-1/1/voices/1",
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{read_voice}");
    assert_eq!(read_voice, created_voice);
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT md5(to_jsonb(v)::text) AS hash,(SELECT count(*) FROM character_voice_profiles WHERE product_id='brioche' AND character_id='aaa-foreign-character-1' AND character_revision=1)::bigint AS own FROM character_voice_profiles v WHERE product_id='hargow' AND character_id='aaa-foreign-character-1' AND character_revision=1 AND revision=1")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<String>("", "hash").unwrap(), h_voice_hash);
    assert_eq!(row.try_get::<i64>("", "own").unwrap(), 1);
    let h_recording_hash=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT md5(to_jsonb(a)::text) AS hash FROM audio_assets a WHERE product_id='hargow' AND asset_id='aaa-foreign-recording-1' AND revision=1")).await.unwrap().unwrap().try_get::<String>("", "hash").unwrap();
    let audit_before = owner
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(*)::bigint AS n FROM audio_import_audit WHERE product_id='brioche'",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "n")
        .unwrap();
    let uploaded = content_app
        .clone()
        .oneshot(recording_upload("aaa-foreign-recording-1", &cookie, &csrf))
        .await
        .unwrap();
    assert_eq!(uploaded.status().as_u16(), 200);
    let repeated = content_app
        .clone()
        .oneshot(recording_upload("aaa-foreign-recording-1", &cookie, &csrf))
        .await
        .unwrap();
    assert_eq!(repeated.status().as_u16(), 409);
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT md5(to_jsonb(a)::text) AS hash,(SELECT count(*) FROM audio_assets WHERE product_id='brioche' AND asset_id='aaa-foreign-recording-1' AND revision=1)::bigint AS own,(SELECT count(*) FROM audio_import_audit WHERE product_id='brioche')::bigint AS audit FROM audio_assets a WHERE product_id='hargow' AND asset_id='aaa-foreign-recording-1' AND revision=1")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<String>("", "hash").unwrap(), h_recording_hash);
    assert_eq!(row.try_get::<i64>("", "own").unwrap(), 1);
    assert_eq!(row.try_get::<i64>("", "audit").unwrap(), audit_before + 1);
    let audio_file = content_app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/operator/recordings/aaa-foreign-recording-1/1/file")
                .header("cookie", &cookie)
                .header("range", "bytes=0-11")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(audio_file.status().as_u16(), 206);
    assert!(
        audio_file.headers()["cache-control"]
            .to_str()
            .unwrap()
            .contains("no-store")
    );
    let bytes = audio_file.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        bytes.as_ref(),
        &include_bytes!("fixtures/audio/synthetic.mp3")[..12]
    );
    let h_audition_hash=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT md5(jsonb_build_array(to_jsonb(a),(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.version) FROM voice_audition_events e WHERE e.product_id=a.product_id AND e.audition_id=a.id),(SELECT to_jsonb(r) FROM voice_audition_reviews r WHERE r.product_id=a.product_id AND r.audition_id=a.id))::text) AS hash FROM voice_auditions a WHERE product_id='hargow' AND id=$1",[foreign_audition.clone().into()])).await.unwrap().unwrap().try_get::<String>("", "hash").unwrap();
    let calls_before = enrollment
        .syntheses
        .load(std::sync::atomic::Ordering::SeqCst);
    let own_audition_request = serde_json::json!({"id":foreign_audition,"candidate":{"characterId":"aaa-foreign-character-1","characterRevision":1,"expectedVoiceRevision":1,"profile":system_profile},"text":"Bonjour !","emotion":"Friendly.","costConfirmed":true,"reason":"Product-local synthetic audition"});
    let (status, submitted_local) = request(
        &content_app,
        "POST",
        audition_route,
        Some(own_audition_request.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{submitted_local}");
    let settled_local = settled_job(&content_app, &foreign_path, &mut cookie, &mut csrf).await;
    assert_eq!(settled_local["status"], "ready", "{settled_local}");
    assert!(
        settled_local["accepted"].is_null(),
        "Foreign review must not enter own audition"
    );
    let (status, retried_local) = request(
        &content_app,
        "POST",
        audition_route,
        Some(own_audition_request.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{retried_local}");
    assert_eq!(retried_local, settled_local);
    let mut changed = own_audition_request;
    changed["text"] = "Au revoir !".into();
    assert_eq!(
        request(
            &content_app,
            "POST",
            audition_route,
            Some(changed),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    assert_eq!(
        enrollment
            .syntheses
            .load(std::sync::atomic::Ordering::SeqCst),
        calls_before + 1
    );
    let review_local = serde_json::json!({"accepted":false,"heard":true,"expectedVoiceRevision":1,"reason":"Synthetic local rejection; no real listening claim"});
    let (status, reviewed_local) = request(
        &content_app,
        "POST",
        &format!("{foreign_path}/review"),
        Some(review_local.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{reviewed_local}");
    assert_eq!(reviewed_local["accepted"], false);
    assert_eq!(
        request(
            &content_app,
            "POST",
            &format!("{foreign_path}/review"),
            Some(review_local),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT md5(jsonb_build_array(to_jsonb(a),(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.version) FROM voice_audition_events e WHERE e.product_id=a.product_id AND e.audition_id=a.id),(SELECT to_jsonb(r) FROM voice_audition_reviews r WHERE r.product_id=a.product_id AND r.audition_id=a.id))::text) AS hash,(SELECT count(*) FROM voice_auditions WHERE product_id='brioche' AND id=$1)::bigint AS own,(SELECT count(*) FROM voice_audition_events WHERE product_id='brioche' AND audition_id=$1)::bigint AS events FROM voice_auditions a WHERE product_id='hargow' AND id=$1",[foreign_audition.clone().into()])).await.unwrap().unwrap();
    assert_eq!(row.try_get::<String>("", "hash").unwrap(), h_audition_hash);
    assert_eq!(row.try_get::<i64>("", "own").unwrap(), 1);
    assert_eq!(row.try_get::<i64>("", "events").unwrap(), 2);
    let h_plan_hash=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT md5(to_jsonb(p)::text) AS hash FROM course_speech_plans p WHERE product_id='hargow' AND id=repeat('4',32)")).await.unwrap().unwrap().try_get::<String>("", "hash").unwrap();
    let mut local_plan_request = plan_request.clone();
    local_plan_request["id"] = "4".repeat(32).into();
    let (status, local_plan) = request(
        &content_app,
        "POST",
        plan_route,
        Some(local_plan_request.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{local_plan}");
    let (status, retried_plan) = request(
        &content_app,
        "POST",
        plan_route,
        Some(local_plan_request.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{retried_plan}");
    assert_eq!(retried_plan, local_plan);
    let mut changed_plan = local_plan_request;
    changed_plan["reason"] = "Changed local plan request".into();
    assert_eq!(
        request(
            &content_app,
            "POST",
            plan_route,
            Some(changed_plan),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    let (status, read_plan) = request(
        &content_app,
        "GET",
        &foreign_plan_path,
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{read_plan}");
    assert_eq!(read_plan, local_plan);
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT md5(to_jsonb(p)::text) AS hash,(SELECT count(*) FROM course_speech_plans WHERE product_id='brioche' AND id=repeat('4',32))::bigint AS own FROM course_speech_plans p WHERE product_id='hargow' AND id=repeat('4',32)")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<String>("", "hash").unwrap(), h_plan_hash);
    assert_eq!(row.try_get::<i64>("", "own").unwrap(), 1);
    // Real archive/report/package operations may reuse a foreign product's IDs,
    // but never its payload, review, source audio, or lesson version.
    let h_delivery_sql = "SELECT md5(jsonb_build_array((SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM speech_alignments a WHERE product_id='hargow'),(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.alignment_id,r.clip_id) FROM speech_alignment_reviews r WHERE product_id='hargow'),(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM speech_package_imports p WHERE product_id='hargow'),(SELECT jsonb_agg(to_jsonb(l) ORDER BY l.lesson_id,l.revision) FROM lesson_revisions l WHERE product_id='hargow'))::text) AS hash";
    let h_delivery_hash = owner
        .query_one_raw(Statement::from_string(DbBackend::Postgres, h_delivery_sql))
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "hash")
        .unwrap();
    let delivery_calls_before = [
        enrollment.creates.load(std::sync::atomic::Ordering::SeqCst),
        enrollment.queries.load(std::sync::atomic::Ordering::SeqCst),
        enrollment
            .syntheses
            .load(std::sync::atomic::Ordering::SeqCst),
    ];
    let response = content_app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&export_path)
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let local_archive = response.into_body().collect().await.unwrap().to_bytes();
    let local_members = tar_members(&local_archive);
    let local_manifest: serde_json::Value =
        serde_json::from_slice(&local_members["manifest.json"]).unwrap();
    let mut local_report = original_alignment_report.clone();
    local_report["sourceArchiveSha256"] = format!("{:x}", Sha256::digest(&local_archive)).into();
    for c in local_report["clips"].as_array_mut().unwrap() {
        let current = local_manifest["clips"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["generationKey"] == c["generationKey"])
            .unwrap();
        c["clipId"] = current["id"].clone();
    }
    let mut local_alignment_request = alignment_request.clone();
    local_alignment_request["id"] = "4".repeat(32).into();
    local_alignment_request["reportJson"] = local_report.to_string().into();
    let (status, local_alignment) = request(
        &content_app,
        "POST",
        alignment_route,
        Some(local_alignment_request.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{local_alignment}");
    assert!(
        local_alignment["clips"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["accepted"].is_null())
    );
    let (status, retry) = request(
        &content_app,
        "POST",
        alignment_route,
        Some(local_alignment_request.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{retry}");
    assert_eq!(retry, local_alignment);
    let (status, read) = request(
        &content_app,
        "GET",
        &foreign_alignment_path,
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{read}");
    assert_eq!(read, local_alignment);
    let mut changed = local_alignment_request;
    changed["reason"] = "Changed product-local alignment".into();
    assert_eq!(
        request(
            &content_app,
            "POST",
            alignment_route,
            Some(changed),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    // Explicit synthetic decisions exercise the package protocol; they are not listening evidence.
    for c in local_alignment["clips"].as_array().unwrap() {
        let review = serde_json::json!({"expectedReportHash":local_alignment["reportHash"],"accepted":true,"heard":true,"timingsChecked":true,"words":c["words"],"reason":"Synthetic product-local timing fixture"});
        let path = format!(
            "{foreign_alignment_path}/clips/{}/review",
            c["clipId"].as_str().unwrap()
        );
        let (status, result) = request(
            &content_app,
            "POST",
            &path,
            Some(review.clone()),
            &mut cookie,
            &mut csrf,
        )
        .await;
        assert_eq!(status, 200, "{result}");
        let (status, retry) = request(
            &content_app,
            "POST",
            &path,
            Some(review),
            &mut cookie,
            &mut csrf,
        )
        .await;
        assert_eq!(status, 200, "{retry}");
        assert_eq!(retry, result);
    }
    let next_revision=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT max(revision)+1 AS revision FROM lesson_revisions WHERE product_id='brioche' AND lesson_id=$1",[lesson.id.clone().into()])).await.unwrap().unwrap().try_get::<i32>("","revision").unwrap();
    let mut local_package_request = package_import.clone();
    local_package_request["id"] = "4".repeat(32).into();
    local_package_request["package"]["expectedReportHash"] = local_alignment["reportHash"].clone();
    local_package_request["package"]["lessonRevision"] = next_revision.into();
    let local_package_path = format!("{foreign_alignment_path}/package/import");
    let (status, local_package) = request(
        &content_app,
        "POST",
        &local_package_path,
        Some(local_package_request.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{local_package}");
    assert_eq!(local_package["revision"], next_revision);
    let (status, retry) = request(
        &content_app,
        "POST",
        &local_package_path,
        Some(local_package_request.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{retry}");
    assert_eq!(retry, local_package);
    let mut changed = local_package_request;
    changed["package"]["reason"] = "Changed product-local package".into();
    assert_eq!(
        request(
            &content_app,
            "POST",
            &local_package_path,
            Some(changed),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    let (status, list) = request(
        &content_app,
        "GET",
        &format!("{foreign_alignment_path}/packages"),
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{list}");
    assert_eq!(list["items"].as_array().unwrap(), &vec![local_package]);
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT (SELECT count(*) FROM speech_alignments WHERE product_id='brioche' AND id=repeat('4',32))::bigint AS alignments,(SELECT count(*) FROM speech_package_imports WHERE product_id='brioche' AND id=repeat('4',32))::bigint AS packages,(SELECT count(*) FROM lesson_import_audit WHERE product_id='brioche' AND lesson_id=$1 AND revision=$2)::bigint AS imports",[lesson.id.clone().into(),next_revision.into()])).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "alignments").unwrap(), 1);
    assert_eq!(row.try_get::<i64>("", "packages").unwrap(), 1);
    assert_eq!(row.try_get::<i64>("", "imports").unwrap(), 1);
    assert_eq!(
        owner
            .query_one_raw(Statement::from_string(DbBackend::Postgres, h_delivery_sql))
            .await
            .unwrap()
            .unwrap()
            .try_get::<String>("", "hash")
            .unwrap(),
        h_delivery_hash
    );
    assert_eq!(
        delivery_calls_before,
        [
            enrollment.creates.load(std::sync::atomic::Ordering::SeqCst),
            enrollment.queries.load(std::sync::atomic::Ordering::SeqCst),
            enrollment
                .syntheses
                .load(std::sync::atomic::Ordering::SeqCst)
        ]
    );
    let h_clip_hash=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT md5(jsonb_build_array(to_jsonb(c),(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.version) FROM course_speech_clip_events e WHERE e.product_id=c.product_id AND e.clip_id=c.id),(SELECT to_jsonb(r) FROM course_speech_clip_reviews r WHERE r.product_id=c.product_id AND r.clip_id=c.id))::text) AS hash FROM course_speech_clips c WHERE product_id='hargow' AND id=$1",[foreign_clip_id.clone().into()])).await.unwrap().unwrap().try_get::<String>("", "hash").unwrap();
    let calls_before = [
        enrollment.creates.load(std::sync::atomic::Ordering::SeqCst),
        enrollment.queries.load(std::sync::atomic::Ordering::SeqCst),
        enrollment
            .syntheses
            .load(std::sync::atomic::Ordering::SeqCst),
    ];
    let mut local_clip_request = clip_request.clone();
    local_clip_request["costConfirmed"] = false.into();
    local_clip_request["id"] = foreign_clip_id.clone().into();
    local_clip_request["planId"] = "4".repeat(32).into();
    local_clip_request["expectedPreviousId"] = reused["id"].clone();
    let (status, local_clip) = request(
        &content_app,
        "POST",
        clip_route,
        Some(local_clip_request.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{local_clip}");
    assert_eq!(local_clip["status"], "ready");
    assert_eq!(local_clip["planId"], "4".repeat(32));
    assert_eq!(local_clip["reusedFrom"], "e".repeat(32));
    let (status, retried_clip) = request(
        &content_app,
        "POST",
        clip_route,
        Some(local_clip_request.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{retried_clip}");
    assert_eq!(retried_clip, local_clip);
    let mut changed_clip = local_clip_request;
    changed_clip["reason"] = "Changed local clip request".into();
    assert_eq!(
        request(
            &content_app,
            "POST",
            clip_route,
            Some(changed_clip),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        409
    );
    let (status, read_clip) = request(
        &content_app,
        "GET",
        &foreign_clip_path,
        None,
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{read_clip}");
    assert_eq!(read_clip, local_clip);
    let review_local = serde_json::json!({"accepted":false,"heard":true,"reason":"Synthetic local clip rejection; no real listening assertion"});
    let (status, reviewed_clip) = request(
        &content_app,
        "POST",
        &format!("{foreign_clip_path}/review"),
        Some(review_local.clone()),
        &mut cookie,
        &mut csrf,
    )
    .await;
    assert_eq!(status, 200, "{reviewed_clip}");
    assert_eq!(reviewed_clip["accepted"], false);
    assert_eq!(
        request(
            &content_app,
            "POST",
            &format!("{foreign_clip_path}/review"),
            Some(review_local),
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        200
    );
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT md5(jsonb_build_array(to_jsonb(c),(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.version) FROM course_speech_clip_events e WHERE e.product_id=c.product_id AND e.clip_id=c.id),(SELECT to_jsonb(r) FROM course_speech_clip_reviews r WHERE r.product_id=c.product_id AND r.clip_id=c.id))::text) AS hash,(SELECT count(*) FROM course_speech_clips WHERE product_id='brioche' AND id=$1)::bigint AS own,(SELECT count(*) FROM course_speech_clip_events WHERE product_id='brioche' AND clip_id=$1)::bigint AS events,(SELECT count(*) FROM course_speech_clip_reviews WHERE product_id='brioche' AND clip_id=$1)::bigint AS reviews FROM course_speech_clips c WHERE product_id='hargow' AND id=$1",[foreign_clip_id.clone().into()])).await.unwrap().unwrap();
    assert_eq!(row.try_get::<String>("", "hash").unwrap(), h_clip_hash);
    assert_eq!(row.try_get::<i64>("", "own").unwrap(), 1);
    assert_eq!(row.try_get::<i64>("", "events").unwrap(), 1);
    assert_eq!(row.try_get::<i64>("", "reviews").unwrap(), 1);
    assert_eq!(
        calls_before,
        [
            enrollment.creates.load(std::sync::atomic::Ordering::SeqCst),
            enrollment.queries.load(std::sync::atomic::Ordering::SeqCst),
            enrollment
                .syntheses
                .load(std::sync::atomic::Ordering::SeqCst)
        ]
    );
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    let result = export_cli
        .run(
            "speech-plan-export-direct",
            plan_id,
            "split@example.test",
            "operator-session.json",
            "identity-offline.tar",
        )
        .await;
    assert!(!result.status.success());
    assert!(!root.join("identity-offline.tar").exists());

    assert_eq!(
        content_app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(live["path"].as_str().unwrap())
                    .body(Body::empty())
                    .unwrap()
            )
            .await
            .unwrap()
            .status()
            .as_u16(),
        503
    );
    assert_eq!(
        request(
            &content_app,
            "GET",
            "/api/v1/operator/overview",
            None,
            &mut cookie,
            &mut csrf
        )
        .await
        .0,
        503
    );
    for path in [
        "/api/v1/operator/assets",
        "/api/v1/operator/recordings",
        "/api/v1/operator/characters",
    ] {
        assert_eq!(
            request(&content_app, "GET", path, None, &mut cookie, &mut csrf)
                .await
                .0,
            503
        );
    }
    for path in [&preview_path, &media_path, &recording_path] {
        assert_eq!(
            request(&content_app, "GET", path, None, &mut cookie, &mut csrf)
                .await
                .0,
            503
        );
    }
    content.close().await.unwrap();
    assert!(
        root.canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap())
    );
    assert_eq!(
        root.file_name().unwrap(),
        format!("brioche-media-{source}").as_str()
    );
    std::fs::remove_dir_all(&root).unwrap();
    learning.close().await.unwrap();
    identity.close().await.unwrap();
    owner.close().await.unwrap();
    admin.execute_unprepared(&format!("DROP SCHEMA {source} CASCADE; DROP SCHEMA {target} CASCADE; DROP SCHEMA {collision} CASCADE; DROP ROLE {id_role}; DROP ROLE {learning_role}; DROP ROLE {content_role}")).await.unwrap();
}
