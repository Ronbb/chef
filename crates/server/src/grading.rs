//! Answer keys stay in this crate. Validation precedes import and grading.
use brioche_course_contract::neutral::{self, NeutralLesson};
pub use brioche_course_contract::normalize_text;
use brioche_course_contract::{
    Block, Exercise, ExerciseAnswer, GradeResult, MAX_TEXT_ANSWER_BYTES,
    MAX_TEXT_ANSWER_UTF16_UNITS, OptionItem, PublicLesson, valid_text_answer_length,
};
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum Rule {
    #[serde(rename_all = "camelCase")]
    Choice {
        correct_option_id: String,
        feedback_zh: String,
    },
    #[serde(rename_all = "camelCase")]
    Text {
        accepted: Vec<String>,
        case_sensitive: bool,
        feedback_zh: String,
    },
    #[serde(rename_all = "camelCase")]
    Order {
        correct_token_ids: Vec<String>,
        feedback_zh: String,
    },
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PrivateRules {
    grading: BTreeMap<String, Rule>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum GradeError {
    InvalidContent,
    UnknownExercise,
    InvalidAnswer,
}

// Borrow only the grading-relevant public fields. No French wire projection or
// copied language-specific grading implementation is needed for version 2.
#[derive(Clone, Copy)]
enum ExerciseView<'a> {
    Choice(&'a [OptionItem]),
    Text,
    Order(&'a [OptionItem]),
}
impl<'a> From<&'a Exercise> for ExerciseView<'a> {
    fn from(exercise: &'a Exercise) -> Self {
        match exercise {
            Exercise::SingleChoice { options, .. } => Self::Choice(options),
            Exercise::FillBlank { .. } => Self::Text,
            Exercise::Order { tokens, .. } => Self::Order(tokens),
        }
    }
}
impl<'a> From<&'a neutral::Exercise> for ExerciseView<'a> {
    fn from(exercise: &'a neutral::Exercise) -> Self {
        match exercise {
            neutral::Exercise::SingleChoice { options, .. } => Self::Choice(options),
            neutral::Exercise::FillBlank { .. } => Self::Text,
            neutral::Exercise::Order { tokens, .. } => Self::Order(tokens),
        }
    }
}

pub struct Grader {
    rules: BTreeMap<String, Rule>,
}
impl Grader {
    fn author_rules(source: &serde_json::Value) -> anyhow::Result<PrivateRules> {
        use anyhow::{Context, bail, ensure};
        // Internally tagged enums buffer their fields and lose nested serde error
        // paths. Decode each known shape directly for author-only diagnostics.
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Sources {
            grading: BTreeMap<String, serde_json::Value>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct ChoiceFields {
            #[serde(rename = "kind")]
            _kind: String,
            correct_option_id: String,
            feedback_zh: String,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct TextFields {
            #[serde(rename = "kind")]
            _kind: String,
            accepted: Vec<String>,
            case_sensitive: bool,
            feedback_zh: String,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct OrderFields {
            #[serde(rename = "kind")]
            _kind: String,
            correct_token_ids: Vec<String>,
            feedback_zh: String,
        }
        let sources: Sources = crate::author_json::from_value(
            source
                .get("serverOnly")
                .cloned()
                .context("/serverOnly: missing private rules")?,
            "/serverOnly",
        )?;
        let mut grading = BTreeMap::new();
        for (id, value) in sources.grading {
            let path = format!(
                "/serverOnly/grading/{}",
                id.replace('~', "~0").replace('/', "~1")
            );
            let kind: String = crate::author_json::from_value(
                value
                    .get("kind")
                    .cloned()
                    .with_context(|| format!("{path}/kind: missing grading kind"))?,
                &format!("{path}/kind"),
            )?;
            let rule = match kind.as_str() {
                "choice" => {
                    let fields: ChoiceFields = crate::author_json::from_value(value, &path)?;
                    Rule::Choice {
                        correct_option_id: fields.correct_option_id,
                        feedback_zh: fields.feedback_zh,
                    }
                }
                "text" => {
                    let fields: TextFields = crate::author_json::from_value(value, &path)?;
                    Rule::Text {
                        accepted: fields.accepted,
                        case_sensitive: fields.case_sensitive,
                        feedback_zh: fields.feedback_zh,
                    }
                }
                "order" => {
                    let fields: OrderFields = crate::author_json::from_value(value, &path)?;
                    Rule::Order {
                        correct_token_ids: fields.correct_token_ids,
                        feedback_zh: fields.feedback_zh,
                    }
                }
                _ => bail!("{path}/kind: unknown grading kind"),
            };
            if let Rule::Text {
                accepted,
                case_sensitive,
                ..
            } = &rule
            {
                ensure!(
                    !accepted.is_empty(),
                    "{path}/accepted: at least one accepted answer is required"
                );
                for (index, answer) in accepted.iter().enumerate() {
                    ensure!(
                        !normalize_text(answer, *case_sensitive).is_empty()
                            && answer.len() <= MAX_TEXT_ANSWER_BYTES
                            && valid_text_answer_length(&normalize_text(answer, true)),
                        "{path}/accepted/{index}: expected nonempty answer of at most {MAX_TEXT_ANSWER_BYTES} source bytes with a normalized representative of at most {MAX_TEXT_ANSWER_UTF16_UNITS} UTF-16 code units"
                    );
                }
            }
            grading.insert(id, rule);
        }
        Ok(PrivateRules { grading })
    }

    /// Check private types and intrinsic text bounds before connecting or hydrating media.
    /// References are checked against course blocks during import preflight and again after hydration.
    pub fn validate_author_schema(source: &serde_json::Value) -> anyhow::Result<()> {
        Self::author_rules(source).map(|_| ())
    }

    pub fn from_source(
        lesson: &PublicLesson,
        source: &serde_json::Value,
    ) -> Result<Self, GradeError> {
        Self::from_author_source(lesson, source).map_err(|_| GradeError::InvalidContent)
    }

    /// Author-only diagnostics. HTTP handlers continue using the opaque GradeError.
    pub fn from_author_source(
        lesson: &PublicLesson,
        source: &serde_json::Value,
    ) -> anyhow::Result<Self> {
        Self::from_exercises(
            lesson
                .blocks
                .iter()
                .enumerate()
                .filter_map(|(index, block)| match block {
                    Block::Exercise { id, exercise } => {
                        Some((index, id.as_str(), ExerciseView::from(exercise)))
                    }
                    _ => None,
                }),
            source,
        )
    }
    pub fn from_neutral_source(
        lesson: &NeutralLesson,
        source: &serde_json::Value,
    ) -> Result<Self, GradeError> {
        Self::from_neutral_author_source(lesson, source).map_err(|_| GradeError::InvalidContent)
    }
    /// Author diagnostics stay private; runtime callers use the opaque error above.
    pub fn from_neutral_author_source(
        lesson: &NeutralLesson,
        source: &serde_json::Value,
    ) -> anyhow::Result<Self> {
        Self::from_exercises(
            lesson
                .blocks
                .iter()
                .enumerate()
                .filter_map(|(index, block)| match block {
                    neutral::Block::Exercise { id, exercise } => {
                        Some((index, id.as_str(), ExerciseView::from(exercise)))
                    }
                    _ => None,
                }),
            source,
        )
    }
    fn from_exercises<'a>(
        items: impl Iterator<Item = (usize, &'a str, ExerciseView<'a>)>,
        source: &serde_json::Value,
    ) -> anyhow::Result<Self> {
        use anyhow::{Context, bail, ensure};
        let rules = Self::author_rules(source)?;
        let mut exercises = BTreeMap::new();
        for (index, id, exercise) in items {
            ensure!(
                exercises.insert(id, exercise).is_none(),
                "/blocks/{index}/id: duplicate exercise ID"
            );
        }
        let path = |id: &str| {
            format!(
                "/serverOnly/grading/{}",
                id.replace('~', "~0").replace('/', "~1")
            )
        };
        for id in rules.grading.keys() {
            ensure!(
                exercises.contains_key(id.as_str()),
                "{}: rule references unknown exercise",
                path(id)
            );
        }
        for (id, exercise) in exercises {
            let pointer = path(id);
            let rule = rules
                .grading
                .get(id)
                .with_context(|| format!("{pointer}: missing grading rule"))?;
            let feedback = match rule {
                Rule::Choice { feedback_zh, .. }
                | Rule::Text { feedback_zh, .. }
                | Rule::Order { feedback_zh, .. } => feedback_zh,
            };
            ensure!(
                !feedback.trim().is_empty(),
                "{pointer}/feedbackZh: expected nonempty feedback"
            );
            match (exercise, rule) {
                (
                    ExerciseView::Choice(options),
                    Rule::Choice {
                        correct_option_id, ..
                    },
                ) => {
                    ensure!(
                        options.iter().any(|option| &option.id == correct_option_id),
                        "{pointer}/correctOptionId: unknown option reference"
                    );
                }
                (ExerciseView::Text, Rule::Text { .. }) => {}
                (
                    ExerciseView::Order(tokens),
                    Rule::Order {
                        correct_token_ids, ..
                    },
                ) => {
                    ensure!(
                        correct_token_ids.len() == tokens.len(),
                        "{pointer}/correctTokenIds: expected every token exactly once"
                    );
                    let mut seen = HashSet::new();
                    let known: HashSet<_> = tokens.iter().map(|token| &token.id).collect();
                    for (index, token) in correct_token_ids.iter().enumerate() {
                        ensure!(
                            known.contains(token) && seen.insert(token),
                            "{pointer}/correctTokenIds/{index}: unknown or duplicate token reference"
                        );
                    }
                }
                _ => bail!("{pointer}/kind: grading kind does not match exercise kind"),
            }
        }
        Ok(Self {
            rules: rules.grading,
        })
    }
    pub fn grade(
        &self,
        lesson: &PublicLesson,
        id: &str,
        answer: &ExerciseAnswer,
    ) -> Result<GradeResult, GradeError> {
        let exercise = lesson
            .blocks
            .iter()
            .find_map(|b| match b {
                Block::Exercise {
                    id: block_id,
                    exercise,
                } if block_id == id => Some(ExerciseView::from(exercise)),
                _ => None,
            })
            .ok_or(GradeError::UnknownExercise)?;
        self.grade_exercise(exercise, id, answer)
    }
    pub fn grade_neutral(
        &self,
        lesson: &NeutralLesson,
        id: &str,
        answer: &ExerciseAnswer,
    ) -> Result<GradeResult, GradeError> {
        let exercise = lesson
            .blocks
            .iter()
            .find_map(|block| match block {
                neutral::Block::Exercise {
                    id: block_id,
                    exercise,
                } if block_id == id => Some(ExerciseView::from(exercise)),
                _ => None,
            })
            .ok_or(GradeError::UnknownExercise)?;
        self.grade_exercise(exercise, id, answer)
    }
    fn grade_exercise(
        &self,
        exercise: ExerciseView<'_>,
        id: &str,
        answer: &ExerciseAnswer,
    ) -> Result<GradeResult, GradeError> {
        let rule = self.rules.get(id).ok_or(GradeError::InvalidContent)?;
        let (correct, feedback) = match (exercise, rule, answer) {
            (
                ExerciseView::Choice(options),
                Rule::Choice {
                    correct_option_id,
                    feedback_zh,
                },
                ExerciseAnswer::Choice { option_id },
            ) => {
                if !options.iter().any(|o| &o.id == option_id) {
                    return Err(GradeError::InvalidAnswer);
                }
                (option_id == correct_option_id, feedback_zh)
            }
            (
                ExerciseView::Text,
                Rule::Text {
                    accepted,
                    case_sensitive,
                    feedback_zh,
                },
                ExerciseAnswer::Text { text },
            ) => {
                if !valid_text_answer_length(text)
                    || normalize_text(text, *case_sensitive).is_empty()
                {
                    return Err(GradeError::InvalidAnswer);
                }
                (
                    accepted.iter().any(|a| {
                        normalize_text(a, *case_sensitive) == normalize_text(text, *case_sensitive)
                    }),
                    feedback_zh,
                )
            }
            (
                ExerciseView::Order(tokens),
                Rule::Order {
                    correct_token_ids,
                    feedback_zh,
                },
                ExerciseAnswer::Order { token_ids },
            ) => {
                let ids: HashSet<_> = token_ids.iter().collect();
                if ids.len() != tokens.len()
                    || ids.len() != token_ids.len()
                    || !tokens.iter().all(|t| ids.contains(&t.id))
                {
                    return Err(GradeError::InvalidAnswer);
                }
                // Equal displayed words are interchangeable; hidden token IDs
                // still prove that every original token was used exactly once.
                let displayed: BTreeMap<_, _> = tokens
                    .iter()
                    .map(|token| (token.id.as_str(), normalize_text(&token.text, true)))
                    .collect();
                let submitted = token_ids
                    .iter()
                    .map(|id| displayed.get(id.as_str()).ok_or(GradeError::InvalidAnswer))
                    .collect::<Result<Vec<_>, _>>()?;
                let expected = correct_token_ids
                    .iter()
                    .map(|id| displayed.get(id.as_str()).ok_or(GradeError::InvalidContent))
                    .collect::<Result<Vec<_>, _>>()?;
                (submitted == expected, feedback_zh)
            }
            _ => return Err(GradeError::InvalidAnswer),
        };
        Ok(GradeResult {
            exercise_id: id.into(),
            correct,
            feedback_zh: feedback.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (PublicLesson, serde_json::Value) {
        let source: serde_json::Value =
            serde_json::from_str(include_str!("../../../docs/examples/a1-bakery.lesson.json"))
                .unwrap();
        (crate::project_source(source.clone()).unwrap(), source)
    }
    #[test]
    fn normalizes_without_removing_accents() {
        assert_eq!(
            normalize_text("  S’il\u{00a0}vous\u{202f}plaît  ", false),
            "s'il vous plaît"
        );
        assert_eq!(normalize_text("cafe\u{301}", false), "café");
        assert_ne!(normalize_text("cafe", false), normalize_text("café", false));
        assert_ne!(normalize_text("Une", true), normalize_text("une", true));
    }
    #[test]
    fn author_text_answers_must_have_a_representative_within_the_web_input_limit() {
        let (lesson, original) = fixture();
        for (accepted, submitted) in [
            ("a".repeat(1024), "a".repeat(1024)),
            ("é".repeat(1024), "é".repeat(1024)),
            ("😀".repeat(512), "😀".repeat(512)),
            ("e\u{301}".repeat(1024), "é".repeat(1024)),
            ("İ".repeat(1024), "İ".repeat(1024)),
            (format!("{}une", " ".repeat(1500)), "une".into()),
        ] {
            let mut source = original.clone();
            source["serverOnly"]["grading"]["exercise-article"]["accepted"] =
                serde_json::json!([accepted]);
            let grader = Grader::from_author_source(&lesson, &source).unwrap();
            assert!(
                grader
                    .grade(
                        &lesson,
                        "exercise-article",
                        &ExerciseAnswer::Text { text: submitted }
                    )
                    .unwrap()
                    .correct
            );
        }
        for accepted in ["a".repeat(1025), "😀".repeat(513)] {
            let mut source = original.clone();
            source["serverOnly"]["grading"]["exercise-article"]["accepted"] =
                serde_json::json!([accepted]);
            let error = Grader::from_author_source(&lesson, &source)
                .err()
                .expect("unreachable web answer must be rejected");
            assert!(
                error
                    .to_string()
                    .starts_with("/serverOnly/grading/exercise-article/accepted/0:")
            );
        }
    }
    #[test]
    fn runtime_text_limit_matches_utf16_web_input_instead_of_only_utf8_bytes() {
        let (lesson, source) = fixture();
        let grader = Grader::from_source(&lesson, &source).unwrap();
        for text in ["a".repeat(1025), "😀".repeat(513)] {
            assert_eq!(
                grader
                    .grade(&lesson, "exercise-article", &ExerciseAnswer::Text { text })
                    .err(),
                Some(GradeError::InvalidAnswer)
            );
        }
    }
    #[test]
    fn grades_three_kinds_and_rejects_forged_inputs() {
        let (lesson, source) = fixture();
        let grader = Grader::from_source(&lesson, &source).unwrap();
        assert!(
            grader
                .grade(
                    &lesson,
                    "exercise-intention",
                    &ExerciseAnswer::Choice {
                        option_id: "request-bread".into()
                    }
                )
                .unwrap()
                .correct
        );
        assert!(
            !grader
                .grade(
                    &lesson,
                    "exercise-intention",
                    &ExerciseAnswer::Choice {
                        option_id: "ask-price".into()
                    }
                )
                .unwrap()
                .correct
        );
        assert!(
            grader
                .grade(
                    &lesson,
                    "exercise-article",
                    &ExerciseAnswer::Text {
                        text: " UNE\u{00a0}".into()
                    }
                )
                .unwrap()
                .correct
        );
        assert!(
            grader
                .grade(
                    &lesson,
                    "exercise-order",
                    &ExerciseAnswer::Order {
                        token_ids: vec!["request".into(), "bread".into(), "please".into()]
                    }
                )
                .unwrap()
                .correct
        );
        assert!(
            !grader
                .grade(
                    &lesson,
                    "exercise-order",
                    &ExerciseAnswer::Order {
                        token_ids: vec!["bread".into(), "request".into(), "please".into()]
                    }
                )
                .unwrap()
                .correct
        );
        for (id, answer) in [
            (
                "exercise-intention",
                ExerciseAnswer::Choice {
                    option_id: "forged".into(),
                },
            ),
            (
                "exercise-intention",
                ExerciseAnswer::Text {
                    text: "request-bread".into(),
                },
            ),
            (
                "exercise-order",
                ExerciseAnswer::Order {
                    token_ids: vec!["request".into(); 3],
                },
            ),
            (
                "exercise-article",
                ExerciseAnswer::Text { text: " ".into() },
            ),
        ] {
            assert_eq!(
                grader.grade(&lesson, id, &answer).unwrap_err(),
                GradeError::InvalidAnswer
            );
        }
    }

    #[test]
    fn order_grades_displayed_words_not_hidden_identity_of_repeated_tokens() {
        let (_, mut source) = fixture();
        let block = source["blocks"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|block| block["id"] == "exercise-order")
            .unwrap();
        block["tokens"] = serde_json::json!([
            {"id":"la-first","text":"la"},
            {"id":"door","text":"porte"},
            {"id":"of","text":"de"},
            {"id":"la-second","text":"la"},
            {"id":"house","text":"maison"}
        ]);
        let correct = vec!["la-first", "door", "of", "la-second", "house"];
        source["serverOnly"]["grading"]["exercise-order"]["correctTokenIds"] =
            serde_json::json!(correct);
        let lesson = crate::project_source(source.clone()).unwrap();
        let grader = Grader::from_source(&lesson, &source).unwrap();
        let grade = |ids: Vec<&str>| {
            grader.grade(
                &lesson,
                "exercise-order",
                &ExerciseAnswer::Order {
                    token_ids: ids.into_iter().map(String::from).collect(),
                },
            )
        };
        assert!(grade(correct).unwrap().correct);
        assert!(
            grade(vec!["la-second", "door", "of", "la-first", "house"])
                .unwrap()
                .correct
        );
        assert!(
            !grade(vec!["la-first", "of", "door", "la-second", "house"])
                .unwrap()
                .correct
        );
        for invalid in [
            vec!["la-first", "door", "of", "la-first", "house"],
            vec!["la-first", "door", "of", "la-second", "unknown"],
            vec!["la-first", "door", "of", "house"],
        ] {
            assert_eq!(grade(invalid).err(), Some(GradeError::InvalidAnswer));
        }
    }
    #[test]
    fn order_equivalence_preserves_accents_and_case_but_normalizes_unicode_and_spacing() {
        let (_, original) = fixture();
        for (first, second, equivalent) in [
            ("café", "cafe\u{301}", true),
            ("la", "  la\u{00a0}", true),
            ("l’eau", "l'eau", true),
            ("café", "cafe", false),
            ("la", "La", false),
        ] {
            let mut source = original.clone();
            let block = source["blocks"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|block| block["id"] == "exercise-order")
                .unwrap();
            block["tokens"] = serde_json::json!([
                {"id":"request","text":first},
                {"id":"bread","text":"et"},
                {"id":"please","text":second}
            ]);
            let lesson = crate::project_source(source.clone()).unwrap();
            let grader = Grader::from_source(&lesson, &source).unwrap();
            let result = grader
                .grade(
                    &lesson,
                    "exercise-order",
                    &ExerciseAnswer::Order {
                        token_ids: vec!["please".into(), "bread".into(), "request".into()],
                    },
                )
                .unwrap();
            assert_eq!(result.correct, equivalent, "{first:?} / {second:?}");
        }
    }
    #[test]
    fn rejects_missing_wrong_and_extra_rules() {
        let (lesson, source) = fixture();
        for pointer in [
            "/serverOnly/grading/exercise-intention/correctOptionId",
            "/serverOnly/grading/exercise-order/correctTokenIds",
            "/serverOnly/grading/exercise-article/accepted",
        ] {
            let mut invalid = source.clone();
            *invalid.pointer_mut(pointer).unwrap() = serde_json::json!(null);
            assert!(Grader::from_source(&lesson, &invalid).is_err());
        }
        let mut invalid = source;
        invalid["serverOnly"]["grading"]["extra"] =
            invalid["serverOnly"]["grading"]["exercise-intention"].clone();
        assert!(Grader::from_source(&lesson, &invalid).is_err());
    }

    #[test]
    fn author_diagnostics_locate_rules_and_runtime_errors_remain_opaque() {
        let (lesson, original) = fixture();
        for (pointer, value, expected) in [
            (
                "/serverOnly/grading/exercise-intention/correctOptionId",
                serde_json::json!(123),
                "/serverOnly/grading/exercise-intention/correctOptionId",
            ),
            (
                "/serverOnly/grading/exercise-article/caseSensitive",
                serde_json::json!(123),
                "/serverOnly/grading/exercise-article/caseSensitive",
            ),
            (
                "/serverOnly/grading/exercise-article/accepted/0",
                serde_json::json!(false),
                "/serverOnly/grading/exercise-article/accepted/0",
            ),
            (
                "/serverOnly/grading/exercise-order/correctTokenIds/1",
                serde_json::json!(42),
                "/serverOnly/grading/exercise-order/correctTokenIds/1",
            ),
            (
                "/serverOnly/grading/exercise-intention/correctOptionId",
                serde_json::json!("private-missing-option"),
                "/serverOnly/grading/exercise-intention/correctOptionId",
            ),
            (
                "/serverOnly/grading/exercise-article/accepted",
                serde_json::json!([]),
                "/serverOnly/grading/exercise-article/accepted",
            ),
            (
                "/serverOnly/grading/exercise-article/accepted/0",
                serde_json::json!(" "),
                "/serverOnly/grading/exercise-article/accepted/0",
            ),
            (
                "/serverOnly/grading/exercise-order/correctTokenIds/1",
                serde_json::json!("private-missing-token"),
                "/serverOnly/grading/exercise-order/correctTokenIds/1",
            ),
            (
                "/serverOnly/grading/exercise-order/correctTokenIds/1",
                serde_json::json!("request"),
                "/serverOnly/grading/exercise-order/correctTokenIds/1",
            ),
            (
                "/serverOnly/grading/exercise-order/correctTokenIds",
                serde_json::json!([]),
                "/serverOnly/grading/exercise-order/correctTokenIds",
            ),
            (
                "/serverOnly/grading/exercise-intention/feedbackZh",
                serde_json::json!(""),
                "/serverOnly/grading/exercise-intention/feedbackZh",
            ),
        ] {
            let mut source = original.clone();
            *source.pointer_mut(pointer).unwrap() = value;
            let error = Grader::from_author_source(&lesson, &source)
                .err()
                .unwrap()
                .to_string();
            assert!(error.starts_with(&format!("{expected}: ")), "{error}");
            assert!(!error.contains("private-missing"));
            assert!(matches!(
                Grader::from_source(&lesson, &source),
                Err(GradeError::InvalidContent)
            ));
        }
        let mut missing = original.clone();
        missing["serverOnly"]["grading"]
            .as_object_mut()
            .unwrap()
            .remove("exercise-intention");
        assert!(
            Grader::from_author_source(&lesson, &missing)
                .err()
                .unwrap()
                .to_string()
                .starts_with("/serverOnly/grading/exercise-intention:")
        );
        let mut extra = original.clone();
        extra["serverOnly"]["grading"]["a/b~c"] =
            original["serverOnly"]["grading"]["exercise-intention"].clone();
        assert!(
            Grader::from_author_source(&lesson, &extra)
                .err()
                .unwrap()
                .to_string()
                .starts_with("/serverOnly/grading/a~1b~0c:")
        );
        let mut mismatch = original.clone();
        mismatch["serverOnly"]["grading"]["exercise-intention"] =
            original["serverOnly"]["grading"]["exercise-article"].clone();
        assert!(
            Grader::from_author_source(&lesson, &mismatch)
                .err()
                .unwrap()
                .to_string()
                .starts_with("/serverOnly/grading/exercise-intention/kind:")
        );
        let mut duplicate = lesson.clone();
        duplicate.blocks.push(
            lesson
                .blocks
                .iter()
                .find(|b| matches!(b, Block::Exercise { .. }))
                .unwrap()
                .clone(),
        );
        assert!(matches!(
            Grader::from_source(&duplicate, &original),
            Err(GradeError::InvalidContent)
        ));
        assert!(Grader::from_author_source(&lesson, &original).is_ok());
    }
}
