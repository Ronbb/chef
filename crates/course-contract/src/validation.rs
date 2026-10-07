use crate::{Block, Exercise, PublicLesson};
use std::collections::HashSet;

pub(crate) fn identifier(value: &str, path: &str) -> Result<(), String> {
    if !crate::valid_content_id(value) {
        return Err(format!(
            "{path}: expected 1..100 ASCII letters, digits, hyphens or underscores"
        ));
    }
    Ok(())
}

fn unique<'a>(
    values: impl Iterator<Item = &'a str>,
    path: &str,
    suffix: &str,
) -> Result<(), String> {
    let mut seen = HashSet::new();
    for (index, value) in values.enumerate() {
        identifier(value, &format!("{path}/{index}{suffix}"))?;
        if value.trim().is_empty() || !seen.insert(value) {
            return Err(format!("{path}/{index}{suffix}: empty or duplicate ID"));
        }
    }
    Ok(())
}
fn nonempty(value: &str, path: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{path}: expected nonempty text"));
    }
    Ok(())
}
fn text_list(values: &[String], path: &str) -> Result<(), String> {
    if values.is_empty() {
        return Err(format!("{path}: expected at least one text item"));
    }
    for (index, value) in values.iter().enumerate() {
        nonempty(value, &format!("{path}/{index}"))?;
    }
    Ok(())
}
impl PublicLesson {
    /// Exercise placement and completion ownership do not depend on registered media.
    pub fn validate_exercise_steps(&self) -> Result<(), String> {
        let exercises: HashSet<_> = self
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Exercise { id, .. } => Some(id.as_str()),
                _ => None,
            })
            .collect();
        let mut required_practice = HashSet::new();
        for (si, step) in self.steps.iter().enumerate() {
            for (bi, block_id) in step.block_ids.iter().enumerate() {
                if !exercises.contains(block_id.as_str()) {
                    continue;
                }
                if step.kind != "practice" {
                    return Err(format!(
                        "/steps/{si}/blockIds/{bi}: exercise requires a practice step"
                    ));
                }
                if self.completion.required_step_ids.contains(&step.id) {
                    required_practice.insert(block_id.as_str());
                }
            }
        }
        for (index, exercise) in self.completion.required_exercise_ids.iter().enumerate() {
            if !required_practice.contains(exercise.as_str()) {
                return Err(format!(
                    "/completion/requiredExerciseIds/{index}: exercise must belong to a required practice step"
                ));
            }
        }
        Ok(())
    }

    /// Intrinsic choice semantics can be checked before registered media hydration.
    pub fn validate_choice_labels(&self) -> Result<(), String> {
        for (index, block) in self.blocks.iter().enumerate() {
            if let Block::Exercise {
                exercise: Exercise::SingleChoice { options, .. },
                ..
            } = block
            {
                let mut labels = HashSet::new();
                for (i, option) in options.iter().enumerate() {
                    if !labels.insert(crate::normalize_text(&option.text, true)) {
                        return Err(format!(
                            "/blocks/{index}/options/{i}/text: duplicate choice text after NFC, whitespace and apostrophe normalization"
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn validate_flow(&self) -> Result<(), String> {
        self.validate_choice_labels()?;
        for (value, path) in [
            (self.id.as_str(), "/id"),
            (self.level_id.as_str(), "/levelId"),
            (self.unit_id.as_str(), "/unitId"),
            (self.title.fr.as_str(), "/title/fr"),
            (self.title.zh.as_str(), "/title/zh"),
            (self.summary_zh.as_str(), "/summaryZh"),
        ] {
            nonempty(value, path)?;
        }
        if !(1..=60).contains(&self.estimated_minutes) {
            return Err("/estimatedMinutes: expected duration from 1 to 60 minutes".into());
        }
        text_list(&self.objectives_zh, "/objectivesZh")?;
        for (index, vocabulary) in self.knowledge.vocabulary.iter().enumerate() {
            for (field, value) in [
                ("lemma", &vocabulary.lemma),
                ("partOfSpeech", &vocabulary.part_of_speech),
                ("meaningZh", &vocabulary.meaning_zh),
            ] {
                nonempty(value, &format!("/knowledge/vocabulary/{index}/{field}"))?;
            }
        }
        for (index, grammar) in self.knowledge.grammar.iter().enumerate() {
            let path = format!("/knowledge/grammar/{index}");
            nonempty(&grammar.title_zh, &format!("{path}/titleZh"))?;
            nonempty(&grammar.body_zh, &format!("{path}/bodyZh"))?;
            for (example_index, example) in grammar.examples.iter().enumerate() {
                nonempty(&example.fr, &format!("{path}/examples/{example_index}/fr"))?;
                nonempty(&example.zh, &format!("{path}/examples/{example_index}/zh"))?;
            }
        }
        if self.steps.is_empty() {
            return Err("/steps: expected at least one step".into());
        }
        if self.completion.strategy != "attempt-all" {
            return Err("/completion/strategy: unsupported completion policy".into());
        }
        if self.completion.required_step_ids.is_empty() {
            return Err("/completion/requiredStepIds: expected at least one required step".into());
        }
        unique(
            self.review_item_ids.iter().map(String::as_str),
            "/reviewItemIds",
            "",
        )?;
        unique(
            self.completion.required_step_ids.iter().map(String::as_str),
            "/completion/requiredStepIds",
            "",
        )?;
        unique(
            self.completion
                .required_exercise_ids
                .iter()
                .map(String::as_str),
            "/completion/requiredExerciseIds",
            "",
        )?;
        let mut anchors = HashSet::new();
        let mut reading_blocks = HashSet::new();
        let mut reading_entries = HashSet::new();
        for (bi, block) in self.blocks.iter().enumerate() {
            let path = format!("/blocks/{bi}");
            let mut entry_field = "";
            let entries: Vec<_> = match block {
                Block::Dialogue {
                    title_zh,
                    turns,
                    speakers,
                    ..
                } => {
                    nonempty(title_zh, &format!("{path}/titleZh"))?;
                    entry_field = "turns";
                    reading_blocks.insert(block.id());
                    if turns.is_empty() {
                        return Err(format!("{path}/turns: empty dialogue"));
                    }
                    if speakers.is_empty() {
                        return Err(format!("{path}/speakers: empty dialogue"));
                    }
                    for (si, speaker) in speakers.iter().enumerate() {
                        nonempty(&speaker.label_zh, &format!("{path}/speakers/{si}/labelZh"))?;
                        let cast = self
                            .cast
                            .iter()
                            .find(|c| c.character_id == speaker.character_id)
                            .ok_or_else(|| {
                                format!("{path}/speakers/{si}/characterId: unknown character")
                            })?;
                        if cast.display_name != speaker.display_name {
                            return Err(format!(
                                "{path}/speakers/{si}/displayName: speaker differs from pinned character"
                            ));
                        }
                        if cast.avatar_id != speaker.avatar_id {
                            return Err(format!(
                                "{path}/speakers/{si}/avatarId: speaker differs from pinned character"
                            ));
                        }
                    }
                    for (index, turn) in turns.iter().enumerate() {
                        nonempty(
                            &turn.translation_zh,
                            &format!("{path}/turns/{index}/translationZh"),
                        )?;
                    }
                    turns.iter().map(|e| (&e.id, &e.segments)).collect()
                }
                Block::Article {
                    title_zh,
                    paragraphs,
                    ..
                } => {
                    nonempty(title_zh, &format!("{path}/titleZh"))?;
                    entry_field = "paragraphs";
                    reading_blocks.insert(block.id());
                    if paragraphs.is_empty() {
                        return Err(format!("{path}/paragraphs: empty article"));
                    }
                    for (index, paragraph) in paragraphs.iter().enumerate() {
                        nonempty(
                            &paragraph.translation_zh,
                            &format!("{path}/paragraphs/{index}/translationZh"),
                        )?;
                    }
                    paragraphs.iter().map(|e| (&e.id, &e.segments)).collect()
                }
                Block::Scene {
                    place_zh,
                    situation_zh,
                    illustration_id,
                    ..
                } => {
                    identifier(illustration_id, &format!("{path}/illustrationId"))?;
                    nonempty(place_zh, &format!("{path}/placeZh"))?;
                    nonempty(situation_zh, &format!("{path}/situationZh"))?;
                    vec![]
                }
                Block::Explanation {
                    title_zh, body_zh, ..
                } => {
                    nonempty(title_zh, &format!("{path}/titleZh"))?;
                    nonempty(body_zh, &format!("{path}/bodyZh"))?;
                    vec![]
                }
                Block::Culture {
                    title_zh,
                    body_zh,
                    scope_zh,
                    ..
                } => {
                    nonempty(title_zh, &format!("{path}/titleZh"))?;
                    nonempty(body_zh, &format!("{path}/bodyZh"))?;
                    nonempty(scope_zh, &format!("{path}/scopeZh"))?;
                    vec![]
                }
                Block::Habit {
                    task_zh,
                    alternative_zh,
                    ..
                } => {
                    nonempty(task_zh, &format!("{path}/taskZh"))?;
                    nonempty(alternative_zh, &format!("{path}/alternativeZh"))?;
                    vec![]
                }
                Block::Summary { takeaways_zh, .. } => {
                    text_list(takeaways_zh, &format!("{path}/takeawaysZh"))?;
                    vec![]
                }
                Block::Exercise { exercise, .. } => {
                    match exercise {
                        Exercise::SingleChoice { prompt_zh, options } => {
                            nonempty(prompt_zh, &format!("{path}/promptZh"))?;
                            if options.len() < 2 {
                                return Err(format!(
                                    "{path}/options: choice needs at least two options"
                                ));
                            }
                            for (i, option) in options.iter().enumerate() {
                                nonempty(&option.text, &format!("{path}/options/{i}/text"))?;
                            }
                            unique(
                                options.iter().map(|o| o.id.as_str()),
                                &format!("{path}/options"),
                                "/id",
                            )?;
                        }
                        Exercise::Order { prompt_zh, tokens } => {
                            nonempty(prompt_zh, &format!("{path}/promptZh"))?;
                            if tokens.len() < 2 {
                                return Err(format!(
                                    "{path}/tokens: order needs at least two tokens"
                                ));
                            }
                            for (i, token) in tokens.iter().enumerate() {
                                nonempty(&token.text, &format!("{path}/tokens/{i}/text"))?;
                            }
                            unique(
                                tokens.iter().map(|o| o.id.as_str()),
                                &format!("{path}/tokens"),
                                "/id",
                            )?;
                        }
                        Exercise::FillBlank {
                            prompt_zh,
                            template_fr,
                            ..
                        } => {
                            nonempty(prompt_zh, &format!("{path}/promptZh"))?;
                            if template_fr.matches("___").count() != 1 {
                                return Err(format!(
                                    "{path}/templateFr: fill-blank needs one blank"
                                ));
                            }
                        }
                    }
                    vec![]
                }
                Block::Vocabulary { entry_ids, .. } | Block::Grammar { entry_ids, .. } => {
                    unique(
                        entry_ids.iter().map(String::as_str),
                        &format!("{path}/entryIds"),
                        "",
                    )?;
                    vec![]
                }
            };
            let entry_path = format!("{path}/{entry_field}");
            unique(
                entries.iter().map(|(id, _)| id.as_str()),
                &entry_path,
                "/id",
            )?;
            for (ei, (id, segments)) in entries.into_iter().enumerate() {
                let segment_path = format!("{entry_path}/{ei}/segments");
                if !segments.iter().any(|s| !s.text.trim().is_empty()) {
                    return Err(format!("{segment_path}: empty sentence"));
                }
                unique(segments.iter().map(|s| s.id.as_str()), &segment_path, "/id")?;
                reading_entries.insert((block.id(), id.as_str()));
                for segment in segments {
                    anchors.insert((block.id(), id.as_str(), segment.id.as_str()));
                }
            }
        }
        for (bi, block) in self.blocks.iter().enumerate() {
            if let Block::Explanation { targets, .. } = block {
                for (ti, target) in targets.iter().enumerate() {
                    let field = if !reading_blocks.contains(target.block_id.as_str()) {
                        Some("blockId")
                    } else if !reading_entries
                        .contains(&(target.block_id.as_str(), target.entry_id.as_str()))
                    {
                        Some("entryId")
                    } else if !anchors.contains(&(
                        target.block_id.as_str(),
                        target.entry_id.as_str(),
                        target.segment_id.as_str(),
                    )) {
                        Some("segmentId")
                    } else {
                        None
                    };
                    if let Some(field) = field {
                        return Err(format!(
                            "/blocks/{bi}/targets/{ti}/{field}: unknown reading anchor"
                        ));
                    }
                }
            }
        }
        let mut reachable = HashSet::new();
        for (si, step) in self.steps.iter().enumerate() {
            if !matches!(
                step.kind.as_str(),
                "discover" | "read" | "explore" | "practice" | "apply" | "recap"
            ) {
                return Err(format!("/steps/{si}/kind: invalid step kind"));
            }
            nonempty(&step.title_zh, &format!("/steps/{si}/titleZh"))?;
            if step.block_ids.is_empty() {
                return Err(format!(
                    "/steps/{si}/blockIds: step needs at least one block"
                ));
            }
            unique(
                step.block_ids.iter().map(String::as_str),
                &format!("/steps/{si}/blockIds"),
                "",
            )?;
            reachable.extend(step.block_ids.iter().map(String::as_str));
        }
        self.validate_exercise_steps()?;
        if let Some(index) = self.blocks.iter().position(|b| !reachable.contains(b.id())) {
            return Err(format!("/blocks/{index}/id: unreachable teaching block"));
        }
        for (ci, cast) in self.cast.iter().enumerate() {
            if !crate::valid_content_revision(cast.revision) {
                return Err(format!(
                    "/cast/{ci}/revision: expected revision in 1..2147483647"
                ));
            }
            nonempty(&cast.display_name, &format!("/cast/{ci}/displayName"))?;
            if cast.speech_locale != crate::CHARACTER_SPEECH_LOCALE {
                return Err(format!(
                    "/cast/{ci}/speechLocale: expected {}",
                    crate::CHARACTER_SPEECH_LOCALE
                ));
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use crate::*;
    fn fixture() -> PublicLesson {
        super::super::tests::fixture()
    }
    #[test]
    fn exercises_only_live_in_practice_and_required_exercises_have_required_steps() {
        let mut lesson = fixture();
        let practice = lesson
            .steps
            .iter()
            .position(|s| s.kind == "practice")
            .unwrap();
        lesson.steps[practice].kind = "read".into();
        let error = lesson.validate().unwrap_err();
        assert!(
            error.starts_with(&format!("/steps/{practice}/blockIds/0:")),
            "{error}"
        );
        assert!(
            error.contains("exercise requires a practice step"),
            "{error}"
        );

        let mut lesson = fixture();
        let practice_id = lesson.steps[practice].id.clone();
        lesson
            .completion
            .required_step_ids
            .retain(|id| id != &practice_id);
        let error = lesson.validate().unwrap_err();
        assert!(
            error.starts_with("/completion/requiredExerciseIds/0:"),
            "{error}"
        );
        assert!(error.contains("required practice step"), "{error}");

        // Optional practice with optional exercises is valid.
        lesson.completion.required_exercise_ids.clear();
        lesson.validate().unwrap();

        // A second optional practice reference does not invalidate required ownership.
        let mut lesson = fixture();
        let mut extra = lesson.steps[practice].clone();
        extra.id = "optional-practice".into();
        lesson.steps.push(extra);
        lesson.validate().unwrap();
    }
    #[test]
    fn choice_labels_must_be_distinguishable_but_order_tokens_may_repeat() {
        for (first, second, duplicate) in [
            ("une baguette", "une baguette", true),
            ("une baguette", "  une\u{00a0}baguette  ", true),
            ("café", "cafe\u{301}", true),
            ("s'il vous plaît", "s’il vous plaît", true),
            ("s'il vous plaît", "sʼil vous plaît", true),
            ("Une", "une", false),
            ("cafe", "café", false),
        ] {
            let mut lesson = fixture();
            let index = lesson
                .blocks
                .iter()
                .position(|b| {
                    matches!(
                        b,
                        Block::Exercise {
                            exercise: Exercise::SingleChoice { .. },
                            ..
                        }
                    )
                })
                .unwrap();
            if let Block::Exercise {
                exercise: Exercise::SingleChoice { options, .. },
                ..
            } = &mut lesson.blocks[index]
            {
                options[0].text = first.into();
                options[1].text = second.into();
            }
            if duplicate {
                let error = lesson.validate().unwrap_err();
                assert!(
                    error.starts_with(&format!("/blocks/{index}/options/1/text:")),
                    "{error}"
                );
                assert!(error.contains("duplicate choice text"), "{error}");
            } else {
                lesson.validate().unwrap();
            }
        }
        let mut lesson = fixture();
        for block in &mut lesson.blocks {
            if let Block::Exercise {
                exercise: Exercise::Order { tokens, .. },
                ..
            } = block
            {
                tokens[1].text = tokens[0].text.clone();
            }
        }
        lesson.validate().unwrap();
    }
    #[test]
    fn character_speech_locale_matches_the_registration_policy() {
        for invalid in [
            "frank",
            "fry",
            "fr",
            "fr-",
            "fr-CA",
            "FR-fr",
            "fr-FR-extra",
            "en-US",
        ] {
            let mut lesson = fixture();
            lesson.cast[0].speech_locale = invalid.into();
            assert!(
                lesson
                    .validate()
                    .unwrap_err()
                    .starts_with("/cast/0/speechLocale:"),
                "{invalid}"
            );
        }
        fixture().validate().unwrap();
    }
    #[test]
    fn all_content_identifiers_match_navigation_and_recovery_limits() {
        fn rename(value: &mut serde_json::Value, old: &str, new: &str) {
            match value {
                serde_json::Value::String(s) if s == old => *s = new.into(),
                serde_json::Value::Array(values) => {
                    for value in values {
                        rename(value, old, new);
                    }
                }
                serde_json::Value::Object(values) => {
                    for value in values.values_mut() {
                        rename(value, old, new);
                    }
                }
                _ => {}
            }
        }
        let original = serde_json::to_value(fixture()).unwrap();
        let block = |kind: &str| {
            original["blocks"]
                .as_array()
                .unwrap()
                .iter()
                .position(|b| b["type"] == kind)
                .unwrap()
        };
        let paths = vec![
            "/id".to_owned(),
            "/levelId".to_owned(),
            "/unitId".to_owned(),
            "/knowledge/vocabulary/0/id".to_owned(),
            "/knowledge/grammar/0/id".to_owned(),
            "/blocks/0/id".to_owned(),
            "/steps/0/id".to_owned(),
            "/cast/0/characterId".to_owned(),
            "/cast/0/avatarId".to_owned(),
            format!("/blocks/{}/speakers/0/id", block("dialogue")),
            format!("/blocks/{}/turns/0/id", block("dialogue")),
            format!("/blocks/{}/turns/0/segments/0/id", block("dialogue")),
            format!("/blocks/{}/paragraphs/0/id", block("article")),
            format!("/blocks/{}/paragraphs/0/segments/0/id", block("article")),
            format!(
                "/blocks/{}/options/0/id",
                original["blocks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .position(|b| b.get("options").is_some())
                    .unwrap()
            ),
            format!(
                "/blocks/{}/tokens/0/id",
                original["blocks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .position(|b| b.get("tokens").is_some())
                    .unwrap()
            ),
        ];
        for path in paths {
            let old = original.pointer(&path).unwrap().as_str().unwrap();
            for id in [
                "route/escape",
                "word:scope",
                "entrée",
                "has space",
                &"a".repeat(101),
            ] {
                let mut value = original.clone();
                rename(&mut value, old, id);
                let lesson: PublicLesson = serde_json::from_value(value).unwrap();
                let error = lesson.validate().expect_err(&format!("{path}: {id}"));
                assert!(error.starts_with(&format!("{path}:")), "{path}: {error}");
            }
            for id in ["Upper_1-legal", &"a".repeat(100)] {
                let mut value = original.clone();
                rename(&mut value, old, id);
                let lesson: PublicLesson = serde_json::from_value(value).unwrap();
                lesson.validate().unwrap();
            }
        }
    }
    #[test]
    fn requires_readable_teaching_text_and_duration() {
        let original = serde_json::to_value(fixture()).unwrap();
        let block = |kind: &str| {
            original["blocks"]
                .as_array()
                .unwrap()
                .iter()
                .position(|b| b["type"] == kind)
                .unwrap()
        };
        let mut cases = vec![
            ("/summaryZh".to_owned(), serde_json::json!("\u{00a0}")),
            ("/objectivesZh".to_owned(), serde_json::json!([])),
            ("/objectivesZh/0".to_owned(), serde_json::json!("\u{00a0}")),
            ("/estimatedMinutes".to_owned(), serde_json::json!(0)),
            ("/estimatedMinutes".to_owned(), serde_json::json!(61)),
        ];
        for (kind, fields) in [
            ("scene", vec!["placeZh", "situationZh"]),
            (
                "dialogue",
                vec!["titleZh", "turns/0/translationZh", "speakers/0/labelZh"],
            ),
            ("article", vec!["titleZh", "paragraphs/0/translationZh"]),
            ("explanation", vec!["titleZh", "bodyZh"]),
            ("culture", vec!["titleZh", "bodyZh", "scopeZh"]),
            ("habit", vec!["taskZh", "alternativeZh"]),
            ("summary", vec!["takeawaysZh/0"]),
        ] {
            for field in fields {
                cases.push((
                    format!("/blocks/{}/{field}", block(kind)),
                    serde_json::json!("\u{00a0}"),
                ));
            }
        }
        cases.push((
            format!("/blocks/{}/takeawaysZh", block("summary")),
            serde_json::json!([]),
        ));
        for field in ["lemma", "partOfSpeech", "meaningZh"] {
            cases.push((
                format!("/knowledge/vocabulary/0/{field}"),
                serde_json::json!("\u{00a0}"),
            ));
        }
        for field in ["titleZh", "bodyZh", "examples/0/fr", "examples/0/zh"] {
            cases.push((
                format!("/knowledge/grammar/0/{field}"),
                serde_json::json!("\u{00a0}"),
            ));
        }
        for (pointer, value) in cases {
            let mut source = original.clone();
            *source.pointer_mut(&pointer).unwrap() = value;
            let lesson: PublicLesson = serde_json::from_value(source).unwrap();
            let error = lesson.validate().expect_err(&pointer);
            assert!(
                error.starts_with(&format!("{pointer}:")),
                "{pointer}: {error}"
            );
        }
    }
    #[test]
    fn rejects_broken_targets_duplicate_tokens_and_unknown_steps() {
        let mut lesson = fixture();
        if let Block::Explanation { targets, .. } = &mut lesson.blocks[3] {
            targets[0].segment_id = "missing".into();
        }
        assert!(lesson.validate().unwrap_err().contains("targets/0"));
        let mut lesson = fixture();
        lesson.steps[0].kind = "execute-script".into();
        assert!(lesson.validate().is_err());
        let mut lesson = fixture();
        if let Block::Exercise {
            exercise: Exercise::Order { tokens, .. },
            ..
        } = &mut lesson.blocks[9]
        {
            tokens[1].id = tokens[0].id.clone();
        }
        assert!(lesson.validate().is_err());
    }
    #[test]
    fn rejects_unreachable_required_content() {
        let mut lesson = fixture();
        lesson.steps.retain(|s| s.kind != "explore");
        assert!(lesson.validate().unwrap_err().contains("unreachable"));
    }

    #[test]
    fn reports_nested_flow_fields_and_duplicate_items() {
        let original = serde_json::to_value(fixture()).unwrap();
        let block = |kind: &str| {
            original["blocks"]
                .as_array()
                .unwrap()
                .iter()
                .position(|b| b["type"] == kind)
                .unwrap()
        };
        let dialogue = block("dialogue");
        let article = block("article");
        let explanation = block("explanation");
        let exercise = |kind: &str| {
            original["blocks"]
                .as_array()
                .unwrap()
                .iter()
                .position(|b| b["type"] == "exercise" && b["exerciseType"] == kind)
                .unwrap()
        };
        let choice = exercise("single-choice");
        let order = exercise("order");
        let fill = exercise("fill-blank");
        for (pointer, value) in [
            ("/title/fr".into(), serde_json::json!("\u{00a0}")),
            ("/completion/strategy".into(), serde_json::json!("unknown")),
            (
                "/completion/requiredStepIds/1".into(),
                original["completion"]["requiredStepIds"][0].clone(),
            ),
            (
                "/reviewItemIds/1".into(),
                original["reviewItemIds"][0].clone(),
            ),
            (
                "/cast/1/characterId".into(),
                original["cast"][0]["characterId"].clone(),
            ),
            ("/cast/0/revision".into(), serde_json::json!(0)),
            ("/cast/0/speechLocale".into(), serde_json::json!("en-US")),
            (
                format!("/blocks/{dialogue}/speakers/1/id"),
                original["blocks"][dialogue]["speakers"][0]["id"].clone(),
            ),
            (
                format!("/blocks/{dialogue}/speakers/0/displayName"),
                serde_json::json!("mismatch"),
            ),
            (
                format!("/blocks/{dialogue}/speakers/0/avatarId"),
                serde_json::json!("mismatch"),
            ),
            (
                format!("/blocks/{dialogue}/turns/1/id"),
                original["blocks"][dialogue]["turns"][0]["id"].clone(),
            ),
            (
                format!("/blocks/{article}/paragraphs/1/id"),
                original["blocks"][article]["paragraphs"][0]["id"].clone(),
            ),
            (
                format!("/blocks/{dialogue}/turns/0/segments/1/id"),
                original["blocks"][dialogue]["turns"][0]["segments"][0]["id"].clone(),
            ),
            (
                format!("/blocks/{article}/paragraphs/0/segments"),
                serde_json::json!([]),
            ),
            (
                format!("/blocks/{choice}/promptZh"),
                serde_json::json!("\u{00a0}"),
            ),
            (
                format!("/blocks/{choice}/options/1/id"),
                original["blocks"][choice]["options"][0]["id"].clone(),
            ),
            (
                format!("/blocks/{choice}/options/1/text"),
                serde_json::json!("\u{00a0}"),
            ),
            (
                format!("/blocks/{order}/tokens/1/id"),
                original["blocks"][order]["tokens"][0]["id"].clone(),
            ),
            (
                format!("/blocks/{order}/tokens/1/text"),
                serde_json::json!("\u{00a0}"),
            ),
            (
                format!("/blocks/{fill}/templateFr"),
                serde_json::json!("No blank"),
            ),
            (
                format!("/blocks/{explanation}/targets/0/blockId"),
                serde_json::json!("missing"),
            ),
            (
                format!("/blocks/{explanation}/targets/0/entryId"),
                serde_json::json!("missing"),
            ),
            (
                format!("/blocks/{explanation}/targets/0/segmentId"),
                serde_json::json!("missing"),
            ),
            ("/steps/0/kind".into(), serde_json::json!("unknown")),
            ("/steps/0/titleZh".into(), serde_json::json!("\u{00a0}")),
        ] {
            let mut source = original.clone();
            *source.pointer_mut(&pointer).unwrap() = value;
            let lesson: PublicLesson = serde_json::from_value(source).unwrap();
            let error = lesson.validate().unwrap_err();
            assert!(
                error.starts_with(&format!("{pointer}:")),
                "{pointer}: {error}"
            );
        }
    }
}
