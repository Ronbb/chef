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
pub fn compile(lesson: &PublicLesson, source: &Value, config: &Config) -> Result<Plan> {
    lesson
        .validate()
        .map_err(|e| anyhow!("invalid lesson: {e}"))?;
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
            lesson.cast.iter().any(
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
        compiler_version: VERSION,
        lesson_id: lesson.id.clone(),
        lesson_revision: lesson.revision,
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
        if block_id.is_some() {
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
        let request = json!({"compilerVersion":VERSION,"voice":voice_key,"profile":profile,"parameters":parameters});
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
                    add(
                        pointer.clone(),
                        Some(id.clone()),
                        turn.id.clone(),
                        turn.segments.iter().map(|s| s.text.as_str()).collect(),
                        resolve(&speaker.character_id, &pointer)?,
                        words(&turn.segments),
                    )?;
                }
            }
            Block::Article {
                id,
                narrator_id,
                paragraphs,
                ..
            } => {
                for (pi, paragraph) in paragraphs.iter().enumerate() {
                    let pointer = format!("/blocks/{bi}/paragraphs/{pi}");
                    add(
                        pointer.clone(),
                        Some(id.clone()),
                        paragraph.id.clone(),
                        paragraph.segments.iter().map(|s| s.text.as_str()).collect(),
                        resolve(narrator_id, &pointer)?,
                        words(&paragraph.segments),
                    )?;
                }
            }
            _ => {}
        }
    }
    for (vi, vocabulary) in lesson.knowledge.vocabulary.iter().enumerate() {
        add(
            format!("/knowledge/vocabulary/{vi}/lemma"),
            None,
            vocabulary.id.clone(),
            vocabulary.lemma.clone(),
            config.knowledge_narrator.clone(),
            Vec::new(),
        )?;
    }
    for (gi, grammar) in lesson.knowledge.grammar.iter().enumerate() {
        for (ei, example) in grammar.examples.iter().enumerate() {
            add(
                format!("/knowledge/grammar/{gi}/examples/{ei}/fr"),
                None,
                grammar.id.clone(),
                example.fr.clone(),
                config.knowledge_narrator.clone(),
                Vec::new(),
            )?;
        }
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
