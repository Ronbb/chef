use chef_engine::{author_json, author_source, speech_plan};
use serde_json::{Value, json};

fn latest_author_source(root: &std::path::Path, path: &std::path::Path) -> author_json::Document {
    let mut document = author_json::Document::load(path).unwrap();
    let id = document.value["id"].as_str().unwrap().to_owned();
    let mut directories = vec![root.join("work-in-progress")];
    for entry in std::fs::read_dir(root.join("releases")).unwrap() {
        let candidate = entry.unwrap().path();
        if candidate.is_dir() {
            directories.push(candidate);
        }
    }
    for directory in directories {
        let candidate = directory.join(format!("{id}.lesson.json"));
        if !candidate.is_file() {
            continue;
        }
        let newer = author_json::Document::load(candidate).unwrap();
        for key in ["id", "levelId", "unitId"] {
            assert_eq!(newer.value[key], document.value[key]);
        }
        let revision = newer.value["revision"].as_u64().unwrap();
        let current = document.value["revision"].as_u64().unwrap();
        if revision > current {
            document = newer;
        } else if revision == current {
            assert_eq!(
                newer.value, document.value,
                "ambiguous author revision {id}"
            );
        }
    }
    document
}

fn config(lesson: &brioche_course_contract::PublicLesson) -> Value {
    let profile = json!({"personality":"Patient and friendly", "speakingStyle":"Clear conversational French", "defaultEmotion":"Calm", "provider":"qwen", "model":"qwen-audio-3.1-tts-flash", "voiceId":"longanlingxin_v3.1", "voiceKind":"system", "locale":"fr-FR", "rate":1.0,"referenceAudio":null});
    // Synthetic compiler fixtures, never registered or used for real synthesis.
    json!({"items":lesson.cast.iter().map(|c| json!({"character":c,"avatarRevision":1,"voiceRevision":1,"profile":profile})).collect::<Vec<_>>(),"knowledgeNarrator":{"characterId":lesson.cast[0].character_id,"characterRevision":lesson.cast[0].revision,"voiceRevision":1},"emotions":{}})
}

#[test]
fn authored_curriculum_can_be_planned_without_provider_calls() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum/docs/content");
    let mut count = 0;
    for level in ["a1", "a2"] {
        for entry in std::fs::read_dir(root.join(level)).unwrap() {
            let path = entry.unwrap().path();
            if !path.to_string_lossy().ends_with(".lesson.json") {
                continue;
            }
            // Keep published history immutable; compile the newest authored revision,
            // including an explicitly saved draft awaiting its final audio package.
            let doc = latest_author_source(&root, &path);
            let lesson = author_source::check_lesson(&doc).unwrap();
            let mut cfg = config(&lesson);
            // Select only roles actually needed by this source and the explicit knowledge narrator.
            let needed: std::collections::BTreeSet<_> = lesson
                .blocks
                .iter()
                .flat_map(|b| match b {
                    brioche_course_contract::Block::Dialogue { speakers, .. } => speakers
                        .iter()
                        .map(|s| s.character_id.clone())
                        .collect::<Vec<_>>(),
                    brioche_course_contract::Block::Article { narrator_id, .. } => {
                        vec![narrator_id.clone()]
                    }
                    _ => vec![],
                })
                .chain(std::iter::once(lesson.cast[0].character_id.clone()))
                .collect();
            cfg["items"]
                .as_array_mut()
                .unwrap()
                .retain(|v| needed.contains(v["character"]["characterId"].as_str().unwrap()));
            let cfg: speech_plan::Config = serde_json::from_value(cfg).unwrap();
            let plan = speech_plan::compile(&lesson, &doc.value, &cfg)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert!(!plan.requests.is_empty());
            count += 1;
        }
    }
    // The remaining bakery source lives under docs/examples and is covered by the CLI test.
    assert_eq!(count, 47);
}

#[test]
fn cli_runs_offline_and_rejects_duplicate_config_members() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/examples/a1-bakery.lesson.json");
    let doc = author_json::Document::load(&path).unwrap();
    let lesson = author_source::check_lesson(&doc).unwrap();
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).unwrap();
    let config_path = std::env::temp_dir().join(format!(
        "brioche-speech-plan-{:x}.json",
        u128::from_le_bytes(bytes)
    ));
    std::fs::write(&config_path, serde_json::to_vec(&config(&lesson)).unwrap()).unwrap();
    let run = || {
        std::process::Command::new(env!("CARGO_BIN_EXE_chef-server"))
            .arg("speech-plan")
            .arg(&path)
            .arg(&config_path)
            .env("DATABASE_URL", "postgres://invalid@127.0.0.1:1/unavailable")
            .env_remove("DASHSCOPE_API_KEY")
            .output()
            .unwrap()
    };
    let valid = run();
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );
    let plan: Value = serde_json::from_slice(&valid.stdout).unwrap();
    assert_eq!(plan["lessonId"], lesson.id);
    std::fs::write(&config_path, br#"{"items":[],"items":[]}"#).unwrap();
    let invalid = run();
    std::fs::remove_file(config_path).unwrap();
    assert!(!invalid.status.success());
    assert!(invalid.stdout.is_empty());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("duplicate"));
}
