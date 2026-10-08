//! Language-neutral source text. Offsets always refer to the unchanged Unicode scalar sequence.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use ts_rs::TS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
pub enum TargetLanguage {
    #[serde(rename = "fr-FR")]
    French,
    #[serde(rename = "yue-Hant-HK")]
    Cantonese,
}
impl TargetLanguage {
    pub fn locale(self) -> &'static str {
        match self {
            Self::French => "fr-FR",
            Self::Cantonese => "yue-Hant-HK",
        }
    }
    pub fn validate_character_locale(self, locale: &str) -> Result<(), String> {
        if locale == self.locale() {
            Ok(())
        } else {
            Err(format!(
                "expected character speech locale {}",
                self.locale()
            ))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum PronunciationSystem {
    Ipa,
    Jyutping,
}

dto!(TextRange {
    start: u32,
    end: u32
});
dto!(Pronunciation {
    range: TextRange,
    system: PronunciationSystem,
    text: String
});
dto!(ReadingText {
    text: String,
    // Explicit author-defined words or phrases, sorted and non-overlapping.
    words: Vec<TextRange>,
    #[serde(default, skip_serializing_if="Vec::is_empty")]
    pronunciations: Vec<Pronunciation>
});

impl ReadingText {
    /// Validate source spans, never infer Cantonese words from whitespace or normalize offsets.
    pub fn validate(&self, language: TargetLanguage) -> Result<(), String> {
        if self.text.is_empty()
            || self.text.len() > 32768
            || self.text.chars().any(char::is_control)
        {
            return Err("/text: expected 1..32768 bytes of text without controls".into());
        }
        if self.words.len() > 1024 || self.pronunciations.len() > self.words.len() * 2 {
            return Err("/words: too many word or pronunciation spans".into());
        }
        let chars: Vec<char> = self.text.chars().collect();
        let mut covered = vec![false; chars.len()];
        let mut previous_end = 0;
        let mut ranges = HashSet::new();
        for (index, range) in self.words.iter().enumerate() {
            let (start, end) = (range.start as usize, range.end as usize);
            if start >= end || end > chars.len() || start < previous_end {
                return Err(format!(
                    "/words/{index}: expected ordered non-overlapping Unicode scalar range"
                ));
            }
            let word = &chars[start..end];
            if word.first().is_some_and(|c| c.is_whitespace())
                || word.last().is_some_and(|c| c.is_whitespace())
                || !word.iter().any(|c| c.is_alphanumeric())
            {
                return Err(format!(
                    "/words/{index}: expected a word or phrase without boundary whitespace"
                ));
            }
            covered[start..end].fill(true);
            ranges.insert((range.start, range.end));
            previous_end = end;
        }
        if chars
            .iter()
            .enumerate()
            .any(|(index, c)| c.is_alphanumeric() && !covered[index])
        {
            return Err(
                "/words: every letter and number must belong to an authored word or phrase".into(),
            );
        }
        let mut annotated = HashSet::new();
        for (index, pronunciation) in self.pronunciations.iter().enumerate() {
            let range = (pronunciation.range.start, pronunciation.range.end);
            if !ranges.contains(&range) || !annotated.insert((range, pronunciation.system)) {
                return Err(format!(
                    "/pronunciations/{index}/range: expected a unique authored word range per system"
                ));
            }
            if pronunciation.text.trim().is_empty()
                || pronunciation.text.len() > 256
                || pronunciation.text.chars().any(char::is_control)
            {
                return Err(format!(
                    "/pronunciations/{index}/text: expected bounded pronunciation without controls"
                ));
            }
            if pronunciation.system == PronunciationSystem::Jyutping {
                if language != TargetLanguage::Cantonese {
                    return Err(format!(
                        "/pronunciations/{index}/system: jyutping requires Cantonese"
                    ));
                }
                if !valid_jyutping(&pronunciation.text) {
                    return Err(format!(
                        "/pronunciations/{index}/text: expected lowercase Jyutping syllables with tones 1..6"
                    ));
                }
            }
        }
        Ok(())
    }

    /// Exact source slice for UI and audio; no byte slicing or UTF-16 conversion.
    pub fn word_text(&self, range: &TextRange) -> Option<String> {
        if range.start >= range.end {
            return None;
        }
        if !self
            .words
            .iter()
            .any(|word| word.start == range.start && word.end == range.end)
        {
            return None;
        }
        let chars: Vec<char> = self.text.chars().collect();
        chars
            .get(range.start as usize..range.end as usize)
            .map(|word| word.iter().collect())
    }

    /// A word cue must use an authored range, independently of its measured audio times.
    pub fn validate_audio_word_range(&self, range: &crate::AudioWordRange) -> Result<(), String> {
        let span = TextRange {
            start: range.start,
            end: range.end,
        };
        if self
            .word_text(&span)
            .is_some_and(|word| word.chars().any(char::is_alphanumeric))
        {
            Ok(())
        } else {
            Err("word audio cue must reference an authored word range".into())
        }
    }
}

// Syntax validation only; it does not certify a linguistically correct Cantonese reading.
fn valid_jyutping(text: &str) -> bool {
    text.split(' ').all(|syllable| {
        let bytes = syllable.as_bytes();
        (2..=9).contains(&bytes.len())
            && matches!(bytes.last(), Some(b'1'..=b'6'))
            && bytes[..bytes.len() - 1].iter().all(u8::is_ascii_lowercase)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cantonese() -> ReadingText {
        ReadingText {
            text: "你好，食咗飯未？".into(),
            words: vec![
                TextRange { start: 0, end: 2 },
                TextRange { start: 3, end: 5 },
                TextRange { start: 5, end: 6 },
                TextRange { start: 6, end: 7 },
            ],
            pronunciations: vec![Pronunciation {
                range: TextRange { start: 0, end: 2 },
                system: PronunciationSystem::Jyutping,
                text: "nei5 hou2".into(),
            }],
        }
    }
    #[test]
    fn cantonese_requires_explicit_words_and_keeps_pronunciation_out_of_source() {
        let source = cantonese();
        source.validate(TargetLanguage::Cantonese).unwrap();
        assert_eq!(source.word_text(&source.words[0]).as_deref(), Some("你好"));
        let serialized = serde_json::to_value(&source).unwrap();
        assert_eq!(serialized["text"], "你好，食咗飯未？");
        assert_eq!(serialized["pronunciations"][0]["text"], "nei5 hou2");
        let mut missing = source.clone();
        missing.words.clear();
        missing.pronunciations.clear();
        assert!(
            missing
                .validate(TargetLanguage::Cantonese)
                .unwrap_err()
                .starts_with("/words:")
        );
    }
    #[test]
    fn scalar_offsets_preserve_supplementary_characters_and_combining_marks() {
        let source = ReadingText {
            text: "𠮷 e\u{301}！".into(),
            words: vec![
                TextRange { start: 0, end: 1 },
                TextRange { start: 2, end: 4 },
            ],
            pronunciations: vec![],
        };
        source.validate(TargetLanguage::French).unwrap();
        assert_eq!(source.word_text(&source.words[0]).as_deref(), Some("𠮷"));
        assert_eq!(
            source.word_text(&source.words[1]).as_deref(),
            Some("e\u{301}")
        );
        assert_eq!(source.text.chars().count(), 5);
    }
    #[test]
    fn malformed_overlapping_reversed_and_uncovered_ranges_fail() {
        for ranges in [
            vec![TextRange { start: 0, end: 99 }],
            vec![TextRange { start: 2, end: 1 }],
            vec![
                TextRange { start: 0, end: 2 },
                TextRange { start: 1, end: 4 },
            ],
            vec![TextRange { start: 0, end: 2 }],
        ] {
            let mut source = cantonese();
            source.words = ranges;
            source.pronunciations.clear();
            assert!(source.validate(TargetLanguage::Cantonese).is_err());
        }
        let mut source = cantonese();
        source.words[0] = TextRange { start: 2, end: 3 };
        assert!(source.validate(TargetLanguage::Cantonese).is_err());
    }
    #[test]
    fn pronunciation_must_attach_to_an_exact_word_once_per_system() {
        let mut source = cantonese();
        source.pronunciations.push(source.pronunciations[0].clone());
        assert!(
            source
                .validate(TargetLanguage::Cantonese)
                .unwrap_err()
                .contains("/range:")
        );
        source.pronunciations.pop();
        source.pronunciations[0].range.end = 1;
        assert!(
            source
                .validate(TargetLanguage::Cantonese)
                .unwrap_err()
                .contains("/range:")
        );
    }
    #[test]
    fn jyutping_language_and_tone_syntax_are_checked_without_claiming_quality() {
        assert!(
            cantonese()
                .validate(TargetLanguage::French)
                .unwrap_err()
                .contains("/system:")
        );
        for reading in [
            "nei",
            "nei7",
            "Nei5",
            "nei5  hou2",
            " nei5",
            "nei5 ",
            "nei5\nhou2",
            "你好",
        ] {
            let mut source = cantonese();
            source.pronunciations[0].text = reading.into();
            assert!(
                source.validate(TargetLanguage::Cantonese).is_err(),
                "{reading}"
            );
        }
        let mut source = cantonese();
        source.pronunciations[0].system = PronunciationSystem::Ipa;
        source.pronunciations[0].text = "neɪ˩˧ hou˧˥".into();
        source.validate(TargetLanguage::Cantonese).unwrap();
    }
    #[test]
    fn audio_word_cues_cannot_invent_or_borrow_ranges() {
        let source = cantonese();
        source
            .validate_audio_word_range(&crate::AudioWordRange { start: 0, end: 2 })
            .unwrap();
        assert!(
            source
                .validate_audio_word_range(&crate::AudioWordRange { start: 0, end: 1 })
                .is_err()
        );
        assert!(
            source
                .word_text(&TextRange {
                    start: 99,
                    end: 100
                })
                .is_none()
        );
    }
    #[test]
    fn direct_range_helpers_reject_malformed_authored_ranges() {
        let source = ReadingText {
            text: "你好".into(),
            words: vec![
                TextRange { start: 0, end: 99 },
                TextRange { start: 1, end: 1 },
            ],
            pronunciations: vec![],
        };
        assert!(
            source
                .validate_audio_word_range(&crate::AudioWordRange { start: 0, end: 99 })
                .is_err()
        );
        assert!(
            source
                .validate_audio_word_range(&crate::AudioWordRange { start: 1, end: 1 })
                .is_err()
        );
        assert!(source.word_text(&TextRange { start: 1, end: 1 }).is_none());
    }
    #[test]
    fn strict_wire_rejects_unknown_fields_systems_and_language_aliases() {
        for locale in ["yue", "zh-CN", "fr", "FR-fr"] {
            assert!(serde_json::from_value::<TargetLanguage>(serde_json::json!(locale)).is_err());
        }
        assert_eq!(
            serde_json::to_value(TargetLanguage::Cantonese).unwrap(),
            "yue-Hant-HK"
        );
        TargetLanguage::Cantonese
            .validate_character_locale("yue-Hant-HK")
            .unwrap();
        assert!(
            TargetLanguage::Cantonese
                .validate_character_locale("fr-FR")
                .is_err()
        );
        let mut value = serde_json::to_value(cantonese()).unwrap();
        value["untrusted"] = serde_json::json!(true);
        assert!(serde_json::from_value::<ReadingText>(value).is_err());
        let mut value = serde_json::to_value(cantonese()).unwrap();
        value["pronunciations"][0]["system"] = serde_json::json!("pinyin");
        assert!(serde_json::from_value::<ReadingText>(value).is_err());
    }
    #[test]
    fn bounded_source_and_pronunciation_reject_controls_and_excess() {
        for text in [String::new(), "x".repeat(32769), "你\t好".into()] {
            let mut source = cantonese();
            source.text = text;
            assert!(
                source
                    .validate(TargetLanguage::Cantonese)
                    .unwrap_err()
                    .starts_with("/text:")
            );
        }
        let mut source = cantonese();
        source.pronunciations[0].text = "a".repeat(257);
        assert!(source.validate(TargetLanguage::Cantonese).is_err());
    }
}
