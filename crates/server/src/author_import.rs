//! Shared immutable lesson import for local author tooling and authenticated operators.
use crate::{
    AppError,
    learning::{exec, field, one},
};
use sea_orm::{ConnectionTrait, DatabaseConnection, TransactionTrait};
use serde_json::Value;
#[derive(Debug)]
pub struct RevisionConflict;

/// Read-only registry/media check. The caller holds a read-only snapshot; no
/// lesson, approval, release, audit or media object is created by this path.
pub(crate) async fn check_registered(
    db: &impl ConnectionTrait,
    product: Option<crate::product::ProductId>,
    document: &crate::author_json::Document,
    root: &std::path::Path,
) -> Result<brioche_course_contract::AdminDocumentCheck, AppError> {
    fn issue(
        document: &crate::author_json::Document,
        error: anyhow::Error,
        message: &str,
    ) -> Result<brioche_course_contract::AdminDocumentCheck, AppError> {
        if error.downcast_ref::<AppError>().is_some() {
            return Err(AppError::Unavailable);
        }
        let pointer = error
            .chain()
            .find_map(|cause| {
                let text = cause.to_string();
                text.split_once(": ")
                    .filter(|(pointer, _)| pointer.starts_with('/'))
                    .map(|(pointer, _)| pointer.to_owned())
            })
            .unwrap_or_else(|| "/".into());
        Ok(document.uploaded_issue(&pointer, message))
    }
    if let Err(error) = check_owner(db, product, &document.value).await {
        return match error {
            AppError::NotFound => {
                Ok(document.uploaded_issue("/id", "课程编号或版本不可用于当前产品。"))
            }
            other => Err(other),
        };
    }
    let source =
        match crate::media::hydrate_source_for_product(db, product, document.value.clone()).await {
            Ok(source) => source,
            Err(error) => return issue(document, error, "图片素材未登记，或引用版本不存在。"),
        };
    let source = match crate::recording::hydrate_source_for_product(db, product, source).await {
        Ok(source) => source,
        Err(error) => return issue(document, error, "录音未登记，或引用版本不存在。"),
    };
    let lesson = match crate::author_source::check_any_source(&source) {
        Ok(lesson) => lesson,
        Err(error) => return issue(document, error, "登记素材与课程结构或引用不匹配。"),
    };
    if let Err(error) =
        crate::media::validate_checked_lesson_detailed(db, product, &lesson, root).await
    {
        match error.runtime {
            AppError::InvalidInput => {
                let pointer = error
                    .diagnostic
                    .strip_prefix("imported lesson ")
                    .and_then(|text| text.split_once(": "))
                    .map_or("/", |(pointer, _)| pointer);
                let message = if error.diagnostic.contains("stored ")
                    || error.diagnostic.contains("decode")
                {
                    "素材文件缺失、损坏或解码信息与登记不一致。"
                } else {
                    "素材、角色或录音未登记，或版本信息不一致。"
                };
                return Ok(document.uploaded_issue(pointer, message));
            }
            other => return Err(other),
        }
    }
    Ok(brioche_course_contract::AdminDocumentCheck {
        valid: true,
        issue: None,
    })
}
impl std::fmt::Display for RevisionConflict {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("/revision: lesson revision already exists; revisions are immutable")
    }
}
impl std::error::Error for RevisionConflict {}
pub(crate) async fn import_operator(
    db: &DatabaseConnection,
    product: Option<crate::product::ProductId>,
    source: Value,
    operator: &crate::product_memberships::Operator,
    reason: &str,
) -> anyhow::Result<brioche_course_contract::AdminImportResult> {
    if product.is_some_and(|product| product != operator.product) {
        return Err(AppError::Forbidden.into());
    }
    let tx = db.begin().await.map_err(|_| AppError::Unavailable)?;
    operator.lock_content(&tx).await?;
    let result =
        import_product_transaction(&tx, product, source, &operator.audit_actor(), reason, true)
            .await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(result)
}
pub async fn import(
    db: &DatabaseConnection,
    source: Value,
    actor: &str,
    reason: &str,
) -> anyhow::Result<brioche_course_contract::AdminImportResult> {
    import_impl(db, None, source, actor, reason, false).await
}
pub async fn import_retry(
    db: &DatabaseConnection,
    source: Value,
    actor: &str,
    reason: &str,
) -> anyhow::Result<brioche_course_contract::AdminImportResult> {
    import_impl(db, None, source, actor, reason, true).await
}
pub(crate) async fn import_author_product(
    db: &DatabaseConnection,
    product: Option<crate::product::ProductId>,
    source: Value,
    actor: &str,
    reason: &str,
) -> anyhow::Result<brioche_course_contract::AdminImportResult> {
    import_impl(db, product, source, actor, reason, false).await
}
async fn import_impl(
    db: &DatabaseConnection,
    product: Option<crate::product::ProductId>,
    source: Value,
    actor: &str,
    reason: &str,
    allow_identical_retry: bool,
) -> anyhow::Result<brioche_course_contract::AdminImportResult> {
    let tx = db.begin().await.map_err(|_| AppError::Unavailable)?;
    let result =
        import_product_transaction(&tx, product, source, actor, reason, allow_identical_retry)
            .await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(result)
}
// Caller owns the transaction and authorization; product comes from trusted assembly.
pub(crate) async fn import_product_transaction(
    db: &impl ConnectionTrait,
    product: Option<crate::product::ProductId>,
    source: Value,
    actor: &str,
    reason: &str,
    allow_identical_retry: bool,
) -> anyhow::Result<brioche_course_contract::AdminImportResult> {
    anyhow::ensure!(
        !actor.trim().is_empty() && actor.len() <= 1000 && !actor.chars().any(char::is_control),
        "/: invalid import actor"
    );
    anyhow::ensure!(
        !reason.trim().is_empty() && reason.len() <= 1000 && !reason.chars().any(char::is_control),
        "/: invalid import reason"
    );
    crate::author_source::validate_any_source_schema(source.clone())?;
    check_owner(db, product, &source).await?;
    crate::media::source_asset_refs(&source)?;
    crate::recording::source_audio_refs(&source)?;
    let source = crate::media::hydrate_source_for_product(db, product, source).await?;
    let source = crate::recording::hydrate_source_for_product(db, product, source).await?;
    let lesson = crate::author_source::check_any_source(&source)?;
    crate::media::validate_checked_product_references(db, product, &lesson).await?;
    crate::recording::validate_checked_product_references(db, product, &lesson).await?;
    // Serialize import retries by their immutable identity, independent of the directory lock.
    exec(
        db,
        "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
        vec![format!("lesson-import:{}:{}", lesson.id(), lesson.revision()).into()],
    )
    .await?;
    // Conservative shared identity lock also serializes legacy-layout imports.
    // Recheck the legacy guard; local layouts read retries only in their product.
    check_owner(db, product, &source).await?;
    let identity = vec![
        lesson.id().to_owned().into(),
        (lesson.revision() as i32).into(),
    ];
    if let Some(existing) = one(
        db,
        &format!(
            "SELECT server_document FROM lesson_revisions WHERE lesson_id=$1 AND revision=$2{}",
            crate::learning::product_filter(product, "product_id")
        ),
        identity,
    )
    .await?
    {
        if !allow_identical_retry || field::<Value>(&existing, "server_document")? != source {
            return Err(RevisionConflict.into());
        }
    } else {
        let mut values = vec![
            lesson.id().to_owned().into(),
            (lesson.revision() as i32).into(),
            lesson.public_document()?.into(),
            source.into(),
        ];
        let sql = if let Some(product) = product {
            values.push(product.as_str().into());
            "INSERT INTO lesson_revisions(lesson_id,revision,published,public_document,server_document,product_id) VALUES($1,$2,false,$3,$4,$5)"
        } else {
            "INSERT INTO lesson_revisions(lesson_id,revision,published,public_document,server_document) VALUES($1,$2,false,$3,$4)"
        };
        exec(db, sql, values).await?;
        let mut values = vec![
            lesson.id().to_owned().into(),
            (lesson.revision() as i32).into(),
            actor.into(),
            reason.into(),
        ];
        let sql = if let Some(product) = product {
            values.push(product.as_str().into());
            "INSERT INTO lesson_import_audit(lesson_id,revision,actor,reason,product_id) VALUES($1,$2,$3,$4,$5)"
        } else {
            "INSERT INTO lesson_import_audit(lesson_id,revision,actor,reason) VALUES($1,$2,$3,$4)"
        };
        exec(db, sql, values).await?;
    }
    Ok(brioche_course_contract::AdminImportResult {
        lesson_id: lesson.id().to_owned(),
        revision: lesson.revision(),
    })
}

// Legacy layouts still need the global-ID collision guard. After both immutable
// lesson and import-audit keys are product-local, foreign identities are irrelevant.
async fn check_owner(
    db: &impl ConnectionTrait,
    product: Option<crate::product::ProductId>,
    source: &Value,
) -> Result<(), AppError> {
    let Some(product) = product else {
        return Ok(());
    };
    let ready=one(db,"SELECT count(*)=2 AS ready FROM pg_catalog.pg_constraint c WHERE c.contype='p' AND c.conrelid IN ('lesson_revisions'::regclass,'lesson_import_audit'::regclass) AND (SELECT array_agg(a.attname::text ORDER BY k.position) FROM unnest(c.conkey) WITH ORDINALITY k(column_number,position) JOIN pg_catalog.pg_attribute a ON a.attrelid=c.conrelid AND a.attnum=k.column_number)=ARRAY['product_id','lesson_id','revision']::text[]",vec![]).await?.ok_or(AppError::Unavailable)?;
    if field::<bool>(&ready, "ready")? {
        return Ok(());
    }
    let id = source["id"].as_str().ok_or(AppError::InvalidInput)?;
    let revision = source["revision"]
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or(AppError::InvalidInput)?;
    if let Some(row) = one(
        db,
        "SELECT product_id FROM lesson_revisions WHERE lesson_id=$1 AND revision=$2",
        vec![id.into(), revision.into()],
    )
    .await?
        && field::<String>(&row, "product_id")? != product.as_str()
    {
        return Err(AppError::NotFound);
    }
    Ok(())
}

#[cfg(test)]
mod neutral_tests {
    use super::*;
    use sea_orm::{ConnectOptions, Database};
    use sea_orm_migration::MigratorTrait;
    use serde_json::json;
    use sha2::{Digest, Sha256};

    async fn totals(db: &DatabaseConnection) -> Value {
        let row = one(db, "SELECT (SELECT count(*) FROM lesson_revisions) AS lessons,(SELECT count(*) FROM lesson_import_audit) AS audits,(SELECT count(*) FROM content_releases) AS releases", vec![]).await.unwrap().unwrap();
        json!([
            field::<i64>(&row, "lessons").unwrap(),
            field::<i64>(&row, "audits").unwrap(),
            field::<i64>(&row, "releases").unwrap()
        ])
    }
    async fn public_request(app: axum::Router, path: &str) -> (axum::http::StatusCode, Value) {
        use http_body_util::BodyExt;
        use tower::ServiceExt;
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri(path)
                    .header("x-product", "brioche")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let value = if status.is_success() {
            serde_json::from_slice(&bytes).unwrap()
        } else {
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
        };
        (status, value)
    }
    #[tokio::test]
    #[ignore = "set TEST_DATABASE_URL to a dedicated PostgreSQL database"]
    async fn neutral_import_hydrates_owned_registries_preserves_wire_and_is_atomic() {
        use crate::product::ProductId;
        let base = std::env::var("TEST_DATABASE_URL").unwrap();
        let admin = Database::connect(&base).await.unwrap();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let schema = format!("neutral_import_{stamp}");
        let identity = format!("neutral_identity_{stamp}");
        admin
            .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
            .await
            .unwrap();
        let mut options = ConnectOptions::new(base);
        options
            .set_schema_search_path(&schema)
            .sqlx_logging(false)
            .max_connections(1);
        let db = Database::connect(options).await.unwrap();
        brioche_migration::Migrator::up(&db, None).await.unwrap();
        crate::schema_split::relocate(&db, &schema, &identity)
            .await
            .unwrap();
        brioche_migration::layout::up(&db, &schema, &identity)
            .await
            .unwrap();
        db.execute_unprepared(
            "INSERT INTO content_state(product_id,singleton) VALUES('hargow',false)",
        )
        .await
        .unwrap();
        let root = std::env::temp_dir().join(format!("chef-neutral-import-{stamp}"));
        std::fs::create_dir(&root).unwrap();
        let visual_root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/visuals");
        let avatar = std::fs::read(visual_root.join("avatars/camille.svg")).unwrap();
        let avatar_sha = format!("{:x}", Sha256::digest(&avatar));
        let mut source: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/neutral-cantonese.lesson.json"
        ))
        .unwrap();
        source["cast"][0]["characterId"] = json!("neutral-character");
        source["cast"][0]["avatarId"] = json!("neutral-avatar");
        source["blocks"][0]["narratorId"] = json!("neutral-character");
        let bundle = json!({"schemaVersion":"1.0","assets":[{"assetId":"neutral-avatar","revision":1,"sha256":avatar_sha,"mimeType":"image/svg+xml","width":96,"height":96,"altZh":"合成协议头像","creditZh":"仅隔离测试","file":"avatars/camille.svg","status":"ready","source":"repository SVG fixture","license":"LicenseRef-TestOnly","creator":"protocol-test","rightsConfirmed":true}],"characters":[{"snapshot":source["cast"][0],"avatarRevision":1}]});
        // Exact locale aliases stay rejected at the registry boundary.
        for locale in ["yue", "zh-HK", "en-US", ""] {
            let mut bad = bundle.clone();
            bad["characters"][0]["snapshot"]["speechLocale"] = json!(locale);
            let bad: crate::media::AssetBundle = serde_json::from_value(bad).unwrap();
            assert!(
                bad.validate_author("protocol-test")
                    .unwrap_err()
                    .to_string()
                    .contains("/characters/0/snapshot/speechLocale")
            );
        }
        crate::media::import_author_bundle(
            &db,
            Some(ProductId::Hargow),
            serde_json::from_value(bundle).unwrap(),
            &visual_root,
            &root,
            "protocol-test",
        )
        .await
        .unwrap();
        let recording = include_bytes!("../tests/fixtures/audio/synthetic.mp3");
        std::fs::write(root.join("synthetic.mp3"), recording).unwrap();
        let info = crate::audio::inspect(recording, "audio/mpeg").unwrap();
        let audio_sha = format!("{:x}", Sha256::digest(recording));
        let bundle = json!({"schemaVersion":"1.0","assets":[{"assetId":"neutral-audio","revision":1,"sha256":audio_sha,"mimeType":"audio/mpeg","durationMs":info.duration_ms,"creditZh":"仅合成协议测试","file":"synthetic.mp3","status":"ready","source":"synthetic protocol fixture","license":"LicenseRef-TestOnly","creator":"protocol-test","rightsConfirmed":true}]});
        crate::recording::import_author_bundle(
            &db,
            Some(ProductId::Hargow),
            serde_json::from_value(bundle).unwrap(),
            &root,
            &root,
            "protocol-test",
        )
        .await
        .unwrap();
        source["assetRefs"] = json!([{"assetId":"neutral-avatar","revision":1}]);
        source["audioRefs"] = json!([{"assetId":"neutral-audio","revision":1}]);
        source["media"] = json!("hydrated registry descriptors");
        source["audio"] = json!("hydrated registry descriptors");
        // Synthetic cues exercise descriptor plumbing only, never real alignment quality.
        source["audioTracks"] = json!([{"blockId":"reading","assetId":"neutral-audio","cues":[
            {"entryId":"paragraph-greeting","segmentId":"segment-greeting","wordRange":{"start":0,"end":2},"startMs":0,"endMs":info.duration_ms},
            {"entryId":"paragraph-greeting","segmentId":"segment-greeting","startMs":0,"endMs":info.duration_ms},
            {"entryId":"paragraph-greeting","startMs":0,"endMs":info.duration_ms}
        ]}]);
        crate::author_source::validate_any_source_schema(source.clone()).unwrap();
        let file = root.join("lesson.json");
        std::fs::write(&file, serde_json::to_vec_pretty(&source).unwrap()).unwrap();
        let document = crate::author_json::Document::load(&file).unwrap();
        assert!(crate::author_json::check_uploaded(&std::fs::read(&file).unwrap(), false).valid);
        let before = totals(&db).await;
        let check = check_registered(&db, Some(ProductId::Hargow), &document, &root)
            .await
            .unwrap();
        assert!(check.valid, "{:?}", check.issue);
        assert_eq!(totals(&db).await, before, "registered check is read-only");
        let foreign = check_registered(&db, Some(ProductId::Brioche), &document, &root)
            .await
            .unwrap();
        assert!(!foreign.valid);
        assert!(
            import_author_product(
                &db,
                Some(ProductId::Brioche),
                source.clone(),
                "protocol-test",
                "foreign refs"
            )
            .await
            .is_err()
        );
        assert_eq!(totals(&db).await, before);
        let result = import_author_product(
            &db,
            Some(ProductId::Hargow),
            source.clone(),
            "protocol-test",
            "synthetic neutral import",
        )
        .await
        .unwrap();
        assert_eq!(result.lesson_id, "neutral-protocol");
        assert_eq!(totals(&db).await, json!([1, 1, 0]));
        let row = one(&db,"SELECT public_document,server_document,published FROM lesson_revisions WHERE product_id='hargow' AND lesson_id='neutral-protocol' AND revision=1",vec![]).await.unwrap().unwrap();
        let public: Value = field(&row, "public_document").unwrap();
        let private: Value = field(&row, "server_document").unwrap();
        assert_eq!(public["schemaVersion"], "2.0");
        assert_eq!(public["targetLanguage"], "yue-Hant-HK");
        assert_eq!(
            public["blocks"][0]["paragraphs"][0]["segments"][0]["reading"],
            source["blocks"][0]["paragraphs"][0]["segments"][0]["reading"]
        );
        assert_eq!(public["audio"][0]["durationMs"], info.duration_ms);
        assert_eq!(public["media"][0]["sha256"], avatar_sha);
        assert!(public.get("serverOnly").is_none() && public["title"].get("fr").is_none());
        assert!(private.get("serverOnly").is_some());
        assert!(!field::<bool>(&row, "published").unwrap());
        // Embedded descriptors cannot bypass product ownership by omitting refs.
        let mut embedded = source.clone();
        embedded.as_object_mut().unwrap().remove("assetRefs");
        embedded.as_object_mut().unwrap().remove("audioRefs");
        embedded["media"] = public["media"].clone();
        embedded["audio"] = public["audio"].clone();
        let error = import_author_product(
            &db,
            Some(ProductId::Brioche),
            embedded.clone(),
            "protocol-test",
            "embedded foreign refs",
        )
        .await
        .unwrap_err();
        assert!(
            error.to_string().starts_with("/media/0/revision:"),
            "{error}"
        );
        let checked = crate::author_source::check_any_source(&embedded).unwrap();
        let error = crate::recording::validate_checked_product_references(
            &db,
            Some(ProductId::Brioche),
            &checked,
        )
        .await
        .unwrap_err();
        assert!(
            error.to_string().starts_with("/audio/0/revision:"),
            "{error}"
        );
        let mut without_visuals = embedded;
        without_visuals["media"] = json!([]);
        let checked = crate::author_source::check_any_source(&without_visuals).unwrap();
        let error = crate::media::validate_checked_product_references(
            &db,
            Some(ProductId::Brioche),
            &checked,
        )
        .await
        .unwrap_err();
        assert!(
            error.to_string().starts_with("/cast/0/revision:"),
            "{error}"
        );
        assert_eq!(totals(&db).await, json!([1, 1, 0]));

        import_impl(
            &db,
            Some(ProductId::Hargow),
            source.clone(),
            "protocol-test",
            "identical retry",
            true,
        )
        .await
        .unwrap();
        let mut changed = source.clone();
        changed["title"]["zh"] = json!("different immutable source");
        assert!(
            import_impl(
                &db,
                Some(ProductId::Hargow),
                changed,
                "protocol-test",
                "conflict",
                true
            )
            .await
            .unwrap_err()
            .is::<RevisionConflict>()
        );
        assert_eq!(totals(&db).await, json!([1, 1, 0]));
        // Registration is not sufficient if the actual stored object changed.
        let object = root.join(format!("{audio_sha}.mp3"));
        std::fs::write(&object, b"broken").unwrap();
        let check = check_registered(&db, Some(ProductId::Hargow), &document, &root)
            .await
            .unwrap();
        assert!(!check.valid);
        assert_eq!(check.issue.unwrap().pointer, "/audioRefs/0");
        std::fs::write(&object, recording).unwrap();
        assert!(
            check_registered(&db, Some(ProductId::Hargow), &document, &root)
                .await
                .unwrap()
                .valid
        );
        // Failing the final audit insert rolls back the new immutable revision.
        db.execute_unprepared("CREATE FUNCTION fail_neutral_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic audit failure'; END $$; CREATE TRIGGER fail_neutral_audit BEFORE INSERT ON lesson_import_audit FOR EACH ROW EXECUTE FUNCTION fail_neutral_audit()").await.unwrap();
        let mut next = source;
        next["revision"] = json!(2);
        assert!(
            import_author_product(
                &db,
                Some(ProductId::Hargow),
                next,
                "protocol-test",
                "audit rollback"
            )
            .await
            .is_err()
        );
        assert_eq!(totals(&db).await, json!([1, 1, 0]));
        db.execute_unprepared("DROP TRIGGER fail_neutral_audit ON lesson_import_audit; DROP FUNCTION fail_neutral_audit()").await.unwrap();
        // Local maintenance kernels exercise v2 publication, not H session/serve readiness.
        let mut publish_source = private.clone();
        publish_source["id"] = json!("neutral-publish");
        publish_source["editorial"] = json!({"status":"reviewed","note":"Explicit synthetic author assertion, no language quality claim"});
        publish_source["audio"] = json!([]);
        publish_source["audioRefs"] = json!([]);
        publish_source["audioTracks"] = json!([]);
        import_author_product(
            &db,
            Some(ProductId::Hargow),
            publish_source,
            "protocol-test",
            "neutral publication protocol",
        )
        .await
        .unwrap();
        let manifest: crate::content::ReleaseManifest = serde_json::from_value(json!({"schemaVersion":"1.0","id":"neutral-release","levels":[{"id":"starter","label":"合成等级","units":[{"id":"greetings","titleZh":"合成单元","lessons":[{"lessonId":"neutral-publish","revision":1}]}]}]})).unwrap();
        let manifest_file = root.join("release.json");
        std::fs::write(&manifest_file, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let manifest_document = crate::author_json::Document::load(&manifest_file).unwrap();
        assert!(
            crate::content::check_registered_release(
                &db,
                Some(ProductId::Hargow),
                &manifest_document,
                &root
            )
            .await
            .unwrap()
            .valid
        );
        let before = totals(&db).await;
        assert!(
            crate::content::stage_author_product(
                &db,
                Some(ProductId::Brioche),
                &manifest,
                "protocol-test",
                "foreign source",
                &root
            )
            .await
            .is_err()
        );
        assert_eq!(totals(&db).await, before);
        crate::content::stage_author_product(
            &db,
            Some(ProductId::Hargow),
            &manifest,
            "protocol-test",
            "neutral stage",
            &root,
        )
        .await
        .unwrap();
        let visual_object = root.join(format!("{avatar_sha}.svg"));
        std::fs::write(&visual_object, b"corrupted").unwrap();
        let before = totals(&db).await;
        assert!(
            crate::content::activate_author_product(
                &db,
                Some(ProductId::Hargow),
                "neutral-release",
                0,
                "protocol-test",
                "corrupted publication",
                &root
            )
            .await
            .is_err()
        );
        assert_eq!(totals(&db).await, before);
        let state = one(
            &db,
            "SELECT generation FROM content_state WHERE product_id='hargow'",
            vec![],
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(field::<i64>(&state, "generation").unwrap(), 0);
        std::fs::write(&visual_object, &avatar).unwrap();
        assert_eq!(
            crate::content::activate_author_product(
                &db,
                Some(ProductId::Hargow),
                "neutral-release",
                0,
                "protocol-test",
                "neutral activation",
                &root
            )
            .await
            .unwrap(),
            1
        );
        let hargow_app = crate::independent_product_router(
            crate::AppState {
                db: Some(db.clone()),
                fixture: None,
            },
            ProductId::Hargow,
        );
        let brioche_app = crate::independent_product_router(
            crate::AppState {
                db: Some(db.clone()),
                fixture: None,
            },
            ProductId::Brioche,
        );
        for (query, expected) in [
            ("", 1),
            ("?q=nei5%20hou2", 1),
            ("?q=%E4%BD%A0%E5%A5%BD", 1),
            ("?q=NEI5%20%E4%BD%A0%E5%A5%BD", 1),
            ("?q=absent", 0),
        ] {
            let (status, wire) =
                public_request(hargow_app.clone(), &format!("/api/v2/catalog{query}")).await;
            assert_eq!(status, axum::http::StatusCode::OK);
            let catalog: brioche_course_contract::neutral::NeutralCatalog =
                serde_json::from_value(wire.clone()).unwrap();
            assert_eq!(
                catalog
                    .levels
                    .iter()
                    .flat_map(|l| &l.units)
                    .flat_map(|u| &u.lessons)
                    .count(),
                expected
            );
            assert!(!catalog.development_fixture);
            if expected == 1 {
                let lesson = &wire["levels"][0]["units"][0]["lessons"][0];
                assert_eq!(lesson["id"], "neutral-publish");
                assert_eq!(lesson["targetLanguage"], "yue-Hant-HK");
                assert_eq!(lesson["title"], private["title"]);
                assert!(lesson.get("knowledge").is_none() && lesson.get("blocks").is_none());
            }
            let (status, wire) =
                public_request(brioche_app.clone(), &format!("/api/v2/catalog{query}")).await;
            assert_eq!(status, axum::http::StatusCode::OK);
            assert!(wire["levels"].as_array().unwrap().is_empty());
        }
        for query in ["?product=brioche", "?q=%00"] {
            let (status, _) =
                public_request(hargow_app.clone(), &format!("/api/v2/catalog{query}")).await;
            assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
        }
        for suffix in ["", "?revision=1"] {
            let path = format!("/api/v2/lessons/neutral-publish{suffix}");
            let (status, public) = public_request(hargow_app.clone(), &path).await;
            assert_eq!(status, axum::http::StatusCode::OK);
            assert_eq!(public["targetLanguage"], "yue-Hant-HK");
            assert_eq!(
                public["blocks"][0]["paragraphs"][0]["segments"][0]["reading"],
                private["blocks"][0]["paragraphs"][0]["segments"][0]["reading"]
            );
            assert!(public.get("serverOnly").is_none() && public["title"].get("fr").is_none());
            let (status, _) = public_request(brioche_app.clone(), &path).await;
            assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
        }
        let (status, _) = public_request(
            hargow_app.clone(),
            "/api/v2/lessons/neutral-protocol?revision=1",
        )
        .await;
        assert_eq!(
            status,
            axum::http::StatusCode::NOT_FOUND,
            "unpublished source"
        );
        let (status, _) = public_request(
            hargow_app.clone(),
            "/api/v2/lessons/neutral-publish?product=brioche",
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
        let (learning_app, learning_id) =
            crate::neutral_learning_tests::exercise(&db, &identity).await;
        let state = one(
            &db,
            "SELECT generation,active_release FROM content_state WHERE product_id='hargow'",
            vec![],
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(field::<i64>(&state, "generation").unwrap(), 1);
        assert_eq!(
            field::<String>(&state, "active_release").unwrap(),
            "neutral-release"
        );
        let brioche = one(
            &db,
            "SELECT generation,active_release FROM content_state WHERE product_id='brioche'",
            vec![],
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(field::<i64>(&brioche, "generation").unwrap(), 0);
        assert!(
            field::<Option<String>>(&brioche, "active_release")
                .unwrap()
                .is_none()
        );
        let published = one(&db,"SELECT published,public_document FROM lesson_revisions WHERE product_id='hargow' AND lesson_id='neutral-publish'",vec![]).await.unwrap().unwrap();
        assert!(field::<bool>(&published, "published").unwrap());
        assert_eq!(
            field::<Value>(&published, "public_document").unwrap()["targetLanguage"],
            "yue-Hant-HK"
        );
        assert!(
            crate::content::activate_author_product(
                &db,
                Some(ProductId::Hargow),
                "neutral-release",
                0,
                "protocol-test",
                "stale activation",
                &root
            )
            .await
            .is_err()
        );
        assert_eq!(
            crate::content::withdraw_author_product(
                &db,
                Some(ProductId::Hargow),
                "neutral-publish",
                1,
                1,
                "protocol-test",
                "neutral withdrawal"
            )
            .await
            .unwrap(),
            2
        );
        assert!(
            crate::content::activate_author_product(
                &db,
                Some(ProductId::Hargow),
                "neutral-release",
                2,
                "protocol-test",
                "withdrawn source",
                &root
            )
            .await
            .is_err()
        );
        let state = one(
            &db,
            "SELECT generation FROM content_state WHERE product_id='hargow'",
            vec![],
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(field::<i64>(&state, "generation").unwrap(), 2);
        for (suffix, expected) in [
            ("", axum::http::StatusCode::NOT_FOUND),
            ("?revision=1", axum::http::StatusCode::GONE),
        ] {
            let (status, _) = public_request(
                hargow_app.clone(),
                &format!("/api/v2/lessons/neutral-publish{suffix}"),
            )
            .await;
            assert_eq!(status, expected);
        }
        let (status, wire) = public_request(hargow_app.clone(), "/api/v2/catalog").await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert!(wire["levels"].as_array().unwrap().is_empty());
        assert_eq!(
            crate::neutral_learning_tests::request(
                &learning_app,
                "GET",
                &format!("/api/v2/learning-sessions/{learning_id}"),
                None
            )
            .await
            .0,
            410
        );
        assert_eq!(
            crate::neutral_learning_tests::request(
                &learning_app,
                "POST",
                "/api/v2/learning-sessions",
                Some(crate::neutral_learning_tests::start_request())
            )
            .await
            .0,
            410
        );
        let (status, overview) = crate::neutral_learning_tests::request(
            &learning_app,
            "GET",
            "/api/v2/me/learning",
            None,
        )
        .await;
        assert_eq!(status, 200);
        assert!(overview["items"].as_array().unwrap().is_empty());
        let (status, saved) = crate::neutral_learning_tests::request(
            &learning_app,
            "GET",
            "/api/v2/me/saved-items/expr-greeting",
            None,
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(saved["withdrawn"], true);
        assert!(saved["vocabulary"].is_null());
        let (status, history) = crate::neutral_learning_tests::request(
            &learning_app,
            "GET",
            "/api/v2/me/review-history",
            None,
        )
        .await;
        assert_eq!(status, 200);
        assert!(history["items"][0]["vocabulary"].is_null());
        let (status, cards) = crate::neutral_learning_tests::request(
            &learning_app,
            "GET",
            "/api/v2/me/review-cards",
            None,
        )
        .await;
        assert_eq!(status, 200);
        assert!(cards["items"].as_array().unwrap().is_empty());
        drop(learning_app);
        drop(hargow_app);
        drop(brioche_app);
        db.close().await.unwrap();
        admin
            .execute_unprepared(&format!(
                "DROP SCHEMA {schema} CASCADE; DROP SCHEMA {identity} CASCADE"
            ))
            .await
            .unwrap();
        admin.close().await.unwrap();
        for entry in std::fs::read_dir(&root).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }
        std::fs::remove_dir(root).unwrap();
    }
}
