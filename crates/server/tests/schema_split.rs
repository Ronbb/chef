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
    // Revoke after the first HTTP verification while the write waits on the same
    // database advisory lock used by identity membership mutations.
    imported_source["id"] = "split-revoked-import".into();
    let course_request = Request::builder().method("POST").uri("/api/v1/operator/lessons/import")
        .header("origin", ORIGIN).header("cookie", &cookie).header("x-csrf-token", &csrf)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::json!({"document":imported_source.to_string(),"reason":"Must not retain revoked authorization"}).to_string())).unwrap();
    // Test each write separately: concurrent parsing is intentionally capped at two.
    let mut revoked_character = character;
    revoked_character["characterId"] = "split-revoked-character".into();
    let mut revoked_voice = voice;
    revoked_voice["expectedVoiceRevision"] = 2.into();
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
    let row=owner.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM character_revisions WHERE character_id='split-revoked-character') + (SELECT count(*) FROM character_voice_profiles WHERE character_id='split-character' AND revision=3) AS n")).await.unwrap().unwrap();
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
