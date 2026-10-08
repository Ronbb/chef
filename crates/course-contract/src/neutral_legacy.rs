//! Read-only v1 French adapter. Original persistence and measured cues are unchanged.
use super::*;
use std::collections::BTreeMap;
use unicode_normalization::char::is_combining_mark;

/// Preserve legacy cue spans verbatim and fill remaining French words. This is
/// deliberately a v1 adapter, never an authoring policy for Cantonese source.
fn reading(text: &str, mut fixed: Vec<TextRange>) -> Result<ReadingText, String> {
    let chars: Vec<char> = text.chars().collect();
    fixed.sort_by_key(|r| (r.start, r.end));
    fixed.dedup_by_key(|r| (r.start, r.end));
    if fixed.windows(2).any(|p| p[0].end > p[1].start) {
        return Err(
            "overlapping legacy word cues cannot be represented without changing authored ranges"
                .into(),
        );
    }
    fn gaps(chars: &[char], start: usize, end: usize, words: &mut Vec<TextRange>) {
        let mut at = start;
        while at < end {
            if !chars[at].is_alphanumeric() {
                at += 1;
                continue;
            }
            let first = at;
            at += 1;
            while at < end {
                let c = chars[at];
                let connector = matches!(c, '\'' | '’' | '-' | '‐' | '‑')
                    && at + 1 < end
                    && chars[at + 1].is_alphanumeric();
                if c.is_alphanumeric() || is_combining_mark(c) || connector {
                    at += 1;
                } else {
                    break;
                }
            }
            words.push(TextRange {
                start: first as u32,
                end: at as u32,
            });
        }
    }
    let mut words = Vec::new();
    let mut previous = 0;
    for word in fixed {
        if word.start >= word.end || word.end as usize > chars.len() {
            return Err("invalid legacy word cue range".into());
        }
        gaps(&chars, previous, word.start as usize, &mut words);
        previous = word.end as usize;
        words.push(word);
    }
    gaps(&chars, previous, chars.len(), &mut words);
    let reading = ReadingText {
        text: text.to_owned(),
        words,
        pronunciations: vec![],
    };
    reading.validate(TargetLanguage::French)?;
    Ok(reading)
}

impl From<&LessonSummary> for NeutralLessonSummary {
    fn from(l: &LessonSummary) -> Self {
        Self {
            id: l.id.clone(),
            revision: l.revision,
            level_id: l.level_id.clone(),
            unit_id: l.unit_id.clone(),
            title: NeutralTitle {
                target: l.title.fr.clone(),
                zh: l.title.zh.clone(),
            },
            target_language: TargetLanguage::French,
            explanation_language: ExplanationLanguage::SimplifiedChinese,
            summary_zh: l.summary_zh.clone(),
            estimated_minutes: l.estimated_minutes,
        }
    }
}

impl TryFrom<&Vocabulary> for NeutralVocabulary {
    type Error = String;
    fn try_from(v: &Vocabulary) -> Result<Self, Self::Error> {
        Ok(Self {
            id: v.id.clone(),
            lemma: reading(&v.lemma, vec![])?,
            part_of_speech: v.part_of_speech.clone(),
            gender: v.gender.clone(),
            meaning_zh: v.meaning_zh.clone(),
            note_zh: v.note_zh.clone(),
            recording: v.recording.clone(),
        })
    }
}

impl TryFrom<&PublicLesson> for NeutralLesson {
    type Error = String;
    fn try_from(legacy: &PublicLesson) -> Result<Self, Self::Error> {
        // Validate v1, including its fixed French locale and measured audio policy.
        legacy.validate()?;
        let mut ranges: BTreeMap<(String, String, String), Vec<TextRange>> = BTreeMap::new();
        for (ti, track) in legacy.audio_tracks.iter().enumerate() {
            for (ci, cue) in track.cues.iter().enumerate() {
                if let (Some(segment), Some(range)) = (&cue.segment_id, &cue.word_range) {
                    let spans = ranges
                        .entry((
                            track.block_id.clone(),
                            cue.entry_id.clone(),
                            segment.clone(),
                        ))
                        .or_default();
                    if spans.iter().any(|r| {
                        r.start < range.end
                            && range.start < r.end
                            && (r.start != range.start || r.end != range.end)
                    }) {
                        return Err(format!(
                            "/audioTracks/{ti}/cues/{ci}/wordRange: overlapping legacy word cues cannot be adapted losslessly"
                        ));
                    }
                    spans.push(TextRange {
                        start: range.start,
                        end: range.end,
                    });
                }
            }
        }
        let mut value =
            serde_json::to_value(legacy).map_err(|_| "cannot serialize validated legacy lesson")?;
        value["schemaVersion"] = serde_json::json!("2.0");
        value["targetLanguage"] = serde_json::json!("fr-FR");
        value["explanationLanguage"] = serde_json::json!("zh-CN");
        let title = value["title"].as_object_mut().unwrap();
        let target = title.remove("fr").unwrap();
        title.insert("target".into(), target);
        for (i, vocab) in value["knowledge"]["vocabulary"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .enumerate()
        {
            let text = vocab["lemma"].as_str().unwrap();
            vocab["lemma"] = serde_json::to_value(
                reading(text, vec![])
                    .map_err(|e| format!("/knowledge/vocabulary/{i}/lemma: {e}"))?,
            )
            .unwrap();
        }
        for (i, grammar) in value["knowledge"]["grammar"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .enumerate()
        {
            for (j, example) in grammar["examples"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .enumerate()
            {
                let fields = example.as_object_mut().unwrap();
                let target = fields.remove("fr").unwrap();
                fields.insert(
                    "target".into(),
                    serde_json::to_value(
                        reading(target.as_str().unwrap(), vec![]).map_err(|e| {
                            format!("/knowledge/grammar/{i}/examples/{j}/target: {e}")
                        })?,
                    )
                    .unwrap(),
                );
            }
        }
        for (bi, block) in value["blocks"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .enumerate()
        {
            match block["type"].as_str().unwrap() {
                "dialogue" | "article" => {
                    let block_id = block["id"].as_str().unwrap().to_owned();
                    let entries = if block["type"] == "dialogue" {
                        "turns"
                    } else {
                        "paragraphs"
                    };
                    for (ei, entry) in block[entries]
                        .as_array_mut()
                        .unwrap()
                        .iter_mut()
                        .enumerate()
                    {
                        let entry_id = entry["id"].as_str().unwrap().to_owned();
                        for (si, segment) in entry["segments"]
                            .as_array_mut()
                            .unwrap()
                            .iter_mut()
                            .enumerate()
                        {
                            let segment_id = segment["id"].as_str().unwrap().to_owned();
                            let fields = segment.as_object_mut().unwrap();
                            let text = fields.remove("text").unwrap();
                            let fixed = ranges
                                .remove(&(block_id.clone(), entry_id.clone(), segment_id.clone()))
                                .unwrap_or_default();
                            let reading = reading(text.as_str().unwrap(), fixed).map_err(|e| {
                                format!("/blocks/{bi}/{entries}/{ei}/segments/{si}/reading: {e}")
                            })?;
                            fields.insert("reading".into(), serde_json::to_value(reading).unwrap());
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
        let neutral: Self =
            serde_json::from_value(value).map_err(|_| "cannot decode adapted neutral lesson")?;
        neutral.validate()?;
        Ok(neutral)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn audio_lesson() -> PublicLesson {
        let mut lesson = crate::tests::fixture();
        let crate::Block::Dialogue { id, turns, .. } = &mut lesson.blocks[1] else {
            panic!()
        };
        turns[0].segments[0].text = "🥐 café,".into();
        let mut cues: Vec<_> = turns
            .iter()
            .enumerate()
            .map(|(i, e)| AudioCue {
                entry_id: e.id.clone(),
                segment_id: None,
                word_range: None,
                start_ms: i as u32 * 1000,
                end_ms: (i as u32 + 1) * 1000,
            })
            .collect();
        cues.push(AudioCue {
            entry_id: turns[0].id.clone(),
            segment_id: Some(turns[0].segments[0].id.clone()),
            word_range: None,
            start_ms: 100,
            end_ms: 600,
        });
        cues.push(AudioCue {
            entry_id: turns[0].id.clone(),
            segment_id: Some(turns[0].segments[0].id.clone()),
            word_range: Some(AudioWordRange { start: 2, end: 7 }),
            start_ms: 200,
            end_ms: 300,
        });
        lesson.audio_tracks = vec![AudioTrack {
            block_id: id.clone(),
            asset_id: "legacy-recording".into(),
            cues,
        }];
        let sha = "a".repeat(64);
        lesson.audio = vec![AudioAsset {
            asset_id: "legacy-recording".into(),
            revision: 1,
            sha256: sha.clone(),
            mime_type: "audio/mpeg".into(),
            duration_ms: 10000,
            credit_zh: "Synthetic contract only".into(),
            url: format!("/api/audio/{sha}.mp3"),
        }];
        lesson.knowledge.vocabulary[0].recording = Some(KnowledgeRecording {
            asset: lesson.audio[0].clone(),
            start_ms: 400,
            end_ms: 500,
        });
        lesson
    }
    #[test]
    fn legacy_adapter_preserves_all_fields_and_measured_audio_without_mutating_source() {
        for legacy in [crate::tests::fixture(), audio_lesson()] {
            let before = serde_json::to_value(&legacy).unwrap();
            let neutral = NeutralLesson::try_from(&legacy).unwrap();
            neutral.validate().unwrap();
            assert_eq!(neutral.schema_version, "2.0");
            assert_eq!(neutral.target_language, TargetLanguage::French);
            assert_eq!(
                serde_json::to_value(neutral.validation_projection().unwrap()).unwrap(),
                before
            );
            assert_eq!(serde_json::to_value(&legacy).unwrap(), before);
            for (r, _) in neutral.readings() {
                assert!(r.pronunciations.is_empty());
                assert!(!r.text.is_empty());
            }
            let value = serde_json::to_value(&neutral).unwrap();
            assert!(value["title"].get("fr").is_none());
            assert_eq!(value["audioTracks"], before["audioTracks"]);
            assert_eq!(value["cast"], before["cast"]);
            assert_eq!(value["steps"], before["steps"]);
        }
        let neutral = NeutralLesson::try_from(&audio_lesson()).unwrap();
        let Block::Dialogue { turns, .. } = &neutral.blocks[1] else {
            panic!()
        };
        assert_eq!(
            turns[0].segments[0]
                .reading
                .word_text(&TextRange { start: 2, end: 7 })
                .unwrap(),
            "café,"
        );
    }
    #[test]
    fn public_version_dispatch_is_strict_and_preserves_native_v2_readings() {
        let legacy = crate::tests::fixture();
        let neutral = NeutralLesson::try_from(&legacy).unwrap();
        let v1 = serde_json::to_value(&legacy).unwrap();
        let v2 = serde_json::to_value(&neutral).unwrap();
        assert_eq!(
            serde_json::to_value(super::super::decode_public(v1.clone()).unwrap()).unwrap(),
            v2
        );
        assert_eq!(
            serde_json::to_value(super::super::decode_public(v2.clone()).unwrap()).unwrap(),
            v2
        );
        for source in [v1.clone(), v2.clone()] {
            for key in [
                "serverOnly",
                "editorial",
                "assetRefs",
                "audioRefs",
                "untrusted",
            ] {
                let mut bad = source.clone();
                bad[key] = serde_json::json!({"accepted":["private-marker"]});
                let error = super::super::decode_public(bad).unwrap_err();
                assert!(!error.contains("private-marker"));
            }
        }
        for version in [
            serde_json::json!("3.0"),
            serde_json::json!(2),
            serde_json::Value::Null,
        ] {
            let mut bad = v2.clone();
            bad["schemaVersion"] = version;
            assert!(super::super::decode_public(bad).is_err());
        }
        let mut bad = v2;
        bad.as_object_mut().unwrap().remove("schemaVersion");
        assert!(super::super::decode_public(bad).is_err());
    }
    #[test]
    fn french_compatibility_words_preserve_unicode_apostrophes_compounds_and_combining_marks() {
        let source = "«C’est l’été… 👩 café e\u{301}lan petit-déjeuner!»";
        let r = reading(source, vec![]).unwrap();
        assert_eq!(r.text, source);
        let words: Vec<_> = r.words.iter().map(|w| r.word_text(w).unwrap()).collect();
        assert_eq!(
            words,
            vec!["C’est", "l’été", "café", "e\u{301}lan", "petit-déjeuner"]
        );
        let punctuation = reading("… !", vec![]).unwrap();
        assert!(punctuation.words.is_empty());
        assert!(reading("粤語", vec![]).unwrap().pronunciations.is_empty());
    }
    #[test]
    fn incompatible_old_audio_ranges_and_invalid_french_documents_are_rejected_not_rewritten() {
        let mut lesson = audio_lesson();
        let mut cue = lesson.audio_tracks[0].cues.last().unwrap().clone();
        cue.word_range = Some(AudioWordRange { start: 2, end: 6 });
        cue.start_ms = 400;
        cue.end_ms = 500;
        lesson.audio_tracks[0].cues.push(cue);
        lesson.validate().unwrap();
        let before = serde_json::to_value(&lesson).unwrap();
        let error = NeutralLesson::try_from(&lesson).unwrap_err();
        assert!(error.starts_with("/audioTracks/0/cues/"), "{error}");
        assert!(error.contains("wordRange: overlapping"), "{error}");
        assert_eq!(serde_json::to_value(&lesson).unwrap(), before);
        let mut invalid = crate::tests::fixture();
        invalid.cast[0].speech_locale = "yue-Hant-HK".into();
        assert!(NeutralLesson::try_from(&invalid).is_err());
        let mut invalid = crate::tests::fixture();
        invalid.schema_version = "2.0".into();
        assert!(NeutralLesson::try_from(&invalid).is_err());
    }
}
