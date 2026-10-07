//! Runs only against an explicitly supplied test database, in a disposable schema.
use chef_engine::{AppState, development_fixture, entity, router};
use sea_orm::{ActiveModelTrait, ConnectOptions, ConnectionTrait, Database, Set};
use sea_orm_migration::MigratorTrait;
use tower::ServiceExt;
mod support;

#[tokio::test]
#[ignore = "set TEST_DATABASE_URL to a dedicated PostgreSQL database"]
async fn migrations_publication_and_revision_uniqueness() {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL is required");
    let admin = Database::connect(&url).await.unwrap();
    let schema = format!(
        "brioche_test_{}",
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
    brioche_migration::Migrator::up(&db, None).await.unwrap();
    let lesson = development_fixture().unwrap();
    let row = entity::ActiveModel {
        lesson_id: Set(lesson.id.clone()),
        revision: Set(1),
        published: Set(false),
        public_document: Set(serde_json::to_value(&lesson).unwrap()),
        server_document: Set(serde_json::json!({"serverOnly":{"grading":"private"}})),
    };
    row.clone().insert(&db).await.unwrap();
    assert!(
        row.insert(&db).await.is_err(),
        "duplicate revision must not overwrite"
    );
    let request = || {
        axum::http::Request::builder()
            .uri("/api/lessons/a1-bakery-buy-breakfast")
            .body(axum::body::Body::empty())
            .unwrap()
    };
    let app = router(AppState {
        db: Some(db.clone()),
        fixture: None,
    });
    assert_eq!(
        app.clone().oneshot(request()).await.unwrap().status(),
        404,
        "draft must not be public"
    );
    let mut lesson = lesson;
    lesson.revision = 2;
    entity::ActiveModel {
        lesson_id: Set(lesson.id.clone()),
        revision: Set(2),
        published: Set(true),
        public_document: Set(serde_json::to_value(&lesson).unwrap()),
        server_document: Set(serde_json::json!({"serverOnly":{"grading":"private"}})),
    }
    .insert(&db)
    .await
    .unwrap();
    support::fixture_release(&db).await;
    let response = app.clone().oneshot(request()).await.unwrap();
    assert_eq!(response.status(), 200);
    use http_body_util::BodyExt;
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let public: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(public["revision"], 2);
    assert!(public.get("serverOnly").is_none());
    let catalog = chef_engine::content::catalog(&db).await.unwrap();
    let summaries: Vec<_> = catalog
        .levels
        .iter()
        .flat_map(|l| &l.units)
        .flat_map(|u| &u.lessons)
        .collect();
    assert_eq!(summaries.len(), 1);
    assert_eq!(
        serde_json::to_value(summaries[0]).unwrap(),
        serde_json::to_value(lesson.summary()).unwrap()
    );
    let explicit = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/lessons/a1-bakery-buy-breakfast?revision=2")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(explicit.status(), 200);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(
            &explicit.into_body().collect().await.unwrap().to_bytes()
        )
        .unwrap(),
        public
    );
    // Exercise the actual adapter against PostgreSQL, including stale in-flight saves.
    use chef_engine::session_store::PgSessionStore;
    use tower_sessions::{
        Session, SessionStore,
        session::{Id, Record},
    };
    let store = PgSessionStore::new(db.clone());
    let expiry = time::OffsetDateTime::from_unix_timestamp(
        time::OffsetDateTime::now_utc().unix_timestamp() + 600,
    )
    .unwrap();
    let mut record = Record {
        id: Id::default(),
        data: Default::default(),
        expiry_date: expiry,
    };
    record
        .data
        .insert("csrf".into(), serde_json::json!("session-bound-value"));
    store.create(&mut record).await.unwrap();
    assert_eq!(store.load(&record.id).await.unwrap(), Some(record.clone()));
    let mut collision = record.clone();
    collision
        .data
        .insert("csrf".into(), serde_json::json!("different-value"));
    store.create(&mut collision).await.unwrap();
    assert_ne!(collision.id, record.id);
    assert_eq!(
        store.load(&record.id).await.unwrap(),
        Some(record.clone()),
        "collision must not overwrite"
    );
    record.data.insert("user".into(), serde_json::json!(123));
    store.save(&record).await.unwrap();
    assert_eq!(store.load(&record.id).await.unwrap(), Some(record.clone()));
    let rows = db
        .query_all_raw(sea_orm::Statement::from_string(
            sea_orm::DbBackend::Postgres,
            "SELECT id_hash, data::text AS data FROM browser_sessions",
        ))
        .await
        .unwrap();
    for row in rows {
        let id_hash: String = row.try_get("", "id_hash").unwrap();
        let data: String = row.try_get("", "data").unwrap();
        assert_eq!(id_hash.len(), 64);
        assert!(
            !data.contains(&record.id.to_string()),
            "raw cookie token is not stored"
        );
    }
    store.delete(&record.id).await.unwrap();
    store.delete(&record.id).await.unwrap();
    assert!(store.load(&record.id).await.unwrap().is_none());
    assert!(
        store.save(&record).await.is_err(),
        "stale request cannot resurrect logout"
    );
    let mut expired = record.clone();
    expired.id = Id::default();
    expired.expiry_date = time::OffsetDateTime::now_utc() - time::Duration::seconds(1);
    store.create(&mut expired).await.unwrap();
    assert!(store.load(&expired.id).await.unwrap().is_none());
    assert!(store.save(&expired).await.is_err());
    assert_eq!(store.delete_expired().await.unwrap(), 1);
    assert_eq!(store.delete_expired().await.unwrap(), 0);
    let store = std::sync::Arc::new(store);
    let session = Session::new(
        None,
        store.clone(),
        Some(tower_sessions::Expiry::OnInactivity(
            time::Duration::minutes(30),
        )),
    );
    session.insert("csrf", "before-login").await.unwrap();
    session.save().await.unwrap();
    let old_id = session.id().unwrap();
    session.cycle_id().await.unwrap();
    session.save().await.unwrap();
    let new_id = session.id().unwrap();
    assert_ne!(old_id, new_id);
    assert!(store.load(&old_id).await.unwrap().is_none());
    assert_eq!(
        session.get::<String>("csrf").await.unwrap().as_deref(),
        Some("before-login")
    );
    session.flush().await.unwrap();
    assert!(store.load(&new_id).await.unwrap().is_none());
    // Verify real HTTP cookies plus session-bound Origin/CSRF enforcement.
    let policy = std::sync::Arc::new(
        chef_engine::csrf::CsrfPolicy::new(["https://brioche.example".into()]).unwrap(),
    );
    let session_layer = tower_sessions::SessionManagerLayer::new(PgSessionStore::new(db.clone()))
        .with_name("__Host-brioche.sid")
        .with_secure(true)
        .with_http_only(true)
        .with_same_site(tower_sessions::cookie::SameSite::Lax)
        .with_expiry(tower_sessions::Expiry::OnInactivity(
            time::Duration::minutes(30),
        ));
    let protected = axum::Router::new()
        .route("/csrf", axum::routing::get(chef_engine::csrf::bootstrap))
        .route(
            "/write",
            axum::routing::post(|| async { axum::http::StatusCode::NO_CONTENT }),
        )
        .route(
            "/rotate",
            axum::routing::post(|session: Session| async move {
                let csrf_token = chef_engine::csrf::rotate(&session).await?;
                Ok::<_, chef_engine::AppError>(axum::Json(brioche_course_contract::CsrfToken {
                    csrf_token,
                }))
            }),
        )
        .layer(axum::middleware::from_fn_with_state(
            policy,
            chef_engine::csrf::protect,
        ))
        .layer(session_layer);
    let bootstrap_request = || {
        axum::http::Request::builder()
            .uri("/csrf")
            .body(axum::body::Body::empty())
            .unwrap()
    };
    let first = protected
        .clone()
        .oneshot(bootstrap_request())
        .await
        .unwrap();
    assert_eq!(first.status(), 200);
    assert_eq!(first.headers()["cache-control"], "private, no-store");
    let set_cookie = first.headers()["set-cookie"].to_str().unwrap().to_owned();
    assert!(
        set_cookie.contains("HttpOnly")
            && set_cookie.contains("Secure")
            && set_cookie.contains("SameSite=Lax")
            && set_cookie.contains("Path=/")
    );
    assert!(!set_cookie.contains("Domain="));
    let cookie = set_cookie.split(';').next().unwrap().to_owned();
    let body = first.into_body().collect().await.unwrap().to_bytes();
    let csrf: brioche_course_contract::CsrfToken = serde_json::from_slice(&body).unwrap();
    let second = protected
        .clone()
        .oneshot(bootstrap_request())
        .await
        .unwrap();
    let other_cookie = second.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let submit = |path: &str, cookie: Option<&str>, origin: Option<&str>, token: Option<&str>| {
        let mut request = axum::http::Request::builder().method("POST").uri(path);
        if let Some(cookie) = cookie {
            request = request.header("cookie", cookie);
        }
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }
        if let Some(token) = token {
            request = request.header("x-csrf-token", token);
        }
        request.body(axum::body::Body::empty()).unwrap()
    };
    for (cookie_value, origin, token, expected) in [
        (
            Some(cookie.as_str()),
            Some("https://brioche.example"),
            Some(csrf.csrf_token.as_str()),
            204,
        ),
        (
            Some(cookie.as_str()),
            None,
            Some(csrf.csrf_token.as_str()),
            403,
        ),
        (
            Some(cookie.as_str()),
            Some("https://evil.example"),
            Some(csrf.csrf_token.as_str()),
            403,
        ),
        (
            Some(cookie.as_str()),
            Some("https://brioche.example"),
            None,
            403,
        ),
        (
            Some(cookie.as_str()),
            Some("https://brioche.example"),
            Some("forged"),
            403,
        ),
        (
            Some(other_cookie.as_str()),
            Some("https://brioche.example"),
            Some(csrf.csrf_token.as_str()),
            403,
        ),
        (
            None,
            Some("https://brioche.example"),
            Some(csrf.csrf_token.as_str()),
            403,
        ),
    ] {
        assert_eq!(
            protected
                .clone()
                .oneshot(submit("/write", cookie_value, origin, token))
                .await
                .unwrap()
                .status(),
            expected
        );
    }
    let rotated = protected
        .clone()
        .oneshot(submit(
            "/rotate",
            Some(&cookie),
            Some("https://brioche.example"),
            Some(&csrf.csrf_token),
        ))
        .await
        .unwrap();
    assert_eq!(rotated.status(), 200);
    let body = rotated.into_body().collect().await.unwrap().to_bytes();
    let new_csrf: brioche_course_contract::CsrfToken = serde_json::from_slice(&body).unwrap();
    assert_ne!(new_csrf.csrf_token, csrf.csrf_token);
    assert_eq!(
        protected
            .clone()
            .oneshot(submit(
                "/write",
                Some(&cookie),
                Some("https://brioche.example"),
                Some(&csrf.csrf_token)
            ))
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        protected
            .oneshot(submit(
                "/write",
                Some(&cookie),
                Some("https://brioche.example"),
                Some(&new_csrf.csrf_token)
            ))
            .await
            .unwrap()
            .status(),
        204
    );
    brioche_migration::Migrator::down(&db, None).await.unwrap();
    drop(db);
    admin
        .execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .unwrap();
}
