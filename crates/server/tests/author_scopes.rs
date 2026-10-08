//! Product-aware author CLI against disposable schemas; synthetic H records are isolation sentinels.
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
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
    let media_inputs = prepare_media_inputs(&root);
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
    for (command, file) in [
        ("assets-import", &media_inputs.visual_file),
        ("audio-import", &media_inputs.audio_file),
    ] {
        rejected(
            invoke(
                &base,
                &learning,
                &root,
                &[
                    command,
                    file.to_str().unwrap(),
                    media_inputs.sources.to_str().unwrap(),
                    "media-cli",
                ],
            ),
            "Author command layout not ready",
        );
    }
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
    let h_media_before =
        verify_media_commands(&db, &cli_base, &learning, &root, &media_inputs).await;
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
    let mut layout_visual = media_inputs.visual.clone();
    layout_visual["assets"][0]["assetId"] = "author-layout-avatar".into();
    layout_visual["characters"][0]["snapshot"]["characterId"] = "author-layout-character".into();
    layout_visual["characters"][0]["snapshot"]["avatarId"] = "author-layout-avatar".into();
    let mut layout_audio = media_inputs.audio.clone();
    layout_audio["assets"][0]["assetId"] = "author-layout-recording".into();
    let layout_visual_file = root.join("layout-visual.json");
    let layout_audio_file = root.join("layout-audio.json");
    std::fs::write(
        &layout_visual_file,
        serde_json::to_vec(&layout_visual).unwrap(),
    )
    .unwrap();
    std::fs::write(
        &layout_audio_file,
        serde_json::to_vec(&layout_audio).unwrap(),
    )
    .unwrap();
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
    for (command, file) in [
        ("assets-import", &layout_visual_file),
        ("audio-import", &layout_audio_file),
    ] {
        rejected(
            invoke(
                &cli_base,
                &learning,
                &root,
                &[
                    command,
                    file.to_str().unwrap(),
                    media_inputs.sources.to_str().unwrap(),
                    "media-cli",
                ],
            ),
            "Author command layout not ready",
        );
    }
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
    for (command, file) in [
        ("assets-import", &layout_visual_file),
        ("audio-import", &layout_audio_file),
    ] {
        rejected(
            invoke(
                &cli_base,
                &learning,
                &root,
                &[
                    command,
                    file.to_str().unwrap(),
                    media_inputs.sources.to_str().unwrap(),
                    "media-cli",
                ],
            ),
            "Author command layout not ready",
        );
    }
    db.execute_unprepared("UPDATE chef_layout_migrations SET version='identity_000001_throttle_expiry',definition='changed' WHERE scope='identity'").await.unwrap();
    rejected(
        invoke(&cli_base, &learning, &root, &["release-status"]),
        "Author command layout not ready",
    );
    rejected(
        invoke(&cli_base, &learning, &root, &["import", next_file]),
        "Author command layout not ready",
    );
    for (command, file) in [
        ("assets-import", &layout_visual_file),
        ("audio-import", &layout_audio_file),
    ] {
        rejected(
            invoke(
                &cli_base,
                &learning,
                &root,
                &[
                    command,
                    file.to_str().unwrap(),
                    media_inputs.sources.to_str().unwrap(),
                    "media-cli",
                ],
            ),
            "Author command layout not ready",
        );
    }
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
    let row=db.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM media_assets WHERE product_id='brioche' AND asset_id='author-layout-avatar')+(SELECT count(*) FROM character_revisions WHERE product_id='brioche' AND character_id='author-layout-character')+(SELECT count(*) FROM audio_assets WHERE product_id='brioche' AND asset_id='author-layout-recording') AS blocked,(SELECT count(*) FROM asset_import_audit WHERE product_id='brioche' AND actor='media-cli')+(SELECT count(*) FROM audio_import_audit WHERE product_id='brioche' AND actor='media-cli') AS audits")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "blocked").unwrap(), 0);
    assert_eq!(row.try_get::<i64>("", "audits").unwrap(), 2);
    assert_eq!(
        db.query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            MEDIA_FINGERPRINT_SQL
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "hash")
        .unwrap(),
        h_media_before
    );
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

struct MediaInputs {
    visual: Value,
    audio: Value,
    sources: PathBuf,
    visual_file: PathBuf,
    audio_file: PathBuf,
}
fn prepare_media_inputs(root: &Path) -> MediaInputs {
    let sources = root.join("author-source");
    std::fs::create_dir(&sources).unwrap();
    let svg=br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96"><rect x="12" y="12" width="72" height="72" rx="18" fill="#FFC96F"/></svg>"##;
    std::fs::write(sources.join("avatar.svg"), svg).unwrap();
    let visual = json!({"schemaVersion":"1.0","assets":[{"assetId":"author-shared-avatar","revision":1,"sha256":format!("{:x}",Sha256::digest(svg)),"mimeType":"image/svg+xml","width":96,"height":96,"altZh":"合成协议头像","creditZh":"仅隔离测试","file":"avatar.svg","status":"ready","source":"synthetic SVG fixture","license":"LicenseRef-TestOnly","creator":"author-cli-fixture","rightsConfirmed":true}],"characters":[{"snapshot":{"characterId":"author-shared-character","revision":1,"displayName":"Fixture","avatarId":"author-shared-avatar","speechLocale":"fr-FR"},"avatarRevision":1}]});
    let bytes = include_bytes!("fixtures/audio/synthetic.mp3");
    std::fs::write(sources.join("recording.mp3"), bytes).unwrap();
    let info = chef_engine::audio::inspect(bytes, "audio/mpeg").unwrap();
    let audio = json!({"schemaVersion":"1.0","assets":[{"assetId":"author-shared-recording","revision":1,"sha256":format!("{:x}",Sha256::digest(bytes)),"mimeType":"audio/mpeg","durationMs":info.duration_ms,"creditZh":"仅隔离测试","file":"recording.mp3","status":"ready","source":"synthetic audio fixture","license":"LicenseRef-TestOnly","creator":"author-cli-fixture","rightsConfirmed":true}]});
    let visual_file = root.join("author-visual.json");
    let audio_file = root.join("author-audio.json");
    std::fs::write(&visual_file, serde_json::to_vec(&visual).unwrap()).unwrap();
    std::fs::write(&audio_file, serde_json::to_vec(&audio).unwrap()).unwrap();
    MediaInputs {
        visual,
        audio,
        sources,
        visual_file,
        audio_file,
    }
}
async fn verify_media_commands(
    db: &DatabaseConnection,
    url: &str,
    schema: &str,
    root: &Path,
    inputs: &MediaInputs,
) -> String {
    let image = &inputs.visual["assets"][0];
    let image_id = image["assetId"].as_str().unwrap();
    let image_sha = image["sha256"].as_str().unwrap();
    let descriptor = json!({"assetId":image_id,"revision":1,"sha256":image_sha,"mimeType":"image/svg+xml","width":96,"height":96,"altZh":"foreign synthetic avatar","creditZh":"foreign fixture","url":format!("/api/media/{image_sha}.svg")});
    let mut provenance = image.clone();
    provenance["creator"] = "foreign-fixture".into();
    let size = std::fs::read(inputs.sources.join("avatar.svg"))
        .unwrap()
        .len() as i64;
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO media_assets(product_id,asset_id,revision,descriptor,provenance,sha256,extension,byte_size) VALUES('hargow',$1,1,$2,$3,$4,'svg',$5)",[image_id.into(),descriptor.into(),provenance.into(),image_sha.into(),size.into()])).await.unwrap();
    let character = &inputs.visual["characters"][0]["snapshot"];
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO character_revisions(product_id,character_id,revision,snapshot,avatar_id,avatar_revision) VALUES('hargow',$1,1,$2,$3,1)",[character["characterId"].as_str().unwrap().into(),character.clone().into(),image_id.into()])).await.unwrap();
    db.execute_unprepared("INSERT INTO asset_import_audit(product_id,actor,bundle_hash,asset_count,character_count) VALUES('hargow','foreign-cli-sentinel',repeat('0',64),1,1)").await.unwrap();
    let audio = &inputs.audio["assets"][0];
    let audio_id = audio["assetId"].as_str().unwrap();
    let audio_sha = audio["sha256"].as_str().unwrap();
    let bytes = include_bytes!("fixtures/audio/synthetic.mp3");
    let info = chef_engine::audio::inspect(bytes, "audio/mpeg").unwrap();
    let descriptor = json!({"assetId":audio_id,"revision":1,"sha256":audio_sha,"mimeType":"audio/mpeg","durationMs":info.duration_ms,"creditZh":"foreign fixture","url":format!("/api/audio/{audio_sha}.mp3")});
    let mut provenance = audio.clone();
    provenance["creator"] = "foreign-fixture".into();
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO audio_assets(product_id,asset_id,revision,descriptor,provenance,sha256,extension,byte_size,duration_ms,sample_rate,channels) VALUES('hargow',$1,1,$2,$3,$4,'mp3',$5,$6,$7,$8)",[audio_id.into(),descriptor.into(),provenance.into(),audio_sha.into(),(bytes.len() as i64).into(),(info.duration_ms as i32).into(),(info.sample_rate as i32).into(),(info.channels as i32).into()])).await.unwrap();
    db.execute_unprepared("INSERT INTO audio_import_audit(product_id,actor,bundle_hash,asset_count) VALUES('hargow','foreign-cli-sentinel',repeat('0',64),1)").await.unwrap();
    // A valid square avatar belonging only to Hargow cannot satisfy a B character.
    db.execute_unprepared("INSERT INTO media_assets(product_id,asset_id,revision,descriptor,provenance,sha256,extension,byte_size) SELECT product_id,'author-foreign-avatar',revision,descriptor||jsonb_build_object('assetId','author-foreign-avatar'),provenance||jsonb_build_object('assetId','author-foreign-avatar'),sha256,extension,byte_size FROM media_assets WHERE product_id='hargow' AND asset_id='author-shared-avatar'").await.unwrap();
    let hash_sql = MEDIA_FINGERPRINT_SQL;
    let before = db
        .query_one_raw(Statement::from_string(DbBackend::Postgres, hash_sql))
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "hash")
        .unwrap();
    let visual_file = inputs.visual_file.to_str().unwrap();
    let audio_file = inputs.audio_file.to_str().unwrap();
    let sources = inputs.sources.to_str().unwrap();
    for (command, file) in [("assets-import", visual_file), ("audio-import", audio_file)] {
        success(invoke(
            url,
            schema,
            root,
            &[command, file, sources, "media-cli"],
        ));
        assert!(
            !invoke(url, schema, root, &[command, file, sources, "media-cli"])
                .status
                .success()
        );
    }
    assert_eq!(
        std::fs::read(root.join(format!("{image_sha}.svg"))).unwrap(),
        std::fs::read(inputs.sources.join("avatar.svg")).unwrap()
    );
    assert_eq!(
        std::fs::read(root.join(format!("{audio_sha}.mp3"))).unwrap(),
        bytes
    );
    let bad_file = root.join("invalid-media.json");
    let bad_file = bad_file.to_str().unwrap();
    let mut foreign_avatar = inputs.visual.clone();
    foreign_avatar["assets"][0]["assetId"] = "author-rollback-avatar".into();
    foreign_avatar["characters"][0]["snapshot"]["characterId"] = "author-rollback-character".into();
    foreign_avatar["characters"][0]["snapshot"]["avatarId"] = "author-foreign-avatar".into();
    std::fs::write(bad_file, serde_json::to_vec(&foreign_avatar).unwrap()).unwrap();
    rejected(
        invoke(
            url,
            schema,
            root,
            &["assets-import", bad_file, sources, "media-cli"],
        ),
        "character avatar revision is missing",
    );
    let mut duplicate_visual = inputs.visual.clone();
    let mut pending = duplicate_visual["assets"][0].clone();
    pending["assetId"] = "author-batch-avatar".into();
    duplicate_visual["assets"]
        .as_array_mut()
        .unwrap()
        .insert(0, pending);
    duplicate_visual["characters"] = json!([]);
    let mut duplicate_audio = inputs.audio.clone();
    let mut pending = duplicate_audio["assets"][0].clone();
    pending["assetId"] = "author-batch-recording".into();
    duplicate_audio["assets"]
        .as_array_mut()
        .unwrap()
        .insert(0, pending);
    for (command, bundle) in [
        ("assets-import", duplicate_visual),
        ("audio-import", duplicate_audio),
    ] {
        std::fs::write(bad_file, serde_json::to_vec(&bundle).unwrap()).unwrap();
        rejected(
            invoke(
                url,
                schema,
                root,
                &[command, bad_file, sources, "media-cli"],
            ),
            "revision already registered",
        );
    }
    for (command, mut bundle) in [
        ("assets-import", inputs.visual.clone()),
        ("audio-import", inputs.audio.clone()),
    ] {
        bundle["assets"][0]["sha256"] = "0".repeat(64).into();
        std::fs::write(bad_file, serde_json::to_vec(&bundle).unwrap()).unwrap();
        rejected(
            invoke(
                url,
                schema,
                root,
                &[command, bad_file, sources, "media-cli"],
            ),
            "hash mismatch",
        );
    }
    let row=db.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM media_assets WHERE product_id='brioche' AND asset_id='author-shared-avatar')::bigint AS images,(SELECT count(*) FROM character_revisions WHERE product_id='brioche' AND character_id='author-shared-character')::bigint AS characters,(SELECT count(*) FROM audio_assets WHERE product_id='brioche' AND asset_id='author-shared-recording')::bigint AS recordings,(SELECT count(*) FROM asset_import_audit WHERE product_id='brioche' AND actor='media-cli')::bigint AS visual_audits,(SELECT count(*) FROM audio_import_audit WHERE product_id='brioche' AND actor='media-cli')::bigint AS audio_audits,(SELECT count(*) FROM media_assets WHERE product_id='brioche' AND asset_id IN ('author-rollback-avatar','author-batch-avatar'))+(SELECT count(*) FROM character_revisions WHERE product_id='brioche' AND character_id='author-rollback-character')+(SELECT count(*) FROM audio_assets WHERE product_id='brioche' AND asset_id='author-batch-recording') AS rejected_members")).await.unwrap().unwrap();
    for field in [
        "images",
        "characters",
        "recordings",
        "visual_audits",
        "audio_audits",
    ] {
        assert_eq!(row.try_get::<i64>("", field).unwrap(), 1);
    }
    assert_eq!(row.try_get::<i64>("", "rejected_members").unwrap(), 0);
    let own=db.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT m.descriptor,m.provenance,c.snapshot FROM media_assets m JOIN character_revisions c ON c.product_id=m.product_id AND c.avatar_id=m.asset_id AND c.avatar_revision=m.revision WHERE m.product_id='brioche' AND m.asset_id='author-shared-avatar'")).await.unwrap().unwrap();
    assert_eq!(
        own.try_get::<Value>("", "provenance").unwrap(),
        inputs.visual["assets"][0]
    );
    assert_eq!(
        own.try_get::<Value>("", "snapshot").unwrap(),
        inputs.visual["characters"][0]["snapshot"]
    );
    let own=db.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT provenance FROM audio_assets WHERE product_id='brioche' AND asset_id='author-shared-recording'")).await.unwrap().unwrap();
    assert_eq!(
        own.try_get::<Value>("", "provenance").unwrap(),
        inputs.audio["assets"][0]
    );
    assert_eq!(
        db.query_one_raw(Statement::from_string(DbBackend::Postgres, hash_sql))
            .await
            .unwrap()
            .unwrap()
            .try_get::<String>("", "hash")
            .unwrap(),
        before
    );
    before
}

const MEDIA_FINGERPRINT_SQL: &str = "SELECT md5(jsonb_build_array((SELECT jsonb_agg(to_jsonb(m) ORDER BY m.asset_id,m.revision) FROM media_assets m WHERE product_id='hargow'),(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.character_id,c.revision) FROM character_revisions c WHERE product_id='hargow'),(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM asset_import_audit a WHERE product_id='hargow'),(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.asset_id,m.revision) FROM audio_assets m WHERE product_id='hargow'),(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM audio_import_audit a WHERE product_id='hargow'))::text) AS hash";
