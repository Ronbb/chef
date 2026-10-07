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
use tower::ServiceExt;
#[path = "support/assets.rs"]
mod assets;
mod support;
const KEY: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const ORIGIN: &str = "http://brioche.example.test";
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
    let root = assets::fixture_assets(&owner, &source).await;
    let content_app =
        chef_engine::admin::independent_router(content.clone(), client.clone(), root.clone())
            .unwrap();
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
    assert_eq!(remote.oneshot(start).await.unwrap().status().as_u16(), 200);
    let invalid = learning.execute_unprepared("INSERT INTO product_user_settings(product_id,user_id,settings) VALUES('brioche',999999,'{}')")
        .await.unwrap_err();
    assert!(invalid.to_string().contains("foreign key constraint"));
    for path in ["/api/v1/operator/overview", "/api/v1/operator/history"] {
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
    // Revoke after the first HTTP verification while the write waits on the same
    // database advisory lock used by identity membership mutations.
    let held = owner.begin().await.unwrap();
    held.execute_unprepared("SELECT pg_advisory_xact_lock(hashtextextended('account-admin',0))")
        .await
        .unwrap();
    imported_source["id"] = "split-revoked-import".into();
    let copy = content_app.clone();
    let mut blocked_cookie = cookie.clone();
    let mut blocked_csrf = csrf.clone();
    let blocked = tokio::spawn(async move {
        request(&copy,"POST","/api/v1/operator/lessons/import",Some(serde_json::json!({"document":imported_source.to_string(),"reason":"Must not retain revoked authorization"})),&mut blocked_cookie,&mut blocked_csrf).await
    });
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,
            "SELECT EXISTS(SELECT 1 FROM pg_locks l JOIN pg_stat_activity a ON a.pid=l.pid WHERE l.locktype='advisory' AND NOT l.granted AND a.usename=$1) AS waiting",[content_role.clone().into()])).await.unwrap().unwrap();
        if row.try_get::<bool>("", "waiting").unwrap() {
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
    assert_eq!(blocked.await.unwrap().0, 403);
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT count(*)::bigint AS n FROM lesson_revisions WHERE lesson_id='split-revoked-import'")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 0);
    task.abort();
    let _ = task.await;
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
