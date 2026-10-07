//! The authored pilot is data, checked with the same projection and grader as imports.
use brioche_course_contract::ExerciseAnswer;
use chef_engine::{author_json::Document, content::ReleaseManifest, grading::Grader};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

#[test]
fn scene_inventory_matches_actual_svg_sources() {
    check_scene_inventory("a1", 6);
}

#[test]
fn a2_scene_inventory_matches_actual_svg_sources() {
    check_scene_inventory("a2", 1);
}

#[test]
fn example_inventory_matches_the_committed_visual_sources() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let document = Document::load(root.join("docs/examples/asset-bundle.json")).unwrap();
    let bundle: chef_engine::media::AssetBundle = serde_json::from_value(document.value).unwrap();
    assert_eq!(bundle.assets.len(), 4);
    for asset in bundle.assets {
        let info = chef_engine::media::inspect_file(
            &root.join("test-fixtures/visuals").join(&asset.file),
            &asset.mime_type,
        )
        .unwrap();
        assert_eq!(
            info.sha256, asset.sha256,
            "stale hash for {}",
            asset.asset_id
        );
        assert_eq!((info.width, info.height), (asset.width, asset.height));
    }
}

fn check_scene_inventory(level: &str, expected_count: usize) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../curriculum/docs/content")
        .join(level);
    let document = Document::load(root.join("scene-assets.bundle.json")).unwrap();
    let bundle: chef_engine::media::AssetBundle = serde_json::from_value(document.value).unwrap();
    assert_eq!(bundle.schema_version, "1.0");
    assert_eq!(bundle.assets.len(), expected_count);
    for asset in bundle.assets {
        let info = chef_engine::media::inspect_file(
            &root.join("assets").join(&asset.file),
            &asset.mime_type,
        )
        .unwrap();
        assert_eq!(
            info.sha256, asset.sha256,
            "stale hash for {}",
            asset.asset_id
        );
        assert_eq!((info.width, info.height), (asset.width, asset.height));
    }
}

#[test]
fn pilot_sources_match_catalog_and_shared_knowledge_and_grade_all_exercises() {
    check_catalog(
        "catalog.release.json",
        &[
            "a1-first-conversations",
            "a1-breakfast-bakery",
            "a1-city-travel",
        ],
    );
}

#[test]
fn extended_sources_match_catalog_and_shared_knowledge_and_grade_all_exercises() {
    check_catalog(
        "catalog.extended.release.json",
        &[
            "a1-first-conversations",
            "a1-breakfast-bakery",
            "a1-city-travel",
            "a1-home-routine",
        ],
    );
}

#[test]
fn five_unit_sources_match_catalog_and_shared_knowledge_and_grade_all_exercises() {
    check_catalog(
        "catalog.five-units.release.json",
        &[
            "a1-first-conversations",
            "a1-breakfast-bakery",
            "a1-city-travel",
            "a1-home-routine",
            "a1-food-shopping",
        ],
    );
}

#[test]
fn full_a1_sources_match_catalog_and_shared_knowledge_and_grade_all_exercises() {
    check_catalog(
        "catalog.full-a1.release.json",
        &[
            "a1-first-conversations",
            "a1-breakfast-bakery",
            "a1-city-travel",
            "a1-home-routine",
            "a1-food-shopping",
            "a1-social-meetings",
        ],
    );
}

#[test]
fn a2_travel_pilot_matches_cross_level_shared_knowledge_and_grades_all_exercises() {
    check_a2_readings(&[
        ("a2-travel-plan-weekend", "article"),
        ("a2-travel-book-room", "dialogue"),
        ("a2-travel-buy-return-ticket", "dialogue"),
        ("a2-travel-tell-weekend", "article"),
    ]);
    check_catalog(
        "../a2/catalog.pilot.release.json",
        &[
            "a1-first-conversations",
            "a1-breakfast-bakery",
            "a1-city-travel",
            "a1-home-routine",
            "a1-food-shopping",
            "a1-social-meetings",
            "a2-weekend-travel",
        ],
    );
}

#[test]
fn a2_shared_living_matches_cross_level_catalog_and_grades_all_exercises() {
    check_a2_readings(&[
        ("a2-home-share-chores", "dialogue"),
        ("a2-home-common-rules", "article"),
        ("a2-home-shared-routine", "article"),
        ("a2-home-compare-rooms", "dialogue"),
    ]);
    check_catalog(
        "../a2/catalog.two-units.release.json",
        &[
            "a1-first-conversations",
            "a1-breakfast-bakery",
            "a1-city-travel",
            "a1-home-routine",
            "a1-food-shopping",
            "a1-social-meetings",
            "a2-weekend-travel",
            "a2-shared-living",
        ],
    );
}

#[test]
fn a2_daily_services_matches_cross_level_catalog_and_grades_all_exercises() {
    check_a2_readings(&[
        ("a2-services-book-appointment", "dialogue"),
        ("a2-services-fill-information", "article"),
        ("a2-services-explain-problem", "dialogue"),
        ("a2-services-ask-next-steps", "dialogue"),
    ]);
    check_catalog(
        "../a2/catalog.three-units.release.json",
        &[
            "a1-first-conversations",
            "a1-breakfast-bakery",
            "a1-city-travel",
            "a1-home-routine",
            "a1-food-shopping",
            "a1-social-meetings",
            "a2-weekend-travel",
            "a2-shared-living",
            "a2-daily-services",
        ],
    );
}

#[test]
fn a2_body_wellbeing_matches_cross_level_catalog_and_grades_all_exercises() {
    check_a2_readings(&[
        ("a2-health-describe-discomfort", "dialogue"),
        ("a2-health-book-consultation", "dialogue"),
        ("a2-health-talk-feelings", "article"),
        ("a2-health-read-instructions", "article"),
    ]);
    check_catalog(
        "../a2/catalog.four-units.release.json",
        &[
            "a1-first-conversations",
            "a1-breakfast-bakery",
            "a1-city-travel",
            "a1-home-routine",
            "a1-food-shopping",
            "a1-social-meetings",
            "a2-weekend-travel",
            "a2-shared-living",
            "a2-daily-services",
            "a2-body-wellbeing",
        ],
    );
}

#[test]
fn a2_work_study_matches_cross_level_catalog_and_grades_all_exercises() {
    check_a2_readings(&[
        ("a2-work-describe-experience", "article"),
        ("a2-work-plan-collaboration", "dialogue"),
        ("a2-work-report-progress", "article"),
        ("a2-work-tell-yesterday", "article"),
    ]);
    check_catalog(
        "../a2/catalog.five-units.release.json",
        &[
            "a1-first-conversations",
            "a1-breakfast-bakery",
            "a1-city-travel",
            "a1-home-routine",
            "a1-food-shopping",
            "a1-social-meetings",
            "a2-weekend-travel",
            "a2-shared-living",
            "a2-daily-services",
            "a2-body-wellbeing",
            "a2-work-study-experience",
        ],
    );
}

#[test]
fn full_a1_a2_catalog_matches_sources_and_grades_all_exercises() {
    check_a2_readings(&[
        ("a2-express-evaluate-experience", "article"),
        ("a2-express-explain-preference", "dialogue"),
        ("a2-express-propose-alternative", "dialogue"),
        ("a2-express-clear-misunderstanding", "dialogue"),
    ]);
    check_catalog(
        "../a2/catalog.full.release.json",
        &[
            "a1-first-conversations",
            "a1-breakfast-bakery",
            "a1-city-travel",
            "a1-home-routine",
            "a1-food-shopping",
            "a1-social-meetings",
            "a2-weekend-travel",
            "a2-shared-living",
            "a2-daily-services",
            "a2-body-wellbeing",
            "a2-work-study-experience",
            "a2-expression-negotiation",
        ],
    );
}

fn check_a2_readings(readings: &[(&str, &str)]) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum/docs/content/a2");
    for &(id, mode) in readings {
        let source = Document::load(root.join(format!("{id}.lesson.json")))
            .unwrap()
            .value;
        // These fixed sources were authorized for direct publication by the owner.
        // Validate that provenance rather than requiring their former draft state.
        let editorial = chef_engine::author_source::editorial(&source).unwrap();
        assert!(matches!(
            editorial.status,
            chef_engine::author_source::EditorialStatus::Reviewed
        ));
        assert!(editorial.note.contains("直接发布"));
        assert!(editorial.note.contains("不声称独立专家审校"));
        let body = &source["blocks"][1];
        assert_eq!(body["type"], mode);
        let entries = body[if mode == "article" {
            "paragraphs"
        } else {
            "turns"
        }]
        .as_array()
        .unwrap();
        let prose = entries
            .iter()
            .map(|entry| {
                entry["segments"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|segment| segment["text"].as_str().unwrap())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            (120..=250).contains(&prose.split_whitespace().count()),
            "A2 reading length: {id}"
        );
    }
}

fn check_catalog(file: &str, expected_units: &[&str]) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum/docs");
    let document = Document::load(root.join("content/a1").join(file)).unwrap();
    let manifest: ReleaseManifest = serde_json::from_value(document.value.clone()).unwrap();
    manifest.validate_author().unwrap();
    let units: Vec<_> = document.value["levels"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|level| {
            level["units"]
                .as_array()
                .unwrap()
                .iter()
                .map(move |unit| (level, unit))
        })
        .collect();
    assert_eq!(
        units
            .iter()
            .map(|(_, unit)| unit["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        expected_units
    );
    let mut knowledge = BTreeMap::<String, Value>::new();
    let mut characters = BTreeMap::<(String, u64), Value>::new();
    let mut checked = 0;
    for (level, unit) in units {
        let lessons = unit["lessons"].as_array().unwrap();
        assert_eq!(lessons.len(), 4);
        for reference in lessons {
            let id = reference["lessonId"].as_str().unwrap();
            let path = if id == "a1-bakery-buy-breakfast" {
                root.join("examples/a1-bakery.lesson.json")
            } else {
                root.join(format!(
                    "content/{}/{id}.lesson.json",
                    level["id"].as_str().unwrap()
                ))
            };
            let source = Document::load(path).unwrap().value;
            assert_eq!(source["id"], reference["lessonId"]);
            assert_eq!(source["revision"], reference["revision"]);
            assert_eq!(source["unitId"], unit["id"]);
            assert_eq!(source["levelId"], level["id"]);
            chef_engine::author_source::editorial(&source).unwrap();
            chef_engine::media::source_asset_refs(&source).unwrap();
            chef_engine::recording::source_audio_refs(&source).unwrap();
            chef_engine::validate_source_schema(source.clone()).unwrap();
            let lesson = chef_engine::project_source(source.clone()).unwrap();
            let grader = Grader::from_author_source(&lesson, &source).unwrap();
            let public = serde_json::to_value(&lesson).unwrap();
            for character in source["cast"].as_array().unwrap() {
                let key = (
                    character["characterId"].as_str().unwrap().to_owned(),
                    character["revision"].as_u64().unwrap(),
                );
                if let Some(previous) = characters.insert(key.clone(), character.clone()) {
                    assert_eq!(
                        previous, *character,
                        "conflicting character snapshot {key:?} in {id}"
                    );
                }
            }
            for private_field in ["serverOnly", "editorial", "assetRefs", "audioRefs"] {
                assert!(public.get(private_field).is_none());
            }
            for group in ["vocabulary", "grammar"] {
                for entry in source["knowledge"][group].as_array().unwrap() {
                    let key = entry["id"].as_str().unwrap().to_owned();
                    if let Some(previous) = knowledge.insert(key.clone(), entry.clone()) {
                        assert_eq!(
                            previous, *entry,
                            "conflicting shared knowledge {key} in {id}"
                        );
                    }
                }
            }
            let rules = source["serverOnly"]["grading"].as_object().unwrap();
            assert_eq!(rules.len(), 3);
            for (exercise_id, rule) in rules {
                let answer = match rule["kind"].as_str().unwrap() {
                    "choice" => ExerciseAnswer::Choice {
                        option_id: rule["correctOptionId"].as_str().unwrap().into(),
                    },
                    "text" => ExerciseAnswer::Text {
                        text: rule["accepted"][0].as_str().unwrap().into(),
                    },
                    "order" => ExerciseAnswer::Order {
                        token_ids: serde_json::from_value(rule["correctTokenIds"].clone()).unwrap(),
                    },
                    kind => panic!("unknown rule {kind}"),
                };
                assert!(grader.grade(&lesson, exercise_id, &answer).unwrap().correct);
                let wrong = match answer {
                    ExerciseAnswer::Choice { option_id } => {
                        let block = source["blocks"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|block| block["id"] == *exercise_id)
                            .unwrap();
                        let alternative = block["options"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|option| option["id"] != option_id)
                            .unwrap();
                        ExerciseAnswer::Choice {
                            option_id: alternative["id"].as_str().unwrap().into(),
                        }
                    }
                    ExerciseAnswer::Text { .. } => ExerciseAnswer::Text {
                        text: "incorrect-answer".into(),
                    },
                    ExerciseAnswer::Order { mut token_ids } => {
                        token_ids.swap(0, 1);
                        ExerciseAnswer::Order { token_ids }
                    }
                };
                assert!(!grader.grade(&lesson, exercise_id, &wrong).unwrap().correct);
            }
            checked += 1;
        }
    }
    assert_eq!(checked, expected_units.len() * 4);
}
