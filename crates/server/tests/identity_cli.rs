//! Audited identity CLI uses a non-owner split identity connection; tokens remain private.
use axum::{Router, body::Body, http::Request};
use http_body_util::BodyExt;
use sea_orm::{ConnectOptions, ConnectionTrait, Database, DbBackend, Statement};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    process::{Command, Output},
};
use tower::ServiceExt;
fn invoke(url: &str, schema: &str, root: &Path, product: &str, args: &[&str]) -> Output {
    let mut p = Command::new(env!("CARGO_BIN_EXE_chef-identity"));
    p.env_clear()
        .env("DATABASE_URL", url)
        .env("IDENTITY_DATABASE_SCHEMA", schema)
        .env("IDENTITY_PRODUCT", product)
        .env("PUBLIC_APP_URL", format!("http://{product}.example.test"))
        .current_dir(root)
        .args(args);
    if let Ok(system) = std::env::var("SystemRoot") {
        p.env("SystemRoot", system);
    }
    p.output().unwrap()
}
fn success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    output
}
fn rejection(output: Output, reason: &str) {
    assert!(!output.status.success());
    let e = String::from_utf8_lossy(&output.stderr);
    assert!(e.contains(reason), "{e}");
    assert!(!e.contains("SELECT ") && !e.contains("INSERT INTO"), "{e}");
}
fn private_token(root: &Path, name: &str, product: &str, email: &str) -> String {
    let link = std::fs::read_to_string(root.join(name)).unwrap();
    let url = url::Url::parse(link.trim()).unwrap();
    assert_eq!(
        url.host_str(),
        Some(format!("{product}.example.test").as_str())
    );
    let pairs: std::collections::BTreeMap<_, _> =
        url::form_urlencoded::parse(url.fragment().unwrap().as_bytes())
            .into_owned()
            .collect();
    assert_eq!(pairs["email"], email);
    assert_eq!(pairs["token"].len(), 64);
    pairs["token"].clone()
}
async fn post(app: &Router, origin: &str, path: &str, body: Value) -> u16 {
    let bootstrap = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/csrf")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bootstrap.status(), 200);
    let cookie = bootstrap.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let csrf: Value =
        serde_json::from_slice(&bootstrap.into_body().collect().await.unwrap().to_bytes()).unwrap();
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("origin", origin)
                .header("cookie", cookie)
                .header("x-csrf-token", csrf["csrfToken"].as_str().unwrap())
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
        .as_u16()
}
#[tokio::test]
#[ignore = "set TEST_DATABASE_URL to a dedicated PostgreSQL database"]
async fn identity_tokens_cli_is_product_scoped_audited_and_private() {
    let base = std::env::var("TEST_DATABASE_URL").expect("dedicated test database required");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let learning = format!("identity_cli_learning_{stamp}");
    let identity = format!("identity_cli_scope_{stamp}");
    let role = format!("identity_cli_login_{stamp}");
    let root = std::env::temp_dir().join(format!("chef-identity-cli-{stamp}"));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join(".env"), "").unwrap();
    let admin = Database::connect(&base).await.unwrap();
    admin
        .execute_unprepared(&format!("CREATE SCHEMA {learning}"))
        .await
        .unwrap();
    let mut options = ConnectOptions::new(&base);
    options
        .sqlx_logging(false)
        .set_schema_search_path(&learning);
    let learning_db = Database::connect(options).await.unwrap();
    brioche_migration::Migrator::up(&learning_db, None)
        .await
        .unwrap();
    rejection(
        invoke(
            &base,
            &learning,
            &root,
            "brioche",
            &[
                "invite",
                "new@example.test",
                "combined.txt",
                "b@example.test",
                "synthetic issuance",
            ],
        ),
        "requires a separate identity schema",
    );
    assert!(!root.join("combined.txt").exists());
    chef_engine::schema_split::relocate(&learning_db, &learning, &identity)
        .await
        .unwrap();
    brioche_migration::layout::up(&learning_db, &learning, &identity)
        .await
        .unwrap();
    admin
        .execute_unprepared(&format!(
            "CREATE ROLE {role} LOGIN NOINHERIT NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS"
        ))
        .await
        .unwrap();
    let grants = include_str!("../../../infra/database/identity-grants.sql")
        .lines()
        .filter(|l| !l.trim_start().starts_with('\\'))
        .collect::<Vec<_>>()
        .join("\n")
        .replace(":\"schema\"", &format!("\"{identity}\""))
        .replace(":\"learning_schema\"", &format!("\"{learning}\""))
        .replace(":\"role\"", &format!("\"{role}\""));
    admin.execute_unprepared(&grants).await.unwrap();
    let mut owner_options = ConnectOptions::new(&base);
    owner_options
        .sqlx_logging(false)
        .set_schema_search_path(&identity);
    let owner = Database::connect(owner_options).await.unwrap();
    owner.execute_unprepared("INSERT INTO users(id,email,password_hash,display_name,role) VALUES(1,'b@example.test','synthetic','B actor','learner'),(2,'global@example.test','synthetic','Global operator','operator'),(3,'h@example.test','synthetic','H actor','learner'); INSERT INTO product_memberships(product_id,user_id,role) VALUES('brioche',1,'operator'),('brioche',2,'operator'),('hargow',2,'learner'),('hargow',3,'operator'); SELECT setval('users_id_seq',3)").await.unwrap();
    let mut role_url = url::Url::parse(&base).unwrap();
    role_url.set_username(&role).unwrap();
    role_url.set_password(None).unwrap();
    let cli_url = role_url.to_string();
    let mut options = ConnectOptions::new(&cli_url);
    options
        .sqlx_logging(false)
        .set_schema_search_path(&identity);
    let role_db = Database::connect(options).await.unwrap();
    assert!(
        role_db
            .query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                format!("SELECT lesson_id FROM {learning}.lesson_revisions")
            ))
            .await
            .is_err()
    );
    let users_before = owner
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT md5(jsonb_agg(to_jsonb(u) ORDER BY id)::text) AS hash FROM users u",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "hash")
        .unwrap();
    rejection(
        invoke(
            &cli_url,
            &identity,
            &root,
            "hargow",
            &[
                "invite",
                "new@example.test",
                "denied.txt",
                "global@example.test",
                "synthetic issuance",
            ],
        ),
        "Operator membership required",
    );
    assert!(!root.join("denied.txt").exists());
    rejection(
        invoke(
            &cli_url,
            &identity,
            &root,
            "brioche",
            &[
                "reset-password",
                "b@example.test",
                "invalid.txt",
                "b@example.test",
                "synthetic issuance",
                "--operator",
            ],
        ),
        "usage:",
    );
    assert!(!root.join("invalid.txt").exists());
    let b = success(invoke(
        &cli_url,
        &identity,
        &root,
        "brioche",
        &[
            "invite",
            "new@example.test",
            "b.txt",
            "b@example.test",
            "synthetic B issuance",
        ],
    ));
    let b_token = private_token(&root, "b.txt", "brioche", "new@example.test");
    assert!(!String::from_utf8_lossy(&b.stderr).contains(&b_token));
    let h = success(invoke(
        &cli_url,
        &identity,
        &root,
        "hargow",
        &[
            "invite",
            "new@example.test",
            "h.txt",
            "h@example.test",
            "synthetic H issuance",
            "--operator",
        ],
    ));
    let h_token = private_token(&root, "h.txt", "hargow", "new@example.test");
    assert!(!String::from_utf8_lossy(&h.stderr).contains(&h_token));
    assert!(b_token != h_token);
    rejection(
        invoke(
            &cli_url,
            &identity,
            &root,
            "hargow",
            &[
                "invite",
                "new@example.test",
                "h.txt",
                "h@example.test",
                "duplicate output",
            ],
        ),
        "Cannot create exclusive",
    );
    assert!(private_token(&root, "h.txt", "hargow", "new@example.test") == h_token);
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT count(*)::bigint AS n,count(*) FILTER(WHERE consumed_at IS NOT NULL)::bigint AS consumed FROM identity_tokens WHERE email='new@example.test'")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 2);
    assert_eq!(row.try_get::<i64>("", "consumed").unwrap(), 0);
    let rows=owner.query_all_raw(Statement::from_string(DbBackend::Postgres,"SELECT product_id,actor_id,details FROM account_admin_audit WHERE target_email='new@example.test' ORDER BY product_id")).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0].try_get::<String>("", "product_id").unwrap(),
        "brioche"
    );
    assert_eq!(rows[0].try_get::<i64>("", "actor_id").unwrap(), 1);
    assert_eq!(rows[1].try_get::<i64>("", "actor_id").unwrap(), 3);
    assert_eq!(
        rows[1].try_get::<Value>("", "details").unwrap()["role"],
        "operator"
    );
    assert_eq!(
        owner
            .query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT md5(jsonb_agg(to_jsonb(u) ORDER BY id)::text) AS hash FROM users u"
            ))
            .await
            .unwrap()
            .unwrap()
            .try_get::<String>("", "hash")
            .unwrap(),
        users_before
    );
    let key = "1".repeat(64);
    let backend = chef_engine::identity::Backend::new(role_db.clone())
        .await
        .unwrap();
    let b_app = chef_engine::identity_service::router(
        backend.clone(),
        chef_engine::csrf::CsrfPolicy::new(["http://brioche.example.test".into()]).unwrap(),
        false,
        chef_engine::identity_service::ServiceConfig::new(
            chef_engine::product::ProductId::Brioche,
            &key,
        )
        .unwrap(),
    );
    let h_app = chef_engine::identity_service::router(
        backend,
        chef_engine::csrf::CsrfPolicy::new(["http://hargow.example.test".into()]).unwrap(),
        false,
        chef_engine::identity_service::ServiceConfig::new(
            chef_engine::product::ProductId::Hargow,
            &key,
        )
        .unwrap(),
    );
    let accept = json!({"email":"new@example.test","token":h_token,"displayName":"New learner","password":"Synthetic-password-123!"});
    assert_eq!(
        post(
            &b_app,
            "http://brioche.example.test",
            "/api/v1/auth/accept-invite",
            accept.clone()
        )
        .await,
        400
    );
    assert_eq!(
        post(
            &h_app,
            "http://hargow.example.test",
            "/api/v1/auth/accept-invite",
            accept
        )
        .await,
        200
    );
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT u.role,m.role AS product_role,(SELECT count(*) FROM product_memberships b WHERE b.user_id=u.id AND b.product_id='brioche')::bigint AS b_members FROM users u JOIN product_memberships m ON m.user_id=u.id AND m.product_id='hargow' WHERE u.email='new@example.test'")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<String>("", "role").unwrap(), "learner");
    assert_eq!(
        row.try_get::<String>("", "product_role").unwrap(),
        "operator"
    );
    assert_eq!(row.try_get::<i64>("", "b_members").unwrap(), 0);
    success(invoke(
        &cli_url,
        &identity,
        &root,
        "brioche",
        &[
            "reset-password",
            "b@example.test",
            "reset-b.txt",
            "b@example.test",
            "synthetic B reset",
        ],
    ));
    success(invoke(
        &cli_url,
        &identity,
        &root,
        "hargow",
        &[
            "reset-password",
            "b@example.test",
            "reset-h.txt",
            "h@example.test",
            "synthetic H reset",
        ],
    ));
    let reset_h = private_token(&root, "reset-h.txt", "hargow", "b@example.test");
    let reset = json!({"token":reset_h,"password":"Updated-synthetic-password-123!"});
    assert_eq!(
        post(
            &b_app,
            "http://brioche.example.test",
            "/api/v1/auth/reset-password",
            reset.clone()
        )
        .await,
        400
    );
    assert_eq!(
        post(
            &h_app,
            "http://hargow.example.test",
            "/api/v1/auth/reset-password",
            reset
        )
        .await,
        200
    );
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT count(*)::bigint AS n,count(*) FILTER(WHERE consumed_at IS NOT NULL)::bigint AS consumed FROM identity_tokens WHERE kind='reset' AND email='b@example.test'")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 2);
    assert_eq!(row.try_get::<i64>("", "consumed").unwrap(), 2);
    rejection(
        invoke(
            &cli_url,
            &identity,
            &root,
            "brioche",
            &[
                "invite",
                "b@example.test",
                "existing-user.txt",
                "b@example.test",
                "synthetic invalid invite",
            ],
        ),
        "Token issuance not confirmed",
    );
    assert!(!root.join("existing-user.txt").exists());
    let digest = format!("{:x}", Sha256::digest(b_token.as_bytes()));
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT product_id,consumed_at IS NULL AS unused FROM identity_tokens WHERE token_hash=$1",[digest.into()])).await.unwrap().unwrap();
    assert_eq!(row.try_get::<String>("", "product_id").unwrap(), "brioche");
    assert!(row.try_get::<bool>("", "unused").unwrap());
    drop(b_app);
    drop(h_app);
    role_db.close().await.unwrap();
    owner.close().await.unwrap();
    learning_db.close().await.unwrap();
    admin.execute_unprepared(&format!("DROP OWNED BY {role}; DROP ROLE {role}; DROP SCHEMA {learning} CASCADE; DROP SCHEMA {identity} CASCADE")).await.unwrap();
    let canonical = root.canonicalize().unwrap();
    assert!(canonical.starts_with(std::env::temp_dir().canonicalize().unwrap()));
    assert_eq!(
        canonical.file_name().unwrap().to_string_lossy(),
        format!("chef-identity-cli-{stamp}")
    );
    std::fs::remove_dir_all(canonical).unwrap();
}
