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
use sea_orm::{ConnectOptions, ConnectionTrait, Database, DbBackend, Statement, TransactionTrait};
use sea_orm_migration::MigratorTrait;
use tower::ServiceExt;
use tower_sessions::{
    SessionStore,
    session::{Id, Record},
};
mod support;

const KEY: &str = "1111111111111111111111111111111111111111111111111111111111111111";

#[tokio::test]
#[ignore = "set TEST_DATABASE_URL to a dedicated PostgreSQL database"]
async fn identity_admin_tokens_roles_and_sessions_are_product_scoped() {
    let url = std::env::var("TEST_DATABASE_URL").expect("dedicated test database required");
    let admin = Database::connect(&url).await.unwrap();
    let schema = format!(
        "identity_admin_{}",
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
    brioche_migration::Migrator::up(&db, Some(29))
        .await
        .unwrap();
    // Actual old rows upgrade without editing immutable audit or inventing Hargow ownership.
    db.execute_unprepared("INSERT INTO users(id,email,password_hash,display_name,role) VALUES(900,'old@example.test','synthetic','Old','learner'); INSERT INTO identity_tokens(token_hash,kind,email,expires_at) VALUES(repeat('0',64),'invite','legacy@example.test',CURRENT_TIMESTAMP+interval '1 day'); INSERT INTO account_admin_audit(action,actor_id,target_email,reason) VALUES('invite',900,'legacy@example.test','Old audit')").await.unwrap();
    brioche_migration::Migrator::up(&db, Some(1)).await.unwrap();
    for table in ["identity_tokens", "account_admin_audit"] {
        let row = db
            .query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                format!("SELECT product_id FROM {table} LIMIT 1"),
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.try_get::<String>("", "product_id").unwrap(), "brioche");
    }
    // Brioche-only rows can safely return to29, retaining both token and immutable audit.
    brioche_migration::Migrator::down(&db, Some(1))
        .await
        .unwrap();
    brioche_migration::Migrator::up(&db, Some(1)).await.unwrap();
    assert!(
        db.execute_unprepared("UPDATE identity_tokens SET product_id='unknown'")
            .await
            .is_err()
    );
    // Identity maintenance spans shared accounts but retains recent expired token history.
    db.execute_unprepared("INSERT INTO auth_throttle(key_hash,attempts,resets_at) VALUES('expired',1,CURRENT_TIMESTAMP-interval '1 hour'),('future',1,CURRENT_TIMESTAMP+interval '1 hour'); INSERT INTO identity_tokens(token_hash,kind,email,expires_at,product_id) VALUES(repeat('1',64),'invite','old-expired@example.test',CURRENT_TIMESTAMP-interval '8 days','hargow'),(repeat('2',64),'invite','recent-expired@example.test',CURRENT_TIMESTAMP-interval '1 day','hargow'); INSERT INTO browser_sessions(id_hash,data,expires_at) VALUES(repeat('0',64),'{}',CURRENT_TIMESTAMP-interval '1 hour'),(repeat('1',64),'{\"chef.product\":\"hargow\"}',CURRENT_TIMESTAMP+interval '1 hour')").await.unwrap();
    let cleanup = chef_engine::identity_cleanup::spawn(db.clone());
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let row = db.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM auth_throttle WHERE key_hash='expired')+(SELECT count(*) FROM identity_tokens WHERE token_hash=repeat('1',64))+(SELECT count(*) FROM browser_sessions WHERE id_hash=repeat('0',64)) AS n")).await.unwrap().unwrap();
            if row.try_get::<i64>("","n").unwrap()==0 { break; }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    cleanup.abort();
    let retained = db.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM auth_throttle WHERE key_hash='future')+(SELECT count(*) FROM identity_tokens WHERE token_hash=repeat('2',64))+(SELECT count(*) FROM browser_sessions WHERE id_hash=repeat('1',64)) AS n")).await.unwrap().unwrap();
    assert_eq!(retained.try_get::<i64>("", "n").unwrap(), 3);
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
    let password = "a long shared account password";
    let invite = backend
        .issue_token("operator@example.test", false, true)
        .await
        .unwrap();
    let (status,created) = french.request("POST","/api/v1/auth/accept-invite",Some(serde_json::json!({"token":invite,"email":"operator@example.test","displayName":"Operator","password":password})),None).await;
    assert_eq!(status, 200);
    let actor = created["user"]["id"].as_str().unwrap().to_owned();
    assert_eq!(
        cantonese
            .request(
                "POST",
                "/api/v1/auth/login",
                Some(serde_json::json!({"email":"operator@example.test","password":password})),
                None
            )
            .await
            .0,
        200
    );
    assert_eq!(
        cantonese
            .request("GET", "/api/v1/operator/accounts", None, None)
            .await
            .0,
        403
    );
    assert_eq!(
        french
            .request("GET", "/api/v1/operator/accounts", None, None)
            .await
            .0,
        200
    );
    // Explicit synthetic bootstrap, never implicitly inherited from the global account role.
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO product_memberships(product_id,user_id,role) VALUES('hargow',$1,'operator')",
        [actor.parse::<i64>().unwrap().into()],
    ))
    .await
    .unwrap();
    let request = |email: &str, kind: &str, operator| serde_json::json!({"email":email,"kind":kind,"operator":operator,"reason":"Synthetic scoped administration"});
    let (_, french_invite) = french
        .request(
            "POST",
            "/api/v1/operator/accounts/token",
            Some(request("new@example.test", "invite", true)),
            None,
        )
        .await;
    let (status, hargow_invite) = cantonese
        .request(
            "POST",
            "/api/v1/operator/accounts/token",
            Some(request("new@example.test", "invite", true)),
            None,
        )
        .await;
    assert_eq!(status, 200);
    assert!(french_invite["token"].is_string());
    let (_, fpending) = french
        .request(
            "GET",
            "/api/v1/operator/accounts/pending-tokens",
            None,
            None,
        )
        .await;
    let (_, hpending) = cantonese
        .request(
            "GET",
            "/api/v1/operator/accounts/pending-tokens",
            None,
            None,
        )
        .await;
    let ftoken = fpending["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["email"] == "new@example.test")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let htoken = hpending["items"][0]["id"].as_str().unwrap().to_owned();
    assert_ne!(ftoken, htoken);
    assert_eq!(hpending["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        french
            .request(
                "POST",
                &format!("/api/v1/operator/accounts/pending-tokens/{htoken}/revoke"),
                Some(serde_json::json!({"reason":"Wrong product"})),
                None
            )
            .await
            .0,
        404
    );
    assert_eq!(
        cantonese
            .request(
                "POST",
                &format!("/api/v1/operator/accounts/pending-tokens/{ftoken}/revoke"),
                Some(serde_json::json!({"reason":"Wrong product"})),
                None
            )
            .await
            .0,
        404
    );
    // A token cannot create an account through the other product, nor be consumed there.
    let accept = |token: &serde_json::Value| serde_json::json!({"token":token,"email":"new@example.test","displayName":"New","password":password});
    let mut new_hargow = Browser {
        app: cantonese.app.clone(),
        origin: cantonese.origin,
        cookie: String::new(),
        csrf: String::new(),
    };
    assert_eq!(
        new_hargow
            .request("GET", "/api/v1/auth/csrf", None, None)
            .await
            .0,
        200
    );
    assert_eq!(
        new_hargow
            .request(
                "POST",
                "/api/v1/auth/accept-invite",
                Some(accept(&french_invite["token"])),
                None
            )
            .await
            .0,
        400
    );
    assert_eq!(
        french
            .request(
                "POST",
                "/api/v1/auth/accept-invite",
                Some(accept(&hargow_invite["token"])),
                None
            )
            .await
            .0,
        400
    );
    let (status, new_account) = new_hargow
        .request(
            "POST",
            "/api/v1/auth/accept-invite",
            Some(accept(&hargow_invite["token"])),
            None,
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(new_account["user"]["role"], "learner");
    let target = new_account["user"]["id"].as_str().unwrap().to_owned();
    let (_, scoped) = new_hargow
        .request("GET", "/api/v1/account/membership", None, None)
        .await;
    assert_eq!(scoped["role"], "operator");
    assert_eq!(
        chef_engine::product_memberships::read(&db, ProductId::Brioche, target.parse().unwrap())
            .await
            .unwrap()
            .version,
        0
    );
    let (_, fsessions) = french
        .request(
            "GET",
            &format!("/api/v1/operator/accounts/{actor}/sessions"),
            None,
            None,
        )
        .await;
    let (_, hsessions) = cantonese
        .request(
            "GET",
            &format!("/api/v1/operator/accounts/{actor}/sessions"),
            None,
            None,
        )
        .await;
    assert_eq!(fsessions["items"].as_array().unwrap().len(), 1);
    assert_eq!(hsessions["items"].as_array().unwrap().len(), 1);
    let fsid = fsessions["items"][0]["id"].as_str().unwrap();
    let hsid = hsessions["items"][0]["id"].as_str().unwrap();
    assert_ne!(fsid, hsid);
    for (browser, foreign) in [(&mut french, hsid), (&mut cantonese, fsid)] {
        assert_eq!(
            browser
                .request(
                    "POST",
                    &format!("/api/v1/operator/accounts/{actor}/sessions/{foreign}/revoke"),
                    Some(serde_json::json!({"reason":"Wrong product session"})),
                    None
                )
                .await
                .0,
            404
        );
        assert_eq!(
            browser
                .request("GET", "/api/v1/account", None, None)
                .await
                .0,
            200
        );
    }
    // Role changes affect only the configured product, and keep the other product's last operator.
    let change = serde_json::json!({"expectedRole":"learner","role":"operator","reason":"Grant in French only"});
    assert_eq!(
        french
            .request(
                "POST",
                &format!("/api/v1/operator/accounts/{target}/role"),
                Some(change),
                None
            )
            .await
            .0,
        200
    );
    assert_eq!(
        chef_engine::product_memberships::read(&db, ProductId::Hargow, target.parse().unwrap())
            .await
            .unwrap()
            .version,
        1
    );
    let (_, accounts) = french
        .request("GET", "/api/v1/operator/accounts?q=new", None, None)
        .await;
    assert_eq!(accounts["items"][0]["role"], "operator");
    // Reset tokens are bound to the issuing entry; successful password replacement revokes all product sessions.
    let (_, freset) = french
        .request(
            "POST",
            "/api/v1/operator/accounts/token",
            Some(request("new@example.test", "reset", false)),
            None,
        )
        .await;
    let (_, hreset) = cantonese
        .request(
            "POST",
            "/api/v1/operator/accounts/token",
            Some(request("new@example.test", "reset", false)),
            None,
        )
        .await;
    let reset = |token: &serde_json::Value| serde_json::json!({"token":token,"password":"a replacement shared password"});
    assert_eq!(
        french
            .request(
                "POST",
                "/api/v1/auth/reset-password",
                Some(reset(&hreset["token"])),
                None
            )
            .await
            .0,
        400
    );
    assert_eq!(
        new_hargow
            .request(
                "POST",
                "/api/v1/auth/reset-password",
                Some(reset(&freset["token"])),
                None
            )
            .await
            .0,
        400
    );
    assert_eq!(
        new_hargow
            .request(
                "POST",
                "/api/v1/auth/reset-password",
                Some(reset(&hreset["token"])),
                None
            )
            .await
            .0,
        200
    );
    let (_, remaining) = french
        .request(
            "GET",
            "/api/v1/operator/accounts/pending-tokens?kind=reset",
            None,
            None,
        )
        .await;
    assert!(remaining["items"].as_array().unwrap().is_empty());
    assert!(
        brioche_migration::Migrator::down(&db, Some(1))
            .await
            .is_err()
    );
    let audits = db
        .query_all_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT product_id,action FROM account_admin_audit WHERE actor_id<>900 ORDER BY id",
        ))
        .await
        .unwrap();
    assert_eq!(audits.len(), 5); // two invites, one role, two resets; foreign operations wrote nothing.
    let (status, history) = cantonese
        .request("GET", "/api/v1/operator/accounts/history", None, None)
        .await;
    assert_eq!(status, 200);
    assert_eq!(history["items"].as_array().unwrap().len(), 2);
    assert_eq!(history["items"][0]["action"], "reset");
    assert_eq!(history["items"][0]["actor"], format!("user:{actor}"));
    let first = &history["items"][0];
    let page = format!(
        "/api/v1/operator/accounts/history?beforeTime={}&beforeKey={}",
        first["createdAt"].as_str().unwrap(),
        first["key"].as_str().unwrap()
    );
    let (status, older) = cantonese.request("GET", &page, None, None).await;
    assert_eq!(status, 200);
    assert_eq!(older["items"].as_array().unwrap().len(), 1);
    assert_eq!(older["items"][0]["action"], "inviteOperator");
    assert_eq!(
        cantonese
            .request(
                "GET",
                "/api/v1/operator/accounts/history?beforeKey=account:1",
                None,
                None
            )
            .await
            .0,
        400
    );
    assert_eq!(
        audits
            .iter()
            .filter(|row| row.try_get::<String>("", "product_id").unwrap() == "hargow")
            .count(),
        2
    );
    // An operator revoked in one product cannot use its old session to issue a token there.
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"UPDATE product_memberships SET role='learner',version=version+1 WHERE product_id='hargow' AND user_id=$1",[actor.parse::<i64>().unwrap().into()])).await.unwrap();
    assert_eq!(
        cantonese
            .request(
                "POST",
                "/api/v1/operator/accounts/token",
                Some(request("denied@example.test", "invite", false)),
                None
            )
            .await
            .0,
        403
    );
    assert_eq!(
        french
            .request("GET", "/api/v1/operator/accounts", None, None)
            .await
            .0,
        200
    );
    for path in [
        "/api/v1/operator/overview",
        "/api/v1/operator/releases/stage",
        "/api/v1/learning/sessions",
    ] {
        assert_eq!(french.request("GET", path, None, None).await.0, 404);
    }
    // Owned disposable schema only; scoped audit deliberately makes production rollback unsafe.
    admin
        .execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .unwrap();
}
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
            response
                .headers()
                .get("cache-control")
                .unwrap_or_else(|| panic!("Missing no-store at {method} {path}, status {status}")),
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
    let identity_role = format!("{schema}_identity");
    db.execute_unprepared(&format!(
        "CREATE ROLE {identity_role} LOGIN NOINHERIT NOSUPERUSER NOCREATEDB NOCREATEROLE"
    ))
    .await
    .unwrap();
    let grants = include_str!("../../../infra/database/identity-grants.sql")
        .lines()
        .filter(|line| !line.starts_with('\\'))
        .collect::<Vec<_>>()
        .join("\n")
        .replace(":\"schema\"", &format!("\"{schema}\""))
        .replace(":\"learning_schema\"", &format!("\"{schema}\""))
        .replace(":\"role\"", &format!("\"{identity_role}\""));
    db.execute_unprepared(&grants).await.unwrap();
    let mut identity_url = url::Url::parse(&std::env::var("TEST_DATABASE_URL").unwrap()).unwrap();
    identity_url.set_username(&identity_role).unwrap();
    identity_url.set_password(None).unwrap();
    let mut identity_options = ConnectOptions::new(identity_url.to_string());
    chef_engine::database_scope::apply(&mut identity_options, Some(&schema)).unwrap();
    identity_options.sqlx_logging(false);
    let identity_db = Database::connect(identity_options).await.unwrap();
    for table in [
        "product_user_settings",
        "learning_sessions",
        "review_cards",
        "saved_items",
        "lesson_revisions",
        "media_assets",
    ] {
        for sql in [
            format!("SELECT * FROM {table} LIMIT 0"),
            format!("DELETE FROM {table} WHERE false"),
        ] {
            assert!(
                identity_db.execute_unprepared(&sql).await.is_err(),
                "Identity role crossed learning boundary: {table}"
            );
        }
    }
    for table in ["account_admin_audit", "product_membership_audit"] {
        assert!(
            identity_db
                .execute_unprepared(&format!("UPDATE {table} SET reason='changed' WHERE false"))
                .await
                .is_err()
        );
        assert!(
            identity_db
                .execute_unprepared(&format!("DELETE FROM {table} WHERE false"))
                .await
                .is_err()
        );
    }
    assert!(
        identity_db
            .execute_unprepared("SELECT chef_lock_release_state()")
            .await
            .is_err()
    );
    let backend = Backend::new(identity_db.clone()).await.unwrap();
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
    for path in [
        "/api/v1/operator/accounts".to_owned(),
        "/api/v1/operator/accounts/history".to_owned(),
        "/api/v1/operator/accounts/pending-tokens".to_owned(),
        format!("/api/v1/operator/accounts/{account}/sessions"),
    ] {
        assert_eq!(french.request("GET", &path, None, None).await.0, 200);
        assert_eq!(cantonese.request("GET", &path, None, None).await.0, 403);
    }
    let (status, _) = french.request(
        "POST", "/api/v1/operator/accounts/token",
        Some(serde_json::json!({"email":"role-grant@example.test","kind":"invite","operator":false,"reason":"Verify runtime audit permission"})), None
    ).await;
    assert_eq!(status, 200);
    let (_, pending) = french
        .request(
            "GET",
            "/api/v1/operator/accounts/pending-tokens",
            None,
            None,
        )
        .await;
    let token_id = pending["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["email"] == "role-grant@example.test")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    assert_eq!(
        french
            .request(
                "POST",
                &format!("/api/v1/operator/accounts/pending-tokens/{token_id}/revoke"),
                Some(serde_json::json!({"reason":"Verify runtime revocation audit"})),
                None
            )
            .await
            .0,
        200
    );
    let audit = identity_db.query_one_raw(Statement::from_string(DbBackend::Postgres,
        "SELECT count(*)::bigint AS count FROM account_admin_audit WHERE target_email='role-grant@example.test'"))
        .await.unwrap().unwrap();
    assert_eq!(audit.try_get::<i64>("", "count").unwrap(), 2);
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
    // Real learning pool has no permission to query credentials, sessions, tokens or grants.
    // This combined synthetic fixture explicitly installs the new learning layout steps.
    // The schema_split suite exercises their real maintenance command and rollback boundary.
    db.execute_unprepared(include_str!(
        "../../migration/src/learning_product_facts.sql"
    ))
    .await
    .unwrap();
    db.execute_unprepared(include_str!(
        "../../migration/src/learning_product_operations.sql"
    ))
    .await
    .unwrap();
    db.execute_unprepared(include_str!(
        "../../migration/src/learning_product_sessions.sql"
    ))
    .await
    .unwrap();
    let learner_role = format!("{schema}_learner");
    db.execute_unprepared(include_str!(
        "../../migration/src/learning_product_content.sql"
    ))
    .await
    .unwrap();
    db.execute_unprepared(include_str!(
        "../../migration/src/learning_product_release_state.sql"
    ))
    .await
    .unwrap();
    db.execute_unprepared(include_str!(
        "../../migration/src/learning_product_reviews.sql"
    ))
    .await
    .unwrap();
    db.execute_unprepared(include_str!(
        "../../migration/src/learning_product_saved.sql"
    ))
    .await
    .unwrap();
    db.execute_unprepared(include_str!(
        "../../migration/src/learning_product_sources.sql"
    ))
    .await
    .unwrap();
    db.execute_unprepared(include_str!(
        "../../migration/src/learning_product_locks.sql"
    ))
    .await
    .unwrap();
    db.execute_unprepared(include_str!(
        "../../migration/src/learning_product_visuals.sql"
    ))
    .await
    .unwrap();
    db.execute_unprepared(include_str!(
        "../../migration/src/learning_product_recordings.sql"
    ))
    .await
    .unwrap();
    db.execute_unprepared(include_str!(
        "../../migration/src/learning_product_voices.sql"
    ))
    .await
    .unwrap();
    db.execute_unprepared(include_str!(
        "../../migration/src/learning_product_voice_work.sql"
    ))
    .await
    .unwrap();
    db.execute_unprepared(include_str!(
        "../../migration/src/learning_product_speech_work.sql"
    ))
    .await
    .unwrap();
    db.execute_unprepared(include_str!(
        "../../migration/src/learning_local_lesson_keys.sql"
    ))
    .await
    .unwrap();
    db.execute_unprepared(include_str!(
        "../../migration/src/learning_local_lesson_records.sql"
    ))
    .await
    .unwrap();
    db.execute_unprepared(include_str!(
        "../../migration/src/learning_local_release_keys.sql"
    ))
    .await
    .unwrap();
    db.execute_unprepared(include_str!(
        "../../migration/src/learning_local_visual_keys.sql"
    ))
    .await
    .unwrap();
    db.execute_unprepared(include_str!(
        "../../migration/src/learning_local_voice_keys.sql"
    ))
    .await
    .unwrap();
    db.execute_unprepared(include_str!(
        "../../migration/src/learning_local_recording_keys.sql"
    ))
    .await
    .unwrap();
    db.execute_unprepared(include_str!(
        "../../migration/src/learning_local_voice_work_keys.sql"
    ))
    .await
    .unwrap();
    db.execute_unprepared(&format!(
        "CREATE ROLE {learner_role} LOGIN NOINHERIT NOSUPERUSER NOCREATEDB NOCREATEROLE"
    ))
    .await
    .unwrap();
    // Exercise the deployment grant artifact itself, resolving only fixed synthetic identifiers.
    let grants = include_str!("../../../infra/database/learning-grants.sql")
        .lines()
        .filter(|line| !line.starts_with('\\'))
        .collect::<Vec<_>>()
        .join("\n")
        .replace(":\"schema\"", &format!("\"{schema}\""))
        .replace(":\"identity_schema\"", &format!("\"{schema}\""))
        .replace(":\"role\"", &format!("\"{learner_role}\""));
    db.execute_unprepared(&grants).await.unwrap();
    let product_grants = include_str!("../../../infra/database/learning-product-grants.sql")
        .lines()
        .filter(|line| !line.starts_with('\\'))
        .collect::<Vec<_>>()
        .join("\n")
        .replace(":\"schema\"", &format!("\"{schema}\""))
        .replace(":\"role\"", &format!("\"{learner_role}\""));
    db.execute_unprepared(&product_grants).await.unwrap();
    let mut learner_url = url::Url::parse(&std::env::var("TEST_DATABASE_URL").unwrap()).unwrap();
    learner_url.set_username(&learner_role).unwrap();
    learner_url.set_password(None).unwrap();
    let mut learner_options = ConnectOptions::new(learner_url.to_string());
    learner_options
        .set_schema_search_path(&schema)
        .sqlx_logging(false);
    let learner_db = Database::connect(learner_options).await.unwrap();
    for table in [
        "users",
        "browser_sessions",
        "identity_tokens",
        "auth_throttle",
        "product_memberships",
        "account_admin_audit",
    ] {
        assert!(
            learner_db
                .execute_unprepared(&format!("SELECT * FROM {table} LIMIT 0"))
                .await
                .is_err()
        );
        assert!(
            learner_db
                .execute_unprepared(&format!("DELETE FROM {table} WHERE false"))
                .await
                .is_err()
        );
    }
    assert!(
        learner_db
            .execute_unprepared("UPDATE lesson_revisions SET published=false WHERE false")
            .await
            .is_err()
    );
    assert!(
        learner_db
            .execute_unprepared("UPDATE content_state SET active_release=NULL WHERE false")
            .await
            .is_err()
    );
    let source = chef_engine::development_source().unwrap();
    let lesson = chef_engine::project_source(source.clone()).unwrap();
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO lesson_revisions(lesson_id,revision,published,public_document,server_document) VALUES($1,$2,true,$3,$4)",[lesson.id.clone().into(),(lesson.revision as i32).into(),serde_json::to_value(&lesson).unwrap().into(),source.into()])).await.unwrap();
    support::fixture_release(&db).await;
    // Narrow SECURITY DEFINER functions retain actual row locks without granting content UPDATE.
    let held = learner_db.begin().await.unwrap();
    held.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT chef_lock_lesson($1,$2)",
        [lesson.id.clone().into(), (lesson.revision as i32).into()],
    ))
    .await
    .unwrap();
    held.query_one_raw(Statement::from_string(
        DbBackend::Postgres,
        "SELECT chef_lock_release_state()",
    ))
    .await
    .unwrap();
    for sql in [
        "UPDATE lesson_revisions SET published=false WHERE lesson_id=$1 AND revision=$2",
        "UPDATE content_state SET active_release=NULL WHERE singleton AND $1::text<>'' AND $2::integer>0",
    ] {
        let writer = db.begin().await.unwrap();
        writer
            .execute_unprepared("SET LOCAL lock_timeout='100ms'")
            .await
            .unwrap();
        let error = writer
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                sql,
                [lesson.id.clone().into(), (lesson.revision as i32).into()],
            ))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("lock timeout"), "{error}");
        writer.rollback().await.unwrap();
    }
    held.commit().await.unwrap();
    let public=db.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT count(*)::bigint AS n FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace CROSS JOIN LATERAL aclexplode(p.proacl) a WHERE n.nspname=current_schema() AND p.proname IN ('chef_lock_lesson','chef_lock_release_state') AND a.grantee=0 AND a.privilege_type='EXECUTE'")).await.unwrap().unwrap();
    assert_eq!(public.try_get::<i64>("", "n").unwrap(), 0);
    let remote = chef_engine::learning_identity::router(learner_db.clone(), client)
        .unwrap()
        .merge(chef_engine::independent_learning_router(
            chef_engine::AppState {
                db: Some(learner_db.clone()),
                fixture: None,
            },
        ));
    assert_eq!(
        remote
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/ready")
                    .body(Body::empty())
                    .unwrap()
            )
            .await
            .unwrap()
            .status(),
        200
    );
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
    // Full learner writes work with read-only content and no account-table privileges.
    let mut learning_browser = Browser {
        app: remote.clone(),
        origin: french.origin,
        cookie: french.cookie.clone(),
        csrf: french.csrf.clone(),
    };
    let (status,started)=learning_browser.request("POST","/api/v1/learning-sessions",Some(serde_json::json!({"lessonId":lesson.id,"schemaVersion":"1.0","idempotencyKey":"acl-start-lesson-01"})),None).await;
    assert_eq!(status, 200);
    let session = started["progress"]["id"].as_str().unwrap();
    assert_eq!(learning_browser.request("PUT",&format!("/api/v1/learning-sessions/{session}/steps/{}",lesson.steps[0].id),Some(serde_json::json!({"version":started["progress"]["version"],"idempotencyKey":"acl-step-learning-01"})),None).await.0,200);
    let knowledge = &lesson.knowledge.vocabulary[0].id;
    assert_eq!(learning_browser.request("PUT",&format!("/api/v1/me/saved-items/{knowledge}"),Some(serde_json::json!({"sourceLessonId":lesson.id,"sourceRevision":lesson.revision,"saved":true,"version":0,"idempotencyKey":"acl-save-vocabulary-01"})),None).await.0,200);
    let (status,card)=learning_browser.request("POST","/api/v1/me/review-enrollments",Some(serde_json::json!({"sourceLessonId":lesson.id,"sourceRevision":lesson.revision,"knowledgeId":knowledge,"idempotencyKey":"acl-enroll-review-01"})),None).await;
    assert_eq!(status, 200);
    assert_eq!(learning_browser.request("POST",&format!("/api/v1/me/reviews/{}/attempts",card["id"].as_str().unwrap()),Some(serde_json::json!({"cardVersion":card["version"],"rating":"remembered","idempotencyKey":"acl-submit-review-01"})),None).await.0,200);
    for path in [
        "/api/v1/me/reviews",
        "/api/v1/me/review-history",
        "/api/v1/me/dashboard",
    ] {
        assert_eq!(
            learning_browser.request("GET", path, None, None).await.0,
            200
        );
    }
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
    // This sessionless probe uses product membership, never the global role.
    let operator_path = format!("/internal/v1/operators/{account}");
    assert_eq!(
        french
            .request("GET", &operator_path, None, Some((KEY, "brioche")))
            .await
            .0,
        204
    );
    assert_eq!(
        cantonese
            .request("GET", &operator_path, None, Some((KEY, "hargow")))
            .await
            .0,
        404
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
    learner_db.close().await.unwrap();
    identity_db.close().await.unwrap();
    // Preserve the legacy audit downgrade guard independently of newer layout
    // dependencies, rather than silently losing its coverage to an earlier error.
    let audit_tx = db.begin().await.unwrap();
    let migration = brioche_migration::Migrator::migrations()
        .into_iter()
        .find(|migration| migration.name() == "m20261007_000015_token_admin")
        .unwrap();
    let audit_downgrade = migration
        .down(&sea_orm_migration::SchemaManager::new(&audit_tx))
        .await
        .unwrap_err();
    assert!(
        audit_downgrade
            .to_string()
            .contains("account_admin_audit_action_check"),
        "{audit_downgrade}"
    );
    audit_tx.rollback().await.unwrap();
    // Product-local character keys replace the old audition FK. Unsupported legacy
    // rollback now stops at that earlier boundary; the audit guard remains separately tested.
    let downgrade = brioche_migration::Migrator::down(&db, None)
        .await
        .unwrap_err();
    assert!(
        downgrade.to_string().contains(
            "constraint \"audition_character\" of relation \"voice_auditions\" does not exist"
        ),
        "{downgrade}"
    );
    let audit = db.query_one_raw(Statement::from_string(DbBackend::Postgres,
        "SELECT count(*)::bigint AS count FROM account_admin_audit WHERE target_email='role-grant@example.test'"))
        .await.unwrap().unwrap();
    assert_eq!(audit.try_get::<i64>("", "count").unwrap(), 2);
    admin
        .execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .unwrap();
    admin
        .execute_unprepared(&format!("DROP ROLE {learner_role}"))
        .await
        .unwrap();
    admin
        .execute_unprepared(&format!("DROP ROLE {identity_role}"))
        .await
        .unwrap();
}
