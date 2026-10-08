//! Public lesson DTOs are a whitelist, independent of editorial and grading data.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use ts_rs::TS;
use unicode_normalization::UnicodeNormalization;
mod admin;
mod audio;
pub use admin::*;
mod validation;

/// NFC, whitespace and French apostrophe variants are equivalent; accents remain meaningful.
pub fn normalize_text(text: &str, case_sensitive: bool) -> String {
    let normalized: String = text
        .nfc()
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' | '\u{02bc}' => '\'',
            _ => c,
        })
        .collect();
    let normalized = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    if case_sensitive {
        normalized
    } else {
        normalized.to_lowercase()
    }
}

/// Current registered character voice policy; regional voices need an explicit content revision.
pub const CHARACTER_SPEECH_LOCALE: &str = "fr-FR";

/// French-supported Flash voices, verified against the official list on 2026-10-07.
/// https://help.aliyun.com/zh/model-studio/qwen-audio-tts-voice-list
pub const QWEN_FRENCH_SYSTEM_VOICES: &[(&str, &str)] = &[
    ("longanlingxin_v3.1", "龙安灵心"),
    ("xunanchuan_v3.1", "许南川"),
    ("longanhuan_v3.1", "龙安欢"),
    ("longanfengyue_v3.1", "龙安风悦"),
];

/// HTML maxlength and JavaScript string length count UTF-16 code units.
pub const MAX_TEXT_ANSWER_UTF16_UNITS: usize = 1024;
pub const MAX_TEXT_ANSWER_BYTES: usize = 4096;
pub fn valid_text_answer_length(text: &str) -> bool {
    text.len() <= MAX_TEXT_ANSWER_BYTES
        && text
            .encode_utf16()
            .take(MAX_TEXT_ANSWER_UTF16_UNITS + 1)
            .count()
            <= MAX_TEXT_ANSWER_UTF16_UNITS
}

/// Immutable revisions share the positive PostgreSQL INTEGER range.
pub fn valid_content_revision(value: u32) -> bool {
    value > 0 && value <= i32::MAX as u32
}

/// Stable author IDs shared by routing, media registration and saved-operation recovery.
pub fn valid_content_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

macro_rules! dto {
    ($name:ident { $($(#[$meta:meta])* $field:ident : $ty:ty),* $(,)? }) => {
        #[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, TS)]
        #[serde(rename_all="camelCase", deny_unknown_fields)]
        pub struct $name { $($(#[$meta])* pub $field: $ty),* }
    };
}
mod reading;
pub use reading::{Pronunciation, PronunciationSystem, ReadingText, TargetLanguage, TextRange};

dto!(Title {
    fr: String,
    zh: String
});
dto!(Vocabulary { id: String, lemma: String, part_of_speech: String, gender: Option<String>, meaning_zh: String, note_zh: String,
    #[serde(default, skip_serializing_if="Option::is_none")] recording: Option<KnowledgeRecording> });
dto!(GrammarExample { fr: String, zh: String,
    #[serde(default, skip_serializing_if="Option::is_none")] recording: Option<KnowledgeRecording> });
dto!(Grammar { id: String, title_zh: String, body_zh: String, examples: Vec<GrammarExample> });
dto!(Knowledge { vocabulary: Vec<Vocabulary>, grammar: Vec<Grammar> });
dto!(Character {
    character_id: String,
    revision: u32,
    display_name: String,
    avatar_id: String,
    speech_locale: String
});
dto!(MediaAsset {
    asset_id: String,
    revision: u32,
    sha256: String,
    mime_type: String,
    width: u32,
    height: u32,
    alt_zh: String,
    credit_zh: String,
    url: String
});
dto!(AudioAsset {
    asset_id: String,
    revision: u32,
    sha256: String,
    mime_type: String,
    duration_ms: u32,
    credit_zh: String,
    url: String
});
// Inline fixed descriptors survive vocabulary snapshots in saved items and review cards.
// They must exactly match this lesson's registered audio registry.
dto!(KnowledgeRecording {
    asset: AudioAsset,
    start_ms: u32,
    end_ms: u32
});
// Unicode scalar offsets within the referenced segment, never UTF-16 offsets.
dto!(AudioWordRange {
    start: u32,
    end: u32
});
dto!(AudioCue { entry_id: String, segment_id: Option<String>, word_range: Option<AudioWordRange>, start_ms: u32, end_ms: u32 });
dto!(AudioTrack { block_id: String, asset_id: String, cues: Vec<AudioCue> });
dto!(Speaker {
    id: String,
    label_zh: String,
    character_id: String,
    display_name: String,
    avatar_id: String
});
dto!(Segment { id: String, text: String, vocabulary_id: Option<String>, grammar_id: Option<String> });
dto!(Turn { id: String, speaker_id: String, segments: Vec<Segment>, translation_zh: String });
dto!(Paragraph { id: String, segments: Vec<Segment>, translation_zh: String });
dto!(Target {
    block_id: String,
    entry_id: String,
    segment_id: String
});
dto!(OptionItem {
    id: String,
    text: String
});
dto!(Step { id: String, kind: String, title_zh: String, block_ids: Vec<String> });
dto!(Completion { strategy: String, required_step_ids: Vec<String>, required_exercise_ids: Vec<String> });

fn parse_type_value<T: serde::de::DeserializeOwned>(
    value: serde_json::Value,
    prefix: &str,
) -> Result<T, String> {
    serde_path_to_error::deserialize(value).map_err(|error| {
        let mut pointer = prefix.to_owned();
        for segment in error.path().iter() {
            let token = match segment {
                serde_path_to_error::Segment::Seq { index } => index.to_string(),
                serde_path_to_error::Segment::Map { key } => key.clone(),
                _ => continue,
            };
            pointer.push('/');
            pointer.push_str(&token.replace('~', "~0").replace('/', "~1"));
        }
        format!(
            "{}: {}",
            if pointer.is_empty() { "/" } else { &pointer },
            error.inner()
        )
    })
}

macro_rules! exercises {
    ($ts_name:literal; $($variant:ident => $kind:literal { $($field:ident: $ty:ty),* $(,)? }),* $(,)?) => {
        #[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, TS)]
        #[serde(tag = "exerciseType", rename_all = "kebab-case", deny_unknown_fields)]
        #[ts(rename = $ts_name)]
        pub enum Exercise {
            $(#[serde(rename_all = "camelCase")] $variant { $($field: $ty),* },)*
        }
        mod exercise_wire {
            use super::*;
            $(#[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            pub(super) struct $variant { $(pub(super) $field: $ty),* })*
        }
        impl Exercise {
            fn from_value_with_path(mut value: serde_json::Value, prefix: &str) -> Result<Self, String> {
                let kind = value.get("exerciseType").and_then(serde_json::Value::as_str)
                    .ok_or_else(|| format!("{prefix}/exerciseType: expected exercise type string"))?.to_owned();
                value.as_object_mut().unwrap().remove("exerciseType");
                match kind.as_str() {
                    $($kind => {
                        let exercise_wire::$variant { $($field),* } = parse_type_value(value, prefix)?;
                        Ok(Self::$variant { $($field),* })
                    },)*
                    _ => Err(format!("{prefix}/exerciseType: unsupported exercise type")),
                }
            }
        }
    };
}
exercises! { "Exercise";
    SingleChoice => "single-choice" {
        prompt_zh: String,
        options: Vec<OptionItem>,
    },
    FillBlank => "fill-blank" {
        prompt_zh: String,
        template_fr: String,
        hint_zh: String,
    },
    Order => "order" {
        prompt_zh: String,
        tokens: Vec<OptionItem>,
    },
}
// Declare regular block fields once for both the public contract and strict wire parser.
// Serde does not support deny_unknown_fields on a container with flattened fields.
// The exercise branch therefore removes only its envelope before parsing Exercise.
macro_rules! blocks {
    ($ts_name:literal; $($(#[$meta:meta])* $variant:ident { $($field:ident: $ty:ty),* $(,)? }),* $(,)?) => {
        #[derive(Clone, Debug, Serialize, JsonSchema, TS)]
        #[serde(tag = "type", rename_all = "lowercase")]
        #[schemars(deny_unknown_fields)]
        #[ts(rename = $ts_name)]
        pub enum Block {
            $($(#[$meta])* $variant { $($field: $ty),* },)*
            Exercise { id: String, #[serde(flatten)] exercise: Exercise },
        }
        mod block_wire {
            use super::*;
            $(#[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            $(#[$meta])*
            pub(super) struct $variant { $(pub(super) $field: $ty),* })*
        }
        impl Block {
            /// Author tools retain nested type paths across the flat exercise envelope.
            /// Uses the same strict types as ordinary deserialization.
            pub fn from_value_with_path(mut value: serde_json::Value, prefix: &str) -> Result<Self, String> {
                let kind = value.get("type").and_then(serde_json::Value::as_str)
                    .ok_or_else(|| format!("{prefix}/type: expected block type string"))?.to_owned();
                if kind == "exercise" {
                    let fields = value.as_object_mut().ok_or_else(|| format!("{prefix}: expected block object"))?;
                    fields.remove("type");
                    let id = fields.remove("id").ok_or_else(|| format!("{prefix}/id: missing field `id`"))?;
                    let id = parse_type_value(id, &format!("{prefix}/id"))?;
                    let exercise = Exercise::from_value_with_path(value, prefix)?;
                    return Ok(Self::Exercise { id, exercise });
                }
                value.as_object_mut().unwrap().remove("type");
                match kind.as_str() {
                    $(kind if kind == stringify!($variant).to_ascii_lowercase() => {
                        let block_wire::$variant { $($field),* } = parse_type_value(value, prefix)?;
                        Ok(Self::$variant { $($field),* })
                    },)*
                    _ => Err(format!("{prefix}/type: unsupported block type")),
                }
            }
        }
        impl<'de> Deserialize<'de> for Block {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                use serde::de::Error;
                let value = serde_json::Value::deserialize(deserializer)?;
                Self::from_value_with_path(value, "").map_err(D::Error::custom)
            }
        }
    };
}
blocks! { "Block";
    #[serde(rename_all = "camelCase")]
    Scene {
        id: String,
        place_zh: String,
        situation_zh: String,
        illustration_id: String,
    },
    #[serde(rename_all = "camelCase")]
    Dialogue {
        id: String,
        title_zh: String,
        speakers: Vec<Speaker>,
        turns: Vec<Turn>,
    },
    #[serde(rename_all = "camelCase")]
    Article {
        id: String,
        title_zh: String,
        narrator_id: String,
        paragraphs: Vec<Paragraph>,
    },
    #[serde(rename_all = "camelCase")]
    Explanation {
        id: String,
        title_zh: String,
        body_zh: String,
        targets: Vec<Target>,
    },
    #[serde(rename_all = "camelCase")]
    Culture {
        id: String,
        title_zh: String,
        body_zh: String,
        scope_zh: String,
    },
    #[serde(rename_all = "camelCase")]
    Vocabulary { id: String, entry_ids: Vec<String> },
    #[serde(rename_all = "camelCase")]
    Grammar { id: String, entry_ids: Vec<String> },
    #[serde(rename_all = "camelCase")]
    Habit {
        id: String,
        task_zh: String,
        alternative_zh: String,
    },
    #[serde(rename_all = "camelCase")]
    Summary {
        id: String,
        takeaways_zh: Vec<String>,
    },
}
impl Block {
    pub fn id(&self) -> &str {
        match self {
            Self::Scene { id, .. }
            | Self::Dialogue { id, .. }
            | Self::Article { id, .. }
            | Self::Explanation { id, .. }
            | Self::Culture { id, .. }
            | Self::Vocabulary { id, .. }
            | Self::Grammar { id, .. }
            | Self::Exercise { id, .. }
            | Self::Habit { id, .. }
            | Self::Summary { id, .. } => id,
        }
    }
}
dto!(PublicLesson {
    schema_version: String, id: String, revision: u32, level_id: String, unit_id: String,
    title: Title, summary_zh: String, estimated_minutes: u32, objectives_zh: Vec<String>,
    knowledge: Knowledge, blocks: Vec<Block>, steps: Vec<Step>, completion: Completion,
    review_item_ids: Vec<String>, cast: Vec<Character>, #[serde(default)] media: Vec<MediaAsset>,
    #[serde(default, skip_serializing_if="Vec::is_empty")] audio: Vec<AudioAsset>,
    #[serde(default, skip_serializing_if="Vec::is_empty")] audio_tracks: Vec<AudioTrack>
});
dto!(LessonSummary {
    id: String,
    revision: u32,
    level_id: String,
    unit_id: String,
    title: Title,
    summary_zh: String,
    estimated_minutes: u32
});
dto!(Unit { id: String, title_zh: String, lessons: Vec<LessonSummary> });
dto!(Level { id: String, label: String, units: Vec<Unit> });
dto!(Catalog { levels: Vec<Level>, development_fixture: bool });
dto!(PreviewRelease { id: String, catalog: Catalog, withdrawn_lesson_ids: Vec<String> });
dto!(ApiError {
    code: String,
    message: String
});

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewRating {
    Again,
    Remembered,
    Familiar,
}
dto!(ReviewCard {
    id: String,
    knowledge_id: String,
    source_lesson_id: String,
    source_revision: u32,
    vocabulary: Vocabulary,
    stage: i16,
    due_at: String,
    version: u32,
    suspended: bool
});
dto!(ReviewPreferenceRequest {
    card_version: u32,
    idempotency_key: String,
    suspended: bool
});
dto!(ReviewEnrollmentRequest {
    knowledge_id: String,
    source_lesson_id: String,
    source_revision: u32,
    idempotency_key: String
});
dto!(SavedItem { id: String, knowledge_id: String, source_lesson_id: String, source_revision: u32, vocabulary: Option<Vocabulary>, saved: bool, withdrawn: bool, version: u32, created_at: String });
dto!(SavedWriteRequest {
    source_lesson_id: String,
    source_revision: u32,
    saved: bool,
    version: u32,
    idempotency_key: String
});
dto!(SavedPage { items: Vec<SavedItem>, next_cursor: Option<String> });
dto!(ReviewHistoryItem { id: String, card_id: String, vocabulary: Option<Vocabulary>, withdrawn: bool, rating: ReviewRating, old_stage: i16, new_stage: i16, reviewed_at: String, due_at: String, time_zone: String, algorithm_version: String });
dto!(ReviewHistoryPage { items: Vec<ReviewHistoryItem>, next_cursor: Option<String> });
dto!(ReviewQueue { items: Vec<ReviewCard>, due_count: u32, next_due_at: Option<String>, local_date: String, time_zone: String });
dto!(ReviewCardsPage { items: Vec<ReviewCard>, next_cursor: Option<String> });
dto!(ReviewAttemptRequest {
    card_version: u32,
    idempotency_key: String,
    rating: ReviewRating
});
dto!(ReviewAttemptResult {
    card: ReviewCard,
    reviewed_at: String,
    time_zone: String
});

/// Submitted values are IDs/text, never a client supplied score or answer key.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ExerciseAnswer {
    #[serde(rename_all = "camelCase")]
    Choice {
        option_id: String,
    },
    Text {
        text: String,
    },
    #[serde(rename_all = "camelCase")]
    Order {
        token_ids: Vec<String>,
    },
}
dto!(GradeRequest {
    revision: u32,
    exercise_id: String,
    answer: ExerciseAnswer
});
dto!(GradeResult {
    exercise_id: String,
    correct: bool,
    feedback_zh: String
});
dto!(CsrfToken { csrf_token: String });
dto!(StartLearningRequest {
    lesson_id: String,
    schema_version: String,
    idempotency_key: String
});
dto!(LearningWriteRequest {
    version: u32,
    idempotency_key: String
});
dto!(SubmitAttemptRequest {
    version: u32,
    idempotency_key: String,
    exercise_id: String,
    answer: ExerciseAnswer
});
dto!(AttemptRecord {
    id: String,
    exercise_id: String,
    attempt_index: u32,
    answer: ExerciseAnswer,
    result: GradeResult,
    hint_used: bool
});
dto!(LearningState {
    id: String, lesson_id: String, revision: u32, version: u32, last_step_id: Option<String>,
    confirmed_step_ids: Vec<String>, hinted_exercise_ids: Vec<String>, attempts: Vec<AttemptRecord>,
    completed_at: Option<String>, first_completed_at: Option<String>
});
dto!(LearningSession {
    lesson: PublicLesson,
    progress: LearningState
});
dto!(AttemptResult {
    result: GradeResult,
    progress: LearningState
});
dto!(HintResult {
    hint_zh: String,
    progress: LearningState
});
dto!(LearningOverviewItem {
    session_id: String, lesson_id: String, revision: u32, title: Title,
    last_step_id: Option<String>, completed_at: Option<String>, first_completed_at: Option<String>, updated_at: String
});
dto!(LearningOverview { items: Vec<LearningOverviewItem>, next_cursor: Option<String>, completed_lessons: u32 });
dto!(StudyDay {
    local_date: String,
    confirmed_steps: u32,
    exercise_attempts: u32,
    review_attempts: u32,
    completed_lessons: u32,
    active: bool
});
dto!(StudyDashboard { local_date: String, time_zone: String, week_start: String, days: Vec<StudyDay>, active_days: u8, weekly_goal_days: u8, daily_goal_minutes: u8, due_reviews: u32, next_review_at: Option<String>, completed_lessons: u32, resume: Option<LearningOverviewItem>, recommended_lesson: Option<LessonSummary>, all_available_completed: bool, course_states: Vec<LearningOverviewItem>, catalog: Catalog });
dto!(UserProfile {
    id: String,
    email: String,
    display_name: String,
    role: String,
    settings: UserSettings,
    version: u32
});
// Shared account versions are independent from product learning preference versions.
dto!(AccountProfile {
    id: String,
    email: String,
    display_name: String,
    role: String,
    version: u32
});
dto!(AccountProfileUpdateRequest {
    expected_account_version: u32,
    display_name: String
});
dto!(AccountAuthResult {
    user: AccountProfile,
    csrf_token: String
});
dto!(UserSettings {
    time_zone: String,
    weekly_days: u8,
    daily_minutes: u8,
    show_translation: bool,
    speech_rate: f64
});
impl Default for UserSettings {
    fn default() -> Self {
        Self {
            time_zone: "Asia/Shanghai".into(),
            weekly_days: 5,
            daily_minutes: 10,
            show_translation: false,
            speech_rate: 1.0,
        }
    }
}
dto!(UpdateProfileRequest {
    version: u32,
    display_name: Option<String>,
    time_zone: Option<String>,
    weekly_days: Option<u8>,
    daily_minutes: Option<u8>,
    show_translation: Option<bool>,
    speech_rate: Option<f64>
});
dto!(AuthResult {
    user: UserProfile,
    csrf_token: String
});
macro_rules! secret_dto {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        #[derive(Clone, Serialize, Deserialize, JsonSchema, TS)]
        #[serde(rename_all="camelCase", deny_unknown_fields)]
        pub struct $name { $(pub $field: $ty),* }
    };
}
secret_dto!(LoginRequest {
    email: String,
    password: String
});
secret_dto!(AcceptInviteRequest {
    token: String,
    email: String,
    password: String,
    display_name: String
});
secret_dto!(ResetPasswordRequest {
    token: String,
    password: String
});

impl PublicLesson {
    pub fn summary(&self) -> LessonSummary {
        LessonSummary {
            id: self.id.clone(),
            revision: self.revision,
            level_id: self.level_id.clone(),
            unit_id: self.unit_id.clone(),
            title: self.title.clone(),
            summary_zh: self.summary_zh.clone(),
            estimated_minutes: self.estimated_minutes,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        self.validate_intrinsic()?;
        self.validate_audio()
    }
    /// Course semantics independent of registered recording descriptors.
    /// Import preflight may defer media descriptors; hydrated lessons still use validate().
    pub fn validate_intrinsic(&self) -> Result<(), String> {
        self.validate_intrinsic_for_locale(CHARACTER_SPEECH_LOCALE)
    }
    pub(crate) fn validate_intrinsic_for_locale(&self, locale: &str) -> Result<(), String> {
        if self.schema_version != "1.0" {
            return Err("/schemaVersion: unsupported version".into());
        }
        if !valid_content_revision(self.revision) {
            return Err("/revision: expected revision in 1..2147483647".into());
        }
        for (id, path) in [
            (&self.id, "/id"),
            (&self.level_id, "/levelId"),
            (&self.unit_id, "/unitId"),
        ] {
            validation::identifier(id, path)?;
        }
        if self.blocks.is_empty() {
            return Err("/blocks: empty lesson".into());
        }
        let mut ids = HashSet::new();
        let mut insert = |id: &str, path: &str| {
            validation::identifier(id, path)?;
            if !ids.insert(id.to_owned()) {
                Err(format!("{path}: duplicate/empty id: {id}"))
            } else {
                Ok(())
            }
        };
        for (i, v) in self.knowledge.vocabulary.iter().enumerate() {
            insert(&v.id, &format!("/knowledge/vocabulary/{i}/id"))?;
        }
        for (i, g) in self.knowledge.grammar.iter().enumerate() {
            insert(&g.id, &format!("/knowledge/grammar/{i}/id"))?;
        }
        for (i, b) in self.blocks.iter().enumerate() {
            insert(b.id(), &format!("/blocks/{i}/id"))?;
        }
        for (i, s) in self.steps.iter().enumerate() {
            insert(&s.id, &format!("/steps/{i}/id"))?;
        }
        let vocab: HashSet<_> = self
            .knowledge
            .vocabulary
            .iter()
            .map(|v| v.id.as_str())
            .collect();
        let grammar: HashSet<_> = self
            .knowledge
            .grammar
            .iter()
            .map(|v| v.id.as_str())
            .collect();
        let mut cast = HashSet::new();
        for (index, character) in self.cast.iter().enumerate() {
            validation::identifier(
                &character.character_id,
                &format!("/cast/{index}/characterId"),
            )?;
            validation::identifier(&character.avatar_id, &format!("/cast/{index}/avatarId"))?;
            if !cast.insert(character.character_id.as_str()) {
                return Err(format!(
                    "/cast/{index}/characterId: duplicate cast character"
                ));
            }
        }
        for (index, media) in self.media.iter().enumerate() {
            validation::identifier(&media.asset_id, &format!("/media/{index}/assetId"))?;
            if !valid_content_revision(media.revision) {
                return Err(format!(
                    "/media/{index}/revision: expected revision in 1..2147483647"
                ));
            }
        }
        let check_segments = |segments: &[Segment], path: &str| -> Result<(), String> {
            for (index, s) in segments.iter().enumerate() {
                for (field, reference, known) in [
                    ("vocabularyId", s.vocabulary_id.as_deref(), &vocab),
                    ("grammarId", s.grammar_id.as_deref(), &grammar),
                ] {
                    if reference.is_some_and(|id| !known.contains(id)) {
                        return Err(format!(
                            "{path}/{index}/{field}: unknown anchor at {}",
                            s.id
                        ));
                    }
                }
            }
            Ok(())
        };
        for (bi, b) in self.blocks.iter().enumerate() {
            match b {
                Block::Dialogue {
                    speakers, turns, ..
                } => {
                    let mut speaker_ids = HashSet::new();
                    for (index, speaker) in speakers.iter().enumerate() {
                        validation::identifier(
                            &speaker.id,
                            &format!("/blocks/{bi}/speakers/{index}/id"),
                        )?;
                        if !speaker_ids.insert(speaker.id.as_str()) {
                            return Err(format!(
                                "/blocks/{bi}/speakers/{index}/id: duplicate speaker"
                            ));
                        }
                    }
                    for (si, s) in speakers.iter().enumerate() {
                        if !cast.contains(s.character_id.as_str()) {
                            return Err(format!(
                                "/blocks/{bi}/speakers/{si}/characterId: unknown cast member"
                            ));
                        }
                    }
                    for (ti, t) in turns.iter().enumerate() {
                        if !speaker_ids.contains(t.speaker_id.as_str()) {
                            return Err(format!(
                                "/blocks/{bi}/turns/{ti}/speakerId: unknown speaker"
                            ));
                        }
                        check_segments(&t.segments, &format!("/blocks/{bi}/turns/{ti}/segments"))?;
                    }
                }
                Block::Article {
                    narrator_id,
                    paragraphs,
                    ..
                } => {
                    if !cast.contains(narrator_id.as_str()) {
                        return Err(format!("/blocks/{bi}/narratorId: unknown narrator"));
                    }
                    for (pi, p) in paragraphs.iter().enumerate() {
                        check_segments(
                            &p.segments,
                            &format!("/blocks/{bi}/paragraphs/{pi}/segments"),
                        )?;
                    }
                }
                Block::Vocabulary { entry_ids, .. } => {
                    if let Some(i) = entry_ids.iter().position(|id| !vocab.contains(id.as_str())) {
                        return Err(format!("/blocks/{bi}/entryIds/{i}: unknown vocabulary"));
                    }
                }
                Block::Grammar { entry_ids, .. } => {
                    if let Some(i) = entry_ids
                        .iter()
                        .position(|id| !grammar.contains(id.as_str()))
                    {
                        return Err(format!("/blocks/{bi}/entryIds/{i}: unknown grammar"));
                    }
                }
                _ => {}
            }
        }
        for (si, s) in self.steps.iter().enumerate() {
            if let Some(i) = s
                .block_ids
                .iter()
                .position(|id| !self.blocks.iter().any(|b| b.id() == id))
            {
                return Err(format!("/steps/{si}/blockIds/{i}: unknown step block"));
            }
        }
        if let Some(i) = self
            .review_item_ids
            .iter()
            .position(|id| !vocab.contains(id.as_str()))
        {
            return Err(format!("/reviewItemIds/{i}: unknown review item"));
        }
        if let Some(i) = self
            .completion
            .required_step_ids
            .iter()
            .position(|id| !self.steps.iter().any(|s| &s.id == id))
        {
            return Err(format!(
                "/completion/requiredStepIds/{i}: unknown completion reference"
            ));
        }
        if let Some(i) = self.completion.required_exercise_ids.iter().position(|id| {
            !self
                .blocks
                .iter()
                .any(|b| matches!(b,Block::Exercise{id:bid,..} if bid==id))
        }) {
            return Err(format!(
                "/completion/requiredExerciseIds/{i}: unknown completion reference"
            ));
        }
        self.validate_flow(locale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(crate) fn fixture() -> PublicLesson {
        let mut source: serde_json::Value =
            serde_json::from_str(include_str!("../../../test-fixtures/a1-bakery.lesson.json"))
                .unwrap();
        source.as_object_mut().unwrap().remove("serverOnly");
        source.as_object_mut().unwrap().remove("editorial");
        source.as_object_mut().unwrap().remove("assetRefs");
        source.as_object_mut().unwrap().remove("audioRefs");
        serde_json::from_value(source).unwrap()
    }
    #[test]
    fn valid_sample() {
        fixture().validate().unwrap();
    }
    #[test]
    fn invalid_anchor() {
        let mut lesson = fixture();
        lesson.knowledge.vocabulary.clear();
        assert!(lesson.validate().is_err());
    }
    #[test]
    fn unknown_block_rejected() {
        assert!(serde_json::from_str::<Block>(r#"{"type":"script","id":"x"}"#).is_err());
    }
    #[test]
    fn lesson_character_and_visual_revisions_fit_the_persistence_range() {
        let mut original = serde_json::to_value(fixture()).unwrap();
        original["media"] = serde_json::json!([{
            "assetId":"revision-fixture", "revision":1, "sha256":"a".repeat(64),
            "mimeType":"image/svg+xml", "width":640, "height":470,
            "altZh":"仅版本协议测试", "creditZh":"仅测试",
            "url":format!("/api/media/{}.svg", "a".repeat(64))
        }]);
        for pointer in ["/revision", "/cast/0/revision", "/media/0/revision"] {
            for revision in [0, i32::MAX as u32 + 1, u32::MAX] {
                let mut value = original.clone();
                *value.pointer_mut(pointer).unwrap() = serde_json::json!(revision);
                let lesson: PublicLesson = serde_json::from_value(value).unwrap();
                let error = lesson
                    .validate()
                    .expect_err("out-of-range revision accepted");
                assert!(error.starts_with(&format!("{pointer}:")), "{error}");
            }
            for revision in [1, i32::MAX as u32] {
                let mut value = original.clone();
                *value.pointer_mut(pointer).unwrap() = serde_json::json!(revision);
                let lesson: PublicLesson = serde_json::from_value(value).unwrap();
                lesson.validate().unwrap();
            }
        }
    }
    #[test]
    fn block_type_paths_preserve_nested_and_escaped_keys_without_changing_wire_shape() {
        for block in fixture().blocks {
            let value = serde_json::to_value(&block).unwrap();
            let parsed = Block::from_value_with_path(value.clone(), "/blocks/0").unwrap();
            assert_eq!(serde_json::to_value(parsed).unwrap(), value);
        }
        let mut block = serde_json::to_value(fixture().blocks.remove(1)).unwrap();
        block["speakers"][0]["意外/字段~"] = serde_json::json!(42);
        let error = Block::from_value_with_path(block, "/blocks/1").unwrap_err();
        assert!(
            error.starts_with("/blocks/1/speakers/0/意外~1字段~0:"),
            "{error}"
        );
        assert!(error.contains("unknown field"), "{error}");
    }

    #[test]
    fn unknown_fields_rejected_in_every_block() {
        for block in fixture().blocks {
            let mut value = serde_json::to_value(&block).unwrap();
            // Every legal block must still round-trip, including all flattened exercises.
            let decoded: Block = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), value);
            value["unexpectedAuthorField"] = serde_json::json!("拼错的内容");
            let result = serde_json::from_value::<Block>(value);
            assert!(
                result.is_err(),
                "{} silently accepted an unknown field",
                block.id()
            );
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("unexpectedAuthorField")
            );
        }
    }
    #[test]
    fn dto_excludes_private_fields() {
        let json = serde_json::to_string(&fixture()).unwrap();
        for secret in [
            "serverOnly",
            "correctOptionId",
            "correctTokenIds",
            "accepted",
            "editorial",
        ] {
            assert!(!json.contains(secret));
        }
    }
}

pub mod neutral;
