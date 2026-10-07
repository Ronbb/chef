//! Private author metadata is validated before projecting the public document.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
const GRADING_CONTEXT: &str = "serverOnly.grading: invalid or inconsistent private grading rules";

/// Offline author checks shared by individual lessons and local release packs.
pub fn check_lesson(
    document: &crate::author_json::Document,
) -> Result<brioche_course_contract::PublicLesson> {
    check_source(&document.value).map_err(|error| {
        let grading = error.to_string() == GRADING_CONTEXT;
        let located = document.semantic(error);
        if grading {
            located.context(GRADING_CONTEXT)
        } else {
            located
        }
    })
}

pub(crate) fn check_source(
    source: &serde_json::Value,
) -> Result<brioche_course_contract::PublicLesson> {
    crate::media::source_asset_refs(source)?;
    crate::recording::source_audio_refs(source)?;
    let lesson = crate::project_source(source.clone())?;
    crate::grading::Grader::from_author_source(&lesson, source).context(GRADING_CONTEXT)?;
    Ok(lesson)
}

/// Resolve referenced `<lessonId>.lesson.json` files in directories and explicitly selected lesson files.
/// No recursive discovery; explicit files may retain legacy filenames.
pub fn check_release_sources(
    document: &crate::author_json::Document,
    manifest: &crate::content::ReleaseManifest,
    locations: &[std::path::PathBuf],
) -> Result<usize> {
    use std::{collections::BTreeSet, path::PathBuf};
    ensure!(
        !locations.is_empty() && locations.len() <= 20,
        "expected 1..20 source files or directories"
    );
    manifest
        .validate_author()
        .map_err(|error| document.semantic(error))?;
    let mut roots = BTreeSet::<PathBuf>::new();
    let mut explicit = Vec::new();
    let references: BTreeSet<_> = manifest
        .levels
        .iter()
        .flat_map(|level| &level.units)
        .flat_map(|unit| &unit.lessons)
        .map(|lesson| lesson.lesson_id.as_str())
        .collect();
    for location in locations {
        let root = location
            .canonicalize()
            .with_context(|| format!("{}: source location unavailable", location.display()))?;
        if root.is_dir() {
            roots.insert(root);
        } else {
            ensure!(
                root.is_file(),
                "{}: expected a source file or directory",
                location.display()
            );
            let selected = crate::author_json::Document::load(&root)?;
            let lesson = check_lesson(&selected)?;
            if !references.contains(lesson.id.as_str()) {
                return Err(selected.semantic(anyhow::anyhow!(
                    "/id: explicitly selected source is not referenced by this release"
                )));
            }
            explicit.push((root, lesson.id));
        }
    }
    let mut count = 0;
    for (li, level) in manifest.levels.iter().enumerate() {
        for (ui, unit) in level.units.iter().enumerate() {
            for (ri, reference) in unit.lessons.iter().enumerate() {
                let pointer = format!("/levels/{li}/units/{ui}/lessons/{ri}/lessonId");
                let mut candidates = BTreeSet::new();
                candidates.extend(
                    explicit
                        .iter()
                        .filter(|(_, id)| id == &reference.lesson_id)
                        .map(|(path, _)| path.clone()),
                );
                for root in &roots {
                    let candidate = root.join(format!("{}.lesson.json", reference.lesson_id));
                    if candidate.try_exists().with_context(|| {
                        format!(
                            "{}: cannot inspect local lesson source",
                            candidate.display()
                        )
                    })? {
                        let resolved = candidate.canonicalize()?;
                        if !resolved.starts_with(root) || !resolved.is_file() {
                            return Err(document.semantic(anyhow::anyhow!("{pointer}: local lesson source must be a file inside its source directory")));
                        }
                        candidates.insert(resolved);
                    }
                }
                if candidates.len() != 1 {
                    let reason = if candidates.is_empty() {
                        "no local lesson source"
                    } else {
                        "ambiguous local lesson sources"
                    };
                    return Err(document.semantic(anyhow::anyhow!("{pointer}: {reason} for {}; expected exactly one matching explicit file or <lessonId>.lesson.json in the supplied directories", reference.lesson_id)));
                }
                let source_document =
                    crate::author_json::Document::load(candidates.first().unwrap())?;
                let lesson = check_lesson(&source_document)?;
                for (field, matches, message) in [
                    (
                        "id",
                        lesson.id == reference.lesson_id,
                        "lesson ID does not match release reference",
                    ),
                    (
                        "revision",
                        lesson.revision == reference.revision,
                        "lesson revision does not match release reference",
                    ),
                    (
                        "levelId",
                        lesson.level_id == level.id,
                        "lesson level does not match release level",
                    ),
                    (
                        "unitId",
                        lesson.unit_id == unit.id,
                        "lesson unit does not match release unit",
                    ),
                ] {
                    if !matches {
                        return Err(
                            source_document.semantic(anyhow::anyhow!("/{field}: {message}"))
                        );
                    }
                }
                count += 1;
            }
        }
    }
    Ok(count)
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum EditorialStatus {
    Draft,
    Reviewed,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Editorial {
    pub status: EditorialStatus,
    pub note: String,
}

/// This schema belongs to author tooling; never export it to the Web contract package.
pub fn schema() -> serde_json::Value {
    #[derive(schemars::JsonSchema)]
    #[schemars(rename_all = "camelCase", deny_unknown_fields)]
    #[allow(dead_code)]
    struct AuthorLesson {
        #[schemars(flatten)]
        lesson: brioche_course_contract::PublicLesson,
        editorial: Editorial,
        server_only: crate::grading::PrivateRules,
        #[schemars(default)]
        asset_refs: Vec<crate::media::AssetRef>,
        #[schemars(default)]
        audio_refs: Vec<crate::media::AssetRef>,
    }
    serde_json::to_value(schemars::schema_for!(AuthorLesson)).expect("schema serialization")
}

pub fn editorial(source: &serde_json::Value) -> Result<Editorial> {
    let value = source
        .get("editorial")
        .context("/editorial: required author metadata missing")?;
    let metadata: Editorial = crate::author_json::from_value(value.clone(), "/editorial")?;
    ensure!(
        !metadata.note.trim().is_empty()
            && metadata.note.len() <= 8000
            && !metadata
                .note
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')),
        "/editorial/note: expected nonempty text of at most 8000 bytes without control characters"
    );
    Ok(metadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn requires_explicit_valid_author_metadata() {
        for source in [
            json!({}),
            json!({"editorial":null}),
            json!({"editorial":{"status":"published","note":"review"}}),
            json!({"editorial":{"status":"draft","note":" "}}),
            json!({"editorial":{"status":"draft","note":"review","ignored":true}}),
        ] {
            assert!(editorial(&source).is_err());
        }
        assert!(matches!(
            editorial(&json!({"editorial":{"status":"draft","note":"Needs review"}}))
                .unwrap()
                .status,
            EditorialStatus::Draft
        ));
        assert!(matches!(
            editorial(
                &json!({"editorial":{"status":"reviewed","note":"Explicit author assertion"}})
            )
            .unwrap()
            .status,
            EditorialStatus::Reviewed
        ));
    }

    #[test]
    fn schema_includes_private_contract_only_for_author_tooling() {
        let schema = schema();
        let required = schema["required"].as_array().unwrap();
        for field in ["id", "blocks", "serverOnly", "editorial"] {
            assert!(required.contains(&serde_json::json!(field)), "{field}");
        }
        assert!(!required.contains(&serde_json::json!("assetRefs")));
        assert!(!required.contains(&serde_json::json!("audioRefs")));
        assert!(schema["properties"]["audioRefs"].is_object());
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["$defs"]["Editorial"]["additionalProperties"], false);
        assert!(schema.to_string().contains("correctOptionId"));
        let public =
            serde_json::to_value(schemars::schema_for!(brioche_course_contract::PublicLesson))
                .unwrap();
        for private in [
            "correctOptionId",
            "accepted",
            "correctTokenIds",
            "serverOnly",
            "editorial",
            "audioRefs",
        ] {
            assert!(!public.to_string().contains(private));
        }
    }
}
