//! Revision, ownership, retries and completion require real PostgreSQL transactions.
use axum::{Router, body::Body, http::Request};
use chef_engine::{
    csrf::CsrfPolicy,
    development_source,
    identity::{self, Backend},
    project_source,
};
use http_body_util::BodyExt;
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use tower::ServiceExt;
#[path = "support/assets.rs"]
mod asset_fixtures;
mod support;

struct Browser {
    app: Router,
    cookie: String,
    csrf: String,
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
        protected: bool,
    ) -> (u16, Value) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("cookie", &self.cookie);
        if protected {
            request = request
                .header("origin", "http://localhost:5173")
                .header("x-csrf-token", &self.csrf);
        }
        let request = if let Some(body) = body {
            request
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap()
        } else {
            request.body(Body::empty()).unwrap()
        };
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status().as_u16();
        assert_eq!(
            response.headers().get("cache-control").unwrap(),
            "private, no-store"
        );
        if let Some(cookie) = response.headers().get("set-cookie") {
            self.cookie = cookie.to_str().unwrap().split(';').next().unwrap().into();
        }
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let result: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        if let Some(csrf) = result.get("csrfToken").and_then(Value::as_str) {
            self.csrf = csrf.into();
        }
        (status, result)
    }
    async fn account(&mut self, backend: &Backend, email: &str) {
        let token = backend.issue_token(email, false, false).await.unwrap();
        assert_eq!(self.send("POST","/api/v1/auth/accept-invite",Some(json!({"email":email,"token":token,"password":"learning integration only passphrase","displayName":"Learner"})),true).await.0,200);
    }
    async fn state(&mut self, id: &str) -> Value {
        let (status, result) = self
            .send(
                "GET",
                &format!("/api/v1/learning-sessions/{id}"),
                None,
                true,
            )
            .await;
        assert_eq!(status, 200);
        result["progress"].clone()
    }
    async fn step(&mut self, id: &str, step: &str, version: &Value, key: &str) -> (u16, Value) {
        self.send(
            "PUT",
            &format!("/api/v1/learning-sessions/{id}/steps/{step}"),
            Some(json!({"version":version,"idempotencyKey":key})),
            true,
        )
        .await
    }
    async fn attempt(
        &mut self,
        id: &str,
        exercise: &str,
        answer: Value,
        version: &Value,
        key: &str,
    ) -> (u16, Value) {
        self.send("POST",&format!("/api/v1/learning-sessions/{id}/attempts"),Some(json!({"version":version,"idempotencyKey":key,"exerciseId":exercise,"answer":answer})),true).await
    }
}
async fn publish(db: &DatabaseConnection, source: Value) {
    let lesson = project_source(source.clone()).unwrap();
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO lesson_revisions (lesson_id,revision,published,public_document,server_document) VALUES ($1,$2,true,$3,$4)",[lesson.id.clone().into(),(lesson.revision as i32).into(),serde_json::to_value(lesson).unwrap().into(),source.into()])).await.unwrap();
    support::fixture_release(db).await;
}
async fn count(db: &DatabaseConnection, table: &str) -> i64 {
    db.query_one_raw(Statement::from_string(
        DbBackend::Postgres,
        format!("SELECT count(*)::bigint AS n FROM {table}"),
    ))
    .await
    .unwrap()
    .unwrap()
    .try_get("", "n")
    .unwrap()
}
fn start_body(lesson: &str, key: &str) -> Value {
    json!({"lessonId":lesson,"schemaVersion":"1.0","idempotencyKey":key})
}
#[tokio::test]
#[ignore = "set TEST_DATABASE_URL to a dedicated PostgreSQL database"]
async fn absent_hints_do_not_change_progress_or_attempt_hint_usage() {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL is required");
    let admin = Database::connect(&url).await.unwrap();
    let schema = format!(
        "hint_test_{}",
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
    options.set_schema_search_path(&schema).sqlx_logging(false);
    let db = Database::connect(options).await.unwrap();
    brioche_migration::Migrator::up(&db, None).await.unwrap();
    let mut source = development_source().unwrap();
    let block = source["blocks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|block| block["exerciseType"] == "fill-blank")
        .unwrap();
    block["hintZh"] = json!("\u{00a0}\u{202f}");
    let exercise = block["id"].as_str().unwrap().to_owned();
    let lesson = project_source(source.clone()).unwrap();
    publish(&db, source).await;
    let backend = Backend::new(db.clone()).await.unwrap();
    let app = identity::router(
        backend.clone(),
        CsrfPolicy::new(["http://localhost:5173".into()]).unwrap(),
        false,
    );
    let mut browser = Browser::new(app).await;
    browser.account(&backend, "hintless@example.test").await;
    let (status, mut state) = browser
        .send(
            "POST",
            "/api/v1/learning-sessions",
            Some(start_body(&lesson.id, "hintless-start-001")),
            true,
        )
        .await;
    assert_eq!(status, 200);
    state = state["progress"].clone();
    let id = state["id"].as_str().unwrap().to_owned();
    let practice = lesson
        .steps
        .iter()
        .position(|step| step.block_ids.contains(&exercise))
        .unwrap();
    for step in &lesson.steps[..practice] {
        if lesson.completion.required_step_ids.contains(&step.id) {
            let (status, next) = browser.send("PUT", &format!("/api/v1/learning-sessions/{id}/steps/{}", step.id),
                Some(json!({"version":state["version"],"idempotencyKey":format!("hintless-step-{}",step.id)})), true).await;
            assert_eq!(status, 200);
            state = next;
        }
    }
    let hint_path = format!("/api/v1/learning-sessions/{id}/hints/{exercise}");
    for _ in 0..2 {
        assert_eq!(
            browser
                .send(
                    "POST",
                    &hint_path,
                    Some(
                        json!({"version":state["version"],"idempotencyKey":"hintless-request-001"})
                    ),
                    true
                )
                .await
                .0,
            404
        );
    }
    assert_eq!(
        browser
            .send(
                "GET",
                &format!("/api/v1/learning-sessions/{id}"),
                None,
                true
            )
            .await
            .1["progress"],
        state
    );
    assert_eq!(count(&db, "exercise_hints").await, 0);
    let (status, attempt) = browser
        .attempt(
            &id,
            &exercise,
            json!({"kind":"text","text":"bonjour"}),
            &state["version"],
            "hintless-attempt-001",
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(attempt["progress"]["attempts"][0]["hintUsed"], false);
    assert_eq!(count(&db, "exercise_hints").await, 0);
    db.close().await.unwrap();
    admin
        .execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "set TEST_DATABASE_URL to a dedicated PostgreSQL database"]
async fn releases_atomic_switch_rollback_and_hard_withdrawal() {
    use chef_engine::{AppError, content};
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let admin = Database::connect(&url).await.unwrap();
    let schema = format!(
        "release_test_{}",
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
    options.set_schema_search_path(&schema).sqlx_logging(false);
    let db = Database::connect(options).await.unwrap();
    brioche_migration::Migrator::up(&db, None).await.unwrap();
    // Synthetic review metadata is only for this isolated protocol test, never editorial evidence.
    let media_root = asset_fixtures::fixture_assets(&db, &schema).await;
    let asset_row = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT provenance,descriptor FROM media_assets WHERE asset_id='art-bakery-morning'",
        ))
        .await
        .unwrap()
        .unwrap();
    let spec: Value = asset_row.try_get("", "provenance").unwrap();
    let descriptor: Value = asset_row.try_get("", "descriptor").unwrap();
    let source_root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/visuals");
    for (field, value) in [
        ("status", json!("planned")),
        ("rightsConfirmed", json!(false)),
        ("sha256", json!("0".repeat(64))),
        ("width", json!(12)),
        ("file", json!("../outside.svg")),
    ] {
        let mut invalid = spec.clone();
        invalid["assetId"] = json!("invalid-asset");
        invalid[field] = value;
        let bundle = serde_json::from_value(
            json!({"schemaVersion":"1.0","assets":[invalid],"characters":[]}),
        )
        .unwrap();
        assert!(
            chef_engine::media::import_bundle(
                &db,
                bundle,
                &source_root,
                &media_root,
                "negative-test"
            )
            .await
            .is_err()
        );
        assert_eq!(count(&db, "media_assets").await, 4);
    }
    assert!(
        db.execute_unprepared("UPDATE character_revisions SET snapshot='{}'")
            .await
            .is_err()
    );
    assert!(
        db.execute_unprepared("UPDATE media_assets SET descriptor='{}'")
            .await
            .is_err()
    );
    let media_app = chef_engine::media::router(db.clone(), media_root.clone());
    let asset_path = descriptor["url"].as_str().unwrap();
    let response = media_app
        .clone()
        .oneshot(
            Request::builder()
                .uri(asset_path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        404,
        "registered assets are private before release activation"
    );
    for (id, revision, reviewed) in [
        ("release-z", 1, true),
        ("release-z", 2, true),
        ("release-a", 1, true),
        ("release-draft", 1, false),
    ] {
        let mut source = development_source().unwrap();
        source["id"] = json!(id);
        source["revision"] = json!(revision);
        if revision == 2 {
            source["knowledge"]["vocabulary"][0]["meaningZh"] = json!("新版问候检索");
        }
        source["assetRefs"] = asset_fixtures::fixture_refs();
        if reviewed {
            source["editorial"]["status"] = json!("reviewed");
        } else {
            source["editorial"]["status"] = json!("draft");
        }
        let source = chef_engine::media::hydrate_source(&db, source)
            .await
            .unwrap();
        let lesson = project_source(source.clone()).unwrap();
        db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO lesson_revisions(lesson_id,revision,public_document,server_document) VALUES($1,$2,$3,$4)",[id.into(),revision.into(),serde_json::to_value(lesson).unwrap().into(),source.into()])).await.unwrap();
    }
    let manifest = |id: &str, revision: u32| -> content::ReleaseManifest {
        serde_json::from_value(json!({"id":id,"schemaVersion":"1.0","levels":[{"id":"a1","label":"A1 入门","units":[{"id":"a1-breakfast-bakery","titleZh":"早餐与面包店","lessons":[{"lessonId":"release-z","revision":revision},{"lessonId":"release-a","revision":1}]}]}]})).unwrap()
    };
    assert!(content::catalog(&db).await.unwrap().levels.is_empty());
    assert!(
        content::search_catalog(
            content::catalog(&db).await.unwrap(),
            &content::search_terms("bakery").unwrap()
        )
        .levels
        .is_empty()
    );
    let mut bad = manifest("bad-draft", 1);
    bad.levels[0].units[0].lessons[1].lesson_id = "release-draft".into();
    assert!(matches!(
        content::stage(&db, &bad, "tester", "draft rejected", &media_root).await,
        Err(AppError::InvalidInput)
    ));
    assert_eq!(count(&db, "content_releases").await, 0);
    let mut missing = manifest("bad-missing", 1);
    missing.levels[0].units[0].lessons[1].lesson_id = "missing".into();
    assert!(matches!(
        content::stage(&db, &missing, "tester", "missing reference", &media_root).await,
        Err(AppError::NotFound)
    ));
    let first = manifest("release-first", 1);
    content::stage(
        &db,
        &first,
        "tester",
        "initial protocol fixture",
        &media_root,
    )
    .await
    .unwrap();
    assert!(
        content::catalog(&db).await.unwrap().levels.is_empty(),
        "staging is private"
    );
    assert_eq!(
        content::activate(
            &db,
            &first.id,
            0,
            "tester",
            "initial activation",
            &media_root
        )
        .await
        .unwrap(),
        1
    );
    let catalog = content::catalog(&db).await.unwrap();
    let searched =
        content::catalog_matching(&db, &content::search_terms("BONJOUR 面包店").unwrap())
            .await
            .unwrap();
    assert_eq!(
        serde_json::to_value(&searched).unwrap(),
        serde_json::to_value(&catalog).unwrap(),
        "vocabulary and scene terms combine without reordering the release"
    );
    assert!(
        content::catalog_matching(&db, &content::search_terms("新版问候检索").unwrap())
            .await
            .unwrap()
            .levels
            .is_empty(),
        "an inactive revision must not contribute vocabulary"
    );
    let public_search = serde_json::to_value(&searched).unwrap().to_string();
    assert!(
        !public_search.contains("searchText")
            && !public_search.contains("serverOnly")
            && !public_search.contains("correctOptionId")
    );
    let response = media_app
        .clone()
        .oneshot(
            Request::builder()
                .uri(asset_path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["content-type"], "image/svg+xml");
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        bytes.as_ref(),
        std::fs::read(source_root.join("bakery.svg")).unwrap()
    );
    assert_eq!(catalog.levels[0].units[0].title_zh, "早餐与面包店");
    assert_eq!(
        catalog.levels[0].units[0]
            .lessons
            .iter()
            .map(|l| l.id.as_str())
            .collect::<Vec<_>>(),
        ["release-z", "release-a"],
        "explicit order beats lexical IDs"
    );
    let backend = Backend::new(db.clone()).await.unwrap();
    let app = identity::router_with_media_root(
        backend.clone(),
        CsrfPolicy::new(["http://localhost:5173".into()]).unwrap(),
        false,
        media_root.clone(),
    );
    let mut a = Browser::new(app.clone()).await;
    a.account(&backend, "release-one@example.test").await;
    let mut b = Browser::new(app.clone()).await;
    b.account(&backend, "release-two@example.test").await;
    let preview = "/api/v1/operator/lessons/release-draft/revisions/1";
    let mut anonymous = Browser::new(app.clone()).await;
    assert_eq!(anonymous.send("GET", preview, None, false).await.0, 401);
    assert_eq!(a.send("GET", preview, None, true).await.0, 403);
    let mut operator = Browser::new(app.clone()).await;
    let token = backend
        .issue_token("preview-operator@example.test", false, true)
        .await
        .unwrap();
    assert_eq!(operator.send("POST", "/api/v1/auth/accept-invite", Some(json!({"email":"preview-operator@example.test","token":token,"password":"isolated operator test passphrase","displayName":"Operator"})), true).await.0,200);
    let before_preview = count(&db, "learning_sessions").await;
    let before_attempts = count(&db, "exercise_attempts").await;
    let grade_path = format!("{preview}/grade");
    let choice = json!({"revision":1,"exerciseId":"exercise-intention","answer":{"kind":"choice","optionId":"request-bread"}});
    assert_eq!(
        anonymous
            .send("POST", &grade_path, Some(choice.clone()), true)
            .await
            .0,
        401
    );
    assert_eq!(
        a.send("POST", &grade_path, Some(choice.clone()), true)
            .await
            .0,
        403
    );
    assert_eq!(
        operator
            .send("POST", &grade_path, Some(choice.clone()), false)
            .await
            .0,
        403
    );
    for (exercise, answer, correct) in [
        (
            "exercise-intention",
            json!({"kind":"choice","optionId":"request-bread"}),
            true,
        ),
        (
            "exercise-article",
            json!({"kind":"text","text":" UNE "}),
            true,
        ),
        (
            "exercise-article",
            json!({"kind":"text","text":"un"}),
            false,
        ),
        (
            "exercise-order",
            json!({"kind":"order","tokenIds":["request","bread","please"]}),
            true,
        ),
        (
            "exercise-order",
            json!({"kind":"order","tokenIds":["please","bread","request"]}),
            false,
        ),
    ] {
        let body = json!({"revision":1,"exerciseId":exercise,"answer":answer});
        let (status, result) = operator.send("POST", &grade_path, Some(body), true).await;
        assert_eq!(status, 200);
        assert_eq!(result["correct"], correct);
        assert_eq!(result["exerciseId"], exercise);
        assert_eq!(result.as_object().unwrap().len(), 3);
    }
    for (body, status) in [
        (
            json!({"revision":2,"exerciseId":"exercise-intention","answer":{"kind":"choice","optionId":"request-bread"}}),
            400,
        ),
        (
            json!({"revision":1,"exerciseId":"missing","answer":{"kind":"choice","optionId":"request-bread"}}),
            404,
        ),
        (
            json!({"revision":1,"exerciseId":"exercise-intention","answer":{"kind":"choice","optionId":"forged"}}),
            400,
        ),
        (
            json!({"revision":1,"exerciseId":"exercise-order","answer":{"kind":"order","tokenIds":["request","request","please"]}}),
            400,
        ),
        (
            json!({"revision":1,"exerciseId":"exercise-article","answer":{"kind":"text","text":"une"},"correct":true}),
            422,
        ),
    ] {
        assert_eq!(
            operator.send("POST", &grade_path, Some(body), true).await.0,
            status
        );
    }
    assert_eq!(count(&db, "exercise_attempts").await, before_attempts);
    assert_eq!(count(&db, "learning_sessions").await, before_preview);
    let staged = manifest("preview-only-release", 1);
    content::stage(&db, &staged, "tester", "isolated preview test", &media_root)
        .await
        .unwrap();
    let release_path = "/api/v1/operator/releases/preview-only-release";
    assert_eq!(
        anonymous.send("GET", release_path, None, false).await.0,
        401
    );
    assert_eq!(a.send("GET", release_path, None, true).await.0, 403);
    let (status, release_preview) = operator.send("GET", release_path, None, true).await;
    assert_eq!(status, 200);
    assert_eq!(release_preview["id"], "preview-only-release");
    let entries = release_preview["catalog"]["levels"][0]["units"][0]["lessons"]
        .as_array()
        .unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|e| e["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["release-z", "release-a"]
    );
    assert_eq!(entries[0]["revision"], 1);
    assert_eq!(release_preview["catalog"]["levels"][0]["label"], "A1 入门");
    assert_eq!(release_preview["withdrawnLessonIds"], json!([]));
    assert_eq!(
        operator
            .send("GET", "/api/v1/operator/releases/missing", None, true)
            .await
            .0,
        404
    );
    for key in [
        "serverOnly",
        "editorial",
        "accepted",
        "correctOptionId",
        "correctTokenIds",
    ] {
        assert!(!release_preview.to_string().contains(key));
    }
    let state = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT active_release,generation FROM content_state WHERE singleton",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        state.try_get::<String>("", "active_release").unwrap(),
        "release-first"
    );
    assert_eq!(state.try_get::<i64>("", "generation").unwrap(), 1);
    let (status, draft) = operator.send("GET", preview, None, true).await;
    assert_eq!(status, 200);
    assert_eq!(draft["id"], "release-draft");
    for key in [
        "serverOnly",
        "editorial",
        "accepted",
        "correctOptionId",
        "correctTokenIds",
    ] {
        assert!(!draft.to_string().contains(key));
    }
    let preview_media = draft["media"][0]["url"].as_str().unwrap();
    assert!(preview_media.starts_with(preview));
    assert_eq!(
        anonymous.send("GET", preview_media, None, false).await.0,
        401
    );
    assert_eq!(a.send("GET", preview_media, None, true).await.0, 403);
    assert_eq!(operator.send("GET", preview_media, None, true).await.0, 200);
    assert_eq!(
        operator
            .send(
                "GET",
                &format!("{preview}/media/{}.svg", "0".repeat(64)),
                None,
                true
            )
            .await
            .0,
        404
    );
    assert_eq!(
        operator
            .send(
                "GET",
                "/api/v1/operator/lessons/missing/revisions/1",
                None,
                true
            )
            .await
            .0,
        404
    );
    assert_eq!(
        operator
            .send(
                "GET",
                "/api/v1/operator/lessons/release-z/revisions/0",
                None,
                true
            )
            .await
            .0,
        400
    );
    assert_eq!(count(&db, "learning_sessions").await, before_preview);
    db.execute_raw(Statement::from_string(
        DbBackend::Postgres,
        "UPDATE users SET role='learner' WHERE email='preview-operator@example.test'",
    ))
    .await
    .unwrap();
    assert_eq!(operator.send("GET", preview, None, true).await.0, 403);
    assert_eq!(
        operator
            .send("POST", &grade_path, Some(choice.clone()), true)
            .await
            .0,
        403
    );
    assert_eq!(operator.send("GET", release_path, None, true).await.0, 403);
    db.execute_raw(Statement::from_string(
        DbBackend::Postgres,
        "UPDATE users SET role='operator' WHERE email='preview-operator@example.test'",
    ))
    .await
    .unwrap();
    let (_, old) = a
        .send(
            "POST",
            "/api/v1/learning-sessions",
            Some(start_body("release-z", "release-old-start")),
            true,
        )
        .await;
    assert_eq!(old["lesson"]["revision"], 1);
    let mut mismatched = project_source(
        chef_engine::media::hydrate_source(&db, {
            let mut source = development_source().unwrap();
            source["assetRefs"] = asset_fixtures::fixture_refs();
            source
        })
        .await
        .unwrap(),
    )
    .unwrap();
    mismatched.cast[2].display_name = "changed narrator".into();
    assert!(matches!(
        chef_engine::media::validate_lesson(&db, &mismatched, &media_root).await,
        Err(AppError::InvalidInput)
    ));
    let newer = manifest("release-second", 2);
    content::stage(&db, &newer, "tester", "second revision", &media_root)
        .await
        .unwrap();
    let stored_path = media_root.join(asset_path.rsplit('/').next().unwrap());
    let original_bytes = std::fs::read(&stored_path).unwrap();
    std::fs::write(&stored_path, b"corrupt object").unwrap();
    assert!(matches!(
        content::activate(
            &db,
            &newer.id,
            1,
            "tester",
            "corruption must prevent publish",
            &media_root
        )
        .await,
        Err(AppError::InvalidInput)
    ));
    let response = media_app
        .clone()
        .oneshot(
            Request::builder()
                .uri(asset_path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 503);
    std::fs::write(&stored_path, original_bytes).unwrap();
    // Audit failure rolls back availability and pointer changes together.
    db.execute_unprepared("ALTER TABLE content_audit ADD CONSTRAINT test_activate_failure CHECK(action<>'activate') NOT VALID").await.unwrap();
    assert!(
        content::activate(&db, &newer.id, 1, "tester", "injected failure", &media_root)
            .await
            .is_err()
    );
    assert_eq!(
        content::catalog(&db).await.unwrap().levels[0].units[0].lessons[0].revision,
        1
    );
    let published: bool = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT published FROM lesson_revisions WHERE lesson_id='release-z' AND revision=2",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get("", "published")
        .unwrap();
    assert!(!published);
    db.execute_unprepared("ALTER TABLE content_audit DROP CONSTRAINT test_activate_failure")
        .await
        .unwrap();
    let (left, right) = tokio::join!(
        content::activate(
            &db,
            &newer.id,
            1,
            "tester-a",
            "concurrent activation",
            &media_root
        ),
        content::activate(
            &db,
            &first.id,
            1,
            "tester-b",
            "concurrent rollback",
            &media_root
        )
    );
    assert!(matches!(
        (&left, &right),
        (Ok(2), Err(AppError::Conflict)) | (Err(AppError::Conflict), Ok(2))
    ));
    assert_eq!(
        content::activate(
            &db,
            &newer.id,
            2,
            "tester",
            "select new version",
            &media_root
        )
        .await
        .unwrap(),
        3
    );
    let (_, current) = b
        .send(
            "POST",
            "/api/v1/learning-sessions",
            Some(start_body("release-z", "release-new-start")),
            true,
        )
        .await;
    assert_eq!(current["lesson"]["revision"], 2);
    let matched = content::catalog_matching(&db, &content::search_terms("新版问候检索").unwrap())
        .await
        .unwrap();
    assert_eq!(matched.levels[0].units[0].lessons.len(), 1);
    assert_eq!(matched.levels[0].units[0].lessons[0].id, "release-z");
    assert_eq!(matched.levels[0].units[0].lessons[0].revision, 2);
    let old_path = format!(
        "/api/v1/learning-sessions/{}",
        old["progress"]["id"].as_str().unwrap()
    );
    assert_eq!(
        a.send("GET", &old_path, None, true).await.1["lesson"]["revision"],
        1
    );
    assert_eq!(
        content::activate(
            &db,
            &first.id,
            3,
            "tester",
            "ordinary rollback",
            &media_root
        )
        .await
        .unwrap(),
        4
    );
    let new_path = format!(
        "/api/v1/learning-sessions/{}",
        current["progress"]["id"].as_str().unwrap()
    );
    assert_eq!(
        b.send("GET", &new_path, None, true).await.1["lesson"]["revision"],
        2,
        "rollback does not withdraw pinned content"
    );
    assert!(
        content::catalog_matching(&db, &content::search_terms("新版问候检索").unwrap())
            .await
            .unwrap()
            .levels
            .is_empty(),
        "rollback restores the active vocabulary snapshot"
    );
    let public_app = chef_engine::router(chef_engine::AppState {
        db: Some(db.clone()),
        fixture: None,
    });
    for (path, status, revision) in [
        ("/api/lessons/release-z", 200, 1),
        ("/api/lessons/release-z?revision=2", 200, 2),
        ("/api/lessons/release-z?revision=0", 400, 0),
    ] {
        let response = public_app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status);
        if status == 200 {
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let document: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(document["revision"], revision);
        }
    }
    assert_eq!(
        content::withdraw(&db, "release-z", 2, 4, "tester", "hard withdrawal")
            .await
            .unwrap(),
        5
    );
    assert_eq!(b.send("GET", &new_path, None, true).await.0, 410);
    assert_eq!(
        operator
            .send(
                "GET",
                "/api/v1/operator/lessons/release-z/revisions/2",
                None,
                true
            )
            .await
            .0,
        410
    );
    let withdrawn_media = format!(
        "/api/v1/operator/lessons/release-z/revisions/2/media/{}",
        preview_media.rsplit('/').next().unwrap()
    );
    assert_eq!(
        operator.send("GET", &withdrawn_media, None, true).await.0,
        410
    );
    let mut withdrawn_grade = choice.clone();
    withdrawn_grade["revision"] = json!(2);
    assert_eq!(
        operator
            .send(
                "POST",
                "/api/v1/operator/lessons/release-z/revisions/2/grade",
                Some(withdrawn_grade),
                true
            )
            .await
            .0,
        410
    );
    let response = public_app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/lessons/release-z?revision=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 410);
    assert!(matches!(
        content::activate(
            &db,
            &newer.id,
            5,
            "tester",
            "cannot restore withdrawn",
            &media_root
        )
        .await,
        Err(AppError::Gone)
    ));
    assert!(
        db.execute_unprepared(
            "UPDATE lesson_revisions SET published=true WHERE lesson_id='release-z' AND revision=2"
        )
        .await
        .is_err()
    );
    assert!(db.execute_unprepared("UPDATE lesson_revisions SET public_document=jsonb_set(public_document,'{summaryZh}','\"changed\"') WHERE lesson_id='release-z' AND revision=1").await.is_err());
    assert!(
        db.execute_unprepared("UPDATE content_releases SET manifest='{}'")
            .await
            .is_err()
    );
    assert!(
        db.execute_unprepared("DELETE FROM content_audit")
            .await
            .is_err()
    );
    assert!(db.execute_unprepared("INSERT INTO release_entries(release_id,lesson_id,revision,position) VALUES('release-first','release-draft',1,2)").await.is_err());
    assert_eq!(
        content::withdraw(&db, "release-z", 1, 5, "tester", "withdraw current")
            .await
            .unwrap(),
        6
    );
    assert_eq!(
        content::catalog(&db).await.unwrap().levels[0].units[0]
            .lessons
            .len(),
        1
    );
    let dashboard = a.send("GET", "/api/v1/me/dashboard", None, true).await.1;
    let (status, withdrawn_release) = operator.send("GET", release_path, None, true).await;
    assert_eq!(status, 200);
    assert_eq!(
        withdrawn_release["withdrawnLessonIds"],
        json!(["release-z"])
    );
    assert_eq!(
        withdrawn_release["catalog"]["levels"][0]["units"][0]["lessons"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(dashboard["recommendedLesson"]["id"], "release-a");
    let response = public_app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/catalog?q=boulangerie")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let searched: brioche_course_contract::Catalog =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(searched.levels[0].units[0].lessons.len(), 1);
    assert_eq!(searched.levels[0].units[0].lessons[0].id, "release-a");
    assert_eq!(
        dashboard["catalog"]["levels"][0]["units"][0]["lessons"][0]["id"],
        dashboard["recommendedLesson"]["id"]
    );
    assert!(dashboard["resume"].is_null());
    let empty: content::ReleaseManifest =
        serde_json::from_value(json!({"id":"release-empty","schemaVersion":"1.0","levels":[]}))
            .unwrap();
    content::stage(&db, &empty, "tester", "empty directory", &media_root)
        .await
        .unwrap();
    assert_eq!(
        content::activate(&db, &empty.id, 6, "tester", "empty release", &media_root)
            .await
            .unwrap(),
        7
    );
    assert!(content::catalog(&db).await.unwrap().levels.is_empty());
    content::withdraw(
        &db,
        "release-a",
        1,
        7,
        "tester",
        "withdraw final shared reference",
    )
    .await
    .unwrap();
    let response = media_app
        .oneshot(
            Request::builder()
                .uri(asset_path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        404,
        "no remaining published reference makes media private"
    );
    brioche_migration::Migrator::down(&db, None).await.unwrap();
    assert!(
        media_root
            .canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap())
    );
    std::fs::remove_dir_all(&media_root).unwrap();
    drop(db);
    admin
        .execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .unwrap();
}
#[tokio::test]
#[ignore = "set TEST_DATABASE_URL to a dedicated PostgreSQL database"]
async fn learning_revision_ownership_idempotency_and_completion() {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL is required");
    let admin = Database::connect(&url).await.unwrap();
    let schema = format!(
        "learning_test_{}",
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
    options.set_schema_search_path(&schema).sqlx_logging(false);
    let db = Database::connect(options).await.unwrap();
    brioche_migration::Migrator::up(&db, None).await.unwrap();
    // Unreviewed fixture is published only into this isolated test schema, never a production import.
    let source = development_source().unwrap();
    let lesson_id = source["id"].as_str().unwrap();
    publish(&db, source.clone()).await;
    let backend = Backend::new(db.clone()).await.unwrap();
    let app = identity::router(
        backend.clone(),
        CsrfPolicy::new(["http://localhost:5173".into()]).unwrap(),
        false,
    );
    let mut a = Browser::new(app.clone()).await;
    assert_eq!(
        a.send("GET", "/api/v1/me/dashboard", None, true).await.0,
        401
    );
    assert_eq!(
        a.send("GET", "/api/v1/me/learning", None, true).await.0,
        401
    );
    assert_eq!(
        a.send(
            "POST",
            "/api/v1/learning-sessions",
            Some(start_body(lesson_id, "learning-start-001")),
            true
        )
        .await
        .0,
        401
    );
    a.account(&backend, "one@example.test").await;
    let mut b = Browser::new(app.clone()).await;
    b.account(&backend, "two@example.test").await;
    let (status, dashboard) = a.send("GET", "/api/v1/me/dashboard", None, true).await;
    assert_eq!(status, 200);
    assert_eq!(dashboard["days"].as_array().unwrap().len(), 7);
    assert_eq!(dashboard["activeDays"], 0);
    assert_eq!(dashboard["completedLessons"], 0);
    assert_eq!(dashboard["dueReviews"], 0);
    assert_eq!(dashboard["recommendedLesson"]["id"], lesson_id);
    assert_eq!(dashboard["allAvailableCompleted"], false);
    assert!(dashboard["resume"].is_null());
    assert_eq!(
        a.send(
            "POST",
            "/api/v1/learning-sessions",
            Some(start_body(lesson_id, "learning-start-001")),
            false
        )
        .await
        .0,
        403
    );
    let mut invalid = start_body(lesson_id, "learning-start-001");
    invalid["schemaVersion"] = json!("99");
    assert_eq!(
        a.send("POST", "/api/v1/learning-sessions", Some(invalid), true)
            .await
            .0,
        400
    );
    assert_eq!(
        a.send(
            "POST",
            "/api/v1/learning-sessions",
            Some(start_body(lesson_id, "short")),
            true
        )
        .await
        .0,
        400
    );
    let initial = start_body(lesson_id, "learning-start-001");
    let (status, opened) = a
        .send(
            "POST",
            "/api/v1/learning-sessions",
            Some(initial.clone()),
            true,
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(opened["lesson"]["revision"], 1);
    let text = serde_json::to_string(&opened).unwrap();
    for forbidden in [
        "serverOnly",
        "correctOptionId",
        "correctTokenIds",
        "accepted",
        "editorial",
    ] {
        assert!(!text.contains(forbidden));
    }
    let id = opened["progress"]["id"].as_str().unwrap().to_owned();
    let (_, dashboard) = a.send("GET", "/api/v1/me/dashboard", None, true).await;
    assert_eq!(dashboard["resume"]["sessionId"], id);
    assert_eq!(
        dashboard["activeDays"], 0,
        "opening a session is not learning activity"
    );
    assert_eq!(
        a.send(
            "POST",
            "/api/v1/learning-sessions",
            Some(initial.clone()),
            true
        )
        .await
        .1,
        opened
    );
    assert_eq!(
        a.send(
            "POST",
            "/api/v1/learning-sessions",
            Some(start_body("different-lesson", "learning-start-001")),
            true
        )
        .await
        .0,
        409
    );
    let mut another_a = Browser::new(app.clone()).await;
    assert_eq!(another_a.send("POST","/api/v1/auth/login",Some(json!({"email":"one@example.test","password":"learning integration only passphrase"})),true).await.0,200);
    let (left, right) = tokio::join!(
        a.send(
            "POST",
            "/api/v1/learning-sessions",
            Some(start_body(lesson_id, "learning-start-002")),
            true
        ),
        another_a.send(
            "POST",
            "/api/v1/learning-sessions",
            Some(start_body(lesson_id, "learning-start-003")),
            true
        )
    );
    assert_eq!(left.0, 200);
    assert_eq!(right.0, 200);
    assert_eq!(left.1["progress"]["id"], right.1["progress"]["id"]);
    assert_eq!(count(&db, "learning_sessions").await, 1);
    assert_eq!(
        a.attempt(
            &id,
            "exercise-intention",
            json!({"kind":"choice","optionId":"request-bread"}),
            &json!(1),
            "attempt-too-early"
        )
        .await
        .0,
        409
    );
    assert_eq!(
        a.step(&id, "step-recap", &json!(1), "recap-too-early-1")
            .await
            .0,
        409
    );
    assert_eq!(
        a.send(
            "POST",
            &format!("/api/v1/learning-sessions/{id}/complete"),
            Some(json!({"version":1,"idempotencyKey":"complete-too-early"})),
            true
        )
        .await
        .0,
        409
    );
    let read_body = json!({"version":1,"idempotencyKey":"read-confirm-001"});
    let (status, read) = a
        .send(
            "PUT",
            &format!("/api/v1/learning-sessions/{id}/steps/step-read"),
            Some(read_body.clone()),
            true,
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(read["version"], 2);
    assert_eq!(read["lastStepId"], "step-explore");
    assert_eq!(
        a.send(
            "PUT",
            &format!("/api/v1/learning-sessions/{id}/steps/step-read"),
            Some(read_body),
            true
        )
        .await
        .1,
        read
    );
    assert_eq!(
        a.step(&id, "step-read", &json!(2), "read-confirm-001")
            .await
            .0,
        409
    );
    assert_eq!(
        a.step(&id, "step-read", &json!(2), "read-confirm-002")
            .await
            .1["version"],
        2
    );
    let alternative = source["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "exercise-intention")
        .unwrap()["options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] != "request-bread")
        .unwrap()["id"]
        .clone();
    let mut updated_source = source.clone();
    updated_source["revision"] = json!(2);
    updated_source["serverOnly"]["grading"]["exercise-intention"]["correctOptionId"] =
        alternative.clone();
    publish(&db, updated_source).await;
    assert_eq!(
        a.send(
            "POST",
            "/api/v1/learning-sessions",
            Some(start_body(lesson_id, "learning-start-004")),
            true
        )
        .await
        .1["lesson"]["revision"],
        1,
        "active session retains the old snapshot"
    );
    assert_eq!(
        a.attempt(
            &id,
            "unknown-exercise",
            json!({"kind":"choice","optionId":"request-bread"}),
            &json!(2),
            "unknown-exercise-1"
        )
        .await
        .0,
        404
    );
    assert_eq!(
        a.attempt(
            &id,
            "exercise-intention",
            json!({"kind":"text","text":"anything"}),
            &json!(2),
            "wrong-kind-test-1"
        )
        .await
        .0,
        400
    );
    assert_eq!(
        a.attempt(
            &id,
            "exercise-intention",
            json!({"kind":"choice","optionId":"request-bread","score":100}),
            &json!(2),
            "forged-score-001"
        )
        .await
        .0,
        422
    );
    assert_eq!(
        a.attempt(
            &id,
            "exercise-intention",
            json!({"kind":"choice","optionId":"unknown"}),
            &json!(2),
            "unknown-option-01"
        )
        .await
        .0,
        400
    );
    assert_eq!(count(&db, "exercise_attempts").await, 0);
    let hint_path = format!("/api/v1/learning-sessions/{id}/hints/exercise-article");
    let hint_body = json!({"version":2,"idempotencyKey":"hint-article-001"});
    let (status, hint) = a
        .send("POST", &hint_path, Some(hint_body.clone()), true)
        .await;
    assert_eq!(status, 200);
    assert_eq!(hint["progress"]["version"], 3);
    assert!(hint["hintZh"].as_str().unwrap().len() > 1);
    assert_eq!(
        a.send("POST", &hint_path, Some(hint_body), true).await.1,
        hint
    );
    assert_eq!(
        a.send(
            "POST",
            &format!("/api/v1/learning-sessions/{id}/hints/exercise-intention"),
            Some(json!({"version":3,"idempotencyKey":"hint-not-found-1"})),
            true
        )
        .await
        .0,
        404
    );
    let wrong_answer = json!({"kind":"choice","optionId":alternative});
    let (status, wrong) = a
        .attempt(
            &id,
            "exercise-intention",
            wrong_answer.clone(),
            &json!(3),
            "attempt-choice-01",
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(wrong["result"]["correct"], false);
    assert_eq!(
        a.attempt(
            &id,
            "exercise-intention",
            wrong_answer,
            &json!(3),
            "attempt-choice-01"
        )
        .await
        .1,
        wrong
    );
    assert_eq!(
        a.attempt(
            &id,
            "exercise-intention",
            json!({"kind":"choice","optionId":"request-bread"}),
            &json!(3),
            "attempt-choice-01"
        )
        .await
        .0,
        409
    );
    let (status, correct) = a
        .attempt(
            &id,
            "exercise-intention",
            json!({"kind":"choice","optionId":"request-bread"}),
            &json!(4),
            "attempt-choice-02",
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(
        correct["result"]["correct"], true,
        "grading uses revision 1, not the new rules"
    );
    let attempts = correct["progress"]["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0]["attemptIndex"], 1);
    assert_eq!(attempts[0]["result"]["correct"], false);
    assert_eq!(attempts[1]["attemptIndex"], 2);
    let order = json!({"kind":"order","tokenIds":["please","bread","request"]}); // deliberately incorrect, but a valid attempt
    let concurrent_version = json!(5);
    let (left, right) = tokio::join!(
        a.attempt(
            &id,
            "exercise-article",
            json!({"kind":"text","text":"une"}),
            &concurrent_version,
            "attempt-article-1"
        ),
        another_a.attempt(
            &id,
            "exercise-order",
            order.clone(),
            &concurrent_version,
            "attempt-order-001"
        )
    );
    assert!((left.0 == 200 && right.0 == 409) || (left.0 == 409 && right.0 == 200));
    assert_eq!(count(&db, "exercise_attempts").await, 3);
    if left.0 == 409 {
        assert_eq!(
            a.attempt(
                &id,
                "exercise-article",
                json!({"kind":"text","text":"une"}),
                &json!(6),
                "attempt-article-2"
            )
            .await
            .0,
            200
        );
    } else {
        assert_eq!(
            a.attempt(
                &id,
                "exercise-order",
                order.clone(),
                &json!(6),
                "attempt-order-002"
            )
            .await
            .0,
            200
        );
    }
    let state = a.state(&id).await;
    assert_eq!(state["version"], 7);
    let hints = state["attempts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["exerciseId"] == "exercise-article")
        .unwrap();
    assert_eq!(hints["hintUsed"], true);
    assert_eq!(
        a.step(&id, "step-practice", &json!(7), "practice-confirm")
            .await
            .0,
        200
    );
    assert_eq!(
        a.step(&id, "step-recap", &json!(8), "recap-confirm-01")
            .await
            .0,
        200
    );
    let complete_path = format!("/api/v1/learning-sessions/{id}/complete");
    let complete_body = json!({"version":9,"idempotencyKey":"complete-final-01"});
    db.execute_unprepared("ALTER TABLE review_cards ADD CONSTRAINT test_card_failure CHECK (knowledge_id <> 'word-baguette') NOT VALID").await.unwrap();
    assert_eq!(
        a.send("POST", &complete_path, Some(complete_body.clone()), true)
            .await
            .0,
        503
    );
    assert_eq!(
        count(&db, "review_cards").await,
        0,
        "partial review insertion must roll back"
    );
    let failed = a.state(&id).await;
    assert_eq!(failed["version"], 9);
    assert!(failed["firstCompletedAt"].is_null());
    assert!(failed["completedAt"].is_null());
    db.execute_unprepared("ALTER TABLE review_cards DROP CONSTRAINT test_card_failure")
        .await
        .unwrap();
    let (status, completed) = a
        .send("POST", &complete_path, Some(complete_body.clone()), true)
        .await;
    assert_eq!(status, 200);
    assert_eq!(completed["version"], 10);
    assert!(completed["completedAt"].is_string());
    assert_eq!(count(&db, "review_cards").await, 3);
    let first = completed["firstCompletedAt"].clone();
    assert_eq!(
        a.send("POST", &complete_path, Some(complete_body), true)
            .await
            .1,
        completed
    );
    assert_eq!(
        a.send(
            "POST",
            &complete_path,
            Some(json!({"version":10,"idempotencyKey":"complete-repeat-1"})),
            true
        )
        .await
        .1["firstCompletedAt"],
        first
    );
    assert_eq!(
        a.attempt(
            &id,
            "exercise-intention",
            json!({"kind":"choice","optionId":"request-bread"}),
            &json!(10),
            "after-complete-1"
        )
        .await
        .0,
        409
    );
    for (method, path, body) in [
        ("GET", format!("/api/v1/learning-sessions/{id}"), None),
        (
            "PUT",
            format!("/api/v1/learning-sessions/{id}/steps/step-read"),
            Some(json!({"version":10,"idempotencyKey":"cross-owner-read"})),
        ),
        (
            "POST",
            complete_path.clone(),
            Some(json!({"version":10,"idempotencyKey":"cross-owner-done"})),
        ),
        (
            "POST",
            hint_path.clone(),
            Some(json!({"version":10,"idempotencyKey":"cross-owner-hint"})),
        ),
        (
            "POST",
            format!("/api/v1/learning-sessions/{id}/attempts"),
            Some(
                json!({"version":10,"idempotencyKey":"cross-owner-test","exerciseId":"exercise-intention","answer":{"kind":"choice","optionId":"request-bread"}}),
            ),
        ),
    ] {
        assert_eq!(b.send(method, &path, body, true).await.0, 404);
    }
    let (_, b_opened) = b
        .send("POST", "/api/v1/learning-sessions", Some(initial), true)
        .await;
    assert_ne!(b_opened["progress"]["id"], id);
    assert_eq!(b_opened["lesson"]["revision"], 2);
    assert_eq!(
        b.send("GET", "/api/v1/me/learning", None, true).await.1["completedLessons"],
        0
    );
    let (status, next) = a
        .send(
            "POST",
            "/api/v1/learning-sessions",
            Some(start_body(lesson_id, "new-run-start-01")),
            true,
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(next["lesson"]["revision"], 2);
    assert_eq!(next["progress"]["firstCompletedAt"], first);
    let new_id = next["progress"]["id"].as_str().unwrap().to_owned();
    assert_ne!(id, new_id);
    let mut version = json!(1);
    let (_, read) = a
        .step(&new_id, "step-read", &version, "second-read-0001")
        .await;
    version = read["version"].clone();
    for (exercise, answer, key) in [
        (
            "exercise-intention",
            json!({"kind":"choice","optionId":"request-bread"}),
            "second-choice-01",
        ),
        (
            "exercise-article",
            json!({"kind":"text","text":"une"}),
            "second-article-1",
        ),
        ("exercise-order", order, "second-order-001"),
    ] {
        let (status, response) = a.attempt(&new_id, exercise, answer, &version, key).await;
        assert_eq!(status, 200);
        if exercise == "exercise-intention" {
            assert_eq!(response["result"]["correct"], false);
        }
        version = response["progress"]["version"].clone();
    }
    for (step, key) in [
        ("step-practice", "second-practice-1"),
        ("step-recap", "second-recap-001"),
    ] {
        let (status, response) = a.step(&new_id, step, &version, key).await;
        assert_eq!(status, 200);
        version = response["version"].clone();
    }
    let (status, again) = a
        .send(
            "POST",
            &format!("/api/v1/learning-sessions/{new_id}/complete"),
            Some(json!({"version":version,"idempotencyKey":"second-complete1"})),
            true,
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(again["firstCompletedAt"], first);
    assert_eq!(
        count(&db, "review_cards").await,
        3,
        "shared knowledge does not reset or duplicate cards"
    );
    let latest:i32=db.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT latest_completed_revision FROM lesson_progress WHERE first_completed_at IS NOT NULL")).await.unwrap().unwrap().try_get("","latest_completed_revision").unwrap();
    assert_eq!(latest, 2);
    let (_, dashboard) = a.send("GET", "/api/v1/me/dashboard", None, true).await;
    assert_eq!(dashboard["completedLessons"], 1);
    assert_eq!(dashboard["dueReviews"], 3);
    assert_eq!(dashboard["allAvailableCompleted"], true);
    assert!(dashboard["resume"].is_null());
    assert_eq!(dashboard["activeDays"], 1);
    let days = dashboard["days"].as_array().unwrap();
    assert_eq!(
        days.iter()
            .map(|day| day["completedLessons"].as_u64().unwrap())
            .sum::<u64>(),
        1
    );
    assert!(
        days.iter()
            .map(|day| day["exerciseAttempts"].as_u64().unwrap())
            .sum::<u64>()
            >= 6
    );
    let (_, other) = b.send("GET", "/api/v1/me/dashboard", None, true).await;
    assert_eq!(other["activeDays"], 0, "activity is isolated by account");
    assert_eq!(other["completedLessons"], 0);
    assert_eq!(
        a.send("GET", "/api/v1/me/learning", None, true).await.1["completedLessons"],
        1
    );
    let (status, queue) = a.send("GET", "/api/v1/me/reviews", None, true).await;
    assert_eq!(status, 200);
    assert_eq!(queue["dueCount"], 3);
    assert_eq!(queue["items"].as_array().unwrap().len(), 3);
    assert_eq!(
        b.send("GET", "/api/v1/me/reviews", None, true).await.1["dueCount"],
        0
    );
    assert_eq!(
        a.send("GET", "/api/v1/me/reviews?date=2999-01-01", None, true)
            .await
            .0,
        400
    );
    assert_eq!(
        a.send("GET", "/api/v1/me/reviews?date=bad", None, true)
            .await
            .0,
        400
    );
    let review_id = queue["items"][0]["id"].as_str().unwrap().to_owned();
    let review_path = format!("/api/v1/me/reviews/{review_id}/attempts");
    let review_body =
        json!({"cardVersion":1,"idempotencyKey":"review-key-000001","rating":"familiar"});
    assert_eq!(
        b.send("POST", &review_path, Some(review_body.clone()), true)
            .await
            .0,
        404
    );
    assert_eq!(
        b.send(
            "GET",
            &format!("/api/v1/me/reviews/{review_id}"),
            None,
            true
        )
        .await
        .0,
        404
    );
    assert_eq!(a.send("POST", &review_path, Some(json!({"cardVersion":1,"idempotencyKey":"review-key-forged","rating":"familiar","stage":4})),true).await.0,422);
    assert_eq!(
        a.send(
            "POST",
            &review_path,
            Some(json!({"cardVersion":1,"idempotencyKey":"review-key-badval","rating":"invented"})),
            true
        )
        .await
        .0,
        422
    );
    db.execute_unprepared(
        "ALTER TABLE review_attempts ADD CONSTRAINT test_review_failure CHECK (false) NOT VALID",
    )
    .await
    .unwrap();
    assert_eq!(
        a.send("POST", &review_path, Some(review_body.clone()), true)
            .await
            .0,
        503
    );
    assert_eq!(count(&db, "review_attempts").await, 0);
    assert_eq!(
        a.send(
            "GET",
            &format!("/api/v1/me/reviews/{review_id}"),
            None,
            true
        )
        .await
        .1["version"],
        1
    );
    db.execute_unprepared("ALTER TABLE review_attempts DROP CONSTRAINT test_review_failure")
        .await
        .unwrap();
    let (status, reviewed) = a
        .send("POST", &review_path, Some(review_body.clone()), true)
        .await;
    assert_eq!(status, 200);
    assert_eq!(reviewed["card"]["stage"], 1);
    assert_eq!(reviewed["card"]["version"], 2);
    assert_eq!(reviewed["timeZone"], "Asia/Shanghai");
    let (_, dashboard_before_retry) = a.send("GET", "/api/v1/me/dashboard", None, true).await;
    assert_eq!(count(&db, "review_attempts").await, 1);
    assert_eq!(
        a.send("POST", &review_path, Some(review_body.clone()), true)
            .await
            .1,
        reviewed
    );
    let (_, dashboard_after_retry) = a.send("GET", "/api/v1/me/dashboard", None, true).await;
    assert_eq!(
        dashboard_before_retry["days"],
        dashboard_after_retry["days"]
    );
    assert_eq!(count(&db, "review_attempts").await, 1);
    assert_eq!(
        a.send(
            "POST",
            &review_path,
            Some(json!({"cardVersion":1,"idempotencyKey":"review-key-000001","rating":"again"})),
            true
        )
        .await
        .0,
        409
    );
    assert_eq!(
        a.send(
            "POST",
            &review_path,
            Some(json!({"cardVersion":1,"idempotencyKey":"review-key-stale01","rating":"again"})),
            true
        )
        .await
        .0,
        409
    );
    assert_eq!(
        a.send(
            "POST",
            &review_path,
            Some(json!({"cardVersion":2,"idempotencyKey":"review-key-future1","rating":"again"})),
            true
        )
        .await
        .0,
        409
    );
    assert_eq!(
        a.send("GET", "/api/v1/me/reviews", None, true).await.1["dueCount"],
        2
    );
    // A timezone edit preserves existing UTC due times. Idempotent replay keeps the original zone.
    db.execute_unprepared(
        "UPDATE users SET settings=jsonb_set(settings,'{timeZone}','\"Europe/Paris\"')",
    )
    .await
    .unwrap();
    assert_eq!(
        a.send(
            "GET",
            &format!("/api/v1/me/reviews/{review_id}"),
            None,
            true
        )
        .await
        .1["dueAt"],
        reviewed["card"]["dueAt"]
    );
    assert_eq!(
        a.send("POST", &review_path, Some(review_body.clone()), true)
            .await
            .1,
        reviewed
    );
    let saved_path = "/api/v1/me/saved-items/word-baguette";
    let save_body = json!({"sourceLessonId":lesson_id,"sourceRevision":1,"saved":true,"version":0,"idempotencyKey":"saved-create-0001"});
    let (status, saved) = a
        .send("PUT", saved_path, Some(save_body.clone()), true)
        .await;
    assert_eq!(status, 200);
    assert_eq!(saved["version"], 1);
    assert_eq!(saved["saved"], true);
    assert_eq!(
        count(&db, "review_cards").await,
        3,
        "bookmark does not add or reset reviews"
    );
    assert_eq!(
        a.send("PUT", saved_path, Some(save_body.clone()), true)
            .await
            .1,
        saved
    );
    assert_eq!(
        a.send("GET", "/api/v1/me/saved-items", None, true).await.1["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(b.send("GET", saved_path, None, true).await.0, 404);
    assert_eq!(
        b.send("GET", "/api/v1/me/saved-items", None, true).await.1["items"],
        json!([])
    );
    assert_eq!(a.send("PUT",saved_path,Some(json!({"sourceLessonId":lesson_id,"sourceRevision":1,"saved":false,"version":0,"idempotencyKey":"saved-stale-0001"})),true).await.0,409);
    assert_eq!(a.send("PUT",saved_path,Some(json!({"sourceLessonId":lesson_id,"sourceRevision":1,"saved":false,"version":1,"idempotencyKey":"saved-create-0001"})),true).await.0,409);
    assert_eq!(
        a.send(
            "PUT",
            "/api/v1/me/saved-items/unknown",
            Some(save_body),
            true
        )
        .await
        .0,
        404
    );
    let removed=a.send("PUT",saved_path,Some(json!({"sourceLessonId":lesson_id,"sourceRevision":1,"saved":false,"version":1,"idempotencyKey":"saved-remove-0001"})),true).await;
    assert_eq!(removed.0, 200);
    assert_eq!(removed.1["version"], 2);
    assert_eq!(
        count(&db, "review_cards").await,
        3,
        "removing a bookmark leaves reviews intact"
    );
    let restored=a.send("PUT",saved_path,Some(json!({"sourceLessonId":lesson_id,"sourceRevision":2,"saved":true,"version":2,"idempotencyKey":"saved-restore-001"})),true).await;
    assert_eq!(restored.0, 200);
    assert_eq!(restored.1["sourceRevision"], 1);
    assert_eq!(restored.1["createdAt"], saved["createdAt"]);
    let enrollment = json!({"knowledgeId":"word-baguette","sourceLessonId":lesson_id,"sourceRevision":2,"idempotencyKey":"enroll-repeat-001"});
    let enrolled = a
        .send(
            "POST",
            "/api/v1/me/review-enrollments",
            Some(enrollment.clone()),
            true,
        )
        .await;
    assert_eq!(enrolled.0, 200);
    assert_eq!(enrolled.1["sourceRevision"], 1);
    assert_eq!(
        a.send(
            "POST",
            "/api/v1/me/review-enrollments",
            Some(enrollment),
            true
        )
        .await
        .1,
        enrolled.1
    );
    assert_eq!(count(&db, "review_cards").await, 3);
    let prefs_path = format!("/api/v1/me/reviews/{review_id}/preferences");
    let pause = json!({"cardVersion":2,"idempotencyKey":"pause-review-0001","suspended":true});
    assert_eq!(
        b.send("PUT", &prefs_path, Some(pause.clone()), true)
            .await
            .0,
        404
    );
    let paused = a.send("PUT", &prefs_path, Some(pause.clone()), true).await;
    assert_eq!(paused.0, 200);
    assert_eq!(paused.1["suspended"], true);
    assert_eq!(paused.1["version"], 3);
    assert_eq!(paused.1["dueAt"], reviewed["card"]["dueAt"]);
    assert_eq!(
        a.send("PUT", &prefs_path, Some(pause), true).await.1,
        paused.1
    );
    assert_eq!(
        a.send(
            "POST",
            &review_path,
            Some(json!({"cardVersion":3,"idempotencyKey":"paused-attempt-01","rating":"again"})),
            true
        )
        .await
        .0,
        409
    );
    let resumed = a
        .send(
            "PUT",
            &prefs_path,
            Some(json!({"cardVersion":3,"idempotencyKey":"resume-review-01","suspended":false})),
            true,
        )
        .await;
    assert_eq!(resumed.0, 200);
    assert_eq!(resumed.1["suspended"], false);
    assert_eq!(resumed.1["dueAt"], paused.1["dueAt"]);
    assert_eq!(resumed.1["stage"], paused.1["stage"]);
    assert_eq!(
        a.send("GET", "/api/v1/me/review-cards", None, true).await.1["items"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        b.send("GET", "/api/v1/me/review-cards", None, true).await.1["items"],
        json!([])
    );
    let history = a.send("GET", "/api/v1/me/review-history", None, true).await;
    assert_eq!(history.0, 200);
    assert_eq!(history.1["items"].as_array().unwrap().len(), 1);
    assert_eq!(history.1["items"][0]["algorithmVersion"], "fixed-v1");
    assert_eq!(history.1["items"][0]["timeZone"], "Asia/Shanghai");
    assert_eq!(
        b.send("GET", "/api/v1/me/review-history", None, true)
            .await
            .1["items"],
        json!([])
    );
    db.execute_unprepared("UPDATE lesson_revisions SET published=false")
        .await
        .unwrap();
    let (_, dashboard) = a.send("GET", "/api/v1/me/dashboard", None, true).await;
    assert_eq!(
        dashboard["completedLessons"], 1,
        "withdrawal preserves historical facts"
    );
    assert_eq!(dashboard["courseStates"], json!([]));
    assert_eq!(dashboard["dueReviews"], 0);
    assert!(dashboard["resume"].is_null());
    assert!(dashboard["recommendedLesson"].is_null());
    let withdrawn_saved = a.send("GET", saved_path, None, true).await;
    assert_eq!(withdrawn_saved.0, 200);
    assert_eq!(withdrawn_saved.1["withdrawn"], true);
    assert!(withdrawn_saved.1["vocabulary"].is_null());
    assert!(
        a.send("GET", "/api/v1/me/review-history", None, true)
            .await
            .1["items"][0]["vocabulary"]
            .is_null()
    );
    assert_eq!(a.send("PUT",saved_path,Some(json!({"sourceLessonId":lesson_id,"sourceRevision":1,"saved":false,"version":3,"idempotencyKey":"withdraw-unsave01"})),true).await.0,200);
    assert_eq!(
        a.send("GET", "/api/v1/me/reviews", None, true).await.1["dueCount"],
        0
    );
    assert_eq!(
        a.send("POST", &review_path, Some(review_body), true)
            .await
            .0,
        410
    );
    assert_eq!(
        a.send(
            "GET",
            &format!("/api/v1/learning-sessions/{id}"),
            None,
            true
        )
        .await
        .0,
        410
    );
    assert_eq!(
        a.send(
            "POST",
            "/api/v1/learning-sessions",
            Some(start_body(lesson_id, "learning-start-001")),
            true
        )
        .await
        .0,
        410
    );
    assert_eq!(
        a.attempt(
            &id,
            "exercise-intention",
            json!({"kind":"choice","optionId":alternative}),
            &json!(3),
            "attempt-choice-01"
        )
        .await
        .0,
        410,
        "withdrawal also blocks cached grading feedback"
    );
    assert_eq!(
        a.send("GET", "/api/v1/me/learning", None, true).await.1["items"],
        json!([])
    );
    // Cursor pagination has a fixed bound, stable tuple order and no foreign-user rows.
    for index in 0..25 {
        let mut page_source = source.clone();
        let page_id = format!("pagination-lesson-{index}");
        page_source["id"] = json!(page_id);
        publish(&db, page_source).await;
        assert_eq!(
            a.send(
                "POST",
                "/api/v1/learning-sessions",
                Some(start_body(&page_id, &format!("pagination-key-{index:03}"))),
                true
            )
            .await
            .0,
            200
        );
    }
    let (_, dashboard) = a.send("GET", "/api/v1/me/dashboard", None, true).await;
    assert_eq!(dashboard["courseStates"].as_array().unwrap().len(), 25);
    assert_eq!(dashboard["allAvailableCompleted"], false);
    let (_, page1) = a.send("GET", "/api/v1/me/learning", None, true).await;
    assert_eq!(page1["items"].as_array().unwrap().len(), 20);
    let cursor = page1["nextCursor"].as_str().unwrap();
    let (_, page2) = a
        .send(
            "GET",
            &format!("/api/v1/me/learning?cursor={cursor}"),
            None,
            true,
        )
        .await;
    assert_eq!(page2["items"].as_array().unwrap().len(), 5);
    assert!(page2["nextCursor"].is_null());
    let mut ids = std::collections::BTreeSet::new();
    for page in [&page1, &page2] {
        for item in page["items"].as_array().unwrap() {
            assert!(ids.insert(item["sessionId"].as_str().unwrap()));
        }
    }
    assert_eq!(
        a.send("GET", "/api/v1/me/learning?cursor=broken", None, true)
            .await
            .0,
        400
    );
    db.execute_unprepared("INSERT INTO review_cards (id,user_id,knowledge_id,source_lesson_id,source_revision,snapshot,due_at) SELECT md5('review-batch-'||n),c.user_id,'batch-'||n,'pagination-lesson-0',1,jsonb_set(c.snapshot,'{id}',to_jsonb('batch-'||n)),CURRENT_TIMESTAMP - (n||' days')::interval FROM (SELECT user_id,snapshot FROM review_cards LIMIT 1) c CROSS JOIN generate_series(1,12) n").await.unwrap();
    let (_, batch) = a.send("GET", "/api/v1/me/reviews", None, true).await;
    assert_eq!(batch["dueCount"], 12);
    assert_eq!(batch["items"].as_array().unwrap().len(), 10);
    assert_eq!(batch["items"][0]["knowledgeId"], "batch-12");
    let batch_id = batch["items"][0]["id"].as_str().unwrap();
    let concurrent_path = format!("/api/v1/me/reviews/{batch_id}/attempts");
    let mut other_tab = Browser {
        app: a.app.clone(),
        cookie: a.cookie.clone(),
        csrf: a.csrf.clone(),
    };
    let (left,right)=tokio::join!(
        a.send("POST",&concurrent_path,Some(json!({"cardVersion":1,"idempotencyKey":"review-concurrent-a","rating":"again"})),true),
        other_tab.send("POST",&concurrent_path,Some(json!({"cardVersion":1,"idempotencyKey":"review-concurrent-b","rating":"remembered"})),true)
    );
    let mut statuses = [left.0, right.0];
    statuses.sort();
    assert_eq!(statuses, [200, 409]);
    assert_eq!(count(&db, "review_attempts").await, 2);
    assert_eq!(
        a.send("GET", "/api/v1/me/reviews", None, true).await.1["dueCount"],
        11
    );
    db.execute_unprepared("INSERT INTO saved_items (id,user_id,knowledge_id,source_lesson_id,source_revision,snapshot) SELECT md5('saved-page-'||n),c.user_id,'saved-page-'||n,'pagination-lesson-0',1,jsonb_set(c.snapshot,'{id}',to_jsonb('saved-page-'||n)) FROM (SELECT user_id,snapshot FROM review_cards WHERE user_id=(SELECT id FROM users WHERE email='one@example.test') LIMIT 1) c CROSS JOIN generate_series(1,25) n").await.unwrap();
    let (_, saved_page1) = a.send("GET", "/api/v1/me/saved-items", None, true).await;
    assert_eq!(saved_page1["items"].as_array().unwrap().len(), 20);
    let saved_cursor = saved_page1["nextCursor"].as_str().unwrap();
    let (_, saved_page2) = a
        .send(
            "GET",
            &format!("/api/v1/me/saved-items?cursor={saved_cursor}"),
            None,
            true,
        )
        .await;
    assert_eq!(saved_page2["items"].as_array().unwrap().len(), 5);
    assert!(saved_page2["nextCursor"].is_null());
    let bookmark = saved_page1["items"][0].clone();
    let knowledge = bookmark["knowledgeId"].as_str().unwrap();
    let bookmark_path = format!("/api/v1/me/saved-items/{knowledge}");
    let body = json!({"sourceLessonId":"pagination-lesson-0","sourceRevision":1,"saved":false,"version":1,"idempotencyKey":"saved-concurrent-a"});
    let mut other_tab = Browser {
        app: a.app.clone(),
        cookie: a.cookie.clone(),
        csrf: a.csrf.clone(),
    };
    let (left,right)=tokio::join!(a.send("PUT",&bookmark_path,Some(body.clone()),true),other_tab.send("PUT",&bookmark_path,Some(json!({"sourceLessonId":"pagination-lesson-0","sourceRevision":1,"saved":false,"version":1,"idempotencyKey":"saved-concurrent-b"})),true));
    let mut statuses = [left.0, right.0];
    statuses.sort();
    assert_eq!(statuses, [200, 409]);
    db.execute_unprepared("INSERT INTO review_attempts (id,card_id,user_id,rating,old_stage,new_stage,old_version,new_version,due_at,reviewed_at,time_zone) SELECT md5('history-page-'||n),c.id,c.user_id,'again',0,0,100+n,101+n,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP-(n||' days')::interval,'Europe/Paris' FROM (SELECT id,user_id FROM review_cards WHERE user_id=(SELECT id FROM users WHERE email='one@example.test') LIMIT 1) c CROSS JOIN generate_series(1,30) n").await.unwrap();
    let (_, history_page1) = a.send("GET", "/api/v1/me/review-history", None, true).await;
    assert_eq!(history_page1["items"].as_array().unwrap().len(), 20);
    let history_cursor = history_page1["nextCursor"].as_str().unwrap();
    let (_, history_page2) = a
        .send(
            "GET",
            &format!("/api/v1/me/review-history?cursor={history_cursor}"),
            None,
            true,
        )
        .await;
    assert_eq!(history_page2["items"].as_array().unwrap().len(), 12);
    let mut history_ids = std::collections::BTreeSet::new();
    for page in [&history_page1, &history_page2] {
        for item in page["items"].as_array().unwrap() {
            assert!(history_ids.insert(item["id"].as_str().unwrap()));
        }
    }
    for path in [
        "/api/v1/me/saved-items?cursor=bad",
        "/api/v1/me/review-history?cursor=bad",
        "/api/v1/me/review-cards?cursor=bad",
    ] {
        assert_eq!(a.send("GET", path, None, true).await.0, 400);
    }
    // A UTC event at Kiritimati's Monday boundary is Honolulu's Sunday.
    let review_total = |dashboard: &Value| {
        dashboard["days"]
            .as_array()
            .unwrap()
            .iter()
            .map(|day| day["reviewAttempts"].as_u64().unwrap())
            .sum::<u64>()
    };
    db.execute_unprepared("UPDATE users SET settings=jsonb_set(settings,'{timeZone}','\"Pacific/Honolulu\"') WHERE email='one@example.test'").await.unwrap();
    let (_, west_before) = a.send("GET", "/api/v1/me/dashboard", None, true).await;
    db.execute_unprepared("UPDATE users SET settings=jsonb_set(settings,'{timeZone}','\"Pacific/Kiritimati\"') WHERE email='one@example.test'").await.unwrap();
    let (_, east_before) = a.send("GET", "/api/v1/me/dashboard", None, true).await;
    let monday: jiff::civil::Date = east_before["weekStart"].as_str().unwrap().parse().unwrap();
    let boundary_event = monday
        .at(0, 0, 0, 0)
        .in_tz("Pacific/Kiritimati")
        .unwrap()
        .timestamp()
        .to_string();
    let west_date = boundary_event
        .parse::<jiff::Timestamp>()
        .unwrap()
        .in_tz("Pacific/Honolulu")
        .unwrap()
        .date()
        .to_string();
    // On Sunday/Monday the two zones can currently be in different weeks.
    let west_increment = u64::from(
        west_before["days"]
            .as_array()
            .unwrap()
            .iter()
            .any(|day| day["localDate"] == west_date),
    );
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,
        "INSERT INTO review_attempts (id,card_id,user_id,rating,old_stage,new_stage,old_version,new_version,due_at,reviewed_at,time_zone) SELECT md5('dashboard-week-boundary'),id,user_id,'again',0,0,2000,2001,CURRENT_TIMESTAMP,$1::timestamptz,'Pacific/Kiritimati' FROM review_cards WHERE user_id=(SELECT id FROM users WHERE email='one@example.test') LIMIT 1",
        [boundary_event.into()])).await.unwrap();
    let (_, east_after) = a.send("GET", "/api/v1/me/dashboard", None, true).await;
    assert_eq!(review_total(&east_after), review_total(&east_before) + 1);
    db.execute_unprepared("UPDATE users SET settings=jsonb_set(settings,'{timeZone}','\"Pacific/Honolulu\"') WHERE email='one@example.test'").await.unwrap();
    let (_, west_after) = a.send("GET", "/api/v1/me/dashboard", None, true).await;
    assert_eq!(
        review_total(&west_after),
        review_total(&west_before) + west_increment
    );
    // An unfinished course must remain resumable after more than one overview page.
    db.execute_unprepared("UPDATE learning_sessions SET completed_at=CURRENT_TIMESTAMP WHERE lesson_id LIKE 'pagination-lesson-%' AND lesson_id <> 'pagination-lesson-0'; UPDATE learning_sessions SET updated_at=CURRENT_TIMESTAMP-interval '40 days' WHERE lesson_id='pagination-lesson-0'").await.unwrap();
    let (_, overview) = a.send("GET", "/api/v1/me/learning", None, true).await;
    assert!(
        !overview["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["lessonId"] == "pagination-lesson-0")
    );
    let (_, dashboard) = a.send("GET", "/api/v1/me/dashboard", None, true).await;
    assert_eq!(dashboard["resume"]["lessonId"], "pagination-lesson-0");
    brioche_migration::Migrator::down(&db, None).await.unwrap();
    drop(db);
    admin
        .execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .unwrap();
}
