//! Private author metadata is validated before projecting the public document.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
const GRADING_CONTEXT: &str = "serverOnly.grading: invalid or inconsistent private grading rules";

use brioche_course_contract::{PublicLesson, neutral::NeutralLesson};

/// Validated public course shape. Author-only fields are never carried here.
pub enum CheckedLesson {
    Legacy(PublicLesson),
    Neutral(NeutralLesson),
}
impl CheckedLesson {
    pub(crate) fn summary_document(
        &self,
        neutral: bool,
    ) -> std::result::Result<serde_json::Value, crate::AppError> {
        let value = match (self, neutral) {
            (Self::Legacy(l), false) => serde_json::to_value(l.summary()),
            (Self::Legacy(l), true) => serde_json::to_value(
                brioche_course_contract::neutral::NeutralLessonSummary::from(&l.summary()),
            ),
            (Self::Neutral(l), true) => serde_json::to_value(l.summary()),
            _ => return Err(crate::AppError::Conflict),
        };
        value.map_err(|_| crate::AppError::Unavailable)
    }

    pub(crate) fn from_public_document(value: serde_json::Value) -> Result<Self> {
        let lesson = match value
            .get("schemaVersion")
            .and_then(serde_json::Value::as_str)
        {
            Some("1.0") => Self::Legacy(serde_json::from_value(value)?),
            Some("2.0") => Self::Neutral(serde_json::from_value(value)?),
            _ => anyhow::bail!("unsupported public course version"),
        };
        lesson.validate_public()?;
        Ok(lesson)
    }
    pub(crate) fn schema_version(&self) -> &str {
        match self {
            Self::Legacy(l) => &l.schema_version,
            Self::Neutral(l) => &l.schema_version,
        }
    }
    pub(crate) fn steps(&self) -> &[brioche_course_contract::Step] {
        match self {
            Self::Legacy(l) => &l.steps,
            Self::Neutral(l) => &l.steps,
        }
    }
    pub(crate) fn completion(&self) -> &brioche_course_contract::Completion {
        match self {
            Self::Legacy(l) => &l.completion,
            Self::Neutral(l) => &l.completion,
        }
    }
    pub(crate) fn review_item_ids(&self) -> &[String] {
        match self {
            Self::Legacy(l) => &l.review_item_ids,
            Self::Neutral(l) => &l.review_item_ids,
        }
    }
    pub(crate) fn vocabulary_snapshot(&self, id: &str) -> Result<serde_json::Value> {
        match self {
            Self::Legacy(l) => serde_json::to_value(
                l.knowledge
                    .vocabulary
                    .iter()
                    .find(|v| v.id == id)
                    .context("missing vocabulary")?,
            ),
            Self::Neutral(l) => serde_json::to_value(
                l.knowledge
                    .vocabulary
                    .iter()
                    .find(|v| v.id == id)
                    .context("missing vocabulary")?,
            ),
        }
        .map_err(Into::into)
    }
    pub(crate) fn hint(&self, id: &str) -> Option<&str> {
        use brioche_course_contract::{Block, Exercise, neutral};
        match self {
            Self::Legacy(l) => l.blocks.iter().find_map(|b| match b {
                Block::Exercise {
                    id: block_id,
                    exercise: Exercise::FillBlank { hint_zh, .. },
                } if block_id == id && !hint_zh.trim().is_empty() => Some(hint_zh.as_str()),
                _ => None,
            }),
            Self::Neutral(l) => l.blocks.iter().find_map(|b| match b {
                neutral::Block::Exercise {
                    id: block_id,
                    exercise: neutral::Exercise::FillBlank { hint_zh, .. },
                } if block_id == id && !hint_zh.trim().is_empty() => Some(hint_zh.as_str()),
                _ => None,
            }),
        }
    }
    pub(crate) fn grade(
        &self,
        source: &serde_json::Value,
        id: &str,
        answer: &brioche_course_contract::ExerciseAnswer,
    ) -> std::result::Result<brioche_course_contract::GradeResult, crate::grading::GradeError> {
        use crate::grading::Grader;
        match self {
            Self::Legacy(l) => Grader::from_source(l, source)?.grade(l, id, answer),
            Self::Neutral(l) => {
                Grader::from_neutral_source(l, source)?.grade_neutral(l, id, answer)
            }
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Legacy(l) => &l.id,
            Self::Neutral(l) => &l.id,
        }
    }
    pub fn revision(&self) -> u32 {
        match self {
            Self::Legacy(l) => l.revision,
            Self::Neutral(l) => l.revision,
        }
    }
    pub fn level_id(&self) -> &str {
        match self {
            Self::Legacy(l) => &l.level_id,
            Self::Neutral(l) => &l.level_id,
        }
    }
    pub fn unit_id(&self) -> &str {
        match self {
            Self::Legacy(l) => &l.unit_id,
            Self::Neutral(l) => &l.unit_id,
        }
    }
    pub fn media(&self) -> &[brioche_course_contract::MediaAsset] {
        match self {
            Self::Legacy(l) => &l.media,
            Self::Neutral(l) => &l.media,
        }
    }
    pub fn cast(&self) -> &[brioche_course_contract::Character] {
        match self {
            Self::Legacy(l) => &l.cast,
            Self::Neutral(l) => &l.cast,
        }
    }
    pub fn audio(&self) -> &[brioche_course_contract::AudioAsset] {
        match self {
            Self::Legacy(l) => &l.audio,
            Self::Neutral(l) => &l.audio,
        }
    }
    pub(crate) fn validate_private_rules(&self, source: &serde_json::Value) -> Result<()> {
        match self {
            Self::Legacy(l) => crate::grading::Grader::from_author_source(l, source),
            Self::Neutral(l) => crate::grading::Grader::from_neutral_author_source(l, source),
        }
        .map(|_| ())
    }
    pub(crate) fn validate_public(&self) -> Result<()> {
        match self {
            Self::Legacy(l) => l.validate(),
            Self::Neutral(l) => l.validate(),
        }
        .map_err(anyhow::Error::msg)
    }
    pub fn public_document(&self) -> Result<serde_json::Value> {
        match self {
            Self::Legacy(l) => serde_json::to_value(l),
            Self::Neutral(l) => serde_json::to_value(l),
        }
        .map_err(Into::into)
    }
    pub(crate) fn scene_references(&self) -> Vec<(usize, &str)> {
        match self {
            Self::Legacy(l) => l
                .blocks
                .iter()
                .enumerate()
                .filter_map(|(i, b)| match b {
                    brioche_course_contract::Block::Scene {
                        illustration_id, ..
                    } => Some((i, illustration_id.as_str())),
                    _ => None,
                })
                .collect(),
            Self::Neutral(l) => l
                .blocks
                .iter()
                .enumerate()
                .filter_map(|(i, b)| match b {
                    brioche_course_contract::neutral::Block::Scene {
                        illustration_id, ..
                    } => Some((i, illustration_id.as_str())),
                    _ => None,
                })
                .collect(),
        }
    }
}

/// Strip only the recognized private author fields; all unknown public fields
/// remain visible to the strict versioned decoder.
pub(crate) fn public_projection(mut source: serde_json::Value) -> Result<serde_json::Value> {
    editorial(&source)?;
    let object = source
        .as_object_mut()
        .context("/: lesson must be an object")?;
    for field in ["serverOnly", "editorial", "assetRefs", "audioRefs"] {
        object.remove(field);
    }
    Ok(source)
}

fn neutral_types(source: serde_json::Value) -> Result<NeutralLesson> {
    let source = public_projection(source)?;
    if let Some(blocks) = source.get("blocks").and_then(serde_json::Value::as_array) {
        for (index, block) in blocks.iter().enumerate() {
            brioche_course_contract::neutral::Block::from_value_with_path(
                block.clone(),
                &format!("/blocks/{index}"),
            )
            .map_err(anyhow::Error::msg)?;
        }
    }
    crate::author_json::from_value(source, "")
}

/// Complete public neutral projection; no French serialization or private keys.
pub fn project_neutral_source(source: serde_json::Value) -> Result<NeutralLesson> {
    let lesson = neutral_types(source)?;
    lesson.validate().map_err(anyhow::Error::msg)?;
    Ok(lesson)
}

/// Future import preflight: registered descriptors are checked after hydration.
/// This function does not import, connect, or grant publication authorization.
pub fn validate_neutral_source_schema(mut source: serde_json::Value) -> Result<()> {
    crate::media::source_asset_refs(&source)?;
    crate::recording::source_audio_refs(&source)?;
    let has_assets = source.get("assetRefs").is_some();
    let has_audio = source.get("audioRefs").is_some();
    if let Some(object) = source.as_object_mut() {
        if has_assets {
            object.remove("media");
        }
        if has_audio {
            object.remove("audio");
        }
    }
    let lesson = neutral_types(source.clone())?;
    lesson.validate_intrinsic().map_err(anyhow::Error::msg)?;
    crate::grading::Grader::from_neutral_author_source(&lesson, &source).map(|_| ())
}

/// Structural preflight before immutable media descriptor hydration.
pub fn validate_any_source_schema(source: serde_json::Value) -> Result<()> {
    match source
        .get("schemaVersion")
        .and_then(serde_json::Value::as_str)
    {
        Some("1.0") => crate::validate_source_schema(source),
        Some("2.0") => validate_neutral_source_schema(source),
        _ => anyhow::bail!("/schemaVersion: expected supported course version 1.0 or 2.0"),
    }
}

/// Projection for publication paths that report structural and private errors separately.
pub(crate) fn project_checked_source(source: &serde_json::Value) -> Result<CheckedLesson> {
    crate::media::source_asset_refs(source)?;
    crate::recording::source_audio_refs(source)?;
    match source
        .get("schemaVersion")
        .and_then(serde_json::Value::as_str)
    {
        Some("1.0") => crate::project_source(source.clone()).map(CheckedLesson::Legacy),
        Some("2.0") => project_neutral_source(source.clone()).map(CheckedLesson::Neutral),
        _ => anyhow::bail!("/schemaVersion: expected supported course version 1.0 or 2.0"),
    }
}

/// Version dispatch belongs to offline author tooling, not an implicit API downgrade.
pub fn check_any_source(source: &serde_json::Value) -> Result<CheckedLesson> {
    let lesson = project_checked_source(source)?;
    lesson
        .validate_private_rules(source)
        .context(GRADING_CONTEXT)?;
    Ok(lesson)
}
pub fn check_any_lesson(document: &crate::author_json::Document) -> Result<CheckedLesson> {
    check_any_source(&document.value).map_err(|error| locate(document, error))
}
fn locate(document: &crate::author_json::Document, error: anyhow::Error) -> anyhow::Error {
    let grading = error.to_string() == GRADING_CONTEXT;
    let located = document.semantic(error);
    if grading {
        located.context(GRADING_CONTEXT)
    } else {
        located
    }
}

/// Offline author checks shared by individual lessons and local release packs.
pub fn check_lesson(
    document: &crate::author_json::Document,
) -> Result<brioche_course_contract::PublicLesson> {
    check_source(&document.value).map_err(|error| locate(document, error))
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
            let lesson = check_any_lesson(&selected)?;
            if !references.contains(lesson.id()) {
                return Err(selected.semantic(anyhow::anyhow!(
                    "/id: explicitly selected source is not referenced by this release"
                )));
            }
            explicit.push((root, lesson.id().to_owned()));
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
                let lesson = check_any_lesson(&source_document)?;
                for (field, matches, message) in [
                    (
                        "id",
                        lesson.id() == reference.lesson_id,
                        "lesson ID does not match release reference",
                    ),
                    (
                        "revision",
                        lesson.revision() == reference.revision,
                        "lesson revision does not match release reference",
                    ),
                    (
                        "levelId",
                        lesson.level_id() == level.id,
                        "lesson level does not match release level",
                    ),
                    (
                        "unitId",
                        lesson.unit_id() == unit.id,
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

/// Version 2 private schema is kept in the server author layer, never exported to Web.
pub fn neutral_schema() -> serde_json::Value {
    #[derive(schemars::JsonSchema)]
    #[schemars(rename_all = "camelCase", deny_unknown_fields)]
    #[allow(dead_code)]
    struct NeutralAuthorLesson {
        #[schemars(flatten)]
        lesson: NeutralLesson,
        editorial: Editorial,
        server_only: crate::grading::PrivateRules,
        #[schemars(default)]
        asset_refs: Vec<crate::media::AssetRef>,
        #[schemars(default)]
        audio_refs: Vec<crate::media::AssetRef>,
    }
    serde_json::to_value(schemars::schema_for!(NeutralAuthorLesson)).expect("schema serialization")
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
