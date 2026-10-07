//! Shared immutable lesson import for local author tooling and authenticated operators.
use crate::{
    AppError,
    learning::{exec, field, one},
};
use sea_orm::{ConnectionTrait, DatabaseConnection, TransactionTrait};
use serde_json::Value;
#[derive(Debug)]
pub struct RevisionConflict;

/// Read-only registry/media check. The caller holds a read-only snapshot; no
/// lesson, approval, release, audit or media object is created by this path.
pub(crate) async fn check_registered(
    db: &impl ConnectionTrait,
    document: &crate::author_json::Document,
    root: &std::path::Path,
) -> Result<brioche_course_contract::AdminDocumentCheck, AppError> {
    fn issue(
        document: &crate::author_json::Document,
        error: anyhow::Error,
        message: &str,
    ) -> Result<brioche_course_contract::AdminDocumentCheck, AppError> {
        if error.downcast_ref::<AppError>().is_some() {
            return Err(AppError::Unavailable);
        }
        let pointer = error
            .chain()
            .find_map(|cause| {
                let text = cause.to_string();
                text.split_once(": ")
                    .filter(|(pointer, _)| pointer.starts_with('/'))
                    .map(|(pointer, _)| pointer.to_owned())
            })
            .unwrap_or_else(|| "/".into());
        Ok(document.uploaded_issue(&pointer, message))
    }
    let source = match crate::media::hydrate_source(db, document.value.clone()).await {
        Ok(source) => source,
        Err(error) => return issue(document, error, "图片素材未登记，或引用版本不存在。"),
    };
    let source = match crate::recording::hydrate_source(db, source).await {
        Ok(source) => source,
        Err(error) => return issue(document, error, "录音未登记，或引用版本不存在。"),
    };
    let lesson = match crate::author_source::check_source(&source) {
        Ok(lesson) => lesson,
        Err(error) => return issue(document, error, "登记素材与课程结构或引用不匹配。"),
    };
    if let Err(error) = crate::media::validate_lesson_detailed(db, &lesson, root).await {
        match error.runtime {
            AppError::InvalidInput => {
                let pointer = error
                    .diagnostic
                    .strip_prefix("imported lesson ")
                    .and_then(|text| text.split_once(": "))
                    .map_or("/", |(pointer, _)| pointer);
                let message = if error.diagnostic.contains("stored ")
                    || error.diagnostic.contains("decode")
                {
                    "素材文件缺失、损坏或解码信息与登记不一致。"
                } else {
                    "素材、角色或录音未登记，或版本信息不一致。"
                };
                return Ok(document.uploaded_issue(pointer, message));
            }
            other => return Err(other),
        }
    }
    Ok(brioche_course_contract::AdminDocumentCheck {
        valid: true,
        issue: None,
    })
}
impl std::fmt::Display for RevisionConflict {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("/revision: lesson revision already exists; revisions are immutable")
    }
}
impl std::error::Error for RevisionConflict {}
pub async fn import(
    db: &DatabaseConnection,
    source: Value,
    actor: &str,
    reason: &str,
) -> anyhow::Result<brioche_course_contract::AdminImportResult> {
    import_impl(db, source, actor, reason, false).await
}
pub async fn import_retry(
    db: &DatabaseConnection,
    source: Value,
    actor: &str,
    reason: &str,
) -> anyhow::Result<brioche_course_contract::AdminImportResult> {
    import_impl(db, source, actor, reason, true).await
}
async fn import_impl(
    db: &DatabaseConnection,
    source: Value,
    actor: &str,
    reason: &str,
    allow_identical_retry: bool,
) -> anyhow::Result<brioche_course_contract::AdminImportResult> {
    let tx = db.begin().await.map_err(|_| AppError::Unavailable)?;
    let result = import_transaction(&tx, source, actor, reason, allow_identical_retry).await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(result)
}
/// The caller owns the transaction, allowing recording registration and draft import
/// to commit together. CLI and standalone imports use this same validation path.
pub(crate) async fn import_transaction(
    db: &impl ConnectionTrait,
    source: Value,
    actor: &str,
    reason: &str,
    allow_identical_retry: bool,
) -> anyhow::Result<brioche_course_contract::AdminImportResult> {
    anyhow::ensure!(
        !actor.trim().is_empty() && actor.len() <= 1000 && !actor.chars().any(char::is_control),
        "/: invalid import actor"
    );
    anyhow::ensure!(
        !reason.trim().is_empty() && reason.len() <= 1000 && !reason.chars().any(char::is_control),
        "/: invalid import reason"
    );
    crate::validate_source_schema(source.clone())?;
    crate::media::source_asset_refs(&source)?;
    crate::recording::source_audio_refs(&source)?;
    let source = crate::media::hydrate_source(db, source).await?;
    let source = crate::recording::hydrate_source(db, source).await?;
    let lesson = crate::project_source(source.clone())?;
    crate::grading::Grader::from_author_source(&lesson, &source)?;
    // Serialize import retries by their immutable identity, independent of the directory lock.
    exec(
        db,
        "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
        vec![format!("lesson-import:{}:{}", lesson.id, lesson.revision).into()],
    )
    .await?;
    let identity = vec![lesson.id.clone().into(), (lesson.revision as i32).into()];
    if let Some(existing) = one(
        db,
        "SELECT server_document FROM lesson_revisions WHERE lesson_id=$1 AND revision=$2",
        identity,
    )
    .await?
    {
        if !allow_identical_retry || field::<Value>(&existing, "server_document")? != source {
            return Err(RevisionConflict.into());
        }
    } else {
        exec(db,"INSERT INTO lesson_revisions(lesson_id,revision,published,public_document,server_document) VALUES($1,$2,false,$3,$4)",vec![lesson.id.clone().into(),(lesson.revision as i32).into(),serde_json::to_value(&lesson)?.into(),source.into()]).await?;
        exec(
            db,
            "INSERT INTO lesson_import_audit(lesson_id,revision,actor,reason) VALUES($1,$2,$3,$4)",
            vec![
                lesson.id.clone().into(),
                (lesson.revision as i32).into(),
                actor.into(),
                reason.into(),
            ],
        )
        .await?;
    }
    Ok(brioche_course_contract::AdminImportResult {
        lesson_id: lesson.id,
        revision: lesson.revision,
    })
}
