use std::{
    path::Path,
    process::{Command, Output},
};

fn random_id() -> u128 {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).unwrap();
    u128::from_le_bytes(bytes)
}

fn run(command: &str, path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_chef-server"))
        .args([command, path.to_str().unwrap()])
        // A deliberately unusable connection proves author checks never connect.
        .env("DATABASE_URL", "postgres://invalid@127.0.0.1:1/unavailable")
        .env("CONTENT_MODE", "fixture")
        .env("APP_ENV", "production")
        .output()
        .unwrap()
}

#[test]
fn media_bundle_checks_inspect_files_offline_and_locate_mismatches() {
    use serde_json::json;
    use sha2::{Digest, Sha256};
    let root = std::env::temp_dir().join(format!("brioche-bundle-check-{}", random_id()));
    std::fs::create_dir(&root).unwrap();
    let image = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96"><circle cx="48" cy="48" r="40"/></svg>"#;
    std::fs::write(root.join("image.svg"), image).unwrap();
    let mut wave = Vec::new();
    wave.extend_from_slice(b"RIFF");
    wave.extend_from_slice(&1636u32.to_le_bytes());
    wave.extend_from_slice(b"WAVEfmt ");
    wave.extend_from_slice(&16u32.to_le_bytes());
    for n in [1u16, 1] {
        wave.extend_from_slice(&n.to_le_bytes());
    }
    for n in [8000u32, 16000] {
        wave.extend_from_slice(&n.to_le_bytes());
    }
    for n in [2u16, 16] {
        wave.extend_from_slice(&n.to_le_bytes());
    }
    wave.extend_from_slice(b"data");
    wave.extend_from_slice(&1600u32.to_le_bytes());
    wave.resize(1644, 0);
    std::fs::write(root.join("sample.wav"), &wave).unwrap();
    let common = json!({"assetId":"sample", "revision":1, "status":"ready", "rightsConfirmed":true, "source":"synthetic test", "license":"LicenseRef-TestOnly", "creator":"protocol-test", "creditZh":"仅测试·中文"});
    let mut visual = common.clone();
    visual.as_object_mut().unwrap().extend(json!({"sha256":format!("{:x}", Sha256::digest(image)), "mimeType":"image/svg+xml", "width":96, "height":96, "altZh":"仅测试", "file":"image.svg"}).as_object().unwrap().clone());
    let mut audio = common;
    audio.as_object_mut().unwrap().extend(json!({"sha256":format!("{:x}", Sha256::digest(&wave)), "mimeType":"audio/wav", "durationMs":100, "file":"sample.wav"}).as_object().unwrap().clone());
    let path = root.join("bundle.json");
    let store = root.join("untouched-store");
    for (command, baseline, mutations) in [
        (
            "assets-check",
            json!({"schemaVersion":"1.0", "assets":[visual], "characters":[]}),
            vec![
                ("/assets/0/sha256", json!("0".repeat(64))),
                ("/assets/0/width", json!(95)),
                ("/assets/0/file", json!("missing.svg")),
            ],
        ),
        (
            "audio-bundle-check",
            json!({"schemaVersion":"1.0", "assets":[audio]}),
            vec![
                ("/assets/0/sha256", json!("0".repeat(64))),
                ("/assets/0/durationMs", json!(101)),
                ("/assets/0/file", json!("missing.wav")),
            ],
        ),
    ] {
        for mutation in std::iter::once(None).chain(mutations.into_iter().map(Some)) {
            let mut source = baseline.clone();
            if let Some((pointer, value)) = &mutation {
                *source.pointer_mut(pointer).unwrap() = value.clone();
            }
            let text = serde_json::to_string_pretty(&source)
                .unwrap()
                .replace('\n', "\r\n");
            std::fs::write(&path, &text).unwrap();
            let output = Command::new(env!("CARGO_BIN_EXE_chef-server"))
                .args([command, path.to_str().unwrap(), root.to_str().unwrap()])
                .env("DATABASE_URL", "postgres://invalid@127.0.0.1:1/unavailable")
                .env("MEDIA_ROOT", &store)
                .env("APP_ENV", "production")
                .env("CONTENT_MODE", "fixture")
                .output()
                .unwrap();
            let error = String::from_utf8_lossy(&output.stderr);
            if let Some((pointer, value)) = mutation {
                let key = format!("\"{}\": ", pointer.rsplit('/').next().unwrap());
                let marker = format!("{key}{}", serde_json::to_string(&value).unwrap());
                let offset = text.find(&marker).unwrap() + key.len();
                let before = &text[..offset];
                let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
                let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
                assert!(!output.status.success());
                assert!(
                    error.contains(&format!("{}:{line}:{column}: {pointer}:", path.display())),
                    "{error}"
                );
                assert!(output.stdout.is_empty());
            } else {
                assert!(output.status.success(), "{error}");
                assert!(
                    String::from_utf8_lossy(&output.stdout).contains("not registered or published")
                );
            }
            assert!(!error.contains("database connection"), "{error}");
            assert!(!store.exists());
        }
    }
    assert_eq!(std::fs::read(root.join("image.svg")).unwrap(), image);
    assert_eq!(std::fs::read(root.join("sample.wav")).unwrap(), wave);
    for file in ["image.svg", "sample.wav", "bundle.json"] {
        std::fs::remove_file(root.join(file)).unwrap();
    }
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn bundled_nonsquare_avatar_is_located_before_files_or_database() {
    use serde_json::json;
    let mut source: serde_json::Value =
        serde_json::from_str(include_str!("../../../docs/examples/asset-bundle.json")).unwrap();
    for asset in source["assets"].as_array_mut().unwrap() {
        asset["status"] = json!("ready");
        asset["rightsConfirmed"] = json!(true);
        asset["license"] = json!("LicenseRef-TestOnly");
    }
    let avatar = source["assets"][0]["assetId"].clone();
    assert_ne!(source["assets"][0]["width"], source["assets"][0]["height"]);
    source["characters"][0]["snapshot"]["avatarId"] = avatar.clone();
    let path = std::env::temp_dir().join(format!("brioche-avatar-preflight-{}.json", random_id()));
    let text = serde_json::to_string_pretty(&source)
        .unwrap()
        .replace('\n', "\r\n");
    std::fs::write(&path, &text).unwrap();
    let key = "\"avatarId\": ";
    let offset = text
        .find(&format!("{key}{}", serde_json::to_string(&avatar).unwrap()))
        .unwrap()
        + key.len();
    let before = &text[..offset];
    let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
    for command in ["assets-import", "assets-check"] {
        let mut invocation = Command::new(env!("CARGO_BIN_EXE_chef-server"));
        invocation.args([command, path.to_str().unwrap(), "missing-source-directory"]);
        if command == "assets-import" {
            invocation.arg("protocol-test");
        }
        let output = invocation
            .env("DATABASE_URL", "postgres://invalid@127.0.0.1:1/unavailable")
            .env("APP_ENV", "production")
            .env("CONTENT_MODE", "database")
            .output()
            .unwrap();
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success());
        assert!(
            error.contains(&format!(
                "{}:{line}:{column}: /characters/0/snapshot/avatarId:",
                path.display()
            )),
            "{error}"
        );
        assert!(error.contains("avatar must be square"), "{error}");
        assert!(!error.contains("database connection"), "{error}");
        assert!(!error.contains("source directory unavailable"), "{error}");
        assert!(output.stdout.is_empty());
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn full_release_checks_all_local_sources_without_database_or_publication() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum/docs/content");
    let output = Command::new(env!("CARGO_BIN_EXE_chef-server"))
        .arg("check-release")
        .arg(root.join("a2/catalog.full.release.json"))
        .arg("--sources")
        .arg(root.join("a1"))
        .arg(root.join("a2"))
        .arg(root.join("../examples/a1-bakery.lesson.json"))
        .env("DATABASE_URL", "postgres://invalid@127.0.0.1:1/unavailable")
        .env("APP_ENV", "production")
        .env("CONTENT_MODE", "database")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(
        report.contains("Checked 48 referenced lesson sources"),
        "{report}"
    );
    assert!(report.contains("Publication still requires"), "{report}");
    assert!(!report.contains("correctOptionId"), "{report}");
}

#[test]
fn local_release_checks_reject_missing_ambiguous_mismatched_and_invalid_sources() {
    let root = std::env::temp_dir().join(format!("brioche-release-sources-{}", random_id()));
    let first = root.join("one");
    let second = root.join("two");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir(&second).unwrap();
    let manifest_path = root.join("release.json");
    std::fs::write(
        &manifest_path,
        include_str!("../../../docs/examples/catalog.release.json"),
    )
    .unwrap();
    let original = chef_engine::development_source().unwrap();
    let filename = format!("{}.lesson.json", original["id"].as_str().unwrap());
    let source_path = first.join(&filename);
    let run_pack = || {
        Command::new(env!("CARGO_BIN_EXE_chef-server"))
            .arg("check-release")
            .arg(&manifest_path)
            .arg("--sources")
            .arg(&first)
            .arg(&second)
            .env("DATABASE_URL", "postgres://invalid@127.0.0.1:1/unavailable")
            .env("CONTENT_MODE", "database")
            .env("APP_ENV", "production")
            .output()
            .unwrap()
    };
    let output = run_pack();
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        error.contains("/levels/0/units/0/lessons/0/lessonId: no local lesson source"),
        "{error}"
    );
    assert!(output.stdout.is_empty());
    for (pointer, invalid, reason) in [
        (
            "/id",
            serde_json::json!("different-local-id"),
            "lesson ID does not match",
        ),
        (
            "/revision",
            serde_json::json!(2),
            "lesson revision does not match",
        ),
        (
            "/levelId",
            serde_json::json!("a2"),
            "lesson level does not match",
        ),
        (
            "/unitId",
            serde_json::json!("other-local-unit"),
            "lesson unit does not match",
        ),
        (
            "/blocks/7/options/0/text",
            serde_json::json!(313161),
            "expected a string",
        ),
        (
            "/serverOnly/grading/exercise-intention/correctOptionId",
            serde_json::json!("unknown-local-option"),
            "correctOptionId",
        ),
    ] {
        let mut source = original.clone();
        *source.pointer_mut(pointer).unwrap() = invalid.clone();
        let text = serde_json::to_string_pretty(&source)
            .unwrap()
            .replace('\n', "\r\n");
        let token = serde_json::to_string(&invalid).unwrap();
        // Locate the revision field rather than an unrelated numeric value.
        let offset = if pointer == "/revision" {
            text.rfind("\"revision\": 2").unwrap() + "\"revision\": ".len()
        } else {
            text.find(&token).unwrap()
        };
        let before = &text[..offset];
        let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
        let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
        std::fs::write(&source_path, text).unwrap();
        let output = run_pack();
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success());
        assert!(
            error.contains(&format!(
                "{}:{line}:{column}: {pointer}:",
                source_path.display()
            )),
            "{error}"
        );
        assert!(error.contains(reason), "{error}");
        assert!(!error.contains("database connection"), "{error}");
        assert!(output.stdout.is_empty());
    }
    let bytes = serde_json::to_vec(&original).unwrap();
    std::fs::write(&source_path, &bytes).unwrap();
    let duplicate = second.join(filename);
    std::fs::write(&duplicate, bytes).unwrap();
    let output = run_pack();
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(error.contains("ambiguous local lesson sources"), "{error}");
    assert!(
        error.contains("/levels/0/units/0/lessons/0/lessonId:"),
        "{error}"
    );
    std::fs::remove_file(duplicate).unwrap();
    assert!(run_pack().status.success());
    let explicit = Command::new(env!("CARGO_BIN_EXE_chef-server"))
        .arg("check-release")
        .arg(&manifest_path)
        .arg("--sources")
        .arg(&source_path)
        .arg(&first)
        .env("DATABASE_URL", "postgres://invalid@127.0.0.1:1/unavailable")
        .output()
        .unwrap();
    assert!(
        explicit.status.success(),
        "{}",
        String::from_utf8_lossy(&explicit.stderr)
    );
    let mut unrelated = original;
    unrelated["id"] = serde_json::json!("unreferenced-local-source");
    std::fs::write(&source_path, serde_json::to_vec(&unrelated).unwrap()).unwrap();
    let explicit = Command::new(env!("CARGO_BIN_EXE_chef-server"))
        .arg("check-release")
        .arg(&manifest_path)
        .arg("--sources")
        .arg(&source_path)
        .env("DATABASE_URL", "postgres://invalid@127.0.0.1:1/unavailable")
        .output()
        .unwrap();
    assert!(!explicit.status.success());
    assert!(
        String::from_utf8_lossy(&explicit.stderr)
            .contains("/id: explicitly selected source is not referenced")
    );
    std::fs::remove_file(source_path).unwrap();
    std::fs::remove_file(manifest_path).unwrap();
    std::fs::remove_dir(first).unwrap();
    std::fs::remove_dir(second).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn checks_drafts_without_database_and_does_not_claim_publication() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples");
    for (command, file) in [
        ("check", "a1-bakery.lesson.json"),
        ("check-release", "catalog.release.json"),
    ] {
        let output = run(command, &root.join(file));
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("Publication still requires"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("correctOptionId"));
    }
}

#[test]
fn exercise_step_semantics_are_located_before_check_or_import_connects() {
    let path = std::env::temp_dir().join(format!("brioche-exercise-step-{}.json", random_id()));
    let original = chef_engine::development_source().unwrap();
    let practice = original["steps"]
        .as_array()
        .unwrap()
        .iter()
        .position(|step| step["kind"] == "practice")
        .unwrap();
    for misplaced in [true, false] {
        let mut source = original.clone();
        let (pointer, search_key, value, message) = if misplaced {
            source["steps"][practice]["kind"] = serde_json::json!("read");
            (
                format!("/steps/{practice}/blockIds/0"),
                "\"steps\"",
                source["steps"][practice]["blockIds"][0].clone(),
                "exercise requires a practice step",
            )
        } else {
            let id = source["steps"][practice]["id"].clone();
            source["completion"]["requiredStepIds"]
                .as_array_mut()
                .unwrap()
                .retain(|value| *value != id);
            (
                "/completion/requiredExerciseIds/0".into(),
                "\"requiredExerciseIds\"",
                source["completion"]["requiredExerciseIds"][0].clone(),
                "required practice step",
            )
        };
        let text = serde_json::to_string_pretty(&source)
            .unwrap()
            .replace('\n', "\r\n");
        let start = text.find(search_key).unwrap();
        let token = serde_json::to_string(&value).unwrap();
        let offset = start + text[start..].find(&token).unwrap();
        let before = &text[..offset];
        let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
        let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
        std::fs::write(&path, text).unwrap();
        for command in ["check", "import"] {
            let output = Command::new(env!("CARGO_BIN_EXE_chef-server"))
                .args([command, path.to_str().unwrap()])
                .env("DATABASE_URL", "postgres://invalid@127.0.0.1:1/unavailable")
                .env("CONTENT_MODE", "database")
                .env("APP_ENV", "production")
                .output()
                .unwrap();
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success());
            assert!(
                error.contains(&format!("{}:{line}:{column}: {pointer}:", path.display())),
                "{error}"
            );
            assert!(error.contains(message), "{error}");
            assert!(!error.contains("database connection"), "{error}");
            assert!(output.stdout.is_empty());
        }
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn duplicate_choice_text_is_located_before_check_or_import_connects() {
    let path = std::env::temp_dir().join(format!("brioche-choice-label-{}.json", random_id()));
    let original = chef_engine::development_source().unwrap();
    let index = original["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .position(|block| block.get("options").is_some())
        .unwrap();
    let pointer = format!("/blocks/{index}/options/1/text");
    for (first, second) in [
        ("une baguette", "une baguette"),
        ("une baguette", "  une\u{00a0}baguette  "),
        ("café", "cafe\u{301}"),
        ("s'il vous plaît", "s’il vous plaît"),
    ] {
        let mut source = original.clone();
        source["blocks"][index]["options"][0]["text"] = serde_json::json!(first);
        *source.pointer_mut(&pointer).unwrap() = serde_json::json!(second);
        let text = serde_json::to_string_pretty(&source)
            .unwrap()
            .replace('\n', "\r\n");
        // Find the second option's text field, even when both literal values match.
        let key = serde_json::to_string(
            source["blocks"][index]["options"][1]["id"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        let option_offset = text.find(&format!("\"id\": {key}")).unwrap();
        let token = serde_json::to_string(second).unwrap();
        let offset = option_offset + text[option_offset..].find(&token).unwrap();
        let before = &text[..offset];
        let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
        let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
        std::fs::write(&path, text).unwrap();
        for command in ["check", "import"] {
            let output = Command::new(env!("CARGO_BIN_EXE_chef-server"))
                .args([command, path.to_str().unwrap()])
                .env("DATABASE_URL", "postgres://invalid@127.0.0.1:1/unavailable")
                .env("CONTENT_MODE", "database")
                .env("APP_ENV", "production")
                .output()
                .unwrap();
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success());
            assert!(
                error.contains(&format!("{}:{line}:{column}: {pointer}:", path.display())),
                "{error}"
            );
            assert!(error.contains("duplicate choice text"), "{error}");
            assert!(!error.contains("database connection"), "{error}");
            assert!(output.stdout.is_empty());
        }
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn unrepresentable_text_answers_are_located_before_check_or_import_connects() {
    let path = std::env::temp_dir().join(format!("brioche-answer-limit-{}.json", random_id()));
    let original = chef_engine::development_source().unwrap();
    let pointer = "/serverOnly/grading/exercise-article/accepted/0";
    for invalid in ["a".repeat(1025), "😀".repeat(513), "e\u{301}".repeat(1025)] {
        let mut source = original.clone();
        *source.pointer_mut(pointer).unwrap() = serde_json::json!(invalid);
        source["editorial"]["note"] = serde_json::json!("中文限额定位");
        let text = serde_json::to_string_pretty(&source)
            .unwrap()
            .replace('\n', "\r\n");
        let token = serde_json::to_string(&invalid).unwrap();
        let offset = text.find(&token).unwrap();
        let before = &text[..offset];
        let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
        let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
        std::fs::write(&path, text).unwrap();
        for command in ["check", "import"] {
            let output = Command::new(env!("CARGO_BIN_EXE_chef-server"))
                .args([command, path.to_str().unwrap()])
                .env("DATABASE_URL", "postgres://invalid@127.0.0.1:1/unavailable")
                .env("CONTENT_MODE", "database")
                .env("APP_ENV", "production")
                .output()
                .unwrap();
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success());
            assert!(
                error.contains(&format!("{}:{line}:{column}: {pointer}:", path.display())),
                "{error}"
            );
            assert!(error.contains("1024 UTF-16 code units"), "{error}");
            assert!(!error.contains("database connection"), "{error}");
            assert!(output.stdout.is_empty());
        }
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn checks_visual_sources_without_database_or_registration() {
    use sha2::{Digest, Sha256};
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum/docs/content/a1/assets");
    for name in ["first-conversations.svg", "city-morning.svg"] {
        let path = root.join(name);
        let output = Command::new(env!("CARGO_BIN_EXE_chef-server"))
            .args(["asset-check", path.to_str().unwrap(), "image/svg+xml"])
            .env("DATABASE_URL", "postgres://invalid@127.0.0.1:1/unavailable")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let info: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(info["width"], 640);
        assert_eq!(info["height"], 470);
        assert_eq!(info["byteLength"], bytes.len());
        assert_eq!(info["sha256"], format!("{:x}", Sha256::digest(&bytes)));
        for mime in ["image/png", "text/html"] {
            let rejected = Command::new(env!("CARGO_BIN_EXE_chef-server"))
                .args(["asset-check", path.to_str().unwrap(), mime])
                .env("DATABASE_URL", "postgres://invalid@127.0.0.1:1/unavailable")
                .output()
                .unwrap();
            assert!(!rejected.status.success());
            assert!(rejected.stdout.is_empty());
        }
    }
}

#[test]
fn rejects_bad_grading_references_and_json_before_any_database_work() {
    let path = std::env::temp_dir().join(format!("brioche-check-{}.json", random_id()));
    let original = chef_engine::development_source().unwrap();
    let mut source = original.clone();
    source["serverOnly"]["grading"] = serde_json::json!({});
    std::fs::write(&path, serde_json::to_vec(&source).unwrap()).unwrap();
    let output = run("check", &path);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("serverOnly.grading"));
    source = original.clone();
    source["assetRefs"] =
        serde_json::json!([{"assetId":"scene", "revision":1},{"assetId":"scene", "revision":2}]);
    std::fs::write(&path, serde_json::to_vec(&source).unwrap()).unwrap();
    let output = run("check", &path);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("assetRefs/1"));
    source = original.clone();
    source["audioRefs"] =
        serde_json::json!([{"assetId":"audio","revision":1},{"assetId":"audio","revision":2}]);
    std::fs::write(&path, serde_json::to_vec(&source).unwrap()).unwrap();
    let output = run("check", &path);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("audioRefs/1"));
    source = original;
    source["revision"] = serde_json::json!("invalid");
    std::fs::write(&path, serde_json::to_vec(&source).unwrap()).unwrap();
    let output = run("check", &path);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("revision"));
    std::fs::write(&path, br#"{"id":"a","id":"b"}"#).unwrap();
    let output = run("check", &path);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("duplicate JSON field"));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn release_check_rejects_invalid_manifest_and_extra_arguments() {
    let path = std::env::temp_dir().join(format!("brioche-check-{}.json", random_id()));
    std::fs::write(
        &path,
        br#"{"id":"release","schemaVersion":"unknown","levels":[]}"#,
    )
    .unwrap();
    assert!(!run("check-release", &path).status.success());
    std::fs::remove_file(path).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_chef-server"))
        .args(["check", "one.json", "--publish"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("usage:"));
}

#[test]
fn audio_check_decodes_without_database_and_reports_actual_duration() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/audio/synthetic.mp3");
    let output = Command::new(env!("CARGO_BIN_EXE_chef-server"))
        .args(["audio-check", file.to_str().unwrap(), "audio/mpeg"])
        .env("DATABASE_URL", "postgres://invalid@127.0.0.1:1/unavailable")
        .env("CONTENT_MODE", "fixture")
        .env("APP_ENV", "production")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let info: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(info["durationMs"], 1000);
    assert_eq!(info["channels"], 1);
    assert_eq!(info["sha256"].as_str().unwrap().len(), 64);
    let output = Command::new(env!("CARGO_BIN_EXE_chef-server"))
        .args(["audio-check", file.to_str().unwrap(), "audio/wav"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("not RIFF WAVE"));
    assert!(!run("audio-check", &file).status.success());
}

#[test]
fn semantic_and_projected_type_errors_point_into_original_author_source() {
    let path = std::env::temp_dir().join(format!("brioche-locations-{}.json", random_id()));
    let original = chef_engine::development_source().unwrap();
    let dialogue_index = original["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .position(|block| block["type"] == "dialogue")
        .unwrap();
    for (pointer, marker) in [
        (
            format!("/blocks/{dialogue_index}/turns/0/segments/0/vocabularyId"),
            "missing-vocabulary",
        ),
        ("/steps/0/blockIds/0".into(), "missing-block"),
        ("/reviewItemIds/0".into(), "missing-review"),
        ("/completion/requiredStepIds/0".into(), "missing-step"),
        ("/revision".into(), "not-a-revision"),
        ("/editorial/status".into(), "unknown-status"),
        ("/title/fr".into(), "\u{00a0}"),
        ("/cast/0/speechLocale".into(), "invalid-speech-locale"),
        ("/steps/0/kind".into(), "unsupported-flow-step"),
        ("/steps/0/titleZh".into(), "\u{00a0}"),
        (
            format!("/blocks/{dialogue_index}/speakers/0/displayName"),
            "unpinned-speaker-name",
        ),
        (
            format!("/blocks/{dialogue_index}/speakers/0/avatarId"),
            "unpinned-speaker-avatar",
        ),
        (
            "/blocks/3/targets/0/blockId".into(),
            "missing-reading-block",
        ),
        (
            "/blocks/3/targets/0/entryId".into(),
            "missing-reading-entry",
        ),
        (
            "/blocks/3/targets/0/segmentId".into(),
            "missing-reading-segment",
        ),
        ("/blocks/7/options/1/text".into(), "\u{00a0}"),
        ("/blocks/8/templateFr".into(), "template-without-blank"),
        ("/blocks/9/tokens/1/text".into(), "\u{00a0}"),
    ] {
        let mut source = original.clone();
        *source.pointer_mut(&pointer).unwrap() = serde_json::json!(marker);
        let text = serde_json::to_string_pretty(&source).unwrap();
        std::fs::write(&path, &text).unwrap();
        let offset = text.find(&format!("\"{marker}\"")).unwrap();
        let before = &text[..offset];
        let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
        let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
        let output = run("check", &path);
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success());
        assert!(
            error.contains(&format!("{}:{line}:{column}: {pointer}: ", path.display())),
            "{error}"
        );
        assert!(!error.contains("database connection"));
    }
    let mut source = original;
    source["audioRefs"] =
        serde_json::json!([{"assetId":"test","revision":"invalid-audio-revision"}]);
    let text = serde_json::to_string_pretty(&source).unwrap();
    std::fs::write(&path, &text).unwrap();
    let output = run("check", &path);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("/audioRefs/0/revision:"));
    source.as_object_mut().unwrap().remove("audioRefs");
    let sha = "a".repeat(64);
    source["audio"] = serde_json::json!([{
        "assetId":"test-recording", "revision":1, "sha256":sha,
        "mimeType":"audio/wav", "durationMs":1000, "creditZh":"Protocol test",
        "url":format!("/api/audio/{sha}.wav")
    }]);
    source["audioTracks"] = serde_json::json!([{
        "blockId":source["blocks"][dialogue_index]["id"], "assetId":"test-recording",
        "cues":[{"entryId":source["blocks"][dialogue_index]["turns"][0]["id"],"startMs":0,"endMs":1500}]
    }]);
    let text = serde_json::to_string_pretty(&source).unwrap();
    std::fs::write(&path, &text).unwrap();
    let end_field = text.find("\"endMs\": 1500").unwrap();
    let before = &text[..end_field + "\"endMs\": ".len()];
    let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
    let output = run("check", &path);
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        error.contains(&format!(
            "{}:{line}:{column}: /audioTracks/0/cues/0/endMs:",
            path.display()
        )),
        "{error}"
    );
    assert!(error.contains("interval outside recording duration"));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn private_rules_and_release_semantics_report_exact_source_fields() {
    let path = std::env::temp_dir().join(format!("brioche-author-rules-{}.json", random_id()));
    let lesson = chef_engine::development_source().unwrap();
    let release: serde_json::Value =
        serde_json::from_str(include_str!("../../../docs/examples/catalog.release.json")).unwrap();
    for (command, original, pointer, marker) in [
        (
            "check",
            &lesson,
            "/serverOnly/grading/exercise-intention/correctOptionId",
            "private-unknown-option",
        ),
        (
            "check",
            &lesson,
            "/serverOnly/grading/exercise-order/correctTokenIds/1",
            "private-unknown-token",
        ),
        (
            "import",
            &lesson,
            "/serverOnly/grading/exercise-intention/correctOptionId",
            "private-unknown-option",
        ),
        (
            "import",
            &lesson,
            "/serverOnly/grading/exercise-order/correctTokenIds/1",
            "private-unknown-token",
        ),
        (
            "import",
            &lesson,
            "/serverOnly/grading/exercise-intention/feedbackZh",
            "\u{00a0}",
        ),
        (
            "check",
            &lesson,
            "/serverOnly/grading/exercise-article/accepted/0",
            "\u{00a0}",
        ),
        (
            "check-release",
            &release,
            "/schemaVersion",
            "unsupported-release-version",
        ),
        (
            "check-release",
            &release,
            "/levels/0/units/0/lessons/0/lessonId",
            "invalid lesson reference",
        ),
        (
            "check-release",
            &release,
            "/levels/0/units/0/lessons/0/revision",
            "invalid-release-revision",
        ),
    ] {
        let mut source = original.clone();
        *source.pointer_mut(pointer).unwrap() = serde_json::json!(marker);
        let text = serde_json::to_string_pretty(&source).unwrap();
        std::fs::write(&path, &text).unwrap();
        let token = serde_json::to_string(marker).unwrap();
        let offset = text.find(&token).unwrap();
        let before = &text[..offset];
        let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
        let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
        let output = run(command, &path);
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success());
        assert!(
            error.contains(&format!("{}:{line}:{column}: {pointer}:", path.display())),
            "{error}"
        );
        if command != "check-release" {
            assert!(!error.contains(marker));
        }
        assert!(!error.contains("database connection"));
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn import_public_types_are_located_before_database_or_media_hydration() {
    let path = std::env::temp_dir().join(format!("brioche-public-preflight-{}.json", random_id()));
    let original = chef_engine::development_source().unwrap();
    for (pointer, invalid) in [
        ("/title/fr", serde_json::json!(314159)),
        ("/revision", serde_json::json!("public-invalid-revision")),
        (
            "/cast/0/revision",
            serde_json::json!("public-invalid-character"),
        ),
        (
            "/knowledge/vocabulary/0/meaningZh",
            serde_json::json!(271828),
        ),
        ("/blocks", serde_json::json!("public-invalid-blocks")),
        (
            "/blocks/1/turns/0/segments/0/text",
            serde_json::json!(314160),
        ),
        (
            "/blocks/1/speakers/0/displayName",
            serde_json::json!(314161),
        ),
        ("/blocks/7/options/0/text", serde_json::json!(314162)),
        ("/blocks/9/tokens/0/text", serde_json::json!(314163)),
        ("/steps", serde_json::json!("public-invalid-steps")),
    ] {
        let mut source = original.clone();
        *source.pointer_mut(pointer).unwrap() = invalid.clone();
        source["editorial"]["note"] = serde_json::json!("中文原文件位置");
        let text = serde_json::to_string_pretty(&source)
            .unwrap()
            .replace('\n', "\r\n");
        let token = serde_json::to_string(&invalid).unwrap();
        let offset = text.find(&token).unwrap();
        let before = &text[..offset];
        let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
        let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
        std::fs::write(&path, text).unwrap();
        for command in ["check", "import"] {
            let output = Command::new(env!("CARGO_BIN_EXE_chef-server"))
                .args([command, path.to_str().unwrap()])
                .env("DATABASE_URL", "postgres://invalid@127.0.0.1:1/unavailable")
                .env("CONTENT_MODE", "database")
                .env("APP_ENV", "production")
                .output()
                .unwrap();
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success());
            assert!(
                error.contains(&format!("{}:{line}:{column}: {pointer}:", path.display())),
                "{error}"
            );
            assert!(!error.contains("database connection"), "{error}");
            assert!(output.stdout.is_empty());
        }
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn import_private_rule_types_fail_before_connecting_with_exact_positions() {
    let path = std::env::temp_dir().join(format!("brioche-rule-preflight-{}.json", random_id()));
    let original = chef_engine::development_source().unwrap();
    for (pointer, invalid) in [
        (
            "/serverOnly/grading/exercise-intention/correctOptionId",
            serde_json::json!(123),
        ),
        (
            "/serverOnly/grading/exercise-article/caseSensitive",
            serde_json::json!("private-invalid-boolean"),
        ),
        (
            "/serverOnly/grading/exercise-article/accepted/0",
            serde_json::json!(false),
        ),
        (
            "/serverOnly/grading/exercise-order/correctTokenIds/1",
            serde_json::json!(42),
        ),
    ] {
        let mut source = original.clone();
        *source.pointer_mut(pointer).unwrap() = invalid.clone();
        // Keep Unicode and CRLF in the original input to verify source positions.
        source["editorial"]["note"] = serde_json::json!("仅测试字段定位");
        let text = serde_json::to_string_pretty(&source)
            .unwrap()
            .replace('\n', "\r\n");
        let token = serde_json::to_string(&invalid).unwrap();
        let offset = text.find(&token).unwrap();
        let before = &text[..offset];
        let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
        let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
        std::fs::write(&path, text).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_chef-server"))
            .args(["import", path.to_str().unwrap()])
            .env("DATABASE_URL", "postgres://invalid@127.0.0.1:1/unavailable")
            .env("CONTENT_MODE", "database")
            .env("APP_ENV", "production")
            .output()
            .unwrap();
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success());
        assert!(
            error.contains(&format!("{}:{line}:{column}: {pointer}:", path.display())),
            "{error}"
        );
        assert!(!error.contains("database connection"), "{error}");
        assert!(output.stdout.is_empty());
    }
    // A valid source passes preflight and still requires the real database import path.
    std::fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_chef-server"))
        .args(["import", path.to_str().unwrap()])
        .env("DATABASE_URL", "postgres://invalid@127.0.0.1:1/unavailable")
        .env("CONTENT_MODE", "database")
        .env("APP_ENV", "production")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("database connection failed"));
    assert!(output.stdout.is_empty());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn import_and_stage_preflight_locate_invalid_source_before_database_connection() {
    let path = std::env::temp_dir().join(format!("brioche-preflight-{}.json", random_id()));
    for (command, source, pointer) in [
        (
            "import",
            serde_json::json!({"assetRefs":[{"assetId":"scene","revision":"bad"}]}),
            "/assetRefs/0/revision",
        ),
        (
            "import",
            serde_json::json!({"editorial":{"status":"bad","note":"test"}}),
            "/editorial/status",
        ),
        (
            "release-stage",
            serde_json::json!({"id":"bad id","schemaVersion":"1.0","levels":[]}),
            "/id",
        ),
        (
            "release-stage",
            serde_json::json!({"id":"release","schemaVersion":"1.0","levels":[{"id":"a1","label":"A1","units":[{"id":"unit","titleZh":"Unit","lessons":[{"lessonId":"lesson","revision":0}]}]}]}),
            "/levels/0/units/0/lessons/0/revision",
        ),
    ] {
        std::fs::write(&path, serde_json::to_vec_pretty(&source).unwrap()).unwrap();
        let mut invocation = Command::new(env!("CARGO_BIN_EXE_chef-server"));
        invocation.args([command, path.to_str().unwrap()]);
        if command == "release-stage" {
            invocation.args(["test-actor", "test-reason"]);
        }
        let output = invocation
            .env("DATABASE_URL", "postgres://invalid@127.0.0.1:1/unavailable")
            .env("CONTENT_MODE", "database")
            .output()
            .unwrap();
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success());
        assert!(error.contains(&format!("{pointer}:")), "{error}");
        assert!(error.contains(path.to_str().unwrap()), "{error}");
        assert!(!error.contains("database connection"), "{error}");
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn media_import_preflight_reports_original_fields_without_connecting() {
    use serde_json::json;
    let path = std::env::temp_dir().join(format!("brioche-media-preflight-{}.json", random_id()));
    let mut visual: serde_json::Value =
        serde_json::from_str(include_str!("../../../docs/examples/asset-bundle.json")).unwrap();
    for asset in visual["assets"].as_array_mut().unwrap() {
        asset["status"] = json!("ready");
        asset["rightsConfirmed"] = json!(true);
        asset["license"] = json!("LicenseRef-TestOnly");
    }
    let audio = json!({"schemaVersion":"1.0","assets":[{"assetId":"recording-test","revision":1,"sha256":"0".repeat(64),"mimeType":"audio/wav","durationMs":100,"creditZh":"仅测试·中文","file":"test.wav","status":"ready","source":"synthetic fixture","license":"LicenseRef-TestOnly","creator":"protocol-test","rightsConfirmed":true}]});
    for (command, baseline, pointer, value) in [
        ("assets-import", &visual, "/assets/0/revision", json!("bad")),
        ("assets-import", &visual, "/assets/0/width", json!(0)),
        (
            "assets-import",
            &visual,
            "/assets/0/rightsConfirmed",
            json!(false),
        ),
        (
            "assets-import",
            &visual,
            "/assets/0/file",
            json!("../escape.svg"),
        ),
        (
            "assets-import",
            &visual,
            "/assets/0/mimeType",
            json!("text/html"),
        ),
        (
            "assets-import",
            &visual,
            "/characters/0/avatarRevision",
            json!(0),
        ),
        (
            "assets-import",
            &visual,
            "/characters/0/snapshot/displayName",
            json!(""),
        ),
        ("audio-import", &audio, "/assets/0/durationMs", json!("bad")),
        ("audio-import", &audio, "/assets/0/durationMs", json!(0)),
        ("audio-import", &audio, "/assets/0/sha256", json!("invalid")),
        ("audio-import", &audio, "/assets/0/status", json!("planned")),
        (
            "audio-import",
            &audio,
            "/assets/0/rightsConfirmed",
            json!(false),
        ),
        ("audio-import", &audio, "/assets/0/license", json!("")),
        (
            "audio-import",
            &audio,
            "/assets/0/file",
            json!("../escape.wav"),
        ),
    ] {
        let mut source = baseline.clone();
        *source.pointer_mut(pointer).unwrap() = value.clone();
        let text = serde_json::to_string_pretty(&source)
            .unwrap()
            .replace('\n', "\r\n");
        std::fs::write(&path, &text).unwrap();
        let field = pointer.rsplit('/').next().unwrap();
        let key = format!("\"{field}\": ");
        let marker = format!("{key}{}", serde_json::to_string(&value).unwrap());
        let offset = text.find(&marker).unwrap() + key.len();
        let before = &text[..offset];
        let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
        let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
        let output = Command::new(env!("CARGO_BIN_EXE_chef-server"))
            .args([
                command,
                path.to_str().unwrap(),
                "missing-source-directory",
                "protocol-test",
            ])
            .env("DATABASE_URL", "postgres://invalid@127.0.0.1:1/unavailable")
            .env("CONTENT_MODE", "database")
            .output()
            .unwrap();
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success());
        assert!(
            error.contains(&format!("{}:{line}:{column}: {pointer}:", path.display())),
            "{error}"
        );
        assert!(!error.contains("database connection"), "{error}");
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn recording_semantics_locate_original_values_before_database_work() {
    use serde_json::json;
    let path =
        std::env::temp_dir().join(format!("brioche-recording-semantics-{}.json", random_id()));
    let mut original = chef_engine::development_source().unwrap();
    let turns = original["blocks"][1]["turns"].as_array().unwrap();
    let mut cues: Vec<_> = turns
        .iter()
        .enumerate()
        .map(|(index, turn)| {
            json!({
                "entryId": turn["id"], "startMs": index * 1000, "endMs": (index + 1) * 1000
            })
        })
        .collect();
    let child_index = cues.len();
    cues.push(json!({"entryId": turns[0]["id"], "segmentId": turns[0]["segments"][0]["id"], "startMs":100,"endMs":600}));
    original["audio"] = json!([{"assetId":"audio-test","revision":1,"durationMs":30000,
        "sha256":"a".repeat(64),"mimeType":"audio/mpeg","creditZh":"中文·synthetic fixture only",
        "url":format!("/api/audio/{}.mp3", "a".repeat(64))}]);
    original["audioTracks"] =
        json!([{"blockId":original["blocks"][1]["id"], "assetId":"audio-test","cues":cues}]);
    chef_engine::project_source(original.clone()).unwrap();
    for (pointer, value) in [
        ("/audio/0/assetId".to_owned(), json!("invalid id")),
        ("/audio/0/revision".to_owned(), json!(0)),
        ("/audio/0/durationMs".to_owned(), json!(0)),
        ("/audio/0/sha256".to_owned(), json!("INVALID-HASH")),
        (
            "/audio/0/url".to_owned(),
            json!("https://other.test/recording.mp3"),
        ),
        ("/audio/0/creditZh".to_owned(), json!(" ")),
        ("/audioTracks/0/cues/0/endMs".to_owned(), json!(30001)),
        ("/audioTracks/0/cues/1/startMs".to_owned(), json!(987)),
        (
            format!("/audioTracks/0/cues/{child_index}/endMs"),
            json!(1099),
        ),
    ] {
        let mut source = original.clone();
        *source.pointer_mut(&pointer).unwrap() = value.clone();
        let text = serde_json::to_string_pretty(&source)
            .unwrap()
            .replace('\n', "\r\n");
        std::fs::write(&path, &text).unwrap();
        let field = pointer.rsplit('/').next().unwrap();
        let key = format!("\"{field}\": ");
        let marker = format!("{key}{}", serde_json::to_string(&value).unwrap());
        assert_eq!(text.matches(&marker).count(), 1, "ambiguous test marker");
        let offset = text.find(&marker).unwrap() + key.len();
        let before = &text[..offset];
        let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
        let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
        let output = run("check", &path);
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success());
        assert!(
            error.contains(&format!("{}:{line}:{column}: {pointer}:", path.display())),
            "{error}"
        );
        assert!(output.stdout.is_empty());
        assert!(!error.contains("database connection"), "{error}");
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn registered_reference_semantics_locate_the_exact_field_before_connection() {
    use serde_json::json;
    let path = std::env::temp_dir().join(format!("brioche-reference-fields-{}.json", random_id()));
    for key in ["assetRefs", "audioRefs"] {
        for (field, value, reason) in [
            ("assetId", json!("invalid reference id"), "invalid asset ID"),
            ("revision", json!(0), "expected revision in database range"),
            (
                "revision",
                json!(2147483648u32),
                "expected revision in database range",
            ),
            (
                "assetId",
                json!("reference-field-fixture"),
                "duplicate asset reference",
            ),
        ] {
            let mut source = chef_engine::development_source().unwrap();
            source[key] = json!([
                {"assetId":"reference-field-fixture", "revision":1},
                {"assetId":"other-reference-fixture", "revision":2}
            ]);
            source[key][1][field] = value.clone();
            let text = serde_json::to_string_pretty(&source)
                .unwrap()
                .replace('\n', "\r\n");
            std::fs::write(&path, &text).unwrap();
            let prefix = format!("\"{field}\": ");
            let marker = format!("{prefix}{}", serde_json::to_string(&value).unwrap());
            let offset = text.rfind(&marker).unwrap() + prefix.len();
            let before = &text[..offset];
            let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
            let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
            let pointer = format!("/{key}/1/{field}");
            for command in ["check", "import"] {
                let output = run(command, &path);
                let error = String::from_utf8_lossy(&output.stderr);
                assert!(!output.status.success());
                assert!(
                    error.contains(&format!("{}:{line}:{column}: {pointer}:", path.display())),
                    "{error}"
                );
                assert!(error.contains(reason), "{error}");
                assert!(!error.contains("database connection"), "{error}");
                assert!(output.stdout.is_empty());
            }
        }
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn required_teaching_text_reports_original_source_values_without_database() {
    use serde_json::json;
    let path = std::env::temp_dir().join(format!("brioche-teaching-text-{}.json", random_id()));
    let original = chef_engine::development_source().unwrap();
    let block = |kind: &str| {
        original["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .position(|b| b["type"] == kind)
            .unwrap()
    };
    for (pointer, value) in [
        ("/summaryZh".to_owned(), json!("\u{00a0}")),
        ("/objectivesZh".to_owned(), json!([])),
        ("/estimatedMinutes".to_owned(), json!(0)),
        (
            format!("/blocks/{}/turns/0/translationZh", block("dialogue")),
            json!("\u{00a0}"),
        ),
        (
            format!("/blocks/{}/paragraphs/0/translationZh", block("article")),
            json!("\u{00a0}"),
        ),
        (
            "/knowledge/vocabulary/0/meaningZh".to_owned(),
            json!("\u{00a0}"),
        ),
        ("/knowledge/grammar/0/bodyZh".to_owned(), json!("\u{00a0}")),
        (
            format!("/blocks/{}/bodyZh", block("explanation")),
            json!("\u{00a0}"),
        ),
        (
            format!("/blocks/{}/scopeZh", block("culture")),
            json!("\u{00a0}"),
        ),
        (
            format!("/blocks/{}/takeawaysZh", block("summary")),
            json!([]),
        ),
    ] {
        let mut source = original.clone();
        *source.pointer_mut(&pointer).unwrap() = value.clone();
        let text = serde_json::to_string_pretty(&source)
            .unwrap()
            .replace('\n', "\r\n");
        std::fs::write(&path, &text).unwrap();
        let field = pointer.rsplit('/').next().unwrap();
        let prefix = format!("\"{field}\": ");
        let marker = format!("{prefix}{}", serde_json::to_string(&value).unwrap());
        assert_eq!(text.matches(&marker).count(), 1);
        let offset = text.find(&marker).unwrap() + prefix.len();
        let before = &text[..offset];
        let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
        let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
        for command in ["check", "import"] {
            let output = run(command, &path);
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success());
            assert!(
                error.contains(&format!("{}:{line}:{column}: {pointer}:", path.display())),
                "{command}: {error}"
            );
            assert!(!error.contains("database connection"), "{error}");
            assert!(output.stdout.is_empty());
        }
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn offline_check_rejects_identifiers_that_navigation_cannot_recover() {
    use serde_json::json;
    let path = std::env::temp_dir().join(format!("brioche-stable-ids-{}.json", random_id()));
    let mut original = chef_engine::development_source().unwrap();
    // The draft has no hydrated visual descriptors; this is only a typed protocol fixture.
    original["media"] = json!([{
        "assetId":"visual-id-fixture", "revision":1, "sha256":"a".repeat(64),
        "mimeType":"image/svg+xml", "width":640, "height":470,
        "altZh":"标识协议测试", "creditZh":"仅测试",
        "url":format!("/api/media/{}.svg", "a".repeat(64))
    }]);
    for (pointer, value) in [
        ("/id", json!("route/escape")),
        ("/levelId", json!("niveau français")),
        ("/unitId", json!("a".repeat(101))),
        ("/knowledge/vocabulary/0/id", json!("word:scope")),
        ("/knowledge/grammar/0/id", json!("percent%id")),
        ("/blocks/0/id", json!("block space")),
        ("/steps/0/id", json!("step/slash")),
        ("/cast/0/characterId", json!("character:scope")),
        ("/cast/0/avatarId", json!("avatar space")),
        ("/media/0/assetId", json!("asset/slash")),
    ] {
        let mut source = original.clone();
        *source.pointer_mut(pointer).unwrap() = value.clone();
        let text = serde_json::to_string_pretty(&source)
            .unwrap()
            .replace('\n', "\r\n");
        std::fs::write(&path, &text).unwrap();
        let field = pointer.rsplit('/').next().unwrap();
        let prefix = format!("\"{field}\": ");
        let marker = format!("{prefix}{}", serde_json::to_string(&value).unwrap());
        assert_eq!(text.matches(&marker).count(), 1);
        let offset = text.find(&marker).unwrap() + prefix.len();
        let before = &text[..offset];
        let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
        let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
        let output = run("check", &path);
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success());
        assert!(
            error.contains(&format!("{}:{line}:{column}: {pointer}:", path.display())),
            "{error}"
        );
        assert!(error.contains("1..100 ASCII"), "{error}");
        assert!(!error.contains("database connection"), "{error}");
        assert!(output.stdout.is_empty());
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn character_locale_rejection_is_located_before_any_registration() {
    let path = std::env::temp_dir().join(format!("brioche-character-locale-{}.json", random_id()));
    for locale in [
        "frank",
        "fry",
        "fr",
        "fr-",
        "fr-CA",
        "FR-fr",
        "fr-FR-extra",
        "en-US",
    ] {
        let mut source = chef_engine::development_source().unwrap();
        source["cast"][0]["speechLocale"] = serde_json::json!(locale);
        let text = serde_json::to_string_pretty(&source)
            .unwrap()
            .replace('\n', "\r\n");
        std::fs::write(&path, &text).unwrap();
        let prefix = "\"speechLocale\": ";
        let marker = format!("{prefix}{}", serde_json::to_string(locale).unwrap());
        assert_eq!(text.matches(&marker).count(), 1);
        let offset = text.find(&marker).unwrap() + prefix.len();
        let before = &text[..offset];
        let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
        let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
        let output = run("check", &path);
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success());
        assert!(
            error.contains(&format!(
                "{}:{line}:{column}: /cast/0/speechLocale:",
                path.display()
            )),
            "{error}"
        );
        assert!(error.contains("expected fr-FR"), "{error}");
        assert!(!error.contains("database connection"), "{error}");
        assert!(output.stdout.is_empty());
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn unknown_block_fields_fail_check_and_import_before_database_access() {
    let path = std::env::temp_dir().join(format!("brioche-block-fields-{}.json", random_id()));
    let original = chef_engine::development_source().unwrap();
    for index in 0..original["blocks"].as_array().unwrap().len() {
        let mut source = original.clone();
        source["blocks"][index]["unexpectedAuthorField"] = serde_json::json!("拼错的内容");
        let text = serde_json::to_string_pretty(&source)
            .unwrap()
            .replace('\n', "\r\n");
        let token = serde_json::to_string("拼错的内容").unwrap();
        assert_eq!(text.matches(&token).count(), 1);
        let offset = text.find(&token).unwrap();
        let before = &text[..offset];
        let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
        let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
        std::fs::write(&path, text).unwrap();
        for command in ["check", "import"] {
            let output = run(command, &path);
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success(), "{command} accepted block {index}");
            assert!(
                error.contains(&format!("/blocks/{index}/unexpectedAuthorField:")),
                "{error}"
            );
            assert!(
                error.contains("unknown field `unexpectedAuthorField`"),
                "{error}"
            );
            assert!(
                error.contains(&format!("{}:{line}:{column}:", path.display())),
                "{error}"
            );
            assert!(!error.contains("database connection"), "{error}");
            assert!(output.stdout.is_empty());
        }
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn offline_revision_bounds_locate_the_original_definition() {
    let path = std::env::temp_dir().join(format!("brioche-revision-bounds-{}.json", random_id()));
    let mut original = chef_engine::development_source().unwrap();
    original["media"] = serde_json::json!([{
        "assetId":"revision-fixture", "revision":1, "sha256":"a".repeat(64),
        "mimeType":"image/svg+xml", "width":640, "height":470,
        "altZh":"仅版本协议测试", "creditZh":"仅测试",
        "url":format!("/api/media/{}.svg", "a".repeat(64))
    }]);
    for pointer in ["/revision", "/cast/0/revision", "/media/0/revision"] {
        for revision in [0, i32::MAX as u32 + 1, u32::MAX] {
            let mut source = original.clone();
            *source.pointer_mut(pointer).unwrap() = serde_json::json!(revision);
            let text = serde_json::to_string_pretty(&source)
                .unwrap()
                .replace('\n', "\r\n");
            std::fs::write(&path, &text).unwrap();
            let prefix = "\"revision\": ";
            let marker = format!("{prefix}{revision}");
            assert_eq!(text.matches(&marker).count(), 1);
            let before = &text[..text.find(&marker).unwrap() + prefix.len()];
            let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
            let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
            let output = run("check", &path);
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success());
            assert!(
                error.contains(&format!("{}:{line}:{column}: {pointer}:", path.display())),
                "{error}"
            );
            assert!(error.contains("1..2147483647"), "{error}");
            assert!(!error.contains("database connection"), "{error}");
            assert!(output.stdout.is_empty());
        }
    }
    std::fs::remove_file(path).unwrap();
}
