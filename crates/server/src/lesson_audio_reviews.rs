//! Separate owner publication authorization and optional human listening declarations.
use crate::{
    AppError,
    identity::{AuthSession, Backend, require_operator},
    learning::{exec, field, hash, one, owner},
};
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    routing::get,
};
use brioche_course_contract::{AdminLessonAudioReview, AdminLessonAudioStatus};
use sea_orm::{ConnectionTrait, TransactionTrait};
use serde_json::Value;
use std::{path::PathBuf, sync::Arc};

pub fn router() -> Router<Backend> {
    Router::new().route(
        "/api/v1/operator/lessons/{id}/revisions/{revision}/audio-review",
        get(read).post(review),
    )
}
pub(crate) fn required(source: &Value) -> bool {
    source["audio"].as_array().is_some_and(|a| !a.is_empty())
}
pub(crate) async fn accepted(
    db: &impl ConnectionTrait,
    id: &str,
    revision: u32,
    source: &Value,
) -> Result<bool, AppError> {
    if !required(source) {
        return Ok(true);
    }
    let status = status(db, id, revision, source).await?;
    Ok(status.accepted)
}
async fn status(
    db: &impl ConnectionTrait,
    id: &str,
    revision: u32,
    source: &Value,
) -> Result<AdminLessonAudioStatus, AppError> {
    let lesson_hash = hash(source).map_err(|_| AppError::Unavailable)?;
    let row = one(db,"SELECT version,lesson_hash,accepted,reason,actor_id FROM lesson_audio_reviews WHERE lesson_id=$1 AND revision=$2 ORDER BY version DESC LIMIT 1",vec![id.into(),(revision as i32).into()]).await?;
    let direct = one(db,"SELECT actor_id,reason FROM lesson_direct_publications d WHERE lesson_id=$1 AND revision=$2 AND lesson_hash=$3 AND NOT EXISTS(SELECT 1 FROM lesson_audio_reviews r WHERE (r.lesson_id,r.revision)=(d.lesson_id,d.revision) AND r.version>d.review_version)",vec![id.into(),(revision as i32).into(),lesson_hash.clone().into()]).await?;
    let mut result = AdminLessonAudioStatus {
        published: false,
        required: required(source),
        lesson_hash: lesson_hash.clone(),
        version: row
            .as_ref()
            .map(|r| field::<i32>(r, "version"))
            .transpose()?
            .unwrap_or(0) as u32,
        accepted: row
            .as_ref()
            .map(|r| -> Result<bool, AppError> {
                Ok(field::<bool>(r, "accepted")?
                    && field::<String>(r, "lesson_hash")? == lesson_hash)
            })
            .transpose()?
            .unwrap_or(false),
        direct_authorized: false,
        reason: row
            .as_ref()
            .map(|r| field(r, "reason"))
            .transpose()?
            .unwrap_or_default(),
        actor: row
            .as_ref()
            .map(|r| field::<i64>(r, "actor_id").map(|id| format!("user:{id}")))
            .transpose()?,
    };
    if let Some(direct) = direct {
        result.accepted = true;
        result.direct_authorized = true;
        result.reason = field(&direct, "reason")?;
        result.actor = Some(format!("user:{}", field::<i64>(&direct, "actor_id")?));
    }
    Ok(result)
}

/// Owner-authorized publication is a separate audit event, never a hearing declaration.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DirectPublication {
    pub expected_lesson_hash: String,
    pub reason: String,
    pub evidence: Value,
}
pub async fn authorize_local(
    b: &Backend,
    actor: i64,
    id: &str,
    revision: u32,
    root: &std::path::Path,
    mut request: DirectPublication,
) -> Result<AdminLessonAudioStatus, AppError> {
    crate::admin::revision(id, revision)?;
    crate::admin::reason(&request.reason)?;
    if !crate::voice_references::hex(&request.expected_lesson_hash, 64)
        || !request.evidence.is_object()
        || serde_json::to_vec(&request.evidence)
            .map_err(|_| AppError::InvalidInput)?
            .len()
            > 65536
    {
        return Err(AppError::InvalidInput);
    }
    request.reason = format!("[owner-direct-publish] {}", request.reason);
    crate::admin::reason(&request.reason)?;
    let request_json = serde_json::to_value(&request).map_err(|_| AppError::InvalidInput)?;
    let tx = b.db.begin().await.map_err(|_| AppError::Unavailable)?;
    crate::voice_references::lock_operator(&tx, actor).await?;
    exec(
        &tx,
        "SELECT singleton FROM content_state WHERE singleton FOR UPDATE",
        vec![],
    )
    .await?;
    let (document, published) = source(&tx, id, revision, true).await?;
    if !required(&document) || hash(&document)? != request.expected_lesson_hash {
        return Err(AppError::Conflict);
    }
    let existing = one(&tx,"SELECT actor_id,request FROM lesson_direct_publications WHERE lesson_id=$1 AND revision=$2",vec![id.into(),(revision as i32).into()]).await?;
    if let Some(existing) = existing {
        if field::<i64>(&existing, "actor_id")? != actor
            || field::<Value>(&existing, "request")? != request_json
        {
            return Err(AppError::Conflict);
        }
    } else {
        if published {
            return Err(AppError::Conflict);
        }
        let lesson = crate::project_source(document).map_err(|_| AppError::Unavailable)?;
        lesson.validate().map_err(|_| AppError::InvalidInput)?;
        crate::recording::validate_lesson(&tx, &lesson, root).await?;
        exec(&tx,"INSERT INTO lesson_direct_publications(lesson_id,revision,lesson_hash,actor_id,reason,request,review_version)VALUES($1,$2,$3,$4,$5,$6,(SELECT COALESCE(MAX(version),0) FROM lesson_audio_reviews WHERE lesson_id=$1 AND revision=$2))",vec![id.into(),(revision as i32).into(),request.expected_lesson_hash.into(),actor.into(),request.reason.into(),request_json.into()]).await?;
    }
    let (source, published) = source(&tx, id, revision, false).await?;
    let mut result = status(&tx, id, revision, &source).await?;
    result.published = published;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(result)
}
async fn source(
    db: &impl ConnectionTrait,
    id: &str,
    rev: u32,
    lock: bool,
) -> Result<(Value, bool), AppError> {
    crate::admin::revision(id, rev)?;
    let sql = format!(
        "SELECT server_document,published,EXISTS(SELECT 1 FROM content_withdrawals WHERE lesson_id=$1 AND revision=$2) AS withdrawn FROM lesson_revisions WHERE lesson_id=$1 AND revision=$2{}",
        if lock { " FOR UPDATE" } else { "" }
    );
    let row = one(db, &sql, vec![id.into(), (rev as i32).into()])
        .await?
        .ok_or(AppError::NotFound)?;
    if field::<bool>(&row, "withdrawn")? {
        return Err(AppError::Gone);
    }
    Ok((field(&row, "server_document")?, field(&row, "published")?))
}
async fn read(
    auth: AuthSession,
    State(b): State<Backend>,
    Path((id, rev)): Path<(String, u32)>,
) -> Result<Json<AdminLessonAudioStatus>, AppError> {
    require_operator(&auth)?;
    let (source, published) = source(&b.db, &id, rev, false).await?;
    let mut current = status(&b.db, &id, rev, &source).await?;
    current.published = published;
    Ok(Json(current))
}
async fn review(
    auth: AuthSession,
    State(b): State<Backend>,
    Path((id, rev)): Path<(String, u32)>,
    Extension(root): Extension<PathBuf>,
    Extension(permits): Extension<Arc<tokio::sync::Semaphore>>,
    Json(request): Json<AdminLessonAudioReview>,
) -> Result<Json<AdminLessonAudioStatus>, AppError> {
    require_operator(&auth)?;
    crate::admin::revision(&id, rev)?;
    crate::admin::reason(&request.reason)?;
    if !crate::voice_references::hex(&request.expected_lesson_hash, 64)
        || (request.accepted && !request.heard)
    {
        return Err(AppError::InvalidInput);
    }
    let actor = owner(&auth)?;
    let _permit = permits
        .try_acquire_many_owned(2)
        .map_err(|_| AppError::RateLimited)?;
    let tx = b.db.begin().await.map_err(|_| AppError::Unavailable)?;
    crate::voice_references::lock_operator(&tx, actor).await?;
    exec(
        &tx,
        "SELECT singleton FROM content_state WHERE singleton FOR UPDATE",
        vec![],
    )
    .await?;
    let (source, published) = source(&tx, &id, rev, true).await?;
    let mut current = status(&tx, &id, rev, &source).await?;
    current.published = published;
    if !current.required || current.lesson_hash != request.expected_lesson_hash {
        return Err(AppError::Conflict);
    }
    if current.version != request.version {
        if current.version == request.version.saturating_add(1) {
            let row=one(&tx,"SELECT accepted,heard,actor_id,reason FROM lesson_audio_reviews WHERE lesson_id=$1 AND revision=$2 AND version=$3",vec![id.into(),(rev as i32).into(),(current.version as i32).into()]).await?.ok_or(AppError::Unavailable)?;
            if field::<bool>(&row, "accepted")? == request.accepted
                && field::<bool>(&row, "heard")? == request.heard
                && field::<i64>(&row, "actor_id")? == actor
                && field::<String>(&row, "reason")? == request.reason
            {
                tx.commit().await.map_err(|_| AppError::Unavailable)?;
                return Ok(Json(current));
            }
        }
        return Err(AppError::Conflict);
    }
    if published {
        return Err(AppError::Conflict);
    }
    if request.accepted {
        let lesson = crate::project_source(source).map_err(|_| AppError::Unavailable)?;
        lesson.validate().map_err(|_| AppError::Unavailable)?;
        crate::recording::validate_lesson(&tx, &lesson, &root).await?;
    }
    let next = i32::try_from(current.version)
        .ok()
        .and_then(|v| v.checked_add(1))
        .ok_or(AppError::Unavailable)?;
    exec(&tx,"INSERT INTO lesson_audio_reviews(lesson_id,revision,version,lesson_hash,accepted,heard,actor_id,reason) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",vec![id.into(),(rev as i32).into(),next.into(),current.lesson_hash.clone().into(),request.accepted.into(),request.heard.into(),actor.into(),request.reason.clone().into()]).await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(AdminLessonAudioStatus {
        version: next as u32,
        accepted: request.accepted,
        direct_authorized: false,
        reason: request.reason,
        actor: Some(format!("user:{actor}")),
        ..current
    }))
}
