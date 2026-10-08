//! Separate owner publication authorization and optional human listening declarations.
use crate::{
    AppError,
    admin_auth::AdminAuth,
    identity::Backend,
    learning::{exec, field, hash, one, product_filter},
};
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use brioche_course_contract::{AdminLessonAudioReview, AdminLessonAudioStatus};
use sea_orm::{ConnectionTrait, TransactionTrait};
use serde_json::Value;
use std::{path::PathBuf, sync::Arc};

#[derive(Clone)]
struct Store {
    db: sea_orm::DatabaseConnection,
    product: Option<crate::product::ProductId>,
}
pub(crate) fn router<S: Clone + Send + Sync + 'static>(
    db: sea_orm::DatabaseConnection,
    product: Option<crate::product::ProductId>,
) -> Router<S> {
    Router::new()
        .route(
            "/api/v1/operator/lessons/{id}/revisions/{revision}/audio-review",
            get(read).post(review),
        )
        .route(
            "/api/v1/operator/lessons/{id}/revisions/{revision}/direct-publication",
            post(authorize),
        )
        .with_state(Store { db, product })
}
pub(crate) fn required(source: &Value) -> bool {
    source["audio"].as_array().is_some_and(|a| !a.is_empty())
}
pub(crate) async fn accepted(
    db: &impl ConnectionTrait,
    product: Option<crate::product::ProductId>,
    id: &str,
    revision: u32,
    source: &Value,
) -> Result<bool, AppError> {
    if !required(source) {
        return Ok(true);
    }
    let status = status(db, product, id, revision, source).await?;
    Ok(status.accepted)
}
async fn status(
    db: &impl ConnectionTrait,
    product: Option<crate::product::ProductId>,
    id: &str,
    revision: u32,
    source: &Value,
) -> Result<AdminLessonAudioStatus, AppError> {
    let lesson_hash = hash(source).map_err(|_| AppError::Unavailable)?;
    let row = one(db,&format!("SELECT version,lesson_hash,accepted,reason,actor_id FROM lesson_audio_reviews WHERE lesson_id=$1 AND revision=$2{} ORDER BY version DESC LIMIT 1",product_filter(product,"product_id")),vec![id.into(),(revision as i32).into()]).await?;
    let direct = one(db,&format!("SELECT actor_id,reason FROM lesson_direct_publications d WHERE lesson_id=$1 AND revision=$2 AND lesson_hash=$3{} AND NOT EXISTS(SELECT 1 FROM lesson_audio_reviews r WHERE (r.lesson_id,r.revision)=(d.lesson_id,d.revision) AND r.version>d.review_version{})",product_filter(product,"d.product_id"),product_filter(product,"r.product_id")),vec![id.into(),(revision as i32).into(),lesson_hash.clone().into()]).await?;
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
pub use brioche_course_contract::AdminDirectPublication as DirectPublication;
pub async fn authorize_local(
    b: &Backend,
    actor: i64,
    id: &str,
    revision: u32,
    root: &std::path::Path,
    request: DirectPublication,
) -> Result<AdminLessonAudioStatus, AppError> {
    let operator = crate::product_memberships::require_operator(
        &b.db,
        crate::product::ProductId::Brioche,
        actor,
    )
    .await?;
    authorize_for_operator(
        &Store {
            db: b.db.clone(),
            product: None,
        },
        &operator,
        id,
        revision,
        root,
        request,
    )
    .await
}
pub(crate) async fn authorize_author(
    db: &sea_orm::DatabaseConnection,
    product: crate::product::ProductId,
    operator: &crate::product_memberships::Operator,
    id: &str,
    revision: u32,
    root: &std::path::Path,
    request: DirectPublication,
) -> Result<AdminLessonAudioStatus, AppError> {
    authorize_for_operator(
        &Store {
            db: db.clone(),
            product: Some(product),
        },
        operator,
        id,
        revision,
        root,
        request,
    )
    .await
}

async fn authorize(
    auth: AdminAuth,
    State(b): State<Store>,
    Path((id, revision)): Path<(String, u32)>,
    Extension(root): Extension<PathBuf>,
    Extension(permits): Extension<Arc<tokio::sync::Semaphore>>,
    Json(request): Json<DirectPublication>,
) -> Result<Json<AdminLessonAudioStatus>, AppError> {
    let operator = auth.require_operator().await?;
    let _permit = permits
        .try_acquire_many_owned(2)
        .map_err(|_| AppError::RateLimited)?;
    Ok(Json(
        authorize_for_operator(&b, &operator, &id, revision, &root, request).await?,
    ))
}
async fn authorize_for_operator(
    b: &Store,
    operator: &crate::product_memberships::Operator,
    id: &str,
    revision: u32,
    root: &std::path::Path,
    request: DirectPublication,
) -> Result<AdminLessonAudioStatus, AppError> {
    if b.product.is_some_and(|product| product != operator.product) {
        return Err(AppError::Forbidden);
    }
    let actor = operator.actor;
    let tx = b.db.begin().await.map_err(|_| AppError::Unavailable)?;
    operator.lock_content(&tx).await?;
    authorize_in(b, tx, actor, id, revision, root, request).await
}

/// Explicit table-owner maintenance, separate from session-authorized runtime operations.
/// Never available to an HTTP request or the restricted content login.
pub(crate) async fn authorize_owner(
    db: &sea_orm::DatabaseConnection,
    product: crate::product::ProductId,
    email: &str,
    id: &str,
    revision: u32,
    root: &std::path::Path,
    mut request: DirectPublication,
) -> Result<AdminLessonAudioStatus, AppError> {
    let tx = db.begin().await.map_err(|_| AppError::Unavailable)?;
    exec(
        &tx,
        "SELECT pg_advisory_xact_lock(hashtextextended('account-admin',0))",
        vec![],
    )
    .await?;
    let layout = one(
        &tx,
        "SELECT learning_schema,identity_schema FROM chef_schema_layout WHERE singleton",
        vec![],
    )
    .await?
    .ok_or(AppError::Forbidden)?;
    let learning: String = field(&layout, "learning_schema")?;
    let identity: String = field(&layout, "identity_schema")?;
    crate::database_scope::validate(&learning).map_err(|_| AppError::Forbidden)?;
    crate::database_scope::validate(&identity).map_err(|_| AppError::Forbidden)?;
    brioche_migration::layout::verify_complete(&tx, &learning, &identity)
        .await
        .map_err(|_| AppError::Forbidden)?;
    let owner = one(&tx, "SELECT count(*)::bigint AS n FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE ((n.nspname=$1 AND c.relname IN ('lesson_revisions','lesson_direct_publications')) OR (n.nspname=$2 AND c.relname IN ('users','product_memberships'))) AND c.relkind='r' AND c.relowner=(SELECT oid FROM pg_catalog.pg_roles WHERE rolname=current_user)", vec![learning.into(),identity.clone().into()]).await?.ok_or(AppError::Forbidden)?;
    if field::<i64>(&owner, "n")? != 4 {
        return Err(AppError::Forbidden);
    }
    let email = crate::identity::normalize_email(email)?;
    let actor = one(&tx, &format!("SELECT u.id FROM \"{identity}\".users u JOIN \"{identity}\".product_memberships m ON m.user_id=u.id WHERE u.email=$1 AND m.product_id=$2 AND m.role='operator'"), vec![email.into(),product.as_str().into()]).await?.ok_or(AppError::Forbidden)?;
    request
        .evidence
        .as_object_mut()
        .ok_or(AppError::InvalidInput)?
        .insert(
            "maintenanceAuthority".into(),
            serde_json::json!("database-table-owner; no browser session asserted"),
        );
    authorize_in(
        &Store {
            db: db.clone(),
            product: Some(product),
        },
        tx,
        field(&actor, "id")?,
        id,
        revision,
        root,
        request,
    )
    .await
}

async fn authorize_in(
    b: &Store,
    tx: sea_orm::DatabaseTransaction,
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
    one(
        &tx,
        &format!(
            "SELECT generation FROM content_state WHERE {} FOR UPDATE",
            b.product.map_or_else(
                || "singleton".to_owned(),
                |p| format!("product_id='{}'", p.as_str())
            )
        ),
        vec![],
    )
    .await?
    .ok_or(AppError::Unavailable)?;
    let (document, published) = source(&tx, b.product, id, revision, true).await?;
    if !required(&document) || hash(&document)? != request.expected_lesson_hash {
        return Err(AppError::Conflict);
    }
    let existing = one(&tx,&format!("SELECT actor_id,request FROM lesson_direct_publications WHERE lesson_id=$1 AND revision=$2{}",product_filter(b.product,"product_id")),vec![id.into(),(revision as i32).into()]).await?;
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
        let lesson =
            crate::author_source::check_any_source(&document).map_err(|_| AppError::Unavailable)?;
        lesson
            .validate_public()
            .map_err(|_| AppError::InvalidInput)?;
        crate::recording::validate_checked_lesson_detailed(&tx, b.product, &lesson, root)
            .await
            .map_err(|error| error.runtime)?;
        let mut values = vec![
            id.into(),
            (revision as i32).into(),
            request.expected_lesson_hash.into(),
            actor.into(),
            request.reason.into(),
            request_json.into(),
        ];
        let scope = product_filter(b.product, "product_id");
        let sql = if let Some(product) = b.product {
            values.push(product.as_str().into());
            format!(
                "INSERT INTO lesson_direct_publications(lesson_id,revision,lesson_hash,actor_id,reason,request,review_version,product_id)VALUES($1,$2,$3,$4,$5,$6,(SELECT COALESCE(MAX(version),0) FROM lesson_audio_reviews WHERE lesson_id=$1 AND revision=$2{scope}),$7)"
            )
        } else {
            "INSERT INTO lesson_direct_publications(lesson_id,revision,lesson_hash,actor_id,reason,request,review_version)VALUES($1,$2,$3,$4,$5,$6,(SELECT COALESCE(MAX(version),0) FROM lesson_audio_reviews WHERE lesson_id=$1 AND revision=$2))".to_owned()
        };
        exec(&tx, &sql, values).await?;
    }
    let (source, published) = source(&tx, b.product, id, revision, false).await?;
    let mut result = status(&tx, b.product, id, revision, &source).await?;
    result.published = published;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(result)
}
async fn source(
    db: &impl ConnectionTrait,
    product: Option<crate::product::ProductId>,
    id: &str,
    rev: u32,
    lock: bool,
) -> Result<(Value, bool), AppError> {
    crate::admin::revision(id, rev)?;
    let sql = format!(
        "SELECT server_document,published,EXISTS(SELECT 1 FROM content_withdrawals w WHERE lesson_id=$1 AND revision=$2{}) AS withdrawn FROM lesson_revisions r WHERE lesson_id=$1 AND revision=$2{}{}",
        product_filter(product, "w.product_id"),
        product_filter(product, "r.product_id"),
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
    auth: AdminAuth,
    State(b): State<Store>,
    Path((id, rev)): Path<(String, u32)>,
) -> Result<Json<AdminLessonAudioStatus>, AppError> {
    auth.require_operator().await?;
    let (source, published) = source(&b.db, b.product, &id, rev, false).await?;
    let mut current = status(&b.db, b.product, &id, rev, &source).await?;
    current.published = published;
    Ok(Json(current))
}
async fn review(
    auth: AdminAuth,
    State(b): State<Store>,
    Path((id, rev)): Path<(String, u32)>,
    Extension(root): Extension<PathBuf>,
    Extension(permits): Extension<Arc<tokio::sync::Semaphore>>,
    Json(request): Json<AdminLessonAudioReview>,
) -> Result<Json<AdminLessonAudioStatus>, AppError> {
    let operator = auth.require_operator().await?;
    crate::admin::revision(&id, rev)?;
    crate::admin::reason(&request.reason)?;
    if !crate::voice_references::hex(&request.expected_lesson_hash, 64)
        || (request.accepted && !request.heard)
    {
        return Err(AppError::InvalidInput);
    }
    let actor = operator.actor;
    let _permit = permits
        .try_acquire_many_owned(2)
        .map_err(|_| AppError::RateLimited)?;
    let tx = b.db.begin().await.map_err(|_| AppError::Unavailable)?;
    if b.product.is_some_and(|product| product != operator.product) {
        return Err(AppError::Forbidden);
    }
    operator.lock_content(&tx).await?;
    one(
        &tx,
        &format!(
            "SELECT generation FROM content_state WHERE {} FOR UPDATE",
            b.product.map_or_else(
                || "singleton".to_owned(),
                |p| format!("product_id='{}'", p.as_str())
            )
        ),
        vec![],
    )
    .await?
    .ok_or(AppError::Unavailable)?;
    let (source, published) = source(&tx, b.product, &id, rev, true).await?;
    let mut current = status(&tx, b.product, &id, rev, &source).await?;
    current.published = published;
    if !current.required || current.lesson_hash != request.expected_lesson_hash {
        return Err(AppError::Conflict);
    }
    if current.version != request.version {
        if current.version == request.version.saturating_add(1) {
            let row=one(&tx,&format!("SELECT accepted,heard,actor_id,reason FROM lesson_audio_reviews WHERE lesson_id=$1 AND revision=$2 AND version=$3{}",product_filter(b.product,"product_id")),vec![id.into(),(rev as i32).into(),(current.version as i32).into()]).await?.ok_or(AppError::Unavailable)?;
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
        let lesson =
            crate::author_source::check_any_source(&source).map_err(|_| AppError::Unavailable)?;
        lesson
            .validate_public()
            .map_err(|_| AppError::Unavailable)?;
        crate::recording::validate_checked_lesson_detailed(&tx, b.product, &lesson, &root)
            .await
            .map_err(|error| error.runtime)?;
    }
    let next = i32::try_from(current.version)
        .ok()
        .and_then(|v| v.checked_add(1))
        .ok_or(AppError::Unavailable)?;
    let mut values = vec![
        id.into(),
        (rev as i32).into(),
        next.into(),
        current.lesson_hash.clone().into(),
        request.accepted.into(),
        request.heard.into(),
        actor.into(),
        request.reason.clone().into(),
    ];
    let sql = if let Some(product) = b.product {
        values.push(product.as_str().into());
        "INSERT INTO lesson_audio_reviews(lesson_id,revision,version,lesson_hash,accepted,heard,actor_id,reason,product_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)"
    } else {
        "INSERT INTO lesson_audio_reviews(lesson_id,revision,version,lesson_hash,accepted,heard,actor_id,reason) VALUES($1,$2,$3,$4,$5,$6,$7,$8)"
    };
    exec(&tx, sql, values).await?;
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
