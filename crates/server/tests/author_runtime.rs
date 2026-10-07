//! Real author commands against an explicitly supplied, disposable PostgreSQL schema.
use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use std::{
    path::Path,
    process::{Command, Output},
};
#[path = "support/assets.rs"]
mod asset_fixtures;

fn invoke(url: &str, root: &Path, command: &str, file: &Path) -> Output {
    let mut process = Command::new(env!("CARGO_BIN_EXE_chef-server"));
    process.args([command, file.to_str().unwrap()]);
    if command == "release-stage" {
        process.args(["protocol-test", "isolated synthetic author test"]);
    }
    process
        .env("DATABASE_URL", url)
        .env("CONTENT_MODE", "database")
        .env("APP_ENV", "production")
        .env("MEDIA_ROOT", root)
        .output()
        .unwrap()
}
fn write(file: &Path, value: &Value) -> String {
    let text = serde_json::to_string_pretty(value)
        .unwrap()
        .replace('\n', "\r\n");
    std::fs::write(file, &text).unwrap();
    text
}
fn invoke_bundle(url: &str, root: &Path, command: &str, file: &Path, source: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_chef-server"))
        .args([
            command,
            file.to_str().unwrap(),
            source.to_str().unwrap(),
            "protocol-test",
        ])
        .env("DATABASE_URL", url)
        .env("CONTENT_MODE", "database")
        .env("APP_ENV", "production")
        .env("MEDIA_ROOT", root)
        .output()
        .unwrap()
}
fn located(output: Output, file: &Path, text: &str, pointer: &str, offset: usize, reason: &str) {
    let error = String::from_utf8_lossy(&output.stderr);
    let before = &text[..offset];
    let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
    assert!(
        !output.status.success(),
        "expected author rejection at {pointer}: {reason}"
    );
    assert!(
        error.contains(&format!("{}:{line}:{column}: {pointer}:", file.display())),
        "{error}"
    );
    assert!(error.contains(reason), "{error}");
    assert!(
        !error.contains("INSERT INTO"),
        "database details must stay private: {error}"
    );
}
async fn count(db: &sea_orm::DatabaseConnection, table: &str) -> i64 {
    db.query_one_raw(Statement::from_string(
        DbBackend::Postgres,
        format!("SELECT count(*) AS n FROM {table}"),
    ))
    .await
    .unwrap()
    .unwrap()
    .try_get("", "n")
    .unwrap()
}

fn activate(url: &str, root: &Path, id: &str, expected: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_chef-server"))
        .args([
            "release-activate",
            id,
            expected,
            "protocol-test",
            "isolated activation diagnostic",
        ])
        .env("DATABASE_URL", url)
        .env("CONTENT_MODE", "database")
        .env("APP_ENV", "production")
        .env("MEDIA_ROOT", root)
        .output()
        .unwrap()
}
fn activation_failure(output: Output, reason: &str) {
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(error.contains(reason), "{error}");
    assert!(
        !error.contains("SELECT ") && !error.contains("UPDATE "),
        "{error}"
    );
}
fn withdraw(url: &str, root: &Path, id: &str, revision: &str, expected: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_chef-server"))
        .args([
            "content-withdraw",
            id,
            revision,
            expected,
            "protocol-test",
            "isolated withdrawal diagnostic",
        ])
        .env("DATABASE_URL", url)
        .env("CONTENT_MODE", "database")
        .env("APP_ENV", "production")
        .env("MEDIA_ROOT", root)
        .output()
        .unwrap()
}

#[tokio::test]
#[ignore = "set TEST_DATABASE_URL to a dedicated PostgreSQL database"]
async fn import_and_stage_cli_locate_original_source_and_preserve_atomicity() {
    let base = std::env::var("TEST_DATABASE_URL").expect("dedicated test database required");
    let admin = Database::connect(&base).await.unwrap();
    let schema = format!(
        "author_runtime_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    admin
        .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
        .await
        .unwrap();
    let mut url = url::Url::parse(&base).unwrap();
    url.query_pairs_mut()
        .append_pair("options", &format!("-c search_path={schema}"));
    let db = Database::connect(url.as_str()).await.unwrap();
    brioche_migration::Migrator::up(&db, None).await.unwrap();
    let root = asset_fixtures::fixture_assets(&db, &schema).await;
    // File checks occur after connection, but still report the original bundle field.
    // Synthetic provenance below authorizes only this isolated protocol fixture.
    let media_file = root.join("media-bundle.json");
    let visual_source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/visuals");
    let mut visual: Value =
        serde_json::from_str(include_str!("../../../docs/examples/asset-bundle.json")).unwrap();
    visual["assets"].as_array_mut().unwrap().truncate(1);
    visual["characters"] = json!([]);
    visual["assets"][0]["status"] = json!("ready");
    visual["assets"][0]["rightsConfirmed"] = json!(true);
    visual["assets"][0]["license"] = json!("LicenseRef-TestOnly");
    let recording_file = root.join("synthetic.mp3");
    std::fs::write(
        &recording_file,
        include_bytes!("fixtures/audio/synthetic.mp3"),
    )
    .unwrap();
    let (recording_bytes, info) =
        chef_engine::audio::inspect_file(&recording_file, "audio/mpeg").unwrap();
    use sha2::{Digest, Sha256};
    let audio = json!({"schemaVersion":"1.0","assets":[{"assetId":"author-recording","revision":1,"sha256":format!("{:x}", Sha256::digest(&recording_bytes)),"mimeType":"audio/mpeg","durationMs":info.duration_ms,"creditZh":"仅测试","file":"synthetic.mp3","status":"ready","source":"synthetic protocol fixture","license":"LicenseRef-TestOnly","creator":"protocol-test","rightsConfirmed":true}]});
    let visual_count = count(&db, "media_assets").await;
    let audit_count = count(&db, "asset_import_audit").await;
    for (command, baseline, source_root, pointer, value, reason) in [
        (
            "assets-import",
            &visual,
            &visual_source,
            "/assets/0/sha256",
            json!("0".repeat(64)),
            "hash mismatch",
        ),
        (
            "assets-import",
            &visual,
            &visual_source,
            "/assets/0/width",
            json!(641),
            "width does not match",
        ),
        (
            "assets-import",
            &visual,
            &visual_source,
            "/assets/0/file",
            json!("missing.svg"),
            "file unavailable",
        ),
        (
            "audio-import",
            &audio,
            &root,
            "/assets/0/sha256",
            json!("0".repeat(64)),
            "hash mismatch",
        ),
        (
            "audio-import",
            &audio,
            &root,
            "/assets/0/durationMs",
            json!(info.duration_ms + 1),
            "duration does not match",
        ),
        (
            "audio-import",
            &audio,
            &root,
            "/assets/0/file",
            json!("missing.mp3"),
            "file unavailable",
        ),
    ] {
        let mut bundle = baseline.clone();
        *bundle.pointer_mut(pointer).unwrap() = value.clone();
        let text = write(&media_file, &bundle);
        let key = format!("\"{}\": ", pointer.rsplit('/').next().unwrap());
        let marker = format!("{key}{}", serde_json::to_string(&value).unwrap());
        let offset = text.find(&marker).unwrap() + key.len();
        let output = Command::new(env!("CARGO_BIN_EXE_chef-server"))
            .args([
                command,
                media_file.to_str().unwrap(),
                source_root.to_str().unwrap(),
                "protocol-test",
            ])
            .env("DATABASE_URL", url.as_str())
            .env("CONTENT_MODE", "database")
            .env("APP_ENV", "production")
            .env("MEDIA_ROOT", &root)
            .output()
            .unwrap();
        located(output, &media_file, &text, pointer, offset, reason);
        assert_eq!(count(&db, "media_assets").await, visual_count);
        assert_eq!(count(&db, "asset_import_audit").await, audit_count);
        assert_eq!(count(&db, "audio_assets").await, 0);
        assert_eq!(count(&db, "audio_import_audit").await, 0);
    }
    // Later duplicate members must not register an earlier, otherwise valid member.
    let mut duplicate_visual = visual.clone();
    let mut fresh_visual = visual["assets"][0].clone();
    fresh_visual["assetId"] = json!("author-new-visual");
    fresh_visual["revision"] = json!(7994);
    duplicate_visual["assets"] = json!([fresh_visual, visual["assets"][0]]);
    let text = write(&media_file, &duplicate_visual);
    located(
        invoke_bundle(
            url.as_str(),
            &root,
            "assets-import",
            &media_file,
            &visual_source,
        ),
        &media_file,
        &text,
        "/assets/1/revision",
        text.rfind("\"revision\": 1").unwrap() + "\"revision\": ".len(),
        "asset revision already registered",
    );
    assert_eq!(count(&db, "media_assets").await, visual_count);
    assert_eq!(count(&db, "asset_import_audit").await, audit_count);

    let character_count = count(&db, "character_revisions").await;
    let snapshot =
        serde_json::to_value(chef_engine::development_fixture().unwrap().cast[0].clone()).unwrap();
    let mut fresh_character = snapshot.clone();
    fresh_character["characterId"] = json!("author-new-character");
    fresh_character["revision"] = json!(7995);
    duplicate_visual["assets"] = json!([fresh_visual]);
    duplicate_visual["characters"] = json!([
        {"snapshot":fresh_character,"avatarRevision":1},
        {"snapshot":snapshot,"avatarRevision":1}
    ]);
    let text = write(&media_file, &duplicate_visual);
    located(
        invoke_bundle(
            url.as_str(),
            &root,
            "assets-import",
            &media_file,
            &visual_source,
        ),
        &media_file,
        &text,
        "/characters/1/snapshot/revision",
        text.rfind("\"revision\": 1").unwrap() + "\"revision\": ".len(),
        "character revision already registered",
    );
    assert_eq!(count(&db, "media_assets").await, visual_count);
    assert_eq!(count(&db, "character_revisions").await, character_count);
    assert_eq!(count(&db, "asset_import_audit").await, audit_count);

    // A character-only package can intentionally refer to an already registered avatar.
    let mut external = snapshot.clone();
    external["characterId"] = json!("author-registry-avatar");
    external["revision"] = json!(7997);
    let mut registry_only = json!({"schemaVersion":"1.0", "assets":[], "characters":[{"snapshot":external,"avatarRevision":1}]});
    write(&media_file, &registry_only);
    let output = invoke_bundle(
        url.as_str(),
        &root,
        "assets-import",
        &media_file,
        &visual_source,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(count(&db, "character_revisions").await, character_count + 1);
    assert_eq!(count(&db, "media_assets").await, visual_count);
    assert_eq!(count(&db, "asset_import_audit").await, audit_count + 1);
    // Package preflight cannot know this external image's shape; the registry still rejects it.
    registry_only["characters"][0]["snapshot"]["characterId"] = json!("author-rejected-avatar");
    registry_only["characters"][0]["snapshot"]["avatarId"] = visual["assets"][0]["assetId"].clone();
    let text = write(&media_file, &registry_only);
    let key = "\"avatarId\": ";
    let marker = format!(
        "{key}{}",
        registry_only["characters"][0]["snapshot"]["avatarId"]
    );
    located(
        invoke_bundle(
            url.as_str(),
            &root,
            "assets-import",
            &media_file,
            &visual_source,
        ),
        &media_file,
        &text,
        "/characters/0/snapshot/avatarId",
        text.find(&marker).unwrap() + key.len(),
        "avatar must be square",
    );
    assert_eq!(count(&db, "character_revisions").await, character_count + 1);
    assert_eq!(count(&db, "media_assets").await, visual_count);
    assert_eq!(count(&db, "asset_import_audit").await, audit_count + 1);

    write(&media_file, &audio);
    assert!(
        invoke_bundle(url.as_str(), &root, "audio-import", &media_file, &root)
            .status
            .success()
    );
    let mut duplicate_audio = audio.clone();
    let mut fresh_audio = audio["assets"][0].clone();
    fresh_audio["assetId"] = json!("author-new-recording");
    fresh_audio["revision"] = json!(7996);
    duplicate_audio["assets"] = json!([fresh_audio, audio["assets"][0]]);
    let text = write(&media_file, &duplicate_audio);
    located(
        invoke_bundle(url.as_str(), &root, "audio-import", &media_file, &root),
        &media_file,
        &text,
        "/assets/1/revision",
        text.rfind("\"revision\": 1").unwrap() + "\"revision\": ".len(),
        "recording revision already registered",
    );
    assert_eq!(count(&db, "audio_assets").await, 1);
    assert_eq!(count(&db, "audio_import_audit").await, 1);

    let mut concurrent_audio = audio.clone();
    concurrent_audio["assets"][0]["assetId"] = json!("author-concurrent-recording");
    let text = write(&media_file, &concurrent_audio);
    let outputs = std::thread::scope(|scope| {
        let a =
            scope.spawn(|| invoke_bundle(url.as_str(), &root, "audio-import", &media_file, &root));
        let b =
            scope.spawn(|| invoke_bundle(url.as_str(), &root, "audio-import", &media_file, &root));
        [a.join().unwrap(), b.join().unwrap()]
    });
    assert_eq!(outputs.iter().filter(|o| o.status.success()).count(), 1);
    for output in outputs.into_iter().filter(|o| !o.status.success()) {
        located(
            output,
            &media_file,
            &text,
            "/assets/0/revision",
            text.find("\"revision\": 1").unwrap() + "\"revision\": ".len(),
            "recording revision already registered",
        );
    }
    assert_eq!(count(&db, "audio_assets").await, 2);
    assert_eq!(count(&db, "audio_import_audit").await, 2);

    let lesson_file = root.join("lesson.json");
    let release_file = root.join("release.json");
    let mut source = chef_engine::development_source().unwrap();
    // The repository example may be user-approved; this fixture explicitly exercises draft rejection.
    source["editorial"] = json!({"status":"draft","note":"isolated draft rejection protocol"});
    source["assetRefs"] = asset_fixtures::fixture_refs();
    // Fixed registry references replace these placeholders before final validation.
    source["media"] = json!("author placeholder replaced by registered images");
    source["audioRefs"] = json!([]);
    source["audio"] = json!("author placeholder replaced by empty recording references");
    for (field, pointer, marker, reason) in [
        (
            "assetRefs",
            "/assetRefs/0/revision",
            7991,
            "registered asset revision missing",
        ),
        (
            "audioRefs",
            "/audioRefs/0/revision",
            7992,
            "registered recording revision missing",
        ),
    ] {
        let mut invalid = source.clone();
        invalid[field] = json!([{"assetId":"missing-registration", "revision":marker}]);
        let text = write(&lesson_file, &invalid);
        located(
            invoke(url.as_str(), &root, "import", &lesson_file),
            &lesson_file,
            &text,
            pointer,
            text.find(&marker.to_string()).unwrap(),
            reason,
        );
        assert_eq!(count(&db, "lesson_revisions").await, 0);
    }
    let mut invalid = source.clone();
    invalid["serverOnly"]["grading"]["exercise-intention"]["correctOptionId"] =
        json!("private-invalid-option");
    let text = write(&lesson_file, &invalid);
    let output = invoke(url.as_str(), &root, "import", &lesson_file);
    assert!(!String::from_utf8_lossy(&output.stderr).contains("private-invalid-option"));
    located(
        output,
        &lesson_file,
        &text,
        "/serverOnly/grading/exercise-intention/correctOptionId",
        text.find("\"private-invalid-option\"").unwrap(),
        "unknown option reference",
    );
    assert_eq!(count(&db, "lesson_revisions").await, 0);
    invalid = source.clone();
    invalid["revision"] = json!(2147483648u64);
    let text = write(&lesson_file, &invalid);
    located(
        invoke(url.as_str(), &root, "import", &lesson_file),
        &lesson_file,
        &text,
        "/revision",
        text.find("2147483648").unwrap(),
        "expected revision in 1..2147483647",
    );
    assert_eq!(count(&db, "lesson_revisions").await, 0);

    let text = write(&lesson_file, &source);
    let output = invoke(url.as_str(), &root, "import", &lesson_file);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    located(
        invoke(url.as_str(), &root, "import", &lesson_file),
        &lesson_file,
        &text,
        "/revision",
        text.rfind("\"revision\": 1").unwrap() + "\"revision\": ".len(),
        "already exists",
    );
    assert_eq!(count(&db, "lesson_revisions").await, 1);
    let stored = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT public_document,server_document FROM lesson_revisions LIMIT 1",
        ))
        .await
        .unwrap()
        .unwrap();
    let public: Value = stored.try_get("", "public_document").unwrap();
    let private: Value = stored.try_get("", "server_document").unwrap();
    assert_eq!(
        public["media"].as_array().unwrap().len(),
        source["assetRefs"].as_array().unwrap().len()
    );
    assert!(private["media"].is_array());
    assert_eq!(private["audio"], json!([]));
    // A second synthetic revision is reviewed only within this disposable protocol test.
    let mut reviewed = source.clone();
    reviewed["id"] = json!("author-reviewed");
    reviewed["editorial"]["status"] = json!("reviewed");
    write(&lesson_file, &reviewed);
    assert!(
        invoke(url.as_str(), &root, "import", &lesson_file)
            .status
            .success()
    );
    let manifest = json!({"id":"author-release", "schemaVersion":"1.0", "levels":[{
    "id":source["levelId"],"label":"A1 入门","units":[{
        "id":source["unitId"],"titleZh":"面包店","lessons":[
            {"lessonId":"author-reviewed","revision":1},
            {"lessonId":source["id"],"revision":1}
        ]}]}]});
    let mut missing = manifest.clone();
    missing["levels"][0]["units"][0]["lessons"][1]["revision"] = json!(7993);
    let text = write(&release_file, &missing);
    located(
        invoke(url.as_str(), &root, "release-stage", &release_file),
        &release_file,
        &text,
        "/levels/0/units/0/lessons/1/revision",
        text.find("7993").unwrap(),
        "has not been imported",
    );
    let text = write(&release_file, &manifest);
    let lesson_marker = text
        .find(&format!("\"lessonId\": {}", source["id"]))
        .unwrap();
    let entry_offset = text[..lesson_marker].rfind('{').unwrap();
    located(
        invoke(url.as_str(), &root, "release-stage", &release_file),
        &release_file,
        &text,
        "/levels/0/units/0/lessons/1",
        entry_offset,
        "requires reviewed editorial status",
    );
    assert_eq!(count(&db, "content_releases").await, 0);
    assert_eq!(count(&db, "release_entries").await, 0);
    assert_eq!(count(&db, "content_audit").await, 0);
    let mut valid = manifest.clone();
    valid["levels"][0]["units"][0]["lessons"]
        .as_array_mut()
        .unwrap()
        .pop();
    let mut mismatch = valid.clone();
    mismatch["levels"][0]["units"][0]["id"] = json!("different-unit");
    let text = write(&release_file, &mismatch);
    let marker = text.find("\"lessonId\": \"author-reviewed\"").unwrap();
    let offset = text[..marker].rfind('{').unwrap();
    located(
        invoke(url.as_str(), &root, "release-stage", &release_file),
        &release_file,
        &text,
        "/levels/0/units/0/lessons/0",
        offset,
        "does not match",
    );
    let text = write(&release_file, &valid);
    let descriptor = db.query_one_raw(Statement::from_string(DbBackend::Postgres,
        "SELECT descriptor FROM media_assets WHERE asset_id='art-bakery-morning' AND revision=1"))
        .await.unwrap().unwrap().try_get::<Value>("", "descriptor").unwrap();
    let stored = root.join(format!("{}.svg", descriptor["sha256"].as_str().unwrap()));
    let original_bytes = std::fs::read(&stored).unwrap();
    std::fs::write(&stored, b"corrupt protocol fixture").unwrap();
    let marker = text.find("\"lessonId\": \"author-reviewed\"").unwrap();
    let offset = text[..marker].rfind('{').unwrap();
    let rejected = invoke(url.as_str(), &root, "release-stage", &release_file);
    std::fs::write(&stored, original_bytes).unwrap();
    located(
        rejected,
        &release_file,
        &text,
        "/levels/0/units/0/lessons/0",
        offset,
        "imported lesson /media/0/sha256: stored visual object hash does not match registered revision",
    );
    assert_eq!(count(&db, "content_releases").await, 0);
    assert_eq!(count(&db, "content_audit").await, 0);
    let original_bytes = std::fs::read(&stored).unwrap();
    std::fs::remove_file(&stored).unwrap();
    let rejected = invoke(url.as_str(), &root, "release-stage", &release_file);
    std::fs::write(&stored, original_bytes).unwrap();
    located(
        rejected,
        &release_file,
        &text,
        "/levels/0/units/0/lessons/0",
        offset,
        "imported lesson /media/0/sha256: stored visual object is missing or unreadable",
    );
    assert_eq!(count(&db, "content_releases").await, 0);
    assert_eq!(count(&db, "release_entries").await, 0);
    assert_eq!(count(&db, "content_audit").await, 0);
    // Import permits author drafts; staging must reject unregistered or divergent cast snapshots.
    for (index, field, value, pointer, reason) in [
        (
            0,
            "displayName",
            json!("Camille draft variant"),
            "/cast/0",
            "character snapshot does not match registered revision",
        ),
        (
            1,
            "revision",
            json!(2),
            "/cast/0/revision",
            "character revision is not registered",
        ),
    ] {
        let mut changed = reviewed.clone();
        let id = format!("author-character-failure-{index}");
        changed["id"] = json!(id);
        changed["cast"][0][field] = value.clone();
        if field == "displayName" {
            changed["blocks"][1]["speakers"][0][field] = value;
        }
        write(&lesson_file, &changed);
        let imported = invoke(url.as_str(), &root, "import", &lesson_file);
        assert!(
            imported.status.success(),
            "{}",
            String::from_utf8_lossy(&imported.stderr)
        );
        let mut rejected_manifest = valid.clone();
        rejected_manifest["levels"][0]["units"][0]["lessons"][0]["lessonId"] = json!(id);
        let failure_text = write(&release_file, &rejected_manifest);
        let marker = failure_text
            .find(&format!("\"lessonId\": \"{id}\""))
            .unwrap();
        let failure_offset = failure_text[..marker].rfind('{').unwrap();
        located(
            invoke(url.as_str(), &root, "release-stage", &release_file),
            &release_file,
            &failure_text,
            "/levels/0/units/0/lessons/0",
            failure_offset,
            &format!("imported lesson {pointer}: {reason}"),
        );
        assert_eq!(count(&db, "content_releases").await, 0);
        assert_eq!(count(&db, "release_entries").await, 0);
        assert_eq!(count(&db, "content_audit").await, 0);
    }
    write(&release_file, &valid);
    let output = invoke(url.as_str(), &root, "release-stage", &release_file);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(count(&db, "content_releases").await, 1);
    assert_eq!(count(&db, "release_entries").await, 1);
    assert_eq!(count(&db, "content_audit").await, 1);
    located(
        invoke(url.as_str(), &root, "release-stage", &release_file),
        &release_file,
        &text,
        "/id",
        text.find("\"author-release\"").unwrap(),
        "already exists",
    );
    activation_failure(
        activate(url.as_str(), &root, "author-release", "9"),
        "expected-generation: content generation changed",
    );
    activation_failure(
        activate(url.as_str(), &root, "missing-release", "0"),
        "release-id: release is not staged",
    );
    let document: Value = db.query_one_raw(Statement::from_string(DbBackend::Postgres, "SELECT public_document FROM lesson_revisions WHERE lesson_id='author-reviewed' AND revision=1"))
        .await.unwrap().unwrap().try_get("", "public_document").unwrap();
    let asset = &document["media"][0];
    assert_eq!(asset["mimeType"], "image/svg+xml");
    let object = root.join(format!("{}.svg", asset["sha256"].as_str().unwrap()));
    let original = std::fs::read(&object).unwrap();
    std::fs::write(&object, b"corrupt synthetic object").unwrap();
    let output = activate(url.as_str(), &root, "author-release", "0");
    std::fs::write(&object, original).unwrap();
    activation_failure(
        output,
        "lesson author-reviewed@1: imported lesson /media/0/sha256: stored visual object hash does not match registered revision",
    );
    assert_eq!(count(&db, "content_audit").await, 1);
    let state = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT active_release,generation FROM content_state WHERE singleton",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        state
            .try_get::<Option<String>>("", "active_release")
            .unwrap(),
        None
    );
    assert_eq!(state.try_get::<i64>("", "generation").unwrap(), 0);
    assert_eq!(
        db.query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(*) AS n FROM lesson_revisions WHERE published"
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "n")
        .unwrap(),
        0
    );

    let activated = activate(url.as_str(), &root, "author-release", "0");
    assert!(
        activated.status.success(),
        "{}",
        String::from_utf8_lossy(&activated.stderr)
    );
    let state = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT active_release,generation FROM content_state WHERE singleton",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        state.try_get::<String>("", "active_release").unwrap(),
        "author-release"
    );
    assert_eq!(state.try_get::<i64>("", "generation").unwrap(), 1);
    assert_eq!(count(&db, "content_audit").await, 2);
    activation_failure(
        withdraw(url.as_str(), &root, "author-reviewed", "1", "0"),
        "expected-generation: content generation changed: expected 0, current 1",
    );
    activation_failure(
        withdraw(url.as_str(), &root, "author-reviewed", "0", "1"),
        "revision: expected revision in 1..2147483647",
    );
    activation_failure(
        withdraw(url.as_str(), &root, "missing-lesson", "1", "1"),
        "lesson-id/revision: lesson revision does not exist",
    );
    assert_eq!(count(&db, "content_withdrawals").await, 0);
    assert_eq!(count(&db, "content_audit").await, 2);
    assert_eq!(
        db.query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT generation FROM content_state WHERE singleton"
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "generation")
        .unwrap(),
        1
    );
    assert_eq!(db.query_one_raw(Statement::from_string(DbBackend::Postgres, "SELECT count(*) AS n FROM lesson_revisions WHERE lesson_id='author-reviewed' AND published")).await.unwrap().unwrap().try_get::<i64>("", "n").unwrap(), 1);
    let withdrawn = withdraw(url.as_str(), &root, "author-reviewed", "1", "1");
    assert!(
        withdrawn.status.success(),
        "{}",
        String::from_utf8_lossy(&withdrawn.stderr)
    );
    activation_failure(
        withdraw(url.as_str(), &root, "author-reviewed", "1", "2"),
        "lesson-id/revision: lesson revision was already withdrawn",
    );
    assert_eq!(count(&db, "content_withdrawals").await, 1);
    assert_eq!(
        db.query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT generation FROM content_state WHERE singleton"
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "generation")
        .unwrap(),
        2
    );
    assert_eq!(db.query_one_raw(Statement::from_string(DbBackend::Postgres, "SELECT count(*) AS n FROM lesson_revisions WHERE lesson_id='author-reviewed' AND published")).await.unwrap().unwrap().try_get::<i64>("", "n").unwrap(), 0);
    activation_failure(
        activate(url.as_str(), &root, "author-release", "2"),
        "release-id: release contains a withdrawn lesson revision",
    );
    valid["id"] = json!("withdrawn-release");
    let text = write(&release_file, &valid);
    let marker = text.find("\"revision\": 1").unwrap() + "\"revision\": ".len();
    located(
        invoke(url.as_str(), &root, "release-stage", &release_file),
        &release_file,
        &text,
        "/levels/0/units/0/lessons/0/revision",
        marker,
        "was withdrawn",
    );
    assert_eq!(count(&db, "content_releases").await, 1);
    assert_eq!(count(&db, "content_audit").await, 3);
    brioche_migration::Migrator::down(&db, None).await.unwrap();
    drop(db);
    admin
        .execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .unwrap();
    assert!(
        root.canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap())
    );
    std::fs::remove_dir_all(root).unwrap();
}
