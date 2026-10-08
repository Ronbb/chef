//! Product-aware author CLI against disposable schemas; synthetic H records are isolation sentinels.
use sea_orm::{ConnectOptions, ConnectionTrait, Database, DbBackend, Statement};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use std::{
    path::Path,
    process::{Command, Output},
};
#[path = "support/assets.rs"]
mod assets;
fn invoke(url: &str, schema: &str, root: &Path, args: &[&str]) -> Output {
    let mut p = Command::new(env!("CARGO_BIN_EXE_chef-server"));
    p.env_clear()
        .env("DATABASE_URL", url)
        .env("DATABASE_SCHEMA", schema)
        .env("CHEF_PRODUCT", "brioche")
        .env("APP_ENV", "development")
        .env("CONTENT_MODE", "database")
        .env("MEDIA_ROOT", root)
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
    output
}
fn rejected(output: Output, reason: &str) {
    let e = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "expected {reason}");
    assert!(e.contains(reason), "{e}");
    assert!(!e.contains("SELECT ") && !e.contains("INSERT INTO"), "{e}");
}
#[tokio::test]
#[ignore = "set TEST_DATABASE_URL to a dedicated PostgreSQL database"]
async fn split_author_cli_uses_own_product_and_requires_complete_history() {
    let base = std::env::var("TEST_DATABASE_URL").expect("dedicated test database required");
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let learning = format!("author_scope_{id}");
    let identity = format!("author_identity_{id}");
    let admin = Database::connect(&base).await.unwrap();
    admin
        .execute_unprepared(&format!("CREATE SCHEMA {learning}"))
        .await
        .unwrap();
    let mut options = ConnectOptions::new(&base);
    options
        .max_connections(1)
        .sqlx_logging(false)
        .set_schema_search_path(&learning);
    let db = Database::connect(options).await.unwrap();
    brioche_migration::Migrator::up(&db, None).await.unwrap();
    let root = assets::fixture_assets(&db, &learning).await;
    std::fs::write(root.join(".env"), "").unwrap();
    let mut source: Value =
        serde_json::from_str(include_str!("../../../docs/examples/a1-bakery.lesson.json")).unwrap();
    source["id"] = "author-split-own-lesson".into();
    source["assetRefs"] = assets::fixture_refs();
    let lesson_id = source["id"].as_str().unwrap();
    let file = root.join("lesson.json");
    std::fs::write(&file, serde_json::to_vec(&source).unwrap()).unwrap();
    let file = file.to_str().unwrap();
    chef_engine::schema_split::relocate(&db, &learning, &identity)
        .await
        .unwrap();
    rejected(
        invoke(&base, &learning, &root, &["import", file]),
        "Author command layout not ready",
    );
    rejected(
        invoke(&base, &learning, &root, &["release-status"]),
        "Author command layout not ready",
    );
    brioche_migration::layout::up(&db, &learning, &identity)
        .await
        .unwrap();
    brioche_migration::layout::verify_complete(&db, &learning, &identity)
        .await
        .unwrap();
    let author_role = format!("author_login_{id}");
    db.execute_unprepared(&format!(
        "CREATE ROLE {author_role} LOGIN NOINHERIT NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS"
    ))
    .await
    .unwrap();
    for template in [
        include_str!("../../../infra/database/content-grants.sql"),
        include_str!("../../../infra/database/author-grants.sql"),
    ] {
        let grants = template
            .lines()
            .filter(|line| !line.trim_start().starts_with('\\'))
            .collect::<Vec<_>>()
            .join("\n")
            .replace(":\"schema\"", &format!("\"{learning}\""))
            .replace(":\"identity_schema\"", &format!("\"{identity}\""))
            .replace(":\"role\"", &format!("\"{author_role}\""));
        db.execute_unprepared(&grants).await.unwrap();
    }
    let mut role_url = url::Url::parse(&base).unwrap();
    role_url.set_username(&author_role).unwrap();
    role_url.set_password(None).unwrap();
    let cli_base = role_url.to_string();
    let mut role_options = ConnectOptions::new(&cli_base);
    role_options
        .sqlx_logging(false)
        .set_schema_search_path(&learning);
    let role_db = Database::connect(role_options).await.unwrap();
    assert!(
        role_db
            .query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                format!("SELECT id FROM {identity}.users")
            ))
            .await
            .is_err()
    );
    assert!(
        role_db
            .execute_unprepared("UPDATE chef_layout_migrations SET definition='not allowed'")
            .await
            .is_err()
    );
    let manifest = json!({"schemaVersion":"1.0","id":"author-shared-release","levels":[{"id":"a1","label":"A1","units":[{"id":source["unitId"],"titleZh":"隔离协议测试","lessons":[{"lessonId":lesson_id,"revision":1}]}]}]});
    let release_file = root.join("release.json");
    std::fs::write(&release_file, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let release_file = release_file.to_str().unwrap();
    // Deliberately minimal foreign sentinels are never projected as real courses.
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO lesson_revisions(product_id,lesson_id,revision,published,public_document,server_document) VALUES('hargow',$1,1,true,$2,$2)",[lesson_id.into(),json!({"sentinel":"foreign","id":lesson_id,"revision":1}).into()])).await.unwrap();
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO content_releases(product_id,id,manifest,content_hash) VALUES('hargow','author-shared-release',$1,$2)",[manifest.clone().into(),"0".repeat(64).into()])).await.unwrap();
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO release_entries(product_id,release_id,lesson_id,revision,position) VALUES('hargow','author-shared-release',$1,1,0)",[lesson_id.into()])).await.unwrap();
    db.execute_unprepared("UPDATE content_state SET active_release='author-shared-release',generation=17 WHERE product_id='hargow'").await.unwrap();
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO lesson_revisions(product_id,lesson_id,revision,published,public_document,server_document) VALUES('hargow','author-foreign-only',1,true,$1,$1)",[json!({"sentinel":"foreign-only","id":"author-foreign-only","revision":1}).into()])).await.unwrap();
    let mut foreign_manifest = manifest.clone();
    foreign_manifest["id"] = "author-foreign-release".into();
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO content_releases(product_id,id,manifest,content_hash) VALUES('hargow','author-foreign-release',$1,$2)",[foreign_manifest.into(),"0".repeat(64).into()])).await.unwrap();
    let hash_sql = "SELECT md5(jsonb_build_array((SELECT jsonb_agg(to_jsonb(l) ORDER BY l.lesson_id,l.revision) FROM lesson_revisions l WHERE product_id='hargow'),(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM content_releases r WHERE product_id='hargow'),(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.position) FROM release_entries e WHERE product_id='hargow'),(SELECT to_jsonb(s) FROM content_state s WHERE product_id='hargow'))::text) AS hash";
    let before = db
        .query_one_raw(Statement::from_string(DbBackend::Postgres, hash_sql))
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "hash")
        .unwrap();
    let initial: Value = serde_json::from_slice(
        &success(invoke(&cli_base, &learning, &root, &["release-status"])).stdout,
    )
    .unwrap();
    assert_eq!(initial["activeRelease"], Value::Null);
    assert_eq!(initial["generation"], 0);
    assert!(
        !invoke(
            &cli_base,
            &learning,
            &root,
            &[
                "release-activate",
                "author-foreign-release",
                "0",
                "protocol-cli",
                "foreign release rejected"
            ]
        )
        .status
        .success()
    );
    assert!(
        !invoke(
            &cli_base,
            &learning,
            &root,
            &[
                "content-withdraw",
                "author-foreign-only",
                "1",
                "0",
                "protocol-cli",
                "foreign lesson rejected"
            ]
        )
        .status
        .success()
    );
    success(invoke(&cli_base, &learning, &root, &["import", file]));
    // Existing CLI duplicate semantics remain rejection rather than implicit retry.
    assert!(
        !invoke(&cli_base, &learning, &root, &["import", file])
            .status
            .success()
    );
    success(invoke(
        &cli_base,
        &learning,
        &root,
        &[
            "release-stage",
            release_file,
            "protocol-cli",
            "synthetic test staging",
        ],
    ));
    success(invoke(
        &cli_base,
        &learning,
        &root,
        &[
            "release-activate",
            "author-shared-release",
            "0",
            "protocol-cli",
            "synthetic test activation",
        ],
    ));
    let active: Value = serde_json::from_slice(
        &success(invoke(&cli_base, &learning, &root, &["release-status"])).stdout,
    )
    .unwrap();
    assert_eq!(active["activeRelease"], "author-shared-release");
    assert_eq!(active["generation"], 1);
    assert!(
        !invoke(
            &cli_base,
            &learning,
            &root,
            &[
                "release-activate",
                "author-shared-release",
                "0",
                "protocol-cli",
                "stale generation"
            ]
        )
        .status
        .success()
    );
    success(invoke(
        &cli_base,
        &learning,
        &root,
        &[
            "content-withdraw",
            lesson_id,
            "1",
            "1",
            "protocol-cli",
            "synthetic test withdrawal",
        ],
    ));
    assert!(
        !invoke(
            &cli_base,
            &learning,
            &root,
            &[
                "content-withdraw",
                lesson_id,
                "1",
                "2",
                "protocol-cli",
                "duplicate withdrawal"
            ]
        )
        .status
        .success()
    );
    let row=db.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT (SELECT count(*) FROM lesson_import_audit WHERE product_id='brioche' AND lesson_id=$1)::bigint AS imports,(SELECT count(*) FROM content_withdrawals WHERE product_id='brioche' AND lesson_id=$1)::bigint AS withdrawals,(SELECT published FROM lesson_revisions WHERE product_id='brioche' AND lesson_id=$1 AND revision=1) AS published,(SELECT generation FROM content_state WHERE product_id='brioche') AS generation",[lesson_id.into()])).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "imports").unwrap(), 1);
    assert_eq!(row.try_get::<i64>("", "withdrawals").unwrap(), 1);
    assert!(!row.try_get::<bool>("", "published").unwrap());
    assert_eq!(row.try_get::<i64>("", "generation").unwrap(), 2);
    assert_eq!(
        db.query_one_raw(Statement::from_string(DbBackend::Postgres, hash_sql))
            .await
            .unwrap()
            .unwrap()
            .try_get::<String>("", "hash")
            .unwrap(),
        before
    );
    let mut next_source = source.clone();
    next_source["revision"] = 2.into();
    let next_file = root.join("next-lesson.json");
    std::fs::write(&next_file, serde_json::to_vec(&next_source).unwrap()).unwrap();
    let next_file = next_file.to_str().unwrap();
    // Missing/unknown/drifted ledger entries cannot silently choose legacy scope.
    db.execute_unprepared("CREATE TEMP TABLE held_author_step AS SELECT * FROM chef_layout_migrations WHERE version='learning_000023_local_speech_work_keys'; DELETE FROM chef_layout_migrations WHERE version='learning_000023_local_speech_work_keys'").await.unwrap();
    rejected(
        invoke(&cli_base, &learning, &root, &["release-status"]),
        "Author command layout not ready",
    );
    rejected(
        invoke(&cli_base, &learning, &root, &["import", next_file]),
        "Author command layout not ready",
    );
    db.execute_unprepared("INSERT INTO chef_layout_migrations SELECT * FROM held_author_step; DROP TABLE held_author_step").await.unwrap();
    db.execute_unprepared(
        "UPDATE chef_layout_migrations SET version='unknown_author_step' WHERE scope='identity'",
    )
    .await
    .unwrap();
    rejected(
        invoke(&cli_base, &learning, &root, &["release-status"]),
        "Author command layout not ready",
    );
    rejected(
        invoke(&cli_base, &learning, &root, &["import", next_file]),
        "Author command layout not ready",
    );
    db.execute_unprepared("UPDATE chef_layout_migrations SET version='identity_000001_throttle_expiry',definition='changed' WHERE scope='identity'").await.unwrap();
    rejected(
        invoke(&cli_base, &learning, &root, &["release-status"]),
        "Author command layout not ready",
    );
    rejected(
        invoke(&cli_base, &learning, &root, &["import", next_file]),
        "Author command layout not ready",
    );
    db.execute_unprepared("UPDATE chef_layout_migrations SET definition='CREATE INDEX chef_throttle_expiry ON auth_throttle(resets_at)' WHERE scope='identity'").await.unwrap();
    rejected(
        invoke(
            &cli_base,
            &learning,
            &root,
            &["invite", "nobody@example.test", "invitation.txt"],
        ),
        "Split schema requires independent identity mode",
    );
    assert!(!root.join("invitation.txt").exists());
    success(invoke(&cli_base, &learning, &root, &["release-status"]));
    assert_eq!(
        db.query_one_raw(Statement::from_string(DbBackend::Postgres, hash_sql))
            .await
            .unwrap()
            .unwrap()
            .try_get::<String>("", "hash")
            .unwrap(),
        before
    );
    let absent=db.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT NOT EXISTS(SELECT 1 FROM lesson_revisions WHERE product_id='brioche' AND lesson_id=$1 AND revision=2) AS absent",[lesson_id.into()])).await.unwrap().unwrap();
    assert!(absent.try_get::<bool>("", "absent").unwrap());
    role_db.close().await.unwrap();
    // Delete only known files beneath this task's canonical fixture root.
    let canonical = root.canonicalize().unwrap();
    assert!(canonical.starts_with(std::env::temp_dir().canonicalize().unwrap()));
    assert_eq!(
        canonical.file_name().unwrap().to_string_lossy(),
        format!("brioche-media-{learning}")
    );
    std::fs::remove_dir_all(&canonical).unwrap();
    db.execute_unprepared(&format!(
        "DROP OWNED BY {author_role}; DROP ROLE {author_role}"
    ))
    .await
    .unwrap();
    db.close().await.unwrap();
    admin
        .execute_unprepared(&format!(
            "DROP SCHEMA {learning} CASCADE; DROP SCHEMA {identity} CASCADE"
        ))
        .await
        .unwrap();
}
