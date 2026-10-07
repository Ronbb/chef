//! Shared accounts and product-bound cookies against an isolated real database.
use axum::{Router, body::Body, http::Request};
use chef_engine::{
    csrf::CsrfPolicy,
    identity::Backend,
    identity_service::{self, ServiceConfig},
    product::ProductId,
    session_store::PgSessionStore,
};
use http_body_util::BodyExt;
use sea_orm::{ConnectOptions, ConnectionTrait, Database, DbBackend, Statement};
use sea_orm_migration::MigratorTrait;
use tower::ServiceExt;
use tower_sessions::{
    SessionStore,
    session::{Id, Record},
};

const KEY: &str = "1111111111111111111111111111111111111111111111111111111111111111";
struct Browser {
    app: Router,
    origin: &'static str,
    cookie: String,
    csrf: String,
}
impl Browser {
    async fn request(
        &mut self,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
        internal: Option<(&str, &str)>,
    ) -> (u16, serde_json::Value) {
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header("cookie", &self.cookie)
            .header("origin", self.origin)
            .header("x-csrf-token", &self.csrf);
        if let Some((key, product)) = internal {
            builder = builder
                .header("authorization", format!("Bearer {key}"))
                .header("x-chef-product", product);
        }
        let body = match body {
            Some(value) => {
                builder = builder.header("content-type", "application/json");
                serde_json::to_vec(&value).unwrap()
            }
            None => vec![],
        };
        let response = self
            .app
            .clone()
            .oneshot(builder.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        let status = response.status().as_u16();
        assert_eq!(
            response.headers().get("cache-control").unwrap(),
            "private, no-store"
        );
        if let Some(cookie) = response.headers().get("set-cookie") {
            let cookie = cookie.to_str().unwrap();
            if !cookie.contains("Max-Age=0") {
                assert!(cookie.contains("HttpOnly"));
            }
            assert!(!cookie.contains("Domain="));
            self.cookie = cookie.split(';').next().unwrap().to_owned();
        }
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        if let Some(csrf) = value.get("csrfToken").and_then(|value| value.as_str()) {
            self.csrf = csrf.to_owned();
        }
        (status, value)
    }
}
#[tokio::test]
#[ignore = "set TEST_DATABASE_URL to a dedicated PostgreSQL database"]
async fn shared_identity_sessions_are_product_bound_and_revoked_globally() {
    let url = std::env::var("TEST_DATABASE_URL").expect("dedicated test database required");
    let admin = Database::connect(&url).await.unwrap();
    let schema = format!(
        "identity_service_{}",
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
    let backend = Backend::new(db.clone()).await.unwrap();
    let app = |product, origin: &str| {
        identity_service::router(
            backend.clone(),
            CsrfPolicy::new([origin.to_owned()]).unwrap(),
            false,
            ServiceConfig::new(product, KEY).unwrap(),
        )
    };
    let mut french = Browser {
        app: app(ProductId::Brioche, "http://brioche.example.test"),
        origin: "http://brioche.example.test",
        cookie: String::new(),
        csrf: String::new(),
    };
    let mut cantonese = Browser {
        app: app(ProductId::Hargow, "http://hargow.example.test"),
        origin: "http://hargow.example.test",
        cookie: String::new(),
        csrf: String::new(),
    };
    assert_eq!(
        french
            .request("GET", "/api/v1/auth/csrf", None, None)
            .await
            .0,
        200
    );
    assert_eq!(
        cantonese
            .request("GET", "/api/v1/auth/csrf", None, None)
            .await
            .0,
        200
    );
    let token = backend
        .issue_token("shared@example.test", false, true)
        .await
        .unwrap();
    let password = "correct horse baguette fromage";
    let (status, created) = french.request("POST", "/api/v1/auth/accept-invite", Some(serde_json::json!({"token":token,"email":"shared@example.test","displayName":"Shared","password":password})), None).await;
    assert_eq!(status, 200);
    assert!(created["user"].get("settings").is_none());
    assert!(created["user"].get("passwordHash").is_none());
    let account = created["user"]["id"].as_str().unwrap().to_owned();
    assert_eq!(
        cantonese
            .request(
                "POST",
                "/api/v1/auth/login",
                Some(serde_json::json!({"email":"shared@example.test","password":password})),
                None
            )
            .await
            .0,
        200
    );
    let (status, hargow) = cantonese
        .request("GET", "/internal/v1/session", None, Some((KEY, "hargow")))
        .await;
    assert_eq!(status, 200);
    assert_eq!(hargow["account"]["id"], account);
    for invalid in [
        serde_json::json!({"expectedAccountVersion":1,"displayName":"","settings":{"showTranslation":true}}),
        serde_json::json!({"expectedAccountVersion":1,"displayName":"Bad","userId":"999"}),
        serde_json::json!({"expectedAccountVersion":1,"displayName":"Bad","role":"operator"}),
    ] {
        assert_eq!(
            french
                .request("PATCH", "/api/v1/account", Some(invalid), None)
                .await
                .0,
            422
        );
    }
    for name in [" ".to_string(), "x".repeat(81), "Wrong\nName".into()] {
        assert_eq!(
            french
                .request(
                    "PATCH",
                    "/api/v1/account",
                    Some(serde_json::json!({"expectedAccountVersion":1,"displayName":name})),
                    None
                )
                .await
                .0,
            400
        );
    }
    let (_, renamed) = french
        .request(
            "PATCH",
            "/api/v1/account",
            Some(serde_json::json!({"expectedAccountVersion":1,"displayName":" Shared name "})),
            None,
        )
        .await;
    assert_eq!(renamed["displayName"], "Shared name");
    assert_eq!(renamed["version"], 2);
    assert!(renamed.get("settings").is_none());
    let (_, shared) = cantonese
        .request("GET", "/api/v1/account", None, None)
        .await;
    assert_eq!(shared["displayName"], "Shared name");
    assert_eq!(shared["version"], 2);
    let (left, right) = tokio::join!(
        french.request(
            "PATCH",
            "/api/v1/account",
            Some(serde_json::json!({"expectedAccountVersion":2,"displayName":"French edit"})),
            None
        ),
        cantonese.request(
            "PATCH",
            "/api/v1/account",
            Some(serde_json::json!({"expectedAccountVersion":2,"displayName":"Cantonese edit"})),
            None
        )
    );
    let mut statuses = [left.0, right.0];
    statuses.sort();
    assert_eq!(statuses, [200, 409]);
    assert_eq!(
        chef_engine::product_settings::read(&db, ProductId::Brioche, account.parse().unwrap())
            .await
            .unwrap()
            .version,
        1
    );
    assert_eq!(
        chef_engine::product_settings::read(&db, ProductId::Hargow, account.parse().unwrap())
            .await
            .unwrap()
            .version,
        1
    );
    assert_eq!(hargow["membership"]["role"], "learner");
    let (_, scoped) = french
        .request("GET", "/internal/v1/session", None, Some((KEY, "brioche")))
        .await;
    assert_eq!(scoped["membership"]["role"], "operator");
    assert_eq!(cantonese.request("PATCH",&format!("/api/v1/account-admin/members/{account}"),Some(serde_json::json!({"role":"operator","expectedVersion":0,"reason":"Attempt to inherit global role"})),None).await.0,403);
    // Identity does not initialize or depend on the learning preferences relation.
    let count = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(*)::bigint AS count FROM product_user_settings",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(count.try_get::<i64>("", "count").unwrap(), 0);
    db.execute_unprepared(
        "ALTER TABLE product_user_settings RENAME TO unavailable_learning_preferences",
    )
    .await
    .unwrap();
    assert_eq!(
        french.request("GET", "/api/v1/account", None, None).await.0,
        200
    );
    assert_eq!(
        cantonese
            .request("GET", "/internal/v1/session", None, Some((KEY, "hargow")))
            .await
            .0,
        200
    );
    db.execute_unprepared(
        "ALTER TABLE unavailable_learning_preferences RENAME TO product_user_settings",
    )
    .await
    .unwrap();
    assert_eq!(hargow["product"], "hargow");
    assert!(hargow["account"].get("settings").is_none());
    // Real learning handlers verify this real identity server over TCP for every request.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let service_url = format!("http://{}", listener.local_addr().unwrap());
    let identity_http = french.app.clone();
    let service_task =
        tokio::spawn(async move { axum::serve(listener, identity_http).await.unwrap() });
    let client =
        chef_engine::learning_identity::Client::new(&service_url, KEY, ProductId::Brioche, false)
            .unwrap();
    let remote = chef_engine::learning_identity::router(backend.clone(), client).unwrap();
    let request = |method: &str, path: &str, body: serde_json::Value, csrf: &str, cookie: &str| {
        Request::builder()
            .method(method)
            .uri(path)
            .header("origin", french.origin)
            .header("cookie", cookie)
            .header("x-csrf-token", csrf)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap()
    };
    let response = remote
        .clone()
        .oneshot(request(
            "GET",
            "/api/v1/me",
            serde_json::Value::Null,
            &french.csrf,
            &french.cookie,
        ))
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let profile: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(profile["id"], account);
    let settings_body = serde_json::json!({"version":1,"showTranslation":true});
    assert_eq!(
        remote
            .clone()
            .oneshot(request(
                "PATCH",
                "/api/v1/me/settings",
                settings_body.clone(),
                "bad",
                &french.cookie
            ))
            .await
            .unwrap()
            .status()
            .as_u16(),
        403
    );
    assert_eq!(
        remote
            .clone()
            .oneshot(request(
                "PATCH",
                "/api/v1/me/settings",
                settings_body,
                &french.csrf,
                &french.cookie
            ))
            .await
            .unwrap()
            .status()
            .as_u16(),
        200
    );
    assert!(
        chef_engine::product_settings::read(&db, ProductId::Brioche, account.parse().unwrap())
            .await
            .unwrap()
            .settings
            .show_translation
    );
    assert!(
        !chef_engine::product_settings::read(&db, ProductId::Hargow, account.parse().unwrap())
            .await
            .unwrap()
            .settings
            .show_translation
    );
    assert_eq!(
        remote
            .clone()
            .oneshot(request(
                "GET",
                "/api/v1/me/saved-items",
                serde_json::Value::Null,
                &french.csrf,
                &french.cookie
            ))
            .await
            .unwrap()
            .status()
            .as_u16(),
        200
    );
    assert_eq!(
        remote
            .clone()
            .oneshot(request(
                "GET",
                "/api/v1/me",
                serde_json::Value::Null,
                &french.csrf,
                &cantonese.cookie
            ))
            .await
            .unwrap()
            .status()
            .as_u16(),
        401
    );
    service_task.abort();
    let _ = service_task.await;
    assert_eq!(
        remote
            .oneshot(request(
                "GET",
                "/api/v1/me",
                serde_json::Value::Null,
                &french.csrf,
                &french.cookie
            ))
            .await
            .unwrap()
            .status()
            .as_u16(),
        503
    );
    assert_eq!(
        french
            .request("GET", "/internal/v1/session", None, None)
            .await
            .0,
        401
    );
    assert_eq!(
        french
            .request("GET", "/internal/v1/session", None, Some((KEY, "hargow")))
            .await
            .0,
        403
    );
    assert_eq!(
        french
            .request(
                "GET",
                "/internal/v1/session",
                None,
                Some((
                    "2222222222222222222222222222222222222222222222222222222222222222",
                    "brioche"
                ))
            )
            .await
            .0,
        401
    );
    let french_cookie = french.cookie.clone();
    let hargow_cookie = cantonese.cookie.clone();
    cantonese.cookie = french_cookie.replacen("brioche.sid=", "hargow.sid=", 1);
    assert_eq!(
        cantonese
            .request("GET", "/internal/v1/session", None, Some((KEY, "hargow")))
            .await
            .0,
        401
    );
    french.cookie = hargow_cookie.replacen("hargow.sid=", "brioche.sid=", 1);
    assert_eq!(
        french
            .request("GET", "/internal/v1/session", None, Some((KEY, "brioche")))
            .await
            .0,
        401
    );
    french.cookie = french_cookie;
    cantonese.cookie = hargow_cookie;
    assert_eq!(
        french
            .request("GET", "/internal/v1/session", None, Some((KEY, "brioche")))
            .await
            .0,
        200
    );
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "UPDATE users SET role='learner' WHERE id=$1",
        [account.parse::<i64>().unwrap().into()],
    ))
    .await
    .unwrap();
    assert_eq!(
        french
            .request("GET", "/internal/v1/session", None, Some((KEY, "brioche")))
            .await
            .1["account"]["role"],
        "learner"
    );
    assert_eq!(
        cantonese
            .request("POST", "/api/v1/auth/logout", None, None)
            .await
            .0,
        200
    );
    assert_eq!(
        french
            .request("GET", "/internal/v1/session", None, Some((KEY, "brioche")))
            .await
            .0,
        200
    );
    assert_eq!(
        cantonese
            .request("GET", "/internal/v1/session", None, Some((KEY, "hargow")))
            .await
            .0,
        401
    );
    assert_eq!(
        cantonese
            .request(
                "POST",
                "/api/v1/auth/login",
                Some(serde_json::json!({"email":"shared@example.test","password":password})),
                None
            )
            .await
            .0,
        200
    );
    let reset = backend
        .issue_token("shared@example.test", true, false)
        .await
        .unwrap();
    assert_eq!(
        french
            .request(
                "POST",
                "/api/v1/auth/reset-password",
                Some(serde_json::json!({"token":reset,"password":"another correct horse fromage"})),
                None
            )
            .await
            .0,
        200
    );
    assert_eq!(
        cantonese
            .request("GET", "/internal/v1/session", None, Some((KEY, "hargow")))
            .await
            .0,
        401
    );
    for path in [
        "/api/v1/me/settings",
        "/api/v1/learning/sessions",
        "/api/v1/admin/accounts",
    ] {
        assert_eq!(french.request("GET", path, None, None).await.0, 404);
    }
    // Legacy records remain Brioche-only; a foreign store cannot update/delete them.
    let legacy = PgSessionStore::new(db.clone());
    let brioche = PgSessionStore::for_product(db.clone(), ProductId::Brioche);
    let hargow = PgSessionStore::for_product(db.clone(), ProductId::Hargow);
    let mut record = Record {
        id: Id::default(),
        data: Default::default(),
        expiry_date: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
    };
    legacy.create(&mut record).await.unwrap();
    assert!(brioche.load(&record.id).await.unwrap().is_some());
    assert!(hargow.load(&record.id).await.unwrap().is_none());
    assert!(hargow.save(&record).await.is_err());
    hargow.delete(&record.id).await.unwrap();
    assert!(legacy.load(&record.id).await.unwrap().is_some());
    brioche.save(&record).await.unwrap();
    assert_eq!(
        legacy.load(&record.id).await.unwrap().unwrap().data["chef.product"],
        "brioche"
    );
    db.execute_unprepared(
        "UPDATE browser_sessions SET expires_at=CURRENT_TIMESTAMP - interval '1 second'",
    )
    .await
    .unwrap();
    assert!(brioche.load(&record.id).await.unwrap().is_none());
    // Restore this synthetic account's earlier global-role mutation before rollback.
    // Product grants are deliberately independent of the global account role.
    db.execute_unprepared("UPDATE users SET role='operator' WHERE email='shared@example.test'")
        .await
        .unwrap();
    brioche_migration::Migrator::down(&db, None).await.unwrap();
    admin
        .execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .unwrap();
}
