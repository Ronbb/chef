//! Durable enrollment attempts. A lost provider reply is never automatically retried.
use crate::{
    AppError,
    admin_auth::AdminAuth,
    learning::{exec, field, one, product_filter},
    qwen::{self, ProviderError, Service},
    voice_references::{hex, inspect},
};
use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use brioche_course_contract::{
    AdminVoiceJob, AdminVoiceJobCheck, AdminVoiceJobRequest, AdminVoiceJobs,
};
use sea_orm::{ConnectionTrait, DbBackend, QueryResult, Statement, TransactionTrait};
use std::{path::PathBuf, sync::Arc};
fn projection(product: Option<crate::product::ProductId>) -> String {
    format!(
        r#"SELECT j.id,j.grant_id,j.prefix,g.character_id,g.character_revision,g.voice_revision,g.model,e.version,
CASE WHEN e.status IN ('submitted','checking') AND e.created_at<clock_timestamp()-interval '60 seconds' THEN 'unknown' ELSE e.status END AS status,
e.voice_id,e.request_id,to_char(j.created_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"') AS created_at,
to_char(e.created_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"') AS updated_at
FROM voice_clone_jobs j JOIN voice_reference_grants g ON g.id=j.grant_id{}
JOIN LATERAL (SELECT * FROM voice_clone_events WHERE job_id=j.id{} ORDER BY version DESC LIMIT 1) e ON true"#,
        if product.is_some() {
            " AND g.product_id=j.product_id"
        } else {
            ""
        },
        if product.is_some() {
            " AND product_id=j.product_id"
        } else {
            ""
        }
    )
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
        .route("/api/v1/operator/voice-jobs", get(list).post(create))
        .route("/api/v1/operator/voice-jobs/{id}", get(read))
        .route("/api/v1/operator/voice-jobs/{id}/check", post(check))
        .with_state(Store { db, product })
}
fn item(row: &QueryResult) -> Result<AdminVoiceJob, AppError> {
    Ok(AdminVoiceJob {
        id: field(row, "id")?,
        grant_id: field(row, "grant_id")?,
        character_id: field(row, "character_id")?,
        character_revision: field::<i32>(row, "character_revision")? as u32,
        voice_revision: field::<i32>(row, "voice_revision")? as u32,
        model: field(row, "model")?,
        prefix: field(row, "prefix")?,
        version: field::<i32>(row, "version")? as u32,
        status: serde_json::from_value(serde_json::Value::String(field(row, "status")?))
            .map_err(|_| AppError::Unavailable)?,
        voice_id: field(row, "voice_id")?,
        request_id: field(row, "request_id")?,
        created_at: field(row, "created_at")?,
        updated_at: field(row, "updated_at")?,
    })
}
pub(crate) async fn load_for_product(
    db: &impl ConnectionTrait,
    product: Option<crate::product::ProductId>,
    id: &str,
) -> Result<AdminVoiceJob, AppError> {
    if !hex(id, 32) {
        return Err(AppError::InvalidInput);
    }
    item(
        &one(
            db,
            &format!(
                "{} WHERE j.id=$1{}",
                projection(product),
                product_filter(product, "j.product_id")
            ),
            vec![id.into()],
        )
        .await?
        .ok_or(AppError::NotFound)?,
    )
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct JobQuery {}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Cursor {
    after_id: Option<String>,
}
async fn list(
    auth: AdminAuth,
    State(backend): State<Store>,
    service: Option<Extension<Service>>,
    Query(cursor): Query<Cursor>,
) -> Result<Json<AdminVoiceJobs>, AppError> {
    auth.require_operator().await?;
    let after = cursor.after_id.unwrap_or_default();
    if !after.is_empty() && !hex(&after, 32) {
        return Err(AppError::InvalidInput);
    }
    let rows = backend
        .db
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            format!(
                "{} WHERE j.id>$1{} ORDER BY j.id LIMIT 21",
                projection(backend.product),
                product_filter(backend.product, "j.product_id")
            ),
            vec![after.into()],
        ))
        .await
        .map_err(|_| AppError::Unavailable)?;
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
    Ok(Json(AdminVoiceJobs {
        items,
        next,
        configured: service.is_some(),
    }))
}
async fn read(
    auth: AdminAuth,
    State(backend): State<Store>,
    Path(id): Path<String>,
    Query(_query): Query<JobQuery>,
) -> Result<Json<AdminVoiceJob>, AppError> {
    auth.require_operator().await?;
    Ok(Json(
        load_for_product(&backend.db, backend.product, &id).await?,
    ))
}
async fn event(
    (db, product): (&impl ConnectionTrait, Option<crate::product::ProductId>),
    id: &str,
    status: &str,
    voice: Option<String>,
    request: Option<String>,
    actor: Option<i64>,
    reason: &str,
) -> Result<(), AppError> {
    let mut values = vec![
        id.into(),
        status.into(),
        voice.into(),
        request.into(),
        actor.into(),
        reason.into(),
    ];
    let sql = if let Some(product) = product {
        values.push(product.as_str().into());
        "INSERT INTO voice_clone_events(job_id,version,status,voice_id,request_id,actor_id,reason,product_id) SELECT $1,COALESCE(max(version),0)+1,$2,$3,$4,$5,$6,$7 FROM voice_clone_events WHERE job_id=$1 AND product_id=$7"
    } else {
        "INSERT INTO voice_clone_events(job_id,version,status,voice_id,request_id,actor_id,reason) SELECT $1,COALESCE(max(version),0)+1,$2,$3,$4,$5,$6 FROM voice_clone_events WHERE job_id=$1"
    };
    exec(db, sql, values).await.map(|_| ())
}
async fn create(
    auth: AdminAuth,
    State(backend): State<Store>,
    service: Option<Extension<Service>>,
    Extension(root): Extension<PathBuf>,
    Extension(media_permits): Extension<Arc<tokio::sync::Semaphore>>,
    Json(request): Json<AdminVoiceJobRequest>,
) -> Result<Json<AdminVoiceJob>, AppError> {
    let operator = auth.require_operator().await?;
    if backend
        .product
        .is_some_and(|product| product != operator.product)
    {
        return Err(AppError::Forbidden);
    }
    crate::admin::reason(&request.reason)?;
    if !request.cost_confirmed || !hex(&request.grant_id, 32) || !hex(&request.token, 64) {
        return Err(AppError::InvalidInput);
    }
    let service = service.ok_or(AppError::Unavailable)?.0;
    let permit = service
        .permits
        .clone()
        .try_acquire_owned()
        .map_err(|_| AppError::RateLimited)?;
    let actor = operator.actor;
    let tx = backend
        .db
        .begin()
        .await
        .map_err(|_| AppError::Unavailable)?;
    operator.lock_content(&tx).await?;
    let row=one(&tx,&format!("SELECT g.descriptor,g.actor_id FROM voice_reference_grants g WHERE g.id=$1 AND g.token_hash=$2 AND g.expires_at>clock_timestamp()+interval '60 seconds'{} AND NOT EXISTS(SELECT 1 FROM voice_reference_revocations r WHERE r.grant_id=g.id{}) AND (SELECT count(*) FROM voice_reference_reads r WHERE r.grant_id=g.id{})<32",product_filter(backend.product,"g.product_id"),if backend.product.is_some(){" AND r.product_id=g.product_id"}else{""},if backend.product.is_some(){" AND r.product_id=g.product_id"}else{""}),vec![request.grant_id.clone().into(),crate::media::digest(request.token.as_bytes()).into()]).await?.ok_or(AppError::NotFound)?;
    let grant_actor: i64 = field(&row, "actor_id")?;
    let grant_authorized = if let Some(remote) = &operator.remote {
        remote.is_operator(grant_actor).await?
    } else {
        crate::product_memberships::read(&tx, operator.product, grant_actor)
            .await?
            .role
            == "operator"
    };
    if !grant_authorized {
        return Err(AppError::NotFound);
    }
    if one(
        &tx,
        &format!(
            "SELECT id FROM voice_clone_jobs WHERE grant_id=$1{}",
            product_filter(backend.product, "product_id")
        ),
        vec![request.grant_id.clone().into()],
    )
    .await?
    .is_some()
    {
        return Err(AppError::Conflict);
    }
    let descriptor =
        serde_json::from_value(field(&row, "descriptor")?).map_err(|_| AppError::Unavailable)?;
    inspect(root, descriptor, media_permits).await?;
    let id = crate::learning::random_id()?;
    let prefix = format!("b{}", &id[..9]);
    let mut values = vec![
        id.clone().into(),
        request.grant_id.clone().into(),
        prefix.clone().into(),
        actor.into(),
        request.reason.clone().into(),
    ];
    let sql = if let Some(product) = backend.product {
        values.push(product.as_str().into());
        "INSERT INTO voice_clone_jobs(id,grant_id,prefix,actor_id,reason,product_id) VALUES($1,$2,$3,$4,$5,$6)"
    } else {
        "INSERT INTO voice_clone_jobs(id,grant_id,prefix,actor_id,reason) VALUES($1,$2,$3,$4,$5)"
    };
    exec(&tx, sql, values).await?;
    event(
        (&tx, backend.product),
        &id,
        "submitted",
        None,
        None,
        Some(actor),
        &request.reason,
    )
    .await?;
    let result = load_for_product(&tx, backend.product, &id).await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    // Only memory holds the capability. Durable state was committed BEFORE any billable call.
    // Losing this worker leaves a recoverable unknown attempt, never an automatic resend.
    let url = format!(
        "{}/api/v1/voice-references/{}/{}",
        service.origin, request.grant_id, request.token
    );
    tokio::spawn(async move {
        let _permit = permit;
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(35),
            service.transport.create(&prefix, &url),
        )
        .await
        .unwrap_or(Err(ProviderError::Unknown));
        let (status, voice, request_id) = match response {
            Ok(receipt)
                if qwen::valid_id(&receipt.voice_id)
                    && receipt
                        .voice_id
                        .starts_with(&format!("{}-{prefix}-", qwen::MODEL))
                    && qwen::valid_id(&receipt.request_id) =>
            {
                (
                    "processing",
                    Some(receipt.voice_id),
                    Some(receipt.request_id),
                )
            }
            Err(ProviderError::Rejected) => ("failed", None, None),
            _ => ("unknown", None, None),
        };
        // A version CAS prevents late worker results overwriting an explicit recovery query.
        if let Err(_error) = finish(
            &backend.db,
            backend.product,
            &id,
            1,
            status,
            voice,
            request_id,
        )
        .await
        {
            tracing::warn!("voice enrollment result persistence unavailable");
        }
    });
    Ok(Json(result))
}
async fn finish(
    db: &sea_orm::DatabaseConnection,
    product: Option<crate::product::ProductId>,
    id: &str,
    expected: u32,
    status: &str,
    voice: Option<String>,
    request: Option<String>,
) -> Result<(), AppError> {
    let tx = db.begin().await.map_err(|_| AppError::Unavailable)?;
    exec(
        &tx,
        &format!(
            "SELECT id FROM voice_clone_jobs WHERE id=$1{} FOR UPDATE",
            product_filter(product, "product_id")
        ),
        vec![id.into()],
    )
    .await?;
    if load_for_product(&tx, product, id).await?.version != expected {
        return Err(AppError::Conflict);
    }
    event(
        (&tx, product),
        id,
        status,
        voice,
        request,
        None,
        "provider response",
    )
    .await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)
}
async fn check(
    auth: AdminAuth,
    State(backend): State<Store>,
    service: Option<Extension<Service>>,
    Path(id): Path<String>,
    Json(request): Json<AdminVoiceJobCheck>,
) -> Result<Json<AdminVoiceJob>, AppError> {
    let operator = auth.require_operator().await?;
    if backend
        .product
        .is_some_and(|product| product != operator.product)
    {
        return Err(AppError::Forbidden);
    }
    crate::admin::reason(&request.reason)?;
    let service = service.ok_or(AppError::Unavailable)?.0;
    let permit = service
        .permits
        .clone()
        .try_acquire_owned()
        .map_err(|_| AppError::RateLimited)?;
    let actor = operator.actor;
    let tx = backend
        .db
        .begin()
        .await
        .map_err(|_| AppError::Unavailable)?;
    operator.lock_content(&tx).await?;
    if !hex(&id, 32) {
        return Err(AppError::InvalidInput);
    }
    exec(
        &tx,
        &format!(
            "SELECT id FROM voice_clone_jobs WHERE id=$1{} FOR UPDATE",
            product_filter(backend.product, "product_id")
        ),
        vec![id.clone().into()],
    )
    .await?;
    let current = load_for_product(&tx, backend.product, &id).await?;
    if current.version != request.expected_version
        || matches!(
            current.status,
            brioche_course_contract::VoiceJobStatus::Checking
                | brioche_course_contract::VoiceJobStatus::Submitted
        )
    {
        return Err(AppError::Conflict);
    }
    let voice = current
        .voice_id
        .or(request.voice_id)
        .filter(|s| {
            qwen::valid_id(s) && s.starts_with(&format!("{}-{}-", current.model, current.prefix))
        })
        .ok_or(AppError::InvalidInput)?;
    event(
        (&tx, backend.product),
        &id,
        "checking",
        Some(voice.clone()),
        None,
        Some(actor),
        &request.reason,
    )
    .await?;
    let result = load_for_product(&tx, backend.product, &id).await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    tokio::spawn(async move {
        let _permit = permit;
        let details = tokio::time::timeout(
            std::time::Duration::from_secs(35),
            service.transport.query(&voice),
        )
        .await;
        let (status, request_id) = match details {
            Ok(Ok(detail)) if qwen::valid_id(&detail.request_id) => {
                let status = if detail.model != qwen::MODEL {
                    "modelMismatch"
                } else {
                    match detail.status.as_str() {
                        "OK" => "ready",
                        "DEPLOYING" => "processing",
                        "UNDEPLOYED" => "unavailable",
                        _ => "checkFailed",
                    }
                };
                (status, Some(detail.request_id))
            }
            _ => ("checkFailed", None),
        };
        if let Err(_error) = finish(
            &backend.db,
            backend.product,
            &id,
            current.version + 1,
            status,
            Some(voice),
            request_id,
        )
        .await
        {
            tracing::warn!("voice verification result persistence unavailable");
        }
    });
    Ok(Json(result))
}
