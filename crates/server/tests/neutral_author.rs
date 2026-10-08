//! Synthetic author/grade protocols, not a publishable Cantonese curriculum.
use brioche_course_contract::{
    ExerciseAnswer,
    neutral::{Block, NeutralLesson},
};
use chef_engine::{
    author_source::{self, CheckedLesson},
    grading::{GradeError, Grader},
};
use serde_json::{Value, json};
use std::process::Command;

fn source() -> Value {
    serde_json::from_str(include_str!("fixtures/neutral-cantonese.lesson.json")).unwrap()
}
fn lesson() -> NeutralLesson {
    match author_source::check_any_source(&source()).unwrap() {
        CheckedLesson::Neutral(lesson) => lesson,
        _ => panic!("neutral source must remain neutral"),
    }
}
fn no_private_or_french_keys(value: &Value) {
    match value {
        Value::Object(fields) => {
            for (key, child) in fields {
                assert!(
                    ![
                        "serverOnly",
                        "grading",
                        "editorial",
                        "assetRefs",
                        "audioRefs",
                        "correctOptionId",
                        "accepted",
                        "correctTokenIds",
                        "fr",
                        "templateFr"
                    ]
                    .contains(&key.as_str()),
                    "{key}"
                );
                no_private_or_french_keys(child);
            }
        }
        Value::Array(items) => items.iter().for_each(no_private_or_french_keys),
        _ => {}
    }
}
#[test]
fn author_projection_and_preflight_preserve_readings_and_keep_private_rules_in_server() {
    let original = source();
    let lesson = lesson();
    let public = serde_json::to_value(&lesson).unwrap();
    no_private_or_french_keys(&public);
    assert_eq!(public["targetLanguage"], "yue-Hant-HK");
    assert_eq!(
        public["knowledge"]["vocabulary"][0]["lemma"],
        original["knowledge"]["vocabulary"][0]["lemma"]
    );
    assert_eq!(
        public["blocks"][0]["paragraphs"][0]["segments"][0]["reading"],
        original["blocks"][0]["paragraphs"][0]["segments"][0]["reading"]
    );
    author_source::validate_neutral_source_schema(original.clone()).unwrap();
    let mut deferred = original.clone();
    deferred["assetRefs"] = json!([]);
    deferred["audioRefs"] = json!([]);
    deferred["media"] = json!("author placeholder replaced by registered descriptors");
    deferred["audio"] = json!("author placeholder replaced by registered recordings");
    author_source::validate_neutral_source_schema(deferred.clone()).unwrap();
    assert!(author_source::project_neutral_source(deferred.clone()).is_err());
    deferred.as_object_mut().unwrap().remove("assetRefs");
    assert!(author_source::validate_neutral_source_schema(deferred).is_err());
    for pointer in ["/assetRefs", "/audioRefs"] {
        let mut bad = original.clone();
        bad.as_object_mut()
            .unwrap()
            .insert(pointer[1..].to_owned(), json!("invalid references"));
        assert!(author_source::validate_neutral_source_schema(bad).is_err());
    }
    let schema = author_source::neutral_schema();
    for field in [
        "targetLanguage",
        "explanationLanguage",
        "editorial",
        "serverOnly",
    ] {
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&json!(field))
        );
    }
    assert_eq!(schema["additionalProperties"], false);
    assert!(schema.to_string().contains("correctOptionId"));
    let public_schema = serde_json::to_value(schemars::schema_for!(NeutralLesson)).unwrap();
    for private in [
        "serverOnly",
        "correctOptionId",
        "correctTokenIds",
        "editorial",
    ] {
        assert!(!public_schema.to_string().contains(private));
    }
    // Old import/runtime projection remains v1-only until its consumers migrate.
    assert!(chef_engine::project_source(original).is_err());
}
#[test]
fn neutral_grading_shares_kind_validation_and_preserves_script_accents_and_authored_ids() {
    let lesson = lesson();
    let original = source();
    let grader = Grader::from_neutral_source(&lesson, &original).unwrap();
    for (id, answer, correct) in [
        (
            "choice",
            ExerciseAnswer::Choice {
                option_id: "greeting".into(),
            },
            true,
        ),
        (
            "choice",
            ExerciseAnswer::Choice {
                option_id: "farewell".into(),
            },
            false,
        ),
        (
            "text",
            ExerciseAnswer::Text {
                text: " 點心\u{00a0}".into(),
            },
            true,
        ),
        (
            "text",
            ExerciseAnswer::Text {
                text: "点心".into(),
            },
            false,
        ),
        (
            "order",
            ExerciseAnswer::Order {
                token_ids: vec!["second".into(), "bye".into(), "first".into()],
            },
            true,
        ),
        (
            "order",
            ExerciseAnswer::Order {
                token_ids: vec!["bye".into(), "first".into(), "second".into()],
            },
            false,
        ),
    ] {
        assert_eq!(
            grader.grade_neutral(&lesson, id, &answer).unwrap().correct,
            correct
        );
    }
    for (id, answer) in [
        (
            "choice",
            ExerciseAnswer::Choice {
                option_id: "forged".into(),
            },
        ),
        (
            "choice",
            ExerciseAnswer::Text {
                text: "greeting".into(),
            },
        ),
        ("text", ExerciseAnswer::Text { text: " ".into() }),
        (
            "text",
            ExerciseAnswer::Text {
                text: "a".repeat(1025),
            },
        ),
        (
            "text",
            ExerciseAnswer::Text {
                text: "😀".repeat(513),
            },
        ),
        (
            "order",
            ExerciseAnswer::Order {
                token_ids: vec!["first".into(), "bye".into(), "first".into()],
            },
        ),
        (
            "order",
            ExerciseAnswer::Order {
                token_ids: vec!["first".into(), "forged".into(), "second".into()],
            },
        ),
    ] {
        assert_eq!(
            grader.grade_neutral(&lesson, id, &answer).err(),
            Some(GradeError::InvalidAnswer)
        );
    }
    assert_eq!(
        grader
            .grade_neutral(
                &lesson,
                "unknown",
                &ExerciseAnswer::Text {
                    text: "點心".into()
                }
            )
            .err(),
        Some(GradeError::UnknownExercise)
    );
    let mut accented = original;
    accented["serverOnly"]["grading"]["text"]["accepted"] = json!(["café"]);
    let accented = Grader::from_neutral_source(&lesson, &accented).unwrap();
    assert!(
        accented
            .grade_neutral(
                &lesson,
                "text",
                &ExerciseAnswer::Text {
                    text: "cafe\u{301}".into()
                }
            )
            .unwrap()
            .correct
    );
    assert!(
        !accented
            .grade_neutral(
                &lesson,
                "text",
                &ExerciseAnswer::Text {
                    text: "cafe".into()
                }
            )
            .unwrap()
            .correct
    );
}
#[test]
fn neutral_private_rule_diagnostics_reject_missing_extra_mismatch_and_duplicate_exercises() {
    let lesson = lesson();
    let original = source();
    for (pointer, value, expected) in [
        (
            "/serverOnly/grading/choice/correctOptionId",
            json!("private-forged-option"),
            "/serverOnly/grading/choice/correctOptionId",
        ),
        (
            "/serverOnly/grading/choice/feedbackZh",
            json!(" "),
            "/serverOnly/grading/choice/feedbackZh",
        ),
        (
            "/serverOnly/grading/text/accepted",
            json!([]),
            "/serverOnly/grading/text/accepted",
        ),
        (
            "/serverOnly/grading/text/accepted/0",
            json!("a".repeat(1025)),
            "/serverOnly/grading/text/accepted/0",
        ),
        (
            "/serverOnly/grading/text/caseSensitive",
            json!(123),
            "/serverOnly/grading/text/caseSensitive",
        ),
        (
            "/serverOnly/grading/order/correctTokenIds/1",
            json!("first"),
            "/serverOnly/grading/order/correctTokenIds/1",
        ),
        (
            "/serverOnly/grading/order/correctTokenIds",
            json!([]),
            "/serverOnly/grading/order/correctTokenIds",
        ),
    ] {
        let mut bad = original.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        let error = Grader::from_neutral_author_source(&lesson, &bad)
            .err()
            .unwrap()
            .to_string();
        assert!(error.starts_with(&format!("{expected}:")), "{error}");
        assert!(!error.contains("private-forged-option"));
        assert!(matches!(
            Grader::from_neutral_source(&lesson, &bad),
            Err(GradeError::InvalidContent)
        ));
    }
    let mut missing = original.clone();
    missing["serverOnly"]["grading"]
        .as_object_mut()
        .unwrap()
        .remove("choice");
    assert!(
        Grader::from_neutral_author_source(&lesson, &missing)
            .err()
            .unwrap()
            .to_string()
            .starts_with("/serverOnly/grading/choice:")
    );
    let mut extra = original.clone();
    extra["serverOnly"]["grading"]["a/b~c"] = original["serverOnly"]["grading"]["choice"].clone();
    assert!(
        Grader::from_neutral_author_source(&lesson, &extra)
            .err()
            .unwrap()
            .to_string()
            .starts_with("/serverOnly/grading/a~1b~0c:")
    );
    let mut mismatch = original.clone();
    mismatch["serverOnly"]["grading"]["choice"] = original["serverOnly"]["grading"]["text"].clone();
    assert!(
        Grader::from_neutral_author_source(&lesson, &mismatch)
            .err()
            .unwrap()
            .to_string()
            .starts_with("/serverOnly/grading/choice/kind:")
    );
    let mut duplicate = lesson.clone();
    duplicate.blocks.push(
        lesson
            .blocks
            .iter()
            .find(|block| matches!(block, Block::Exercise { .. }))
            .unwrap()
            .clone(),
    );
    assert!(matches!(
        Grader::from_neutral_source(&duplicate, &original),
        Err(GradeError::InvalidContent)
    ));
}
#[test]
fn versioned_decoding_rejects_legacy_fields_and_unknown_private_or_reading_fields() {
    let original = source();
    for pointer in [
        "/title/fr",
        "/knowledge/vocabulary/0/lemma/untrusted",
        "/blocks/4/templateFr",
        "/serverOnly/grading/text/untrusted",
        "/editorial/untrusted",
    ] {
        let mut bad = original.clone();
        let (parent, name) = pointer.rsplit_once('/').unwrap();
        bad.pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(name.to_owned(), json!("hidden-marker"));
        assert!(author_source::check_any_source(&bad).is_err(), "{pointer}");
    }
    for version in [json!(""), json!("3.0"), json!(2), Value::Null] {
        let mut bad = original.clone();
        bad["schemaVersion"] = version;
        assert!(
            author_source::check_any_source(&bad)
                .err()
                .unwrap()
                .to_string()
                .starts_with("/schemaVersion:")
        );
    }
    let mut missing = original;
    missing.as_object_mut().unwrap().remove("editorial");
    assert!(
        author_source::check_any_source(&missing)
            .err()
            .unwrap()
            .to_string()
            .starts_with("/editorial:")
    );
}
fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_chef-server"))
        .args(args)
        .env("CHEF_PRODUCT", "hargow")
        .env("DATABASE_URL", "postgres://invalid@127.0.0.1:1/unavailable")
        .env("APP_ENV", "production")
        .env("CONTENT_MODE", "database")
        .output()
        .unwrap()
}
#[test]
fn actual_neutral_author_cli_checks_mixed_release_sources_and_locates_errors_without_database() {
    let mut id = [0; 16];
    getrandom::fill(&mut id).unwrap();
    let root =
        std::env::temp_dir().join(format!("chef-neutral-author-{}", u128::from_le_bytes(id)));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("neutral-protocol.lesson.json");
    let manifest = root.join("release.json");
    let original = source();
    let write = |source: &Value| {
        std::fs::write(
            &path,
            serde_json::to_string_pretty(source)
                .unwrap()
                .replace('\n', "\r\n"),
        )
        .unwrap()
    };
    write(&original);
    let output = run(&["check", path.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Structural checks passed"));
    assert!(!stdout.contains("點心") && !stdout.contains("correctOptionId"));
    let legacy = chef_engine::development_source().unwrap();
    let legacy_path = root.join(format!("{}.lesson.json", legacy["id"].as_str().unwrap()));
    std::fs::write(&legacy_path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    let release = json!({"schemaVersion":"1.0","id":"mixed-protocol","levels":[
        {"id":"starter","label":"入门","units":[{"id":"greetings","titleZh":"合成单元","lessons":[{"lessonId":"neutral-protocol","revision":1}]}]},
        {"id":legacy["levelId"],"label":"法语兼容","units":[{"id":legacy["unitId"],"titleZh":"旧版兼容","lessons":[{"lessonId":legacy["id"],"revision":legacy["revision"]}]}]}
    ]});
    std::fs::write(&manifest, serde_json::to_vec(&release).unwrap()).unwrap();
    let pack = || {
        run(&[
            "check-release",
            manifest.to_str().unwrap(),
            "--sources",
            root.to_str().unwrap(),
        ])
    };
    let output = pack();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Checked 2 referenced lesson sources")
    );
    for (pointer, value, expected) in [
        (
            "/targetLanguage",
            json!("fr-FR"),
            "/knowledge/vocabulary/0/lemma/pronunciations/0/system",
        ),
        (
            "/knowledge/vocabulary/0/lemma/words/0/end",
            json!(3),
            "/knowledge/vocabulary/0/lemma/words/0",
        ),
        (
            "/blocks/3/options/0/text",
            json!("再見"),
            "/blocks/3/options/1/text",
        ),
        (
            "/serverOnly/grading/choice/correctOptionId",
            json!("private-unknown-marker"),
            "/serverOnly/grading/choice/correctOptionId",
        ),
        (
            "/cast/0/speechLocale",
            json!("fr-FR"),
            "/cast/0/speechLocale",
        ),
        ("/id", json!("other-id"), "/id"),
        ("/revision", json!(2), "/revision"),
        ("/levelId", json!("other-level"), "/levelId"),
        ("/unitId", json!("other-unit"), "/unitId"),
        ("/schemaVersion", json!("3.0"), "/schemaVersion"),
    ] {
        let mut bad = original.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        write(&bad);
        let output = pack();
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains(expected), "{error}");
        assert!(error.contains(&format!("{}:", path.display())), "{error}");
        assert!(
            !error.contains("private-unknown-marker") && !error.contains("database connection")
        );
        assert!(output.stdout.is_empty());
    }
    write(&original);
    // The unknown private value is located at its original CRLF/Unicode source position.
    let mut bad = original.clone();
    bad["serverOnly"]["grading"]["text"]["caseSensitive"] = json!(93217);
    write(&bad);
    let text = std::fs::read_to_string(&path).unwrap();
    let offset = text.find("93217").unwrap();
    let before = &text[..offset];
    let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
    let output = run(&["check", path.to_str().unwrap()]);
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        error.contains(&format!(
            "{}:{line}:{column}: /serverOnly/grading/text/caseSensitive:",
            path.display()
        )),
        "{error}"
    );
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(manifest).unwrap();
    std::fs::remove_file(legacy_path).unwrap();
    std::fs::remove_dir(root).unwrap();
}
