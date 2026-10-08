//! Audition attempts are durable before the paid call. Files remain private and reviews append versions.
use crate::{
    AppError,
    admin_auth::AdminAuth,
    identity::Backend,
    learning::{exec, field, one, product_filter},
    qwen::{ProviderError, Service},
    voice_references::hex,
};
use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::{get, post},
};
use brioche_course_contract::{
    AdminAudition, AdminAuditionRequest, AdminAuditionReview, AdminAuditions,
    AdminCharacterVoiceRequest, CharacterVoiceProfile, VoiceJobStatus,
};
use sea_orm::{ConnectionTrait, DbBackend, QueryResult, Statement, TransactionTrait};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};
fn projection(product: Option<crate::product::ProductId>) -> String {
    let same = |column: &str| {
        if product.is_some() {
            format!(" AND {column}=a.product_id")
        } else {
            String::new()
        }
    };
    format!(
        r#"SELECT a.id,a.clone_job_id,COALESCE(g.character_id,a.character_id) AS character_id,COALESCE(g.character_revision,a.character_revision) AS character_revision,COALESCE(g.voice_revision,a.base_voice_revision) AS voice_revision,a.profile,a.parameters,e.result,
CASE WHEN e.status='submitted' AND e.created_at<clock_timestamp()-interval '300 seconds' THEN 'unknown' ELSE e.status END AS status,
r.accepted,r.voice_revision AS applied_voice_revision,to_char(a.created_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"') AS created_at
FROM voice_auditions a LEFT JOIN voice_clone_jobs j ON j.id=a.clone_job_id{} LEFT JOIN voice_reference_grants g ON g.id=j.grant_id{}
JOIN LATERAL (SELECT * FROM voice_audition_events WHERE audition_id=a.id{} ORDER BY version DESC LIMIT 1) e ON true
LEFT JOIN voice_audition_reviews r ON r.audition_id=a.id{}"#,
        same("j.product_id"),
        same("g.product_id"),
        same("product_id"),
        same("r.product_id")
    )
}
async fn local_audition_keys(db: &impl ConnectionTrait) -> Result<bool, AppError> {
    let row=one(db,r#"SELECT EXISTS(SELECT 1 FROM pg_catalog.pg_constraint c WHERE c.contype='p' AND c.conrelid='voice_auditions'::regclass
        AND (SELECT array_agg(a.attname::text ORDER BY k.position) FROM unnest(c.conkey) WITH ORDINALITY k(column_number,position) JOIN pg_catalog.pg_attribute a ON a.attrelid=c.conrelid AND a.attnum=k.column_number)=ARRAY['product_id','id']::text[]) AS ready"#,vec![]).await?.ok_or(AppError::Unavailable)?;
    field(&row, "ready")
}

#[derive(Clone)]
struct Store {
    product: Option<crate::product::ProductId>,
    db: sea_orm::DatabaseConnection,
}
pub(crate) fn router<S: Clone + Send + Sync + 'static>(
    db: sea_orm::DatabaseConnection,
    product: Option<crate::product::ProductId>,
) -> Router<S> {
    Router::new()
        .route("/api/v1/operator/voice-auditions", get(list).post(create))
        .route("/api/v1/operator/voice-auditions/{id}", get(read))
        .route("/api/v1/operator/voice-auditions/{id}/file", get(file))
        .route("/api/v1/operator/voice-auditions/{id}/review", post(review))
        .with_state(Store { db, product })
}
fn item(row: &QueryResult) -> Result<AdminAudition, AppError> {
    let p: Value = field(row, "profile")?;
    let parameters: Value = field(row, "parameters")?;
    let result: Option<Value> = field(row, "result")?;
    Ok(AdminAudition {
        id: field(row, "id")?,
        clone_job_id: field(row, "clone_job_id")?,
        profile: serde_json::from_value(p.clone()).map_err(|_| AppError::Unavailable)?,
        character_id: field(row, "character_id")?,
        character_revision: field::<i32>(row, "character_revision")? as u32,
        base_voice_revision: field::<i32>(row, "voice_revision")? as u32,
        voice_id: p["voiceId"].as_str().ok_or(AppError::Unavailable)?.into(),
        text: parameters["input"]["text"]
            .as_str()
            .ok_or(AppError::Unavailable)?
            .into(),
        emotion: parameters["sceneEmotion"]
            .as_str()
            .ok_or(AppError::Unavailable)?
            .into(),
        status: field(row, "status")?,
        duration_ms: result
            .as_ref()
            .and_then(|r| r["durationMs"].as_u64())
            .map(|n| n as u32),
        request_id: result
            .as_ref()
            .and_then(|r| r["requestId"].as_str())
            .map(String::from),
        accepted: field(row, "accepted")?,
        applied_voice_revision: field::<Option<i32>>(row, "applied_voice_revision")?
            .map(|n| n as u32),
        created_at: field(row, "created_at")?,
    })
}
async fn load(
    db: &impl ConnectionTrait,
    product: Option<crate::product::ProductId>,
    id: &str,
) -> Result<QueryResult, AppError> {
    if !hex(id, 32) {
        return Err(AppError::InvalidInput);
    }
    one(
        db,
        &format!(
            "{} WHERE a.id=$1{}",
            projection(product),
            product_filter(product, "a.product_id")
        ),
        vec![id.into()],
    )
    .await?
    .ok_or(AppError::NotFound)
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Cursor {
    after_id: Option<String>,
    clone_job_id: Option<String>,
    character_id: Option<String>,
    character_revision: Option<u32>,
}
async fn list(
    auth: AdminAuth,
    State(b): State<Store>,
    service: Option<Extension<Service>>,
    Query(cursor): Query<Cursor>,
) -> Result<Json<AdminAuditions>, AppError> {
    auth.require_operator().await?;
    let after = cursor.after_id.unwrap_or_default();
    let clone = cursor.clone_job_id;
    if (!after.is_empty() && !hex(&after, 32)) || clone.as_ref().is_some_and(|s| !hex(s, 32)) {
        return Err(AppError::InvalidInput);
    }
    if cursor.character_id.is_some() != cursor.character_revision.is_some()
        || cursor
            .character_id
            .as_deref()
            .is_some_and(|id| !brioche_course_contract::valid_content_id(id))
        || cursor
            .character_revision
            .is_some_and(|v| !brioche_course_contract::valid_content_revision(v))
    {
        return Err(AppError::InvalidInput);
    }
    let rows=b.db.query_all_raw(Statement::from_sql_and_values(DbBackend::Postgres,format!("{} WHERE a.id>$1 AND ($2::text IS NULL OR a.clone_job_id=$2) AND ($3::text IS NULL OR COALESCE(g.character_id,a.character_id)=$3) AND ($4::integer IS NULL OR COALESCE(g.character_revision,a.character_revision)=$4) {} ORDER BY a.id LIMIT 21",projection(b.product),product_filter(b.product,"a.product_id")),vec![after.into(),clone.into(),cursor.character_id.into(),cursor.character_revision.map(|v|v as i32).into()])).await.map_err(|_|AppError::Unavailable)?;
    let items = rows
        .iter()
        .take(20)
        .map(item)
        .collect::<Result<Vec<_>, _>>()?;
    let next = if rows.len() > 20 {
        items.last().map(|i| i.id.clone())
    } else {
        None
    };
    Ok(Json(AdminAuditions {
        items,
        next,
        configured: service.is_some(),
    }))
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemQuery {}
async fn read(
    auth: AdminAuth,
    State(b): State<Store>,
    Path(id): Path<String>,
    Query(_query): Query<ItemQuery>,
) -> Result<Json<AdminAudition>, AppError> {
    auth.require_operator().await?;
    Ok(Json(item(&load(&b.db, b.product, &id).await?)?))
}

/// Trusted local operator entry: shares persistence, current-role checks and exact retry with HTTP.
/// Keeps the runtime alive for the dispatched job; never records a listening approval.
pub async fn submit_local(
    b: Backend,
    actor: i64,
    service: Option<Service>,
    root: PathBuf,
    request: AdminAuditionRequest,
) -> Result<AdminAudition, AppError> {
    let operator = crate::product_memberships::require_operator(
        &b.db,
        crate::product::ProductId::Brioche,
        actor,
    )
    .await?;
    submit_authorized(
        Store {
            db: b.db.clone(),
            product: None,
        },
        &operator,
        service,
        root,
        request,
    )
    .await
}

pub(crate) async fn submit_author(
    db: &sea_orm::DatabaseConnection,
    product: crate::product::ProductId,
    operator: &crate::product_memberships::Operator,
    service: Option<Service>,
    root: PathBuf,
    request: AdminAuditionRequest,
) -> Result<AdminAudition, AppError> {
    submit_authorized(
        Store {
            db: db.clone(),
            product: Some(product),
        },
        operator,
        service,
        root,
        request,
    )
    .await
}

async fn submit_authorized(
    b: Store,
    operator: &crate::product_memberships::Operator,
    service: Option<Service>,
    root: PathBuf,
    mut request: AdminAuditionRequest,
) -> Result<AdminAudition, AppError> {
    request.reason = format!("[local-cli] {}", request.reason);
    let id = request.id.clone();
    let Json(mut result) = create_for_actor(b.clone(), operator, service, root, request).await?;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(245);
    while result.status == "submitted" && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        result = item(&load(&b.db, b.product, &id).await?)?;
    }
    Ok(result)
}

async fn create(
    auth: AdminAuth,
    State(b): State<Store>,
    service: Option<Extension<Service>>,
    Extension(root): Extension<PathBuf>,
    Json(request): Json<AdminAuditionRequest>,
) -> Result<Json<AdminAudition>, AppError> {
    let operator = auth.require_operator().await?;
    create_for_actor(b, &operator, service.map(|s| s.0), root, request).await
}
async fn create_for_actor(
    b: Store,
    operator: &crate::product_memberships::Operator,
    service: Option<Service>,
    root: PathBuf,
    request: AdminAuditionRequest,
) -> Result<Json<AdminAudition>, AppError> {
    if b.product.is_some_and(|product| product != operator.product) {
        return Err(AppError::Forbidden);
    }
    let actor = operator.actor;
    crate::admin::reason(&request.reason)?;
    if !request.cost_confirmed || !hex(&request.id, 32) {
        return Err(AppError::InvalidInput);
    }
    match (
        &request.clone_job_id,
        request.expected_clone_version,
        &request.candidate,
    ) {
        (Some(id), Some(v), None)
            if hex(id, 32) && brioche_course_contract::valid_content_revision(v) => {}
        (None, None, Some(c))
            if brioche_course_contract::valid_content_id(&c.character_id)
                && brioche_course_contract::valid_content_revision(c.character_revision)
                && c.expected_voice_revision < i32::MAX as u32
                && c.profile.voice_kind == "system"
                && c.profile.reference_audio.is_none() =>
        {
            crate::qwen::SpeechRequest {
                profile: c.profile.clone(),
                text: request.text.clone(),
                emotion: request.emotion.clone(),
            }
            .parameters()
            .map_err(|_| AppError::InvalidInput)?;
        }
        _ => return Err(AppError::InvalidInput),
    }
    let candidate_json =
        serde_json::to_value(&request.candidate).map_err(|_| AppError::InvalidInput)?;
    let tx = b.db.begin().await.map_err(|_| AppError::Unavailable)?;
    operator.lock_content(&tx).await?;
    // Serialize the id before any external call. Exact retries return the original attempt without resending.
    exec(
        &tx,
        "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
        vec![format!("voice-audition:{}", request.id).into()],
    )
    .await?;
    // Only a verified product primary key permits local IDs; old layouts retain the collision guard.
    let local_ids = b.product.is_some() && local_audition_keys(&tx).await?;
    if let Some(product) = b.product.filter(|_| !local_ids)
        && one(
            &tx,
            "SELECT 1 FROM voice_auditions WHERE id=$1 AND product_id<>$2",
            vec![request.id.clone().into(), product.as_str().into()],
        )
        .await?
        .is_some()
    {
        return Err(AppError::NotFound);
    }
    if let Some(row)=one(&tx,&format!("SELECT actor_id,reason,clone_job_id,clone_version,parameters FROM voice_auditions WHERE id=$1{}",product_filter(b.product,"product_id")),vec![request.id.clone().into()]).await?{
        let p:Value=field(&row,"parameters")?;
        if field::<i64>(&row,"actor_id")?!=actor||field::<String>(&row,"reason")?!=request.reason||field::<Option<String>>(&row,"clone_job_id")?!=request.clone_job_id||field::<Option<i32>>(&row,"clone_version")?.map(|v|v as u32)!=request.expected_clone_version||p["candidate"]!=candidate_json||p["input"]["text"]!=request.text||p["sceneEmotion"]!=request.emotion{return Err(AppError::Conflict);}
        return Ok(Json(item(&load(&tx,b.product,&request.id).await?)?));
    }
    let service = service.ok_or(AppError::Unavailable)?;
    let permit = service
        .permits
        .clone()
        .try_acquire_owned()
        .map_err(|_| AppError::RateLimited)?;
    let (profile, character_id, character_revision, base_voice_revision) = if let Some(c) =
        &request.candidate
    {
        let row=one(&tx,&format!("SELECT COALESCE(MAX(v.revision),0)::integer AS voice_revision FROM character_revisions c LEFT JOIN character_voice_profiles v ON v.character_id=c.character_id AND v.character_revision=c.revision{} WHERE c.character_id=$1 AND c.revision=$2{} GROUP BY c.character_id,c.revision",if b.product.is_some(){" AND v.product_id=c.product_id"}else{""},product_filter(b.product,"c.product_id")),vec![c.character_id.clone().into(),(c.character_revision as i32).into()]).await?.ok_or(AppError::NotFound)?;
        if field::<i32>(&row, "voice_revision")? as u32 != c.expected_voice_revision {
            return Err(AppError::Conflict);
        }
        (
            c.profile.clone(),
            Some(c.character_id.clone()),
            Some(c.character_revision as i32),
            Some(c.expected_voice_revision as i32),
        )
    } else {
        let job_id = request
            .clone_job_id
            .as_deref()
            .ok_or(AppError::InvalidInput)?;
        exec(
            &tx,
            &format!(
                "SELECT id FROM voice_clone_jobs WHERE id=$1{} FOR UPDATE",
                product_filter(b.product, "product_id")
            ),
            vec![job_id.into()],
        )
        .await?;
        let source = crate::voice_jobs::load_for_product(&tx, b.product, job_id).await?;
        if Some(source.version) != request.expected_clone_version
            || !matches!(source.status, VoiceJobStatus::Ready)
        {
            return Err(AppError::Conflict);
        }
        let row=one(&tx,&format!("SELECT profile FROM character_voice_profiles WHERE character_id=$1 AND character_revision=$2 AND revision=$3{}",product_filter(b.product,"product_id")),vec![source.character_id.into(),(source.character_revision as i32).into(),(source.voice_revision as i32).into()]).await?.ok_or(AppError::NotFound)?;
        let mut profile: CharacterVoiceProfile =
            serde_json::from_value(field(&row, "profile")?).map_err(|_| AppError::Unavailable)?;
        profile.voice_id = source.voice_id.ok_or(AppError::Conflict)?;
        profile.voice_kind = "cloned".into();
        profile.model = crate::qwen::MODEL.into();
        profile.provider = "qwen".into();
        (profile, None, None, None)
    };
    let speech = crate::qwen::SpeechRequest {
        profile,
        text: request.text,
        emotion: request.emotion,
    };
    let mut parameters = speech.parameters().map_err(|_| AppError::InvalidInput)?;
    parameters["sceneEmotion"] = json!(speech.emotion); // Private reproducibility metadata, not sent to Qwen.
    parameters["candidate"] = candidate_json;
    let mut values = vec![
        request.id.clone().into(),
        request.clone_job_id.into(),
        request.expected_clone_version.map(|v| v as i32).into(),
        serde_json::to_value(&speech.profile)
            .map_err(|_| AppError::InvalidInput)?
            .into(),
        parameters.into(),
        actor.into(),
        request.reason.into(),
        character_id.into(),
        character_revision.into(),
        base_voice_revision.into(),
    ];
    let sql = if let Some(product) = b.product {
        values.push(product.as_str().into());
        "INSERT INTO voice_auditions(id,clone_job_id,clone_version,profile,parameters,actor_id,reason,character_id,character_revision,base_voice_revision,product_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)"
    } else {
        "INSERT INTO voice_auditions(id,clone_job_id,clone_version,profile,parameters,actor_id,reason,character_id,character_revision,base_voice_revision) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"
    };
    exec(&tx, sql, values).await?;
    let mut values = vec![request.id.clone().into()];
    let sql = if let Some(product) = b.product {
        values.push(product.as_str().into());
        "INSERT INTO voice_audition_events(audition_id,version,status,product_id) VALUES($1,1,'submitted',$2)"
    } else {
        "INSERT INTO voice_audition_events(audition_id,version,status) VALUES($1,1,'submitted')"
    };
    exec(&tx, sql, values).await?;
    let result = item(&load(&tx, b.product, &request.id).await?)?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    let cloned = speech.profile.voice_kind == "cloned";
    tokio::spawn(async move {
        let _permit = permit;
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(240),
            service.transport.synthesize(&speech),
        )
        .await
        .unwrap_or(Err(ProviderError::Unknown));
        let (status, result) = match response {
            Ok(s) => match tokio::task::spawn_blocking(move || {
                crate::speech_media::store(&root, s, cloned)
            })
            .await
            {
                Ok(Ok(r)) => ("ready", Some(r)),
                _ => ("unknown", None),
            },
            Err(ProviderError::Rejected) => ("failed", None),
            Err(ProviderError::Unknown) => ("unknown", None),
        };
        if finish(&b.db, b.product, &request.id, status, result)
            .await
            .is_err()
        {
            tracing::warn!("voice audition result persistence unavailable");
        }
    });
    Ok(Json(result))
}
async fn finish(
    db: &impl ConnectionTrait,
    product: Option<crate::product::ProductId>,
    id: &str,
    status: &str,
    result: Option<Value>,
) -> Result<(), AppError> {
    let mut values = vec![id.into(), status.into(), result.into()];
    let sql = if let Some(product) = product {
        values.push(product.as_str().into());
        "INSERT INTO voice_audition_events(audition_id,version,status,result,product_id) VALUES($1,2,$2,$3,$4)"
    } else {
        "INSERT INTO voice_audition_events(audition_id,version,status,result) VALUES($1,2,$2,$3)"
    };
    exec(db, sql, values).await.map(|_| ())
}
async fn file(
    auth: AdminAuth,
    State(b): State<Store>,
    Path(id): Path<String>,
    Extension(root): Extension<PathBuf>,
    Extension(permits): Extension<Arc<tokio::sync::Semaphore>>,
    headers: HeaderMap,
    Query(_query): Query<ItemQuery>,
) -> Result<axum::response::Response, AppError> {
    auth.require_operator().await?;
    let row = load(&b.db, b.product, &id).await?;
    if item(&row)?.status != "ready" {
        return Err(AppError::NotFound);
    }
    let result: Value = field::<Option<Value>>(&row, "result")?.ok_or(AppError::NotFound)?;
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
    auth: AdminAuth,
    State(b): State<Store>,
    Path(id): Path<String>,
    Json(request): Json<AdminAuditionReview>,
) -> Result<Json<AdminAudition>, AppError> {
    let operator = auth.require_operator().await?;
    Ok(Json(review_for_actor(&b, &operator, id, request).await?))
}

/// Record an explicit human decision through the same transaction as the HTTP route.
pub async fn review_local(
    b: &Backend,
    actor: i64,
    id: String,
    mut request: AdminAuditionReview,
) -> Result<AdminAudition, AppError> {
    request.reason = format!("[local-cli] {}", request.reason);
    let operator = crate::product_memberships::require_operator(
        &b.db,
        crate::product::ProductId::Brioche,
        actor,
    )
    .await?;
    review_for_actor(
        &Store {
            db: b.db.clone(),
            product: None,
        },
        &operator,
        id,
        request,
    )
    .await
}

async fn review_for_actor(
    b: &Store,
    operator: &crate::product_memberships::Operator,
    id: String,
    request: AdminAuditionReview,
) -> Result<AdminAudition, AppError> {
    if b.product.is_some_and(|product| product != operator.product) {
        return Err(AppError::Forbidden);
    }
    let actor = operator.actor;
    crate::admin::reason(&request.reason)?;
    if !hex(&id, 32) || !request.heard {
        return Err(AppError::InvalidInput);
    }
    let tx = b.db.begin().await.map_err(|_| AppError::Unavailable)?;
    operator.lock_content(&tx).await?;
    exec(
        &tx,
        &format!(
            "SELECT id FROM voice_auditions WHERE id=$1{} FOR UPDATE",
            product_filter(b.product, "product_id")
        ),
        vec![id.clone().into()],
    )
    .await?;
    let row = load(&tx, b.product, &id).await?;
    let audition = item(&row)?;
    if audition.status != "ready"
        || audition.accepted.is_some()
        || request.expected_voice_revision != audition.base_voice_revision
    {
        return Err(AppError::Conflict);
    }
    let voice = if request.accepted {
        let profile =
            serde_json::from_value(field(&row, "profile")?).map_err(|_| AppError::Unavailable)?;
        Some(
            crate::character_voices::append_profile_authorized_for_product(
                &tx,
                b.product,
                operator,
                AdminCharacterVoiceRequest {
                    character_id: audition.character_id.clone(),
                    character_revision: audition.character_revision,
                    expected_voice_revision: request.expected_voice_revision,
                    profile,
                    reason: request.reason.clone(),
                },
            )
            .await?
            .voice_revision as i32,
        )
    } else {
        None
    };
    let mut values = vec![
        id.clone().into(),
        request.accepted.into(),
        audition.character_id.into(),
        (audition.character_revision as i32).into(),
        voice.into(),
        actor.into(),
        request.reason.into(),
    ];
    let sql = if let Some(product) = b.product {
        values.push(product.as_str().into());
        "INSERT INTO voice_audition_reviews(audition_id,accepted,character_id,character_revision,voice_revision,actor_id,reason,product_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8)"
    } else {
        "INSERT INTO voice_audition_reviews(audition_id,accepted,character_id,character_revision,voice_revision,actor_id,reason) VALUES($1,$2,$3,$4,$5,$6,$7)"
    };
    exec(&tx, sql, values).await?;
    let result = item(&load(&tx, b.product, &id).await?)?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(result)
}
