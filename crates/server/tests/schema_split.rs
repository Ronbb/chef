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
mod support;
const KEY: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const ORIGIN: &str = "http://brioche.example.test";
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
    let output = invoke(&["migrate-layout", &source]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(invoke(&["migrate-layout", &source]).status.success());
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,format!("SELECT to_regclass('{target}.chef_throttle_expiry') IS NOT NULL AND to_regclass('{source}.chef_attempt_owner_time') IS NOT NULL AND to_regclass('{source}.chef_throttle_expiry') IS NULL AND (SELECT count(*)=9 FROM chef_layout_migrations) AS correct"))).await.unwrap().unwrap();
    assert!(row.try_get::<bool>("", "correct").unwrap());
    assert_eq!(product_facts::snapshot(&owner).await, fact_snapshot);
    product_facts::verify(&owner).await;
    assert_eq!(product_content::snapshot(&owner).await, content_snapshot);
    product_content::verify(&owner, &lesson.id, lesson.revision as i32).await;
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
    let root = assets::fixture_assets(&owner, &source).await;
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
    let other_session = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO learning_sessions(id,product_id,user_id,lesson_id,revision,schema_version) VALUES($1,'hargow',$2,$3,$4,'1.0')",[other_session.into(),account.into(),lesson.id.clone().into(),(lesson.revision as i32).into()])).await.unwrap();
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO lesson_progress(product_id,user_id,lesson_id,last_session_id,first_completed_at,latest_completed_revision) VALUES('hargow',$1,$2,$3,'2026-01-01T00:00:00Z',$4)",[account.into(),lesson.id.clone().into(),other_session.into(),(lesson.revision as i32).into()])).await.unwrap();
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
    let untouched=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT s.version,s.completed_at IS NULL AS active,p.first_completed_at='2026-01-01T00:00:00Z'::timestamptz AS preserved,(SELECT count(*) FROM learning_sessions WHERE user_id=$1 AND lesson_id=$2 AND completed_at IS NULL)::bigint AS active_count FROM learning_sessions s JOIN lesson_progress p ON p.product_id=s.product_id AND p.last_session_id=s.id WHERE s.id=$3",[account.into(),lesson.id.clone().into(),other_session.into()])).await.unwrap().unwrap();
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
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO saved_items(product_id,id,user_id,knowledge_id,source_lesson_id,source_revision,snapshot,saved) VALUES('hargow',$1,$2,$3,$4,$5,$6,true)",[other_saved.into(),account.into(),knowledge.id.clone().into(),lesson.id.clone().into(),(lesson.revision as i32).into(),serde_json::to_value(knowledge).unwrap().into()])).await.unwrap();
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
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO review_cards(product_id,id,user_id,knowledge_id,source_lesson_id,source_revision,snapshot) VALUES('hargow',$1,$2,$3,$4,$5,$6)",[other_card.into(),account.into(),knowledge.id.clone().into(),lesson.id.clone().into(),(lesson.revision as i32).into(),serde_json::to_value(knowledge).unwrap().into()])).await.unwrap();
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
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"UPDATE lesson_progress SET first_completed_at=CURRENT_TIMESTAMP WHERE product_id='hargow' AND user_id=$1 AND lesson_id=$2",[account.into(),lesson.id.clone().into()])).await.unwrap();
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
            Some(audio_request),
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
    let mut revoked_character = character;
    revoked_character["characterId"] = "split-revoked-character".into();
    let mut revoked_voice = voice;
    revoked_voice["expectedVoiceRevision"] = 3.into();
    let mut revoked_audition = audition_request;
    revoked_audition["id"] = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into();
    let mut revoked_plan = plan_request;
    revoked_plan["id"] = "dddddddddddddddddddddddddddddddd".into();
    let mut revoked_clip = clip_request;
    revoked_clip["id"] = "99999999999999999999999999999999".into();
    revoked_clip["expectedPreviousId"] = "ffffffffffffffffffffffffffffffff".into();
    let mut revoked_alignment = alignment_request;
    revoked_alignment["id"] = "77777777777777777777777777777777".into();
    let mut revoked_package = package_import;
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
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 1);
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM character_revisions WHERE character_id='split-revoked-character') + (SELECT count(*) FROM character_voice_profiles WHERE character_id='split-character' AND revision=4) AS n")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 0);
    let row = owner
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(*)::bigint AS n FROM voice_reference_grants",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 2);
    let row = owner
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(*)::bigint AS n FROM voice_reference_revocations",
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
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(*)::bigint AS n FROM voice_clone_events",
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
        keys.len() as i64 + 1
    );
    assert_eq!(
        row.try_get::<i64>("", "events").unwrap(),
        2 * keys.len() as i64 + 1
    );
    assert_eq!(
        row.try_get::<i64>("", "reviews").unwrap(),
        keys.len() as i64
    );
    let row=owner.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT count(*)::bigint AS n FROM speech_alignments WHERE id='88888888888888888888888888888888' AND actor_id=$1 AND reason='Independent synthetic alignment'",[account.into()])).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 1);
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM speech_alignments) AS reports,(SELECT count(*) FROM speech_alignment_reviews) AS reviews")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "reports").unwrap(), 1);
    assert_eq!(
        row.try_get::<i64>("", "reviews").unwrap(),
        keys.len() as i64
    );
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM speech_package_imports) AS packages,(SELECT count(*) FROM lesson_audio_reviews) AS decisions,(SELECT count(*) FROM lesson_direct_publications) AS direct")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "packages").unwrap(), 1);
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
    let mut h_lesson = lesson.clone();
    h_lesson.id = "hargow-catalog-fixture".into();
    h_lesson.title.fr = "Hargow catalogue sentinel".into();
    h_lesson.title.zh = "Hargow 合成目录测试".into();
    h_lesson.validate().unwrap();
    let manifest = serde_json::json!({"id":"hargow-catalog-release","schemaVersion":"1.0","levels":[{"id":h_lesson.level_id,"label":"Synthetic Hargow fixture","units":[{"id":h_lesson.unit_id,"titleZh":"合成测试单元","lessons":[{"lessonId":h_lesson.id,"revision":h_lesson.revision}]}]}]});
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO lesson_revisions(product_id,lesson_id,revision,published,public_document,server_document) VALUES('hargow',$1,$2,true,$3,'{}')",[h_lesson.id.clone().into(),(h_lesson.revision as i32).into(),serde_json::to_value(&h_lesson).unwrap().into()])).await.unwrap();
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO content_releases(product_id,id,manifest,content_hash) VALUES('hargow','hargow-catalog-release',$1,repeat('a',64))",[manifest.into()])).await.unwrap();
    owner.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO release_entries(product_id,release_id,lesson_id,revision,position) VALUES('hargow','hargow-catalog-release',$1,$2,0)",[h_lesson.id.clone().into(),(h_lesson.revision as i32).into()])).await.unwrap();
    owner.execute_unprepared("UPDATE content_state SET active_release='hargow-catalog-release' WHERE product_id='hargow'").await.unwrap();
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
    task.abort();
    let _ = task.await;
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
