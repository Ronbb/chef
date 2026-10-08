//! Immutable snapshots retain their source wire. Only responses are adapted.
use crate::AppError;
use brioche_course_contract::{Vocabulary, neutral::NeutralVocabulary};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum Snapshot {
    Legacy(Vocabulary),
    Neutral(NeutralVocabulary),
}
impl Snapshot {
    pub(crate) fn from_row(row: &sea_orm::QueryResult) -> Result<Self, AppError> {
        use crate::learning::field;
        let value = field::<serde_json::Value>(row, "snapshot")?;
        match field::<String>(row, "source_schema")?.as_str() {
            "1.0" => Ok(Self::Legacy(
                serde_json::from_value(value).map_err(|_| AppError::Unavailable)?,
            )),
            "2.0" => {
                let vocabulary: NeutralVocabulary =
                    serde_json::from_value(value).map_err(|_| AppError::Unavailable)?;
                let locale = field::<String>(row, "target_language")?;
                let target = match locale.as_str() {
                    "fr-FR" => brioche_course_contract::TargetLanguage::French,
                    "yue-Hant-HK" => brioche_course_contract::TargetLanguage::Cantonese,
                    _ => return Err(AppError::Unavailable),
                };
                vocabulary
                    .lemma
                    .validate(target)
                    .map_err(|_| AppError::Unavailable)?;
                Ok(Self::Neutral(vocabulary))
            }
            _ => Err(AppError::Unavailable),
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) enum KnowledgeWire {
    Legacy,
    Neutral,
}
impl KnowledgeWire {
    pub(crate) fn is_neutral(self) -> bool {
        matches!(self, Self::Neutral)
    }
    pub(crate) fn check(self, snapshot: &Snapshot) -> Result<(), AppError> {
        if matches!((self, snapshot), (Self::Legacy, Snapshot::Neutral(_))) {
            return Err(AppError::Conflict);
        }
        Ok(())
    }
    fn vocabulary(self, value: &mut serde_json::Value) -> Result<(), AppError> {
        if value.is_null() {
            return Ok(());
        }
        let snapshot: Snapshot =
            serde_json::from_value(value.clone()).map_err(|_| AppError::Unavailable)?;
        self.check(&snapshot)?;
        if let (Self::Neutral, Snapshot::Legacy(old)) = (self, snapshot) {
            *value = serde_json::to_value(
                NeutralVocabulary::try_from(&old).map_err(|_| AppError::Unavailable)?,
            )
            .map_err(|_| AppError::Unavailable)?;
        }
        Ok(())
    }
    fn adapt(self, value: &mut serde_json::Value) -> Result<(), AppError> {
        match value {
            serde_json::Value::Object(fields) => {
                for (key, child) in fields {
                    if key == "vocabulary" {
                        self.vocabulary(child)?;
                    } else {
                        self.adapt(child)?;
                    }
                }
            }
            serde_json::Value::Array(items) => {
                for child in items {
                    self.adapt(child)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub(crate) fn response<L: DeserializeOwned, N: DeserializeOwned>(
        self,
        data: &impl Serialize,
    ) -> Result<serde_json::Value, AppError> {
        let mut value = serde_json::to_value(data).map_err(|_| AppError::Unavailable)?;
        self.adapt(&mut value)?;
        match self {
            Self::Legacy => {
                serde_json::from_value::<L>(value.clone()).map_err(|_| AppError::Unavailable)?;
            }
            Self::Neutral => {
                serde_json::from_value::<N>(value.clone()).map_err(|_| AppError::Unavailable)?;
            }
        }
        Ok(value)
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoredReviewCard {
    pub id: String,
    pub knowledge_id: String,
    pub source_lesson_id: String,
    pub source_revision: u32,
    pub vocabulary: Snapshot,
    pub stage: i16,
    pub due_at: String,
    pub version: u32,
    pub suspended: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StoredSavedItem {
    pub id: String,
    pub knowledge_id: String,
    pub source_lesson_id: String,
    pub source_revision: u32,
    pub vocabulary: Option<Snapshot>,
    pub saved: bool,
    pub withdrawn: bool,
    pub version: u32,
    pub created_at: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredHistoryItem {
    pub id: String,
    pub card_id: String,
    pub vocabulary: Option<Snapshot>,
    pub withdrawn: bool,
    pub rating: brioche_course_contract::ReviewRating,
    pub old_stage: i16,
    pub new_stage: i16,
    pub reviewed_at: String,
    pub due_at: String,
    pub time_zone: String,
    pub algorithm_version: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredOverviewItem {
    pub session_id: String,
    pub lesson_id: String,
    pub revision: u32,
    pub title: serde_json::Value,
    pub last_step_id: Option<String>,
    pub completed_at: Option<String>,
    pub first_completed_at: Option<String>,
    pub updated_at: String,
}
pub(crate) fn overview_item(
    row: &sea_orm::QueryResult,
    wire: KnowledgeWire,
) -> Result<StoredOverviewItem, AppError> {
    use crate::learning::field;
    let value = field::<serde_json::Value>(row, "title")?;
    let schema = field::<String>(row, "course_schema")?;
    let title = match (schema.as_str(), wire) {
        ("1.0", KnowledgeWire::Legacy) => {
            let t: brioche_course_contract::Title =
                serde_json::from_value(value).map_err(|_| AppError::Unavailable)?;
            serde_json::to_value(t)
        }
        ("1.0", KnowledgeWire::Neutral) => {
            let t: brioche_course_contract::Title =
                serde_json::from_value(value).map_err(|_| AppError::Unavailable)?;
            serde_json::to_value(brioche_course_contract::neutral::NeutralTitle {
                target: t.fr,
                zh: t.zh,
            })
        }
        ("2.0", KnowledgeWire::Neutral) => {
            let t: brioche_course_contract::neutral::NeutralTitle =
                serde_json::from_value(value).map_err(|_| AppError::Unavailable)?;
            serde_json::to_value(t)
        }
        ("2.0", KnowledgeWire::Legacy) => return Err(AppError::Conflict),
        _ => return Err(AppError::Unavailable),
    }
    .map_err(|_| AppError::Unavailable)?;
    Ok(StoredOverviewItem {
        session_id: field(row, "id")?,
        lesson_id: field(row, "lesson_id")?,
        revision: u32::try_from(field::<i32>(row, "revision")?)
            .map_err(|_| AppError::Unavailable)?,
        title,
        last_step_id: field(row, "last_step_id")?,
        completed_at: field(row, "completed")?,
        first_completed_at: field(row, "first_completed")?,
        updated_at: field(row, "updated")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use brioche_course_contract::{ReviewCard, neutral};
    fn stored(snapshot: Snapshot) -> StoredReviewCard {
        StoredReviewCard {
            id: "a".repeat(32),
            knowledge_id: "expr-test".into(),
            source_lesson_id: "protocol-test".into(),
            source_revision: 1,
            vocabulary: snapshot,
            stage: -1,
            due_at: "2026-10-08T09:00:00Z".into(),
            version: 1,
            suspended: false,
        }
    }
    #[test]
    fn display_versions_preserve_original_cache_snapshots_and_native_pronunciation() {
        let original = crate::development_fixture()
            .unwrap()
            .knowledge
            .vocabulary
            .remove(0);
        let card = stored(Snapshot::Legacy(original.clone()));
        let before = serde_json::to_value(&card).unwrap();
        let old = KnowledgeWire::Legacy
            .response::<ReviewCard, neutral::NeutralReviewCard>(&card)
            .unwrap();
        assert_eq!(old, before);
        let new = KnowledgeWire::Neutral
            .response::<ReviewCard, neutral::NeutralReviewCard>(&card)
            .unwrap();
        assert_eq!(new["vocabulary"]["lemma"]["text"], original.lemma);
        assert_eq!(serde_json::to_value(&card).unwrap(), before);
        let source: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/neutral-cantonese.lesson.json"
        ))
        .unwrap();
        let snapshot: Snapshot =
            serde_json::from_value(source["knowledge"]["vocabulary"][0].clone()).unwrap();
        let native = stored(snapshot);
        let encoded = KnowledgeWire::Neutral
            .response::<ReviewCard, neutral::NeutralReviewCard>(&native)
            .unwrap();
        assert_eq!(encoded, serde_json::to_value(&native).unwrap());
        assert!(matches!(
            KnowledgeWire::Legacy.response::<ReviewCard, neutral::NeutralReviewCard>(&native),
            Err(AppError::Conflict)
        ));
        // A cached response is projected by the current trusted route, never rewritten.
        let cache: StoredReviewCard = serde_json::from_value(before.clone()).unwrap();
        assert_eq!(
            KnowledgeWire::Neutral
                .response::<ReviewCard, neutral::NeutralReviewCard>(&cache)
                .unwrap(),
            new
        );
        assert_eq!(serde_json::to_value(cache).unwrap(), before);
        let mut invalid = source["knowledge"]["vocabulary"][0].clone();
        invalid["serverOnly"] = serde_json::json!({"private":"sentinel"});
        assert!(serde_json::from_value::<Snapshot>(invalid).is_err());
    }
}
