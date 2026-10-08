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
    product: Option<crate::product::ProductId>,
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
    if let Err(error) = check_owner(db, product, &document.value).await {
        return match error {
            AppError::NotFound => {
                Ok(document.uploaded_issue("/id", "课程编号或版本不可用于当前产品。"))
            }
            other => Err(other),
        };
    }
    let source =
        match crate::media::hydrate_source_for_product(db, product, document.value.clone()).await {
            Ok(source) => source,
            Err(error) => return issue(document, error, "图片素材未登记，或引用版本不存在。"),
        };
    let source = match crate::recording::hydrate_source_for_product(db, product, source).await {
        Ok(source) => source,
        Err(error) => return issue(document, error, "录音未登记，或引用版本不存在。"),
    };
    let lesson = match crate::author_source::check_source(&source) {
        Ok(lesson) => lesson,
        Err(error) => return issue(document, error, "登记素材与课程结构或引用不匹配。"),
    };
    if let Err(error) = crate::media::validate_lesson_detailed(db, product, &lesson, root).await {
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
pub(crate) async fn import_operator(
    db: &DatabaseConnection,
    product: Option<crate::product::ProductId>,
    source: Value,
    operator: &crate::product_memberships::Operator,
    reason: &str,
) -> anyhow::Result<brioche_course_contract::AdminImportResult> {
    if product.is_some_and(|product| product != operator.product) {
        return Err(AppError::Forbidden.into());
    }
    let tx = db.begin().await.map_err(|_| AppError::Unavailable)?;
    operator.lock_content(&tx).await?;
    let result =
        import_product_transaction(&tx, product, source, &operator.audit_actor(), reason, true)
            .await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(result)
}
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
    import_product_transaction(db, None, source, actor, reason, allow_identical_retry).await
}
// Caller owns the transaction and authorization; product comes from trusted assembly.
pub(crate) async fn import_product_transaction(
    db: &impl ConnectionTrait,
    product: Option<crate::product::ProductId>,
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
    check_owner(db, product, &source).await?;
    crate::media::source_asset_refs(&source)?;
    crate::recording::source_audio_refs(&source)?;
    let source = crate::media::hydrate_source_for_product(db, product, source).await?;
    let source = crate::recording::hydrate_source_for_product(db, product, source).await?;
    let lesson = crate::project_source(source.clone())?;
    crate::grading::Grader::from_author_source(&lesson, &source)?;
    crate::media::validate_product_references(db, product, &lesson).await?;
    crate::recording::validate_product_references(db, product, &lesson).await?;
    // Serialize import retries by their immutable identity, independent of the directory lock.
    exec(
        db,
        "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
        vec![format!("lesson-import:{}:{}", lesson.id, lesson.revision).into()],
    )
    .await?;
    // Recheck after the global identity lock to prevent a concurrent foreign
    // import becoming an identical retry. Product-local keys are still pending.
    check_owner(db, product, &source).await?;
    let identity = vec![lesson.id.clone().into(), (lesson.revision as i32).into()];
    if let Some(existing) = one(
        db,
        &format!(
            "SELECT server_document FROM lesson_revisions WHERE lesson_id=$1 AND revision=$2{}",
            crate::learning::product_filter(product, "product_id")
        ),
        identity,
    )
    .await?
    {
        if !allow_identical_retry || field::<Value>(&existing, "server_document")? != source {
            return Err(RevisionConflict.into());
        }
    } else {
        let mut values = vec![
            lesson.id.clone().into(),
            (lesson.revision as i32).into(),
            serde_json::to_value(&lesson)?.into(),
            source.into(),
        ];
        let sql = if let Some(product) = product {
            values.push(product.as_str().into());
            "INSERT INTO lesson_revisions(lesson_id,revision,published,public_document,server_document,product_id) VALUES($1,$2,false,$3,$4,$5)"
        } else {
            "INSERT INTO lesson_revisions(lesson_id,revision,published,public_document,server_document) VALUES($1,$2,false,$3,$4)"
        };
        exec(db, sql, values).await?;
        let mut values = vec![
            lesson.id.clone().into(),
            (lesson.revision as i32).into(),
            actor.into(),
            reason.into(),
        ];
        let sql = if let Some(product) = product {
            values.push(product.as_str().into());
            "INSERT INTO lesson_import_audit(lesson_id,revision,actor,reason,product_id) VALUES($1,$2,$3,$4,$5)"
        } else {
            "INSERT INTO lesson_import_audit(lesson_id,revision,actor,reason) VALUES($1,$2,$3,$4)"
        };
        exec(db, sql, values).await?;
    }
    Ok(brioche_course_contract::AdminImportResult {
        lesson_id: lesson.id,
        revision: lesson.revision,
    })
}

// Temporary global-ID collision guard: do not read another product's private
// document. Product-local identities will replace this global ownership lookup.
async fn check_owner(
    db: &impl ConnectionTrait,
    product: Option<crate::product::ProductId>,
    source: &Value,
) -> Result<(), AppError> {
    let Some(product) = product else {
        return Ok(());
    };
    let id = source["id"].as_str().ok_or(AppError::InvalidInput)?;
    let revision = source["revision"]
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or(AppError::InvalidInput)?;
    if let Some(row) = one(
        db,
        "SELECT product_id FROM lesson_revisions WHERE lesson_id=$1 AND revision=$2",
        vec![id.into(), revision.into()],
    )
    .await?
        && field::<String>(&row, "product_id")? != product.as_str()
    {
        return Err(AppError::NotFound);
    }
    Ok(())
}
