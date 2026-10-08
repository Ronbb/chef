//! Version 2 public course document. Never project Cantonese into a French API response.
use super::*;
#[path = "neutral_legacy.rs"]
mod legacy;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, TS)]
pub enum ExplanationLanguage {
    #[serde(rename = "zh-CN")]
    SimplifiedChinese,
    #[serde(rename = "zh-Hant-HK")]
    TraditionalChinese,
}

dto!(NeutralTitle {
    target: String,
    zh: String
});
dto!(NeutralSegment { id:String, reading:ReadingText, vocabulary_id:Option<String>, grammar_id:Option<String> });
dto!(NeutralTurn { id:String, speaker_id:String, segments:Vec<NeutralSegment>, translation_zh:String });
dto!(NeutralParagraph { id:String, segments:Vec<NeutralSegment>, translation_zh:String });
dto!(NeutralVocabulary { id:String, lemma:ReadingText, part_of_speech:String, gender:Option<String>, meaning_zh:String, note_zh:String,
    #[serde(default,skip_serializing_if="Option::is_none")] recording:Option<KnowledgeRecording> });
dto!(NeutralGrammarExample { target:ReadingText, zh:String,
    #[serde(default,skip_serializing_if="Option::is_none")] recording:Option<KnowledgeRecording> });
dto!(NeutralGrammar { id:String,title_zh:String,body_zh:String,examples:Vec<NeutralGrammarExample> });
dto!(NeutralKnowledge { vocabulary:Vec<NeutralVocabulary>,grammar:Vec<NeutralGrammar> });

exercises! { "NeutralExercise";
    SingleChoice => "single-choice" { prompt_zh:String, options:Vec<OptionItem> },
    FillBlank => "fill-blank" { prompt_zh:String, template_target:String, hint_zh:String },
    Order => "order" { prompt_zh:String,tokens:Vec<OptionItem> },
}
blocks! { "NeutralBlock";
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
        turns: Vec<NeutralTurn>,
    },
    #[serde(rename_all = "camelCase")]
    Article {
        id: String,
        title_zh: String,
        narrator_id: String,
        paragraphs: Vec<NeutralParagraph>,
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
dto!(NeutralLesson {
    schema_version:String, target_language:TargetLanguage, explanation_language:ExplanationLanguage,
    id:String,revision:u32,level_id:String,unit_id:String,title:NeutralTitle,
    summary_zh:String,estimated_minutes:u32,objectives_zh:Vec<String>,knowledge:NeutralKnowledge,
    blocks:Vec<Block>,steps:Vec<Step>,completion:Completion,review_item_ids:Vec<String>,cast:Vec<Character>,
    #[serde(default)] media:Vec<MediaAsset>,
    #[serde(default,skip_serializing_if="Vec::is_empty")] audio:Vec<AudioAsset>,
    #[serde(default,skip_serializing_if="Vec::is_empty")] audio_tracks:Vec<AudioTrack>
});

dto!(NeutralLessonSummary {
    id: String,
    revision: u32,
    level_id: String,
    unit_id: String,
    title: NeutralTitle,
    target_language: TargetLanguage,
    explanation_language: ExplanationLanguage,
    summary_zh: String,
    estimated_minutes: u32
});
dto!(NeutralUnit { id:String,title_zh:String,lessons:Vec<NeutralLessonSummary> });
dto!(NeutralLevel { id:String,label:String,units:Vec<NeutralUnit> });
dto!(NeutralCatalog { levels:Vec<NeutralLevel>,development_fixture:bool });
dto!(NeutralLearningSession {
    lesson: NeutralLesson,
    progress: LearningState
});
dto!(NeutralLearningOverviewItem { session_id:String,lesson_id:String,revision:u32,title:NeutralTitle,last_step_id:Option<String>,completed_at:Option<String>,first_completed_at:Option<String>,updated_at:String });
dto!(NeutralLearningOverview { items:Vec<NeutralLearningOverviewItem>,next_cursor:Option<String>,completed_lessons:u32 });
dto!(NeutralStudyDashboard { local_date:String,time_zone:String,week_start:String,days:Vec<StudyDay>,active_days:u8,weekly_goal_days:u8,daily_goal_minutes:u8,due_reviews:u32,next_review_at:Option<String>,completed_lessons:u32,resume:Option<NeutralLearningOverviewItem>,recommended_lesson:Option<NeutralLessonSummary>,all_available_completed:bool,course_states:Vec<NeutralLearningOverviewItem>,catalog:NeutralCatalog });
dto!(NeutralReviewCard {
    id: String,
    knowledge_id: String,
    source_lesson_id: String,
    source_revision: u32,
    vocabulary: NeutralVocabulary,
    stage: i16,
    due_at: String,
    version: u32,
    suspended: bool
});
dto!(NeutralReviewQueue { items:Vec<NeutralReviewCard>,due_count:u32,next_due_at:Option<String>,local_date:String,time_zone:String });
dto!(NeutralReviewCardsPage { items:Vec<NeutralReviewCard>,next_cursor:Option<String> });
dto!(NeutralReviewAttemptResult {
    card: NeutralReviewCard,
    reviewed_at: String,
    time_zone: String
});
dto!(NeutralSavedItem { id:String,knowledge_id:String,source_lesson_id:String,source_revision:u32,vocabulary:Option<NeutralVocabulary>,saved:bool,withdrawn:bool,version:u32,created_at:String });
dto!(NeutralSavedPage { items:Vec<NeutralSavedItem>,next_cursor:Option<String> });
dto!(NeutralReviewHistoryItem { id:String,card_id:String,vocabulary:Option<NeutralVocabulary>,withdrawn:bool,rating:ReviewRating,old_stage:i16,new_stage:i16,reviewed_at:String,due_at:String,time_zone:String,algorithm_version:String });
dto!(NeutralReviewHistoryPage { items:Vec<NeutralReviewHistoryItem>,next_cursor:Option<String> });
impl NeutralLesson {
    pub fn summary(&self) -> NeutralLessonSummary {
        NeutralLessonSummary {
            id: self.id.clone(),
            revision: self.revision,
            level_id: self.level_id.clone(),
            unit_id: self.unit_id.clone(),
            title: self.title.clone(),
            target_language: self.target_language,
            explanation_language: self.explanation_language,
            summary_zh: self.summary_zh.clone(),
            estimated_minutes: self.estimated_minutes,
        }
    }
}

/// Read a public document without private author fields or an implicit v2-to-v1 downgrade.
pub fn decode_public(value: serde_json::Value) -> Result<NeutralLesson, String> {
    match value
        .get("schemaVersion")
        .and_then(serde_json::Value::as_str)
    {
        Some("1.0") => {
            let legacy: PublicLesson =
                serde_json::from_value(value).map_err(|_| "invalid public v1 document")?;
            NeutralLesson::try_from(&legacy)
        }
        Some("2.0") => {
            let lesson: NeutralLesson =
                serde_json::from_value(value).map_err(|_| "invalid public v2 document")?;
            lesson.validate()?;
            Ok(lesson)
        }
        _ => Err("unsupported public course version".into()),
    }
}

impl NeutralLesson {
    fn readings(&self) -> Vec<(&ReadingText, String)> {
        let mut texts = Vec::new();
        for (i, v) in self.knowledge.vocabulary.iter().enumerate() {
            texts.push((&v.lemma, format!("/knowledge/vocabulary/{i}/lemma")));
        }
        for (i, g) in self.knowledge.grammar.iter().enumerate() {
            for (j, e) in g.examples.iter().enumerate() {
                texts.push((
                    &e.target,
                    format!("/knowledge/grammar/{i}/examples/{j}/target"),
                ));
            }
        }
        for (bi, block) in self.blocks.iter().enumerate() {
            match block {
                Block::Dialogue { turns, .. } => {
                    for (ti, t) in turns.iter().enumerate() {
                        for (si, s) in t.segments.iter().enumerate() {
                            texts.push((
                                &s.reading,
                                format!("/blocks/{bi}/turns/{ti}/segments/{si}/reading"),
                            ));
                        }
                    }
                }
                Block::Article { paragraphs, .. } => {
                    for (pi, p) in paragraphs.iter().enumerate() {
                        for (si, s) in p.segments.iter().enumerate() {
                            texts.push((
                                &s.reading,
                                format!("/blocks/{bi}/paragraphs/{pi}/segments/{si}/reading"),
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        texts
    }
    pub fn validate_intrinsic(&self) -> Result<(), String> {
        if self.schema_version != "2.0" {
            return Err("/schemaVersion: unsupported version".into());
        }
        for (reading, path) in self.readings() {
            reading
                .validate(self.target_language)
                .map_err(|error| format!("{path}{error}"))?;
        }
        self.validation_projection()?
            .validate_intrinsic_for_locale(self.target_language.locale())
            .map_err(neutral_error)
    }
    pub fn validate(&self) -> Result<(), String> {
        self.validate_intrinsic()?;
        self.validation_projection()?
            .validate_audio_with_word_policy(false)
            .map_err(neutral_error)?;
        for (ti, track) in self.audio_tracks.iter().enumerate() {
            for (ci, cue) in track.cues.iter().enumerate() {
                if let Some(range) = &cue.word_range {
                    let reading = self
                        .segment_reading(
                            &track.block_id,
                            &cue.entry_id,
                            cue.segment_id.as_deref().unwrap_or(""),
                        )
                        .ok_or_else(|| {
                            format!("/audioTracks/{ti}/cues/{ci}: unknown reading segment")
                        })?;
                    reading.validate_audio_word_range(range).map_err(|error| {
                        format!("/audioTracks/{ti}/cues/{ci}/wordRange: {error}")
                    })?;
                }
            }
        }
        Ok(())
    }
    fn segment_reading(
        &self,
        block_id: &str,
        entry_id: &str,
        segment_id: &str,
    ) -> Option<&ReadingText> {
        match self.blocks.iter().find(|b| b.id() == block_id)? {
            Block::Dialogue { turns, .. } => turns
                .iter()
                .find(|t| t.id == entry_id)?
                .segments
                .iter()
                .find(|s| s.id == segment_id)
                .map(|s| &s.reading),
            Block::Article { paragraphs, .. } => paragraphs
                .iter()
                .find(|p| p.id == entry_id)?
                .segments
                .iter()
                .find(|s| s.id == segment_id)
                .map(|s| &s.reading),
            _ => None,
        }
    }
    // Private structural view for the shared validator. It is never returned or persisted as a public v1 course.
    // Locale remains the real target language; only field locations are mapped to the existing kernel.
    fn validation_projection(&self) -> Result<PublicLesson, String> {
        let mut value = serde_json::to_value(self)
            .map_err(|_| "cannot construct course validation view".to_owned())?;
        let root = value.as_object_mut().unwrap();
        root.remove("targetLanguage");
        root.remove("explanationLanguage");
        root.insert("schemaVersion".into(), serde_json::json!("1.0"));
        let title = root.get_mut("title").unwrap().as_object_mut().unwrap();
        let target = title.remove("target").unwrap();
        title.insert("fr".into(), target);
        for vocabulary in value["knowledge"]["vocabulary"].as_array_mut().unwrap() {
            vocabulary["lemma"] = vocabulary["lemma"]["text"].clone();
        }
        for grammar in value["knowledge"]["grammar"].as_array_mut().unwrap() {
            for example in grammar["examples"].as_array_mut().unwrap() {
                let fields = example.as_object_mut().unwrap();
                let target = fields.remove("target").unwrap();
                fields.insert("fr".into(), target["text"].clone());
            }
        }
        for block in value["blocks"].as_array_mut().unwrap() {
            match block["type"].as_str().unwrap() {
                "dialogue" | "article" => {
                    let entries = if block["type"] == "dialogue" {
                        "turns"
                    } else {
                        "paragraphs"
                    };
                    for entry in block[entries].as_array_mut().unwrap() {
                        for segment in entry["segments"].as_array_mut().unwrap() {
                            let fields = segment.as_object_mut().unwrap();
                            let reading = fields.remove("reading").unwrap();
                            fields.insert("text".into(), reading["text"].clone());
                        }
                    }
                }
                "exercise" if block["exerciseType"] == "fill-blank" => {
                    let fields = block.as_object_mut().unwrap();
                    let template = fields.remove("templateTarget").unwrap();
                    fields.insert("templateFr".into(), template);
                }
                _ => {}
            }
        }
        parse_type_value(value, "").map_err(neutral_error)
    }
}
fn neutral_error(error: String) -> String {
    error
        .replace("/title/fr:", "/title/target:")
        .replace("/fr:", "/target:")
        .replace("/templateFr:", "/templateTarget:")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    // Synthetic protocol conversion only; no claim of linguistic Cantonese content or a production v1 adapter.
    fn reading(text: Value) -> Value {
        let text = text.as_str().unwrap();
        let positions: Vec<_> = text
            .chars()
            .enumerate()
            .filter(|(_, c)| c.is_alphanumeric())
            .map(|(i, _)| i)
            .collect();
        let words = match (positions.first(), positions.last()) {
            (Some(start), Some(end)) => vec![json!({"start":start,"end":end+1})],
            _ => vec![],
        };
        json!({"text":text,"words":words})
    }
    fn fixture_value() -> Value {
        let mut v = serde_json::to_value(crate::tests::fixture()).unwrap();
        v["schemaVersion"] = json!("2.0");
        v["targetLanguage"] = json!("fr-FR");
        v["explanationLanguage"] = json!("zh-CN");
        let title = v["title"].as_object_mut().unwrap();
        let target = title.remove("fr").unwrap();
        title.insert("target".into(), target);
        for vocab in v["knowledge"]["vocabulary"].as_array_mut().unwrap() {
            vocab["lemma"] = reading(vocab["lemma"].take());
        }
        for grammar in v["knowledge"]["grammar"].as_array_mut().unwrap() {
            for example in grammar["examples"].as_array_mut().unwrap() {
                let fields = example.as_object_mut().unwrap();
                let target = fields.remove("fr").unwrap();
                fields.insert("target".into(), reading(target));
            }
        }
        for block in v["blocks"].as_array_mut().unwrap() {
            match block["type"].as_str().unwrap() {
                "dialogue" | "article" => {
                    let entries = if block["type"] == "dialogue" {
                        "turns"
                    } else {
                        "paragraphs"
                    };
                    for entry in block[entries].as_array_mut().unwrap() {
                        for segment in entry["segments"].as_array_mut().unwrap() {
                            let fields = segment.as_object_mut().unwrap();
                            let text = fields.remove("text").unwrap();
                            fields.insert("reading".into(), reading(text));
                        }
                    }
                }
                "exercise" if block["exerciseType"] == "fill-blank" => {
                    let fields = block.as_object_mut().unwrap();
                    let template = fields.remove("templateFr").unwrap();
                    fields.insert("templateTarget".into(), template);
                }
                _ => {}
            }
        }
        v
    }
    fn fixture() -> NeutralLesson {
        serde_json::from_value(fixture_value()).unwrap()
    }
    fn cantonese() -> NeutralLesson {
        let mut lesson = fixture();
        lesson.target_language = TargetLanguage::Cantonese;
        lesson.title.target = "打招呼".into();
        lesson.level_id = "starter".into();
        for character in &mut lesson.cast {
            character.speech_locale = "yue-Hant-HK".into();
        }
        // Change every synthetic reading recursively, independently of French space tokenization.
        let mut value = serde_json::to_value(lesson).unwrap();
        fn change(value: &mut Value) {
            match value {
                Value::Object(fields)
                    if fields.contains_key("words") && fields.contains_key("text") =>
                {
                    *value = json!({"text":"你好","words":[{"start":0,"end":1},{"start":1,"end":2}],"pronunciations":[{"range":{"start":0,"end":1},"system":"jyutping","text":"nei5"},{"range":{"start":1,"end":2},"system":"jyutping","text":"hou2"}]});
                }
                Value::Object(fields) => {
                    for child in fields.values_mut() {
                        change(child)
                    }
                }
                Value::Array(items) => {
                    for item in items {
                        change(item)
                    }
                }
                _ => {}
            }
        }
        change(&mut value);
        serde_json::from_value(value).unwrap()
    }
    #[test]
    fn full_french_and_cantonese_documents_share_structural_validation_without_french_wire_fields()
    {
        fixture().validate().unwrap();
        let lesson = cantonese();
        lesson.validate().unwrap();
        let value = serde_json::to_value(lesson).unwrap();
        fn keys(value: &Value) {
            match value {
                Value::Object(fields) => {
                    for (key, child) in fields {
                        assert!(key != "fr" && key != "templateFr");
                        keys(child);
                    }
                }
                Value::Array(items) => {
                    for item in items {
                        keys(item)
                    }
                }
                _ => {}
            }
        }
        keys(&value);
        assert_eq!(value["targetLanguage"], "yue-Hant-HK");
        assert_eq!(value["levelId"], "starter");
    }
    #[test]
    fn target_locale_is_real_and_legacy_french_policy_cannot_be_bypassed() {
        let mut lesson = cantonese();
        lesson.cast[0].speech_locale = "fr-FR".into();
        assert!(
            lesson
                .validate()
                .unwrap_err()
                .contains("speechLocale: expected yue-Hant-HK")
        );
        let mut legacy = crate::tests::fixture();
        legacy.cast[0].speech_locale = "yue-Hant-HK".into();
        assert!(legacy.validate().is_err());
    }
    #[test]
    fn strict_nested_wire_and_version_do_not_silently_accept_legacy_fields() {
        let value = fixture_value();
        let paths = [
            "/title/fr",
            "/knowledge/vocabulary/0/untrusted",
            "/knowledge/grammar/0/examples/0/fr",
        ];
        for path in paths {
            let mut bad = value.clone();
            let (parent, key) = path.rsplit_once('/').unwrap();
            bad.pointer_mut(parent)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert(key.into(), json!("forged"));
            assert!(
                serde_json::from_value::<NeutralLesson>(bad).is_err(),
                "{path}"
            );
        }
        for block in value["blocks"].as_array().unwrap() {
            let mut bad = block.clone();
            bad["forged"] = json!(true);
            assert!(serde_json::from_value::<Block>(bad).is_err());
        }
        let mut lesson = fixture();
        lesson.schema_version = "1.0".into();
        assert!(
            lesson
                .validate()
                .unwrap_err()
                .starts_with("/schemaVersion:")
        );
    }
    #[test]
    fn references_completion_and_neutral_field_diagnostics_reuse_the_kernel() {
        let mut lesson = fixture();
        lesson.title.target = " ".into();
        assert!(lesson.validate().unwrap_err().starts_with("/title/target:"));
        let mut lesson = fixture();
        lesson.completion.required_step_ids = vec!["missing".into()];
        assert!(lesson.validate().is_err());
        let mut lesson = fixture();
        lesson.steps[0].block_ids = vec!["missing".into()];
        assert!(lesson.validate().is_err());
        let mut lesson = cantonese();
        lesson.knowledge.vocabulary[0].lemma.words.clear();
        assert!(
            lesson
                .validate()
                .unwrap_err()
                .starts_with("/knowledge/vocabulary/0/lemma/words:")
        );
    }
    #[test]
    fn public_v2_audio_requires_a_measured_valid_track_and_an_authored_word_range() {
        let mut lesson = cantonese();
        let (block_id, entry_id, segment_id) = lesson
            .blocks
            .iter()
            .find_map(|block| match block {
                Block::Dialogue { id, turns, .. } => Some((
                    id.clone(),
                    turns[0].id.clone(),
                    turns[0].segments[0].id.clone(),
                )),
                _ => None,
            })
            .unwrap();
        lesson.audio = vec![AudioAsset {
            asset_id: "recording".into(),
            revision: 1,
            sha256: "a".repeat(64),
            mime_type: "audio/wav".into(),
            duration_ms: 1000,
            credit_zh: "合成测试".into(),
            url: format!("/api/audio/{}.wav", "a".repeat(64)),
        }];
        lesson.audio_tracks = vec![AudioTrack {
            block_id,
            asset_id: "recording".into(),
            cues: vec![AudioCue {
                entry_id,
                segment_id: Some(segment_id),
                word_range: Some(AudioWordRange { start: 0, end: 1 }),
                start_ms: 100,
                end_ms: 300,
            }],
        }];
        let ids = lesson
            .blocks
            .iter()
            .find_map(|block| match block {
                Block::Dialogue { id, turns, .. } if id == &lesson.audio_tracks[0].block_id => {
                    Some(turns.iter().map(|turn| turn.id.clone()).collect::<Vec<_>>())
                }
                _ => None,
            })
            .unwrap();
        let word = lesson.audio_tracks[0].cues[0].clone();
        lesson.audio_tracks[0].cues = ids
            .into_iter()
            .enumerate()
            .map(|(i, id)| AudioCue {
                entry_id: id,
                segment_id: None,
                word_range: None,
                start_ms: (i as u32) * 100,
                end_ms: (i as u32 + 1) * 100,
            })
            .collect();
        lesson.audio_tracks[0].cues.insert(
            0,
            AudioCue {
                entry_id: word.entry_id.clone(),
                segment_id: word.segment_id.clone(),
                word_range: None,
                start_ms: 0,
                end_ms: 100,
            },
        );
        lesson.audio_tracks[0].cues.insert(
            0,
            AudioCue {
                start_ms: 10,
                end_ms: 50,
                ..word
            },
        );
        lesson.validate().unwrap();
        lesson.audio_tracks[0].cues[0].word_range = Some(AudioWordRange { start: 0, end: 2 });
        assert!(
            lesson
                .validate()
                .unwrap_err()
                .contains("wordRange: word audio cue must reference an authored word range")
        );
        lesson.audio_tracks[0].cues[0].word_range = Some(AudioWordRange { start: 0, end: 1 });
        lesson.audio_tracks[0].cues[0].end_ms = 1001;
        assert!(lesson.validate().is_err());
    }
}
