//! Real PostgreSQL and HTTP authentication lifecycle, never a production database.
use axum::{Router, body::Body, http::Request};
use chef_engine::{
    csrf::CsrfPolicy,
    identity::{self, Backend},
};
use http_body_util::BodyExt;
use sea_orm::{ConnectOptions, ConnectionTrait, Database};
use sea_orm_migration::MigratorTrait;
use tower::ServiceExt;

struct Browser {
    app: Router,
    cookie: String,
    csrf: String,
}
impl Browser {
    async fn new(app: Router) -> Self {
        let mut browser = Self {
            app,
            cookie: String::new(),
            csrf: String::new(),
        };
        let (status, _) = browser.send("GET", "/api/v1/auth/csrf", None, true).await;
        assert_eq!(status, 200);
        browser
    }
    async fn send(
        &mut self,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
        protect: bool,
    ) -> (u16, serde_json::Value) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("cookie", &self.cookie);
        if protect {
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
        if let Some(cookie) = response.headers().get("set-cookie") {
            self.cookie = cookie
                .to_str()
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .to_owned();
        }
        assert!(response.headers().get("cache-control").is_some());
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let data: serde_json::Value =
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        if let Some(token) = data.get("csrfToken").and_then(|v| v.as_str()) {
            self.csrf = token.to_owned();
        }
        (status, data)
    }
}
#[tokio::test]
#[ignore = "set TEST_DATABASE_URL to a dedicated PostgreSQL database"]
async fn invite_login_reset_logout_and_races() {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL is required");
    let admin = Database::connect(&url).await.unwrap();
    let schema = format!(
        "identity_test_{}",
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
    let app = identity::router(
        backend.clone(),
        CsrfPolicy::new(["http://localhost:5173".into()]).unwrap(),
        false,
    );
    let mut browser = Browser::new(app.clone()).await;
    assert_eq!(browser.send("GET", "/api/v1/me", None, true).await.0, 401);
    assert_eq!(
        browser
            .send(
                "PATCH",
                "/api/v1/me/settings",
                Some(serde_json::json!({"version":1,"showTranslation":true})),
                true
            )
            .await
            .0,
        401
    );
    let password = "correct horse baguette fromage";
    let token = backend
        .issue_token("LEARNER@example.test", false, false)
        .await
        .unwrap();
    let before = browser.cookie.clone();
    let csrf_before = browser.csrf.clone();
    let invite = serde_json::json!({"token":token,"email":"learner@example.test","password":password,"displayName":"Camille"});
    assert_eq!(
        browser
            .send(
                "POST",
                "/api/v1/auth/accept-invite",
                Some(invite.clone()),
                false
            )
            .await
            .0,
        403
    );
    let (status, accepted) = browser
        .send(
            "POST",
            "/api/v1/auth/accept-invite",
            Some(invite.clone()),
            true,
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(accepted["user"]["role"], "learner");
    assert!(accepted["user"].get("passwordHash").is_none());
    assert!(
        browser.cookie != before && browser.csrf != csrf_before,
        "authentication rotates both identifiers"
    );
    assert_eq!(
        browser
            .send("POST", "/api/v1/auth/accept-invite", Some(invite), true)
            .await
            .0,
        400
    );
    let (_, user) = browser.send("GET", "/api/v1/me", None, true).await;
    assert_eq!(user["displayName"], "Camille");
    assert_eq!(user["version"], 1);
    assert_eq!(user["settings"]["timeZone"], "Asia/Shanghai");
    assert_eq!(
        browser
            .send(
                "PATCH",
                "/api/v1/me/settings",
                Some(serde_json::json!({"version":1,"showTranslation":true})),
                false
            )
            .await
            .0,
        403
    );
    for invalid in [
        serde_json::json!({"version":1,"timeZone":"Mars/Olympus"}),
        serde_json::json!({"version":1,"speechRate":2}),
        serde_json::json!({"version":1,"weeklyDays":4}),
        serde_json::json!({"version":1,"dailyMinutes":99}),
        serde_json::json!({"version":1,"displayName":"\n"}),
        serde_json::json!({"version":1}),
    ] {
        assert_eq!(
            browser
                .send("PATCH", "/api/v1/me/settings", Some(invalid), true)
                .await
                .0,
            400
        );
    }
    assert_eq!(
        browser
            .send(
                "PATCH",
                "/api/v1/me/settings",
                Some(serde_json::json!({"version":1,"role":"operator","showTranslation":true})),
                true
            )
            .await
            .0,
        422
    );
    let (_, updated) = browser.send("PATCH", "/api/v1/me/settings", Some(serde_json::json!({"version":1,"displayName":" Camille Li ","timeZone":"Europe/Paris","weeklyDays":3,"dailyMinutes":15,"showTranslation":true,"speechRate":0.75})), true).await;
    assert_eq!(updated["displayName"], "Camille Li");
    assert_eq!(updated["version"], 2);
    assert_eq!(updated["settings"]["timeZone"], "Europe/Paris");
    assert_eq!(updated["role"], "learner");
    let mut stale = Browser {
        app: app.clone(),
        cookie: before,
        csrf: csrf_before,
    };
    assert_eq!(stale.send("GET", "/api/v1/me", None, true).await.0, 401);
    let mut second = Browser::new(app.clone()).await;
    let login = serde_json::json!({"email":"learner@example.test","password":password});
    assert_eq!(
        second
            .send("POST", "/api/v1/auth/login", Some(login.clone()), false)
            .await
            .0,
        403
    );
    assert_eq!(
        second
            .send("POST", "/api/v1/auth/login", Some(login.clone()), true)
            .await
            .0,
        200
    );
    let (_, from_second) = second.send("GET", "/api/v1/me", None, true).await;
    assert_eq!(
        from_second["settings"], updated["settings"],
        "settings survive a new login"
    );
    let (left, right) = tokio::join!(
        browser.send(
            "PATCH",
            "/api/v1/me/settings",
            Some(serde_json::json!({"version":2,"dailyMinutes":5})),
            true
        ),
        second.send(
            "PATCH",
            "/api/v1/me/settings",
            Some(serde_json::json!({"version":2,"weeklyDays":7})),
            true
        )
    );
    assert!(
        (left.0 == 200 && right.0 == 409) || (right.0 == 200 && left.0 == 409),
        "stale writes cannot overwrite another device"
    );
    assert_eq!(
        browser.send("GET", "/api/v1/me", None, true).await.1["version"],
        3
    );
    let reset = backend
        .issue_token("learner@example.test", true, false)
        .await
        .unwrap();
    let new_password = "another long private passphrase";
    let reset_request = serde_json::json!({"token":reset,"password":new_password});
    assert_eq!(
        browser
            .send(
                "POST",
                "/api/v1/auth/reset-password",
                Some(reset_request.clone()),
                true
            )
            .await
            .0,
        200
    );
    assert_eq!(browser.send("GET", "/api/v1/me", None, true).await.0, 401);
    assert_eq!(
        second.send("GET", "/api/v1/me", None, true).await.0,
        401,
        "all prior sessions revoked"
    );
    assert_eq!(
        browser
            .send(
                "POST",
                "/api/v1/auth/reset-password",
                Some(reset_request),
                true
            )
            .await
            .0,
        400
    );
    assert_eq!(
        browser
            .send("POST", "/api/v1/auth/login", Some(login), true)
            .await
            .0,
        401
    );
    assert_eq!(
        browser
            .send(
                "POST",
                "/api/v1/auth/login",
                Some(serde_json::json!({"email":"learner@example.test","password":new_password})),
                true
            )
            .await
            .0,
        200
    );
    let mut logged_cookie = Browser {
        app: app.clone(),
        cookie: browser.cookie.clone(),
        csrf: browser.csrf.clone(),
    };
    assert_eq!(
        browser
            .send("POST", "/api/v1/auth/logout", None, true)
            .await
            .0,
        200
    );
    assert_eq!(
        logged_cookie.send("GET", "/api/v1/me", None, true).await.0,
        401
    );
    assert_eq!(
        browser
            .send("POST", "/api/v1/auth/logout", None, true)
            .await
            .0,
        200
    );

    // Simultaneous token consumption creates exactly one account.
    let race_token = backend
        .issue_token("race@example.test", false, false)
        .await
        .unwrap();
    let request = brioche_course_contract::AcceptInviteRequest {
        token: race_token,
        email: "race@example.test".into(),
        password: password.into(),
        display_name: "Luc".into(),
    };
    let (left, right) = tokio::join!(
        backend.accept_invite(request.clone()),
        backend.accept_invite(request)
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    let mut other = Browser::new(app.clone()).await;
    assert_eq!(
        other
            .send(
                "POST",
                "/api/v1/auth/login",
                Some(serde_json::json!({"email":"race@example.test","password":password})),
                true
            )
            .await
            .0,
        200
    );
    let (_, other_profile) = other.send("GET", "/api/v1/me", None, true).await;
    assert_eq!(other_profile["settings"]["timeZone"], "Asia/Shanghai");
    assert_eq!(other_profile["settings"]["showTranslation"], false);
    assert_eq!(other.send("PATCH", "/api/v1/me/settings", Some(serde_json::json!({"version":1,"userId":accepted["user"]["id"],"showTranslation":true})), true).await.0, 422);
    // Reissuing a token invalidates the previous link; expiry is checked in the transaction.
    let old = backend
        .issue_token("renew@example.test", false, false)
        .await
        .unwrap();
    let latest = backend
        .issue_token("renew@example.test", false, false)
        .await
        .unwrap();
    let old_request = brioche_course_contract::AcceptInviteRequest {
        token: old,
        email: "renew@example.test".into(),
        password: password.into(),
        display_name: "Léa".into(),
    };
    assert!(backend.accept_invite(old_request).await.is_err());
    db.execute_unprepared("UPDATE identity_tokens SET expires_at=CURRENT_TIMESTAMP - interval '1 second' WHERE email='renew@example.test'").await.unwrap();
    assert!(
        backend
            .accept_invite(brioche_course_contract::AcceptInviteRequest {
                token: latest,
                email: "renew@example.test".into(),
                password: password.into(),
                display_name: "Léa".into()
            })
            .await
            .is_err()
    );
    // Per-account rate limiting persists independently of browser sessions.
    for _ in 0..10 {
        let mut visitor = Browser::new(app.clone()).await;
        assert_eq!(
            visitor
                .send(
                    "POST",
                    "/api/v1/auth/login",
                    Some(serde_json::json!({"email":"unknown@example.test","password":password})),
                    true
                )
                .await
                .0,
            401
        );
    }
    assert_eq!(
        browser
            .send(
                "POST",
                "/api/v1/auth/login",
                Some(serde_json::json!({"email":"unknown@example.test","password":password})),
                true
            )
            .await
            .0,
        429
    );
    brioche_migration::Migrator::down(&db, None).await.unwrap();
    drop(db);
    admin
        .execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .unwrap();
}
