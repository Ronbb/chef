//! Durable paid clip attempts from immutable plans. No automatic retries or publication.
use crate::{
    AppError,
    identity::{AuthSession, Backend, require_operator},
    learning::{exec, field, one, owner},
    qwen::{ProviderError, Service, SpeechRequest},
    voice_references::{hex, lock_operator},
};
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    routing::{get, post},
};
use brioche_course_contract::{
    AdminSpeechClip, AdminSpeechClipRequest, AdminSpeechClipReview, AdminSpeechClips,
};
use sea_orm::{ConnectionTrait, DbBackend, QueryResult, Statement, TransactionTrait};
use serde_json::Value;
use std::{path::PathBuf, sync::Arc};
pub(crate) const SELECT: &str = r#"SELECT a.id,a.plan_id,a.generation_key,a.reused_from,e.result,
CASE WHEN e.status='submitted' AND e.created_at<clock_timestamp()-interval '300 seconds' THEN 'unknown' ELSE e.status END AS status,
COALESCE(r.accepted,original_review.accepted) AS accepted,
COALESCE(r.actor_id,original_review.actor_id) AS review_actor,
COALESCE(r.reason,original_review.reason) AS review_reason,
to_char(a.created_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"') AS created_at
FROM course_speech_clips a JOIN course_speech_plans p ON p.id=a.plan_id
LEFT JOIN course_speech_clips original ON original.id=a.reused_from
LEFT JOIN course_speech_plans original_plan ON original_plan.id=original.plan_id
JOIN LATERAL(SELECT * FROM course_speech_clip_events WHERE clip_id=a.id ORDER BY version DESC LIMIT 1)e ON true
LEFT JOIN course_speech_clip_reviews r ON r.clip_id=a.id
LEFT JOIN course_speech_clip_reviews original_review ON original_review.clip_id=original.id"#;
pub(crate) const VISIBLE: &str = "NOT EXISTS(SELECT 1 FROM content_withdrawals w WHERE (w.lesson_id,w.revision)=(p.lesson_id,p.lesson_revision) OR (w.lesson_id,w.revision)=(original_plan.lesson_id,original_plan.lesson_revision))";
pub fn router() -> Router<Backend> {
    Router::new()
        .route("/api/v1/operator/speech-plans/{id}/clips", get(list))
        .route("/api/v1/operator/speech-clips", post(create))
        .route("/api/v1/operator/speech-clips/{id}", get(read))
        .route("/api/v1/operator/speech-clips/{id}/file", get(file))
        .route("/api/v1/operator/speech-clips/{id}/review", post(review))
}
pub(crate) fn item(row: &QueryResult) -> Result<AdminSpeechClip, AppError> {
    let result: Option<Value> = field(row, "result")?;
    Ok(AdminSpeechClip {
        id: field(row, "id")?,
        plan_id: field(row, "plan_id")?,
        generation_key: field(row, "generation_key")?,
        reused_from: field(row, "reused_from")?,
        status: field(row, "status")?,
        accepted: field(row, "accepted")?,
        created_at: field(row, "created_at")?,
        duration_ms: result
            .as_ref()
            .and_then(|v| v["durationMs"].as_u64())
            .map(|v| v as u32),
        request_id: result
            .as_ref()
            .and_then(|v| v["requestId"].as_str())
            .map(String::from),
    })
}
async fn load(db: &impl ConnectionTrait, id: &str) -> Result<QueryResult, AppError> {
    if !hex(id, 32) {
        return Err(AppError::InvalidInput);
    }
    one(
        db,
        &format!("{SELECT} WHERE a.id=$1 AND {VISIBLE}"),
        vec![id.into()],
    )
    .await?
    .ok_or(AppError::NotFound)
}
pub(crate) async fn plan(db: &impl ConnectionTrait, id: &str) -> Result<Value, AppError> {
    if !hex(id, 32) {
        return Err(AppError::InvalidInput);
    }
    let row=one(db,"SELECT p.plan FROM course_speech_plans p WHERE p.id=$1 AND NOT EXISTS(SELECT 1 FROM content_withdrawals w WHERE (w.lesson_id,w.revision)=(p.lesson_id,p.lesson_revision))",vec![id.into()]).await?.ok_or(AppError::NotFound)?;
    field(&row, "plan")
}
pub(crate) async fn latest(
    db: &impl ConnectionTrait,
    key: &str,
) -> Result<Option<QueryResult>, AppError> {
    one(db,&format!("{SELECT} WHERE a.generation_key=$1 AND {VISIBLE} ORDER BY a.created_at DESC,a.id DESC LIMIT 1"),vec![key.into()]).await
}
async fn read(
    auth: AuthSession,
    State(b): State<Backend>,
    Path(id): Path<String>,
) -> Result<Json<AdminSpeechClip>, AppError> {
    require_operator(&auth).await?;
    Ok(Json(item(&load(&b.db, &id).await?)?))
}
async fn list(
    auth: AuthSession,
    State(b): State<Backend>,
    Path(id): Path<String>,
    service: Option<Extension<Service>>,
) -> Result<Json<AdminSpeechClips>, AppError> {
    require_operator(&auth).await?;
    let source = plan(&b.db, &id).await?;
    let requests = source["requests"]
        .as_object()
        .ok_or(AppError::Unavailable)?;
    let keys: Vec<String> = requests.keys().cloned().collect();
    let rows=b.db.query_all_raw(Statement::from_sql_and_values(DbBackend::Postgres,format!("SELECT DISTINCT ON(a.generation_key) * FROM ({SELECT} WHERE a.generation_key=ANY($1::text[]) AND {VISIBLE}) a ORDER BY a.generation_key,a.created_at DESC,a.id DESC"),vec![keys.into()])).await.map_err(|_|AppError::Unavailable)?;
    Ok(Json(AdminSpeechClips {
        items: rows.iter().map(item).collect::<Result<_, _>>()?,
        configured: service.is_some(),
    }))
}
fn speech(source: &Value, key: &str) -> Result<SpeechRequest, AppError> {
    let request = source["requests"].get(key).ok_or(AppError::InvalidInput)?;
    let target = source["targets"]
        .as_array()
        .and_then(|ts| ts.iter().find(|t| t["generationKey"] == key))
        .ok_or(AppError::Unavailable)?;
    let speech = SpeechRequest {
        profile: serde_json::from_value(request["profile"].clone())
            .map_err(|_| AppError::Unavailable)?,
        text: target["text"].as_str().ok_or(AppError::Unavailable)?.into(),
        emotion: target["emotion"]
            .as_str()
            .ok_or(AppError::Unavailable)?
            .into(),
    };
    if request["parameters"] != speech.parameters().map_err(|_| AppError::InvalidInput)?
        || crate::media::digest(&serde_json::to_vec(request).map_err(|_| AppError::Unavailable)?)
            != key
    {
        return Err(AppError::Unavailable);
    }
    Ok(speech)
}
async fn create(
    auth: AuthSession,
    State(b): State<Backend>,
    service: Option<Extension<Service>>,
    Extension(root): Extension<PathBuf>,
    Extension(read_permits): Extension<Arc<tokio::sync::Semaphore>>,
    Json(request): Json<AdminSpeechClipRequest>,
) -> Result<Json<AdminSpeechClip>, AppError> {
    require_operator(&auth).await?;
    create_for_actor(
        b,
        owner(&auth)?,
        service.map(|s| s.0),
        root,
        read_permits,
        request,
    )
    .await
}

/// Trusted local generation from a persisted plan. Does not accept or publish audio.
pub async fn submit_local(
    b: Backend,
    actor: i64,
    service: Option<Service>,
    root: PathBuf,
    mut request: AdminSpeechClipRequest,
) -> Result<AdminSpeechClip, AppError> {
    request.reason = format!("[local-cli] {}", request.reason);
    let id = request.id.clone();
    let Json(mut result) = create_for_actor(
        b.clone(),
        actor,
        service,
        root,
        Arc::new(tokio::sync::Semaphore::new(2)),
        request,
    )
    .await?;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(245);
    while result.status == "submitted" && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        result = item(&load(&b.db, &id).await?)?;
    }
    Ok(result)
}

async fn create_for_actor(
    b: Backend,
    actor: i64,
    service: Option<Service>,
    root: PathBuf,
    read_permits: Arc<tokio::sync::Semaphore>,
    request: AdminSpeechClipRequest,
) -> Result<Json<AdminSpeechClip>, AppError> {
    crate::admin::reason(&request.reason)?;
    if !hex(&request.id, 32)
        || !hex(&request.plan_id, 32)
        || !hex(&request.generation_key, 64)
        || !hex(&request.expected_plan_hash, 64)
        || request
            .expected_previous_id
            .as_deref()
            .is_some_and(|id| !hex(id, 32))
    {
        return Err(AppError::InvalidInput);
    }
    let payload = serde_json::to_value(&request).map_err(|_| AppError::InvalidInput)?;
    let tx = b.db.begin().await.map_err(|_| AppError::Unavailable)?;
    lock_operator(&tx, actor).await?;
    if let Some(row) = one(
        &tx,
        "SELECT actor_id,request FROM course_speech_clips WHERE id=$1",
        vec![request.id.clone().into()],
    )
    .await?
    {
        if field::<i64>(&row, "actor_id")? != actor || field::<Value>(&row, "request")? != payload {
            return Err(AppError::Conflict);
        }
        return Ok(Json(item(&load(&tx, &request.id).await?)?));
    }
    exec(
        &tx,
        "SELECT singleton FROM content_state WHERE singleton FOR UPDATE",
        vec![],
    )
    .await?;
    let source = plan(&tx, &request.plan_id).await?;
    if source["planHash"] != request.expected_plan_hash {
        return Err(AppError::Conflict);
    }
    let speech = speech(&source, &request.generation_key)?;
    let previous = latest(&tx, &request.generation_key).await?;
    let old = previous.as_ref().map(item).transpose()?;
    if old.as_ref().map(|i| &i.id) != request.expected_previous_id.as_ref() {
        return Err(AppError::Conflict);
    }
    let mut reuse = None;
    let mut cached = None;
    if let Some(old) = &old {
        if old.status == "submitted"
            || (old.status == "unknown" && !request.retry_unknown_confirmed)
        {
            return Err(AppError::Conflict);
        }
        if old.status == "ready" && old.accepted != Some(false) {
            let result: Value =
                field::<Option<Value>>(previous.as_ref().ok_or(AppError::Unavailable)?, "result")?
                    .ok_or(AppError::Unavailable)?;
            let _permit = read_permits
                .try_acquire_owned()
                .map_err(|_| AppError::RateLimited)?;
            let directory = root.clone();
            let verify = result.clone();
            tokio::task::spawn_blocking(move || crate::speech_media::read(&directory, &verify))
                .await
                .map_err(|_| AppError::Unavailable)??;
            reuse = Some(old.reused_from.clone().unwrap_or_else(|| old.id.clone()));
            cached = Some(result);
        }
    }
    let paid = if cached.is_none() {
        if !request.cost_confirmed {
            return Err(AppError::InvalidInput);
        }
        let service = service.ok_or(AppError::Unavailable)?;
        let permit = service
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| AppError::RateLimited)?;
        Some((service, permit))
    } else {
        None
    };
    exec(&tx,"INSERT INTO course_speech_clips(id,plan_id,generation_key,request,actor_id,reason,reused_from)VALUES($1,$2,$3,$4,$5,$6,$7)",vec![request.id.clone().into(),request.plan_id.into(),request.generation_key.into(),payload.into(),actor.into(),request.reason.into(),reuse.into()]).await?;
    exec(
        &tx,
        "INSERT INTO course_speech_clip_events(clip_id,version,status,result)VALUES($1,1,$2,$3)",
        vec![
            request.id.clone().into(),
            if cached.is_some() {
                "ready"
            } else {
                "submitted"
            }
            .into(),
            cached.into(),
        ],
    )
    .await?;
    let receipt = item(&load(&tx, &request.id).await?)?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    if let Some((service, permit)) = paid {
        tokio::spawn(async move {
            let _permit = permit;
            let cloned = speech.profile.voice_kind == "cloned";
            let response = tokio::time::timeout(
                std::time::Duration::from_secs(240),
                service.transport.synthesize(&speech),
            )
            .await
            .unwrap_or(Err(ProviderError::Unknown));
            let (status, result) = match response {
                Ok(output) => match tokio::task::spawn_blocking(move || {
                    crate::speech_media::store(&root, output, cloned)
                })
                .await
                {
                    Ok(Ok(result)) => ("ready", Some(result)),
                    _ => ("unknown", None),
                },
                Err(ProviderError::Rejected) => ("failed", None),
                Err(ProviderError::Unknown) => ("unknown", None),
            };
            if exec(&b.db,"INSERT INTO course_speech_clip_events(clip_id,version,status,result)VALUES($1,2,$2,$3)",vec![request.id.into(),status.into(),result.into()]).await.is_err(){tracing::warn!("course speech result persistence unavailable");}
        });
    }
    Ok(Json(receipt))
}
async fn file(
    auth: AuthSession,
    State(b): State<Backend>,
    Path(id): Path<String>,
    Extension(root): Extension<PathBuf>,
    Extension(permits): Extension<Arc<tokio::sync::Semaphore>>,
    headers: HeaderMap,
) -> Result<axum::response::Response, AppError> {
    require_operator(&auth).await?;
    let row = load(&b.db, &id).await?;
    if item(&row)?.status != "ready" {
        return Err(AppError::NotFound);
    }
    let result = field::<Option<Value>>(&row, "result")?.ok_or(AppError::Unavailable)?;
    let _permit = permits
        .try_acquire_owned()
        .map_err(|_| AppError::RateLimited)?;
    let (sha, bytes) =
        tokio::task::spawn_blocking(move || crate::speech_media::read(&root, &result))
            .await
            .map_err(|_| AppError::Unavailable)??;
    crate::recording::bytes_response("audio/wav".into(), format!("\"{sha}\""), bytes, headers)
}
async fn review(
    auth: AuthSession,
    State(b): State<Backend>,
    Path(id): Path<String>,
    Json(request): Json<AdminSpeechClipReview>,
) -> Result<Json<AdminSpeechClip>, AppError> {
    require_operator(&auth).await?;
    Ok(Json(
        review_for_actor(&b, owner(&auth)?, id, request).await?,
    ))
}

/// Record an explicit human decision through the same transaction as the HTTP route.
pub async fn review_local(
    b: &Backend,
    actor: i64,
    id: String,
    mut request: AdminSpeechClipReview,
) -> Result<AdminSpeechClip, AppError> {
    request.reason = format!("[local-cli] {}", request.reason);
    review_for_actor(b, actor, id, request).await
}

async fn review_for_actor(
    b: &Backend,
    actor: i64,
    id: String,
    request: AdminSpeechClipReview,
) -> Result<AdminSpeechClip, AppError> {
    crate::admin::reason(&request.reason)?;
    if !hex(&id, 32) || !request.heard {
        return Err(AppError::InvalidInput);
    }
    let tx = b.db.begin().await.map_err(|_| AppError::Unavailable)?;
    lock_operator(&tx, actor).await?;
    exec(
        &tx,
        "SELECT singleton FROM content_state WHERE singleton FOR UPDATE",
        vec![],
    )
    .await?;
    let clip = item(&load(&tx, &id).await?)?;
    if clip.status != "ready" {
        return Err(AppError::Conflict);
    }
    if let Some(row) = one(
        &tx,
        "SELECT actor_id,accepted,reason FROM course_speech_clip_reviews WHERE clip_id=$1",
        vec![id.clone().into()],
    )
    .await?
    {
        if field::<i64>(&row, "actor_id")? != actor
            || field::<bool>(&row, "accepted")? != request.accepted
            || field::<String>(&row, "reason")? != request.reason
        {
            return Err(AppError::Conflict);
        }
        return Ok(clip);
    }
    exec(&tx,"INSERT INTO course_speech_clip_reviews(clip_id,accepted,actor_id,reason)VALUES($1,$2,$3,$4)",vec![id.clone().into(),request.accepted.into(),actor.into(),request.reason.into()]).await?;
    let clip = item(&load(&tx, &id).await?)?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(clip)
}
