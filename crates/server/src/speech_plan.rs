//! Offline, private synthesis planning. Text boundaries are not audio timestamps.
use anyhow::{Result, anyhow, ensure};
use brioche_course_contract::{AdminCharacterVoice, Block, PublicLesson, Segment};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use unicode_segmentation::UnicodeSegmentation;

pub use brioche_course_contract::AdminSpeechVoice as VoiceKey;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub items: Vec<AdminCharacterVoice>,
    pub knowledge_narrator: VoiceKey,
    pub emotions: BTreeMap<String, String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub compiler_version: &'static str,
    pub lesson_id: String,
    pub lesson_revision: u32,
    pub source_hash: String,
    pub plan_hash: String,
    pub targets: Vec<Target>,
    pub requests: BTreeMap<String, Value>,
    pub total_request_characters: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub pointer: String,
    pub block_id: Option<String>,
    pub entry_id: String,
    pub text: String,
    pub voice: VoiceKey,
    pub emotion: String,
    pub generation_key: String,
    pub words: Vec<Word>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Word {
    pub segment_id: String,
    pub text: String,
    pub segment_start: usize,
    pub segment_end: usize,
    pub entry_start: usize,
    pub entry_end: usize,
}
const VERSION: &str = "speech-plan-1/uax29-1.13.3";
fn hash(value: &impl Serialize) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}
fn key(voice: &AdminCharacterVoice) -> VoiceKey {
    VoiceKey {
        character_id: voice.character.character_id.clone(),
        character_revision: voice.character.revision,
        voice_revision: voice.voice_revision,
    }
}
fn words(segments: &[Segment]) -> Vec<Word> {
    let mut result = Vec::new();
    let mut offset = 0;
    for segment in segments {
        for (byte_start, word) in segment.text.unicode_word_indices() {
            let start = segment.text[..byte_start].chars().count();
            let end = start + word.chars().count();
            result.push(Word {
                segment_id: segment.id.clone(),
                text: word.into(),
                segment_start: start,
                segment_end: end,
                entry_start: offset + start,
                entry_end: offset + end,
            });
        }
        offset += segment.text.chars().count();
    }
    result
}
struct Inputs {
    id: String,
    revision: u32,
    cast: Vec<brioche_course_contract::Character>,
    version: &'static str,
    entries: Vec<Input>,
}
pub(crate) struct Input {
    pub(crate) pointer: String,
    pub(crate) block_id: Option<String>,
    pub(crate) entry_id: String,
    pub(crate) text: String,
    character_id: Option<String>,
    words: Vec<Word>,
}
fn compile_inputs(inputs: Inputs, source: &Value, config: &Config) -> Result<Plan> {
    let Inputs {
        id,
        revision,
        cast,
        version,
        entries: inputs,
    } = inputs;
    ensure!(
        !config.items.is_empty() && config.items.len() <= 100,
        "items: expected 1–100 fixed voices"
    );
    ensure!(
        config.emotions.len() <= 1000,
        "emotions: too many overrides"
    );
    let mut voices = BTreeMap::new();
    for voice in &config.items {
        let k = key(voice);
        ensure!(
            brioche_course_contract::valid_content_revision(k.voice_revision),
            "items: voice revision must be positive"
        );
        ensure!(
            cast.iter().any(
                |c| serde_json::to_value(c).ok() == serde_json::to_value(&voice.character).ok()
            ),
            "items: character snapshot must match the fixed lesson cast"
        );
        ensure!(voice.profile.is_some(), "items: voice profile is missing");
        crate::character_voices::validate_for_character(
            voice.profile.as_ref().unwrap(),
            &voice.character,
        )
        .map_err(|_| anyhow!("items: invalid profile or character speech locale mismatch"))?;
        ensure!(
            voices.insert(k, voice).is_none(),
            "items: duplicate fixed voice"
        );
    }
    ensure!(
        voices.contains_key(&config.knowledge_narrator),
        "knowledgeNarrator: fixed voice not selected"
    );
    let mut plan = Plan {
        compiler_version: version,
        lesson_id: id,
        lesson_revision: revision,
        source_hash: hash(source)?,
        plan_hash: String::new(),
        targets: Vec::new(),
        requests: BTreeMap::new(),
        total_request_characters: 0,
    };
    let mut used = BTreeSet::new();
    let mut used_overrides = BTreeSet::new();
    let mut add = |pointer: String,
                   block_id: Option<String>,
                   entry_id: String,
                   text: String,
                   voice_key: VoiceKey,
                   word_targets: Vec<Word>|
     -> Result<()> {
        if version == VERSION && block_id.is_some() {
            let whole_words: Vec<_> = text
                .unicode_word_indices()
                .map(|(byte, word)| {
                    let start = text[..byte].chars().count();
                    (start, start + word.chars().count(), word)
                })
                .collect();
            ensure!(
                whole_words
                    == word_targets
                        .iter()
                        .map(|w| (w.entry_start, w.entry_end, w.text.as_str()))
                        .collect::<Vec<_>>(),
                "{pointer}: text segments split a whole word; keep each complete word in one segment before generating speech"
            );
        }
        let voice = voices
            .get(&voice_key)
            .ok_or_else(|| anyhow!("{pointer}: fixed voice missing"))?;
        let profile = voice.profile.as_ref().unwrap();
        let emotion = config
            .emotions
            .get(&pointer)
            .unwrap_or(&profile.default_emotion)
            .clone();
        if config.emotions.contains_key(&pointer) {
            used_overrides.insert(pointer.clone());
        }
        let parameters = crate::qwen::SpeechRequest { profile: profile.clone(), text: text.clone(), emotion: emotion.clone() }.parameters().map_err(|_| anyhow!("{pointer}: invalid synthesis text/profile/emotion (maximum 600 Unicode characters per entry)"))?;
        let mut request = json!({"compilerVersion":version,"voice":voice_key,"profile":profile,"parameters":parameters});
        if version == NEUTRAL_VERSION {
            // Different authored boundaries require distinct alignment work, even
            // when the spoken text and voice happen to match another target.
            request["wordUnits"] = json!(
                word_targets
                    .iter()
                    .map(|w| { json!({"text":w.text,"start":w.entry_start,"end":w.entry_end}) })
                    .collect::<Vec<_>>()
            );
        }
        let generation_key = hash(&request)?;
        if !plan.requests.contains_key(&generation_key) {
            plan.total_request_characters += text.chars().count();
        }
        plan.requests.insert(generation_key.clone(), request);
        used.insert(voice_key.clone());
        plan.targets.push(Target {
            pointer,
            block_id,
            entry_id,
            text,
            voice: voice_key,
            emotion,
            generation_key,
            words: word_targets,
        });
        Ok(())
    };
    let resolve = |character: &str, pointer: &str| -> Result<VoiceKey> {
        let candidates: Vec<_> = voices
            .keys()
            .filter(|k| k.character_id == character)
            .collect();
        ensure!(
            candidates.len() == 1,
            "{pointer}: select exactly one fixed voice for {character}"
        );
        Ok(candidates[0].clone())
    };
    for input in inputs {
        let voice = match input.character_id {
            Some(character) => resolve(&character, &input.pointer)?,
            None => config.knowledge_narrator.clone(),
        };
        add(
            input.pointer,
            input.block_id,
            input.entry_id,
            input.text,
            voice,
            input.words,
        )?;
    }
    ensure!(!plan.targets.is_empty(), "lesson: no speech targets");
    ensure!(used.len() == voices.len(), "items: unused selected voice");
    ensure!(
        used_overrides.len() == config.emotions.len(),
        "emotions: unknown target pointer"
    );
    plan.plan_hash = hash(&plan)?;
    Ok(plan)
}

fn legacy_inputs(lesson: &PublicLesson) -> Result<Inputs> {
    lesson
        .validate()
        .map_err(|e| anyhow!("invalid lesson: {e}"))?;
    let mut entries = Vec::new();
    for (bi, block) in lesson.blocks.iter().enumerate() {
        match block {
            Block::Dialogue {
                id,
                speakers,
                turns,
                ..
            } => {
                for (ti, turn) in turns.iter().enumerate() {
                    let pointer = format!("/blocks/{bi}/turns/{ti}");
                    let speaker = speakers
                        .iter()
                        .find(|s| s.id == turn.speaker_id)
                        .ok_or_else(|| anyhow!("{pointer}: speaker missing"))?;
                    entries.push(Input {
                        pointer,
                        block_id: Some(id.clone()),
                        entry_id: turn.id.clone(),
                        text: turn.segments.iter().map(|s| s.text.as_str()).collect(),
                        character_id: Some(speaker.character_id.clone()),
                        words: words(&turn.segments),
                    });
                }
            }
            Block::Article {
                id,
                narrator_id,
                paragraphs,
                ..
            } => {
                for (pi, paragraph) in paragraphs.iter().enumerate() {
                    entries.push(Input {
                        pointer: format!("/blocks/{bi}/paragraphs/{pi}"),
                        block_id: Some(id.clone()),
                        entry_id: paragraph.id.clone(),
                        text: paragraph.segments.iter().map(|s| s.text.as_str()).collect(),
                        character_id: Some(narrator_id.clone()),
                        words: words(&paragraph.segments),
                    });
                }
            }
            _ => {}
        }
    }
    for (vi, vocabulary) in lesson.knowledge.vocabulary.iter().enumerate() {
        entries.push(Input {
            pointer: format!("/knowledge/vocabulary/{vi}/lemma"),
            block_id: None,
            entry_id: vocabulary.id.clone(),
            text: vocabulary.lemma.clone(),
            character_id: None,
            words: Vec::new(),
        });
    }
    for (gi, grammar) in lesson.knowledge.grammar.iter().enumerate() {
        for (ei, example) in grammar.examples.iter().enumerate() {
            entries.push(Input {
                pointer: format!("/knowledge/grammar/{gi}/examples/{ei}/fr"),
                block_id: None,
                entry_id: grammar.id.clone(),
                text: example.fr.clone(),
                character_id: None,
                words: Vec::new(),
            });
        }
    }
    Ok(Inputs {
        id: lesson.id.clone(),
        revision: lesson.revision,
        cast: lesson.cast.clone(),
        version: VERSION,
        entries,
    })
}

pub(crate) const NEUTRAL_VERSION: &str = "speech-plan-2/author-scalar-1";
fn knowledge_words(id: &str, reading: &brioche_course_contract::ReadingText) -> Vec<Word> {
    authored_words(&[brioche_course_contract::neutral::NeutralSegment {
        id: id.into(),
        reading: reading.clone(),
        vocabulary_id: None,
        grammar_id: None,
    }])
}
fn authored_words(segments: &[brioche_course_contract::neutral::NeutralSegment]) -> Vec<Word> {
    let mut words = Vec::new();
    let mut offset = 0;
    for segment in segments {
        for range in &segment.reading.words {
            // The complete lesson is validated first; preserve the author's exact word/phrase.
            let text = segment
                .reading
                .word_text(range)
                .expect("validated authored word");
            words.push(Word {
                segment_id: segment.id.clone(),
                text,
                segment_start: range.start as usize,
                segment_end: range.end as usize,
                entry_start: offset + range.start as usize,
                entry_end: offset + range.end as usize,
            });
        }
        offset += segment.reading.text.chars().count();
    }
    words
}
fn neutral_inputs(lesson: &brioche_course_contract::neutral::NeutralLesson) -> Result<Inputs> {
    use brioche_course_contract::neutral::Block;
    lesson
        .validate()
        .map_err(|e| anyhow!("invalid lesson: {e}"))?;
    let mut entries = Vec::new();
    for (bi, block) in lesson.blocks.iter().enumerate() {
        match block {
            Block::Dialogue {
                id,
                speakers,
                turns,
                ..
            } => {
                for (ti, turn) in turns.iter().enumerate() {
                    let pointer = format!("/blocks/{bi}/turns/{ti}");
                    let speaker = speakers
                        .iter()
                        .find(|s| s.id == turn.speaker_id)
                        .ok_or_else(|| anyhow!("{pointer}: speaker missing"))?;
                    entries.push(Input {
                        pointer,
                        block_id: Some(id.clone()),
                        entry_id: turn.id.clone(),
                        text: turn
                            .segments
                            .iter()
                            .map(|s| s.reading.text.as_str())
                            .collect(),
                        character_id: Some(speaker.character_id.clone()),
                        words: authored_words(&turn.segments),
                    });
                }
            }
            Block::Article {
                id,
                narrator_id,
                paragraphs,
                ..
            } => {
                for (pi, paragraph) in paragraphs.iter().enumerate() {
                    entries.push(Input {
                        pointer: format!("/blocks/{bi}/paragraphs/{pi}"),
                        block_id: Some(id.clone()),
                        entry_id: paragraph.id.clone(),
                        text: paragraph
                            .segments
                            .iter()
                            .map(|s| s.reading.text.as_str())
                            .collect(),
                        character_id: Some(narrator_id.clone()),
                        words: authored_words(&paragraph.segments),
                    });
                }
            }
            _ => {}
        }
    }
    for (vi, vocabulary) in lesson.knowledge.vocabulary.iter().enumerate() {
        entries.push(Input {
            pointer: format!("/knowledge/vocabulary/{vi}/lemma"),
            block_id: None,
            entry_id: vocabulary.id.clone(),
            text: vocabulary.lemma.text.clone(),
            character_id: None,
            words: knowledge_words(&vocabulary.id, &vocabulary.lemma),
        });
    }
    for (gi, grammar) in lesson.knowledge.grammar.iter().enumerate() {
        for (ei, example) in grammar.examples.iter().enumerate() {
            entries.push(Input {
                pointer: format!("/knowledge/grammar/{gi}/examples/{ei}/target"),
                block_id: None,
                entry_id: grammar.id.clone(),
                text: example.target.text.clone(),
                character_id: None,
                words: knowledge_words(&grammar.id, &example.target),
            });
        }
    }
    Ok(Inputs {
        id: lesson.id.clone(),
        revision: lesson.revision,
        cast: lesson.cast.clone(),
        version: NEUTRAL_VERSION,
        entries,
    })
}
pub fn compile(lesson: &PublicLesson, source: &Value, config: &Config) -> Result<Plan> {
    compile_inputs(legacy_inputs(lesson)?, source, config)
}
pub fn compile_neutral(
    lesson: &brioche_course_contract::neutral::NeutralLesson,
    source: &Value,
    config: &Config,
) -> Result<Plan> {
    compile_inputs(neutral_inputs(lesson)?, source, config)
}
pub(crate) fn source_inputs(lesson: &crate::author_source::CheckedLesson) -> Result<Vec<Input>> {
    Ok(match lesson {
        crate::author_source::CheckedLesson::Legacy(l) => legacy_inputs(l)?,
        crate::author_source::CheckedLesson::Neutral(l) => neutral_inputs(l)?,
    }
    .entries)
}
pub(crate) fn compile_checked(
    lesson: &crate::author_source::CheckedLesson,
    source: &Value,
    config: &Config,
) -> Result<Plan> {
    match lesson {
        crate::author_source::CheckedLesson::Legacy(lesson) => compile(lesson, source, config),
        crate::author_source::CheckedLesson::Neutral(lesson) => {
            compile_neutral(lesson, source, config)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (PublicLesson, Value, Config) {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");
        let doc = crate::author_json::Document::load(root.join("examples/a1-bakery.lesson.json"))
            .unwrap();
        let lesson = crate::author_source::check_lesson(&doc).unwrap();
        let mut config: Config = serde_json::from_value(json!({"items":serde_json::from_str::<Value>(&std::fs::read_to_string(root.join("characters/voices.json")).unwrap()).unwrap()["items"],"knowledgeNarrator":{"characterId":"character-camille","characterRevision":1,"voiceRevision":1},"emotions":{}})).unwrap();
        // Synthetic test fixture only: no claim that this is an approved Léa voice.
        let mut narrator = config.items[0].clone();
        narrator.character = lesson
            .cast
            .iter()
            .find(|c| c.character_id == "character-lea")
            .unwrap()
            .clone();
        config.items.push(narrator);
        (lesson, doc.value, config)
    }
    #[test]
    fn neutral_compiler_preserves_author_phrases_and_never_downgrades_source() {
        use brioche_course_contract::{ReadingText, TargetLanguage, TextRange, neutral};
        let (legacy, _, mut config) = fixture();
        let mut lesson = neutral::NeutralLesson::try_from(&legacy).unwrap();
        lesson.target_language = TargetLanguage::Cantonese;
        for character in &mut lesson.cast {
            character.speech_locale = "yue-Hant-HK".into();
        }
        for voice in &mut config.items {
            voice.character.speech_locale = "yue-Hant-HK".into();
            let profile = voice.profile.as_mut().unwrap();
            profile.locale = "yue-Hant-HK".into();
            profile.speaking_style = "自然的香港粤语".into();
        }
        let neutral::Block::Dialogue { turns, .. } = lesson
            .blocks
            .iter_mut()
            .find(|b| matches!(b, neutral::Block::Dialogue { .. }))
            .unwrap()
        else {
            unreachable!()
        };
        let first = &mut turns[0];
        first.segments.truncate(1);
        first.segments[0].reading = ReadingText {
            text: "兩位，唔該！".into(),
            words: vec![
                TextRange { start: 0, end: 2 },
                TextRange { start: 3, end: 5 },
            ],
            pronunciations: vec![],
        };
        let source = serde_json::to_value(&lesson).unwrap();
        let unchanged = source.clone();
        let plan = compile_neutral(&lesson, &source, &config).unwrap();
        assert_eq!(source, unchanged);
        assert_eq!(plan.compiler_version, NEUTRAL_VERSION);
        assert_eq!(plan.targets[0].text, "兩位，唔該！");
        assert_eq!(
            plan.targets[0]
                .words
                .iter()
                .map(|w| (w.text.as_str(), w.entry_start, w.entry_end))
                .collect::<Vec<_>>(),
            vec![("兩位", 0, 2), ("唔該", 3, 5)]
        );
        assert!(plan.targets.iter().any(|t| t.pointer.ends_with("/target")));
        assert!(!plan.targets.iter().any(|t| t.pointer.ends_with("/fr")));
        assert_eq!(
            plan.plan_hash,
            compile_neutral(&lesson, &source, &config)
                .unwrap()
                .plan_hash
        );
        let dispatched = compile_checked(
            &crate::author_source::CheckedLesson::Neutral(lesson.clone()),
            &source,
            &config,
        )
        .unwrap();
        assert_eq!(plan.plan_hash, dispatched.plan_hash);
        config.items[0].profile.as_mut().unwrap().locale = "fr-FR".into();
        assert!(compile_neutral(&lesson, &source, &config).is_err());
    }
    #[test]
    fn covers_all_readings_and_knowledge_with_fixed_narrator_and_stable_hashes() {
        let (lesson, source, config) = fixture();
        let plan = compile(&lesson, &source, &config).unwrap();
        let reading_count: usize = lesson
            .blocks
            .iter()
            .map(|b| match b {
                Block::Dialogue { turns, .. } => turns.len(),
                Block::Article { paragraphs, .. } => paragraphs.len(),
                _ => 0,
            })
            .sum();
        assert_eq!(
            plan.targets.len(),
            reading_count
                + lesson.knowledge.vocabulary.len()
                + lesson
                    .knowledge
                    .grammar
                    .iter()
                    .map(|g| g.examples.len())
                    .sum::<usize>()
        );
        assert!(
            plan.targets
                .iter()
                .filter(|t| t.pointer.contains("/paragraphs/"))
                .all(|t| t.voice.character_id == "character-lea")
        );
        assert_eq!(
            plan.plan_hash,
            compile(&lesson, &source, &config).unwrap().plan_hash
        );
        for target in &plan.targets {
            let scalars: Vec<_> = target.text.chars().collect();
            for word in &target.words {
                assert_eq!(
                    word.text,
                    scalars[word.entry_start..word.entry_end]
                        .iter()
                        .collect::<String>()
                );
            }
            assert!(!target.text.contains("我想要"));
        }
        assert!(plan.requests.len() < plan.targets.len());
    }
    #[test]
    fn rejects_missing_ambiguous_invalid_and_unrecognized_selections() {
        let (lesson, source, mut config) = fixture();
        let narrator = config.items.pop().unwrap();
        assert!(
            compile(&lesson, &source, &config)
                .err()
                .unwrap()
                .to_string()
                .contains("fixed voice")
        );
        config.items.push(narrator);
        config
            .emotions
            .insert("/not-a-target".into(), "Friendly".into());
        assert!(compile(&lesson, &source, &config).is_err());
        config.emotions.clear();
        config.items[0].voice_revision = 0;
        assert!(compile(&lesson, &source, &config).is_err());
        config.items[0].voice_revision = 1;
        config.items.push(config.items[0].clone());
        assert!(compile(&lesson, &source, &config).is_err());
    }
    #[test]
    fn emotion_and_profile_changes_invalidate_only_affected_requests() {
        let (lesson, source, mut config) = fixture();
        let original = compile(&lesson, &source, &config).unwrap();
        config.emotions.insert(
            original.targets[0].pointer.clone(),
            "A little surprised, warmly".into(),
        );
        let changed = compile(&lesson, &source, &config).unwrap();
        assert_ne!(original.plan_hash, changed.plan_hash);
        assert_ne!(
            original.targets[0].generation_key,
            changed.targets[0].generation_key
        );
        assert_eq!(
            original.targets[1].generation_key,
            changed.targets[1].generation_key
        );
        config.items[0].profile.as_mut().unwrap().rate = 0.75;
        assert_ne!(
            changed.targets[0].generation_key,
            compile(&lesson, &source, &config).unwrap().targets[0].generation_key
        );
    }
    #[test]
    fn refuses_segment_boundaries_inside_words_before_creating_requests() {
        let (mut lesson, source, config) = fixture();
        let original = compile(&lesson, &source, &config).unwrap();
        let Block::Dialogue { turns, .. } = lesson
            .blocks
            .iter_mut()
            .find(|b| matches!(b, Block::Dialogue { .. }))
            .unwrap()
        else {
            unreachable!()
        };
        let segments = turns[0].segments.clone();
        turns[0].segments = vec![
            Segment {
                id: "split-prefix".into(),
                text: "Je m’".into(),
                vocabulary_id: None,
                grammar_id: None,
            },
            Segment {
                id: "split-word".into(),
                text: "appelle Camille.".into(),
                vocabulary_id: None,
                grammar_id: None,
            },
        ];
        let error = compile(&lesson, &source, &config).err().unwrap();
        assert!(error.to_string().contains("segments split a whole word"));
        let Block::Dialogue { turns, .. } = lesson
            .blocks
            .iter_mut()
            .find(|b| matches!(b, Block::Dialogue { .. }))
            .unwrap()
        else {
            unreachable!()
        };
        turns[0].segments = segments;
        let restored = compile(&lesson, &source, &config).unwrap();
        assert_eq!(original.plan_hash, restored.plan_hash);
    }
    #[test]
    fn scalar_ranges_preserve_apostrophes_accents_and_combining_marks() {
        let segments = vec![Segment {
            id: "s".into(),
            text: "🙂 L’été, s’il plaît, cafe\u{301} ! petit-déjeuner 32,3".into(),
            vocabulary_id: None,
            grammar_id: None,
        }];
        let targets = words(&segments);
        assert_eq!(
            targets.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(),
            vec![
                "L’été",
                "s’il",
                "plaît",
                "cafe\u{301}",
                "petit",
                "déjeuner",
                "32,3"
            ]
        );
        assert_eq!(targets[0].segment_start, 2);
    }
}
