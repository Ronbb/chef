//! Operator controls reuse content transactions; author snapshots remain immutable.
use crate::{
    AppError,
    admin_auth::AdminAuth,
    identity::Backend,
    learning::{exec, field, one, owner, product_filter},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use brioche_course_contract::{
    AdminActivateRequest, AdminDocumentRequest, AdminImportResult, AdminLesson, AdminLessonCursor,
    AdminOverview, AdminRelease, AdminReviewRequest, AdminWithdrawRequest,
};
use sea_orm::{ConnectionTrait, DbBackend, IsolationLevel, Statement, TransactionTrait};
use serde_json::Value;

pub fn router(root: std::path::PathBuf, db: sea_orm::DatabaseConnection) -> Router<Backend> {
    Router::new()
        .merge(crate::voice_references::delivery_router(db.clone(), None))
        .merge(crate::account_admin::router(
            crate::product::ProductId::Brioche,
        ))
        .merge(content_router(db, root.clone(), true, None))
        .layer(axum::Extension(root))
        .layer(axum::Extension(std::sync::Arc::new(
            tokio::sync::Semaphore::new(2),
        )))
}
#[derive(Clone)]
struct Store {
    db: sea_orm::DatabaseConnection,
    include_accounts: bool,
    product: Option<crate::product::ProductId>,
}
impl Store {
    fn state_selector(&self) -> String {
        self.product.map_or_else(
            || "singleton".to_owned(),
            |p| format!("product_id='{}'", p.as_str()),
        )
    }
}
pub fn independent_router(
    db: sea_orm::DatabaseConnection,
    client: crate::learning_identity::Client,
    root: std::path::PathBuf,
) -> anyhow::Result<Router> {
    anyhow::ensure!(
        client.product() == crate::product::ProductId::Brioche,
        "Content tenant migration incomplete"
    );
    let delivery = crate::voice_references::delivery_router(db.clone(), Some(client.clone()))
        .layer(axum::Extension(root.clone()))
        .layer(axum::Extension(std::sync::Arc::new(
            tokio::sync::Semaphore::new(2),
        )));
    let product = client.product();
    Ok(
        crate::learning_identity::protect(content_router(db, root, false, Some(product)), client)
            .merge(delivery),
    )
}
pub(crate) fn content_router<S: Clone + Send + Sync + 'static>(
    db: sea_orm::DatabaseConnection,
    root: std::path::PathBuf,
    include_accounts: bool,
    product: Option<crate::product::ProductId>,
) -> Router<S> {
    Router::new()
        .merge(crate::admin_assets::router(db.clone(), product))
        .merge(crate::admin_recordings::router(db.clone(), product))
        .merge(crate::character_voices::router(db.clone(), product))
        .merge(crate::voice_references::router(db.clone(), product))
        .merge(crate::voice_jobs::router(db.clone(), product))
        .merge(crate::voice_auditions::router(db.clone(), product))
        .merge(crate::admin_speech_plans::router(db.clone(), product))
        .merge(crate::speech_clips::router(db.clone(), product))
        .merge(crate::speech_export::router(db.clone(), product))
        .merge(crate::speech_alignments::router(db.clone(), product))
        .merge(crate::speech_package::router(db.clone(), product))
        .merge(crate::speech_automatic::router(db.clone(), product))
        .merge(crate::lesson_audio_reviews::router(db.clone(), product))
        .merge(crate::preview::router(root.clone(), db.clone(), product))
        .route("/api/v1/operator/overview", get(overview))
        .route("/api/v1/operator/history", get(history))
        .route(
            "/api/v1/operator/documents/{kind}/check",
            post(check_document).layer(axum::extract::DefaultBodyLimit::max(4 * 1024 * 1024)),
        )
        .route(
            "/api/v1/operator/lessons/{id}/revisions/{revision}/review",
            post(review),
        )
        .route("/api/v1/operator/releases/activate", post(activate))
        .route(
            "/api/v1/operator/releases/stage",
            post(stage).layer(axum::extract::DefaultBodyLimit::max(4 * 1024 * 1024)),
        )
        .route(
            "/api/v1/operator/lessons/import",
            post(import_lesson).layer(axum::extract::DefaultBodyLimit::max(4 * 1024 * 1024)),
        )
        .route(
            "/api/v1/operator/lessons/{id}/revisions/{revision}/withdraw",
            post(withdraw),
        )
        .layer(axum::Extension(root))
        .layer(axum::Extension(std::sync::Arc::new(
            tokio::sync::Semaphore::new(2),
        )))
        .with_state(Store {
            db,
            include_accounts,
            product,
        })
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HistoryQuery {
    before_time: Option<String>,
    before_key: Option<String>,
}
async fn history(
    auth: AdminAuth,
    State(backend): State<Store>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<brioche_course_contract::AdminHistory>, AppError> {
    use brioche_course_contract::{AdminHistory, AdminHistoryCursor, AdminHistoryItem};
    auth.require_operator().await?;
    if query.before_time.is_some() != query.before_key.is_some() {
        return Err(AppError::InvalidInput);
    }
    if let Some(time) = &query.before_time
        && (time.len() > 40 || time.parse::<jiff::Timestamp>().is_err())
    {
        return Err(AppError::InvalidInput);
    }
    if let Some(key) = &query.before_key
        && (key.is_empty()
            || key.len() > 256
            || !key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_:".contains(&b)))
    {
        return Err(AppError::InvalidInput);
    }
    let account_events = if backend.include_accounts {
        "UNION ALL SELECT 'account:'||id, CASE WHEN action='invite' AND details->>'role'='operator' THEN 'inviteOperator' ELSE action END, target_email, 'user:'||actor_id, reason, created_at FROM account_admin_audit WHERE product_id='brioche'"
    } else {
        ""
    };
    let course_scope = product_filter(backend.product, "product_id");
    let rows = backend.db.query_all_raw(Statement::from_sql_and_values(DbBackend::Postgres, format!(r#"
        WITH events AS (
            SELECT 'review:'||lesson_id||':'||revision||':'||version AS key,
                CASE WHEN approved THEN 'approve' ELSE 'reject' END AS action,
                lesson_id||' v'||revision AS target, 'user:'||actor_id AS actor, reason, created_at
            FROM editorial_reviews WHERE true{course_scope}
            UNION ALL
            SELECT 'content:'||id, action, COALESCE(release_id,lesson_id||' v'||revision,'未指定对象'), actor, reason, created_at FROM content_audit WHERE true{course_scope}
            UNION ALL
            SELECT 'import:'||lesson_id||':'||revision, 'import', lesson_id||' v'||revision, actor, reason, created_at FROM lesson_import_audit WHERE true{course_scope}
            {account_events}
            UNION ALL
            SELECT 'voice:'||character_id||':'||character_revision||':'||revision, 'voiceProfile', character_id||' v'||character_revision||' / voice v'||revision, 'user:'||actor_id, reason, created_at FROM character_voice_profiles WHERE true{course_scope}
            UNION ALL
            SELECT 'alignment:'||id,'alignmentImport',id,'user:'||actor_id,reason,created_at FROM speech_alignments WHERE true{course_scope}
            UNION ALL
            SELECT 'alignmentReview:'||alignment_id||':'||clip_id,CASE WHEN accepted THEN 'alignmentAccepted' ELSE 'alignmentRejected' END,alignment_id||':'||clip_id,'user:'||actor_id,reason,created_at FROM speech_alignment_reviews WHERE true{course_scope}
            UNION ALL
            SELECT 'lessonAudio:'||lesson_id||':'||revision||':'||version,CASE WHEN accepted THEN 'lessonAudioAccepted' ELSE 'lessonAudioRejected' END,lesson_id||' v'||revision,'user:'||actor_id,reason,created_at FROM lesson_audio_reviews WHERE true{course_scope}
            UNION ALL
            SELECT 'directPublication:'||lesson_id||':'||revision,'lessonDirectPublication',lesson_id||' v'||revision,'user:'||actor_id,reason,created_at FROM lesson_direct_publications WHERE true{course_scope}
            UNION ALL
            SELECT 'speechPackage:'||id,'speechPackageImport',lesson_id||' v'||revision,'user:'||actor_id,reason,created_at FROM speech_package_imports WHERE true{course_scope}
            UNION ALL
            SELECT 'speechClip:'||id, 'speechClip', id, 'user:'||actor_id, reason, created_at FROM course_speech_clips WHERE true{course_scope}
            UNION ALL
            SELECT 'speechClipReview:'||clip_id, CASE WHEN accepted THEN 'speechClipAccepted' ELSE 'speechClipRejected' END, clip_id, 'user:'||actor_id, reason, created_at FROM course_speech_clip_reviews WHERE true{course_scope}
            UNION ALL
            SELECT 'speechPlan:'||id, 'speechPlan', lesson_id||' v'||lesson_revision, 'user:'||actor_id, reason, created_at FROM course_speech_plans WHERE true{course_scope}
            UNION ALL
            SELECT 'asset:'||id, 'assetImport', target, 'user:'||actor_id, reason, created_at FROM asset_import_audit WHERE actor_id IS NOT NULL{course_scope}
            UNION ALL
            SELECT 'audio:'||id, 'audioImport', target, 'user:'||actor_id, reason, created_at FROM audio_import_audit WHERE actor_id IS NOT NULL{course_scope}
            UNION ALL
            SELECT 'reference:'||id, 'referenceGrant', character_id||' v'||character_revision||' / voice v'||voice_revision, 'user:'||actor_id, reason, created_at FROM voice_reference_grants WHERE true{course_scope}
            UNION ALL
            SELECT 'referenceRevoked:'||grant_id, 'referenceRevoke', grant_id, 'user:'||actor_id, reason, created_at FROM voice_reference_revocations WHERE true{course_scope}
            UNION ALL
            SELECT 'voiceJob:'||id, 'voiceJobCreated', id, 'user:'||actor_id, reason, created_at FROM voice_clone_jobs WHERE true{course_scope}
            UNION ALL
            SELECT 'voiceCheck:'||job_id||':'||version, 'voiceJobCheck', job_id, 'user:'||actor_id, reason, created_at FROM voice_clone_events WHERE status='checking' AND actor_id IS NOT NULL{course_scope}
            UNION ALL
            SELECT 'audition:'||id, 'voiceAudition', id, 'user:'||actor_id, reason, created_at FROM voice_auditions WHERE true{course_scope}
            UNION ALL
            SELECT 'auditionReview:'||audition_id, CASE WHEN accepted THEN 'voiceAuditionAccepted' ELSE 'voiceAuditionRejected' END, audition_id, 'user:'||actor_id, reason, created_at FROM voice_audition_reviews WHERE true{course_scope}
        )
        SELECT key,action,target,actor,reason,to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"') AS created_at
        FROM events WHERE $1::timestamptz IS NULL OR (created_at,key COLLATE "C") < ($1::timestamptz,$2::text COLLATE "C")
        ORDER BY created_at DESC,key COLLATE "C" DESC LIMIT 21
    "#), vec![query.before_time.into(), query.before_key.into()])).await.map_err(|_|AppError::Unavailable)?;
    let has_more = rows.len() > 20;
    let mut items = Vec::new();
    for row in rows.into_iter().take(20) {
        items.push(AdminHistoryItem {
            key: field(&row, "key")?,
            action: field(&row, "action")?,
            target: field(&row, "target")?,
            actor: field(&row, "actor")?,
            reason: field(&row, "reason")?,
            created_at: field(&row, "created_at")?,
        });
    }
    let next = if has_more {
        items.last().map(|item| AdminHistoryCursor {
            before_time: item.created_at.clone(),
            before_key: item.key.clone(),
        })
    } else {
        None
    };
    Ok(Json(AdminHistory { items, next }))
}
async fn check_document(
    auth: AdminAuth,
    State(backend): State<Store>,
    Path(kind): Path<String>,
    axum::Extension(root): axum::Extension<std::path::PathBuf>,
    axum::Extension(permits): axum::Extension<std::sync::Arc<tokio::sync::Semaphore>>,
    Json(request): Json<AdminDocumentRequest>,
) -> Result<Json<brioche_course_contract::AdminDocumentCheck>, AppError> {
    auth.require_operator().await?;
    reason(&request.reason)?;
    let release = match kind.as_str() {
        "lesson" => false,
        "release" => true,
        _ => return Err(AppError::InvalidInput),
    };
    let permit = permits
        .clone()
        .try_acquire_owned()
        .map_err(|_| AppError::RateLimited)?;
    // A disconnected request must not free admission while detached blocking
    // file work or database checks continue. The bounded job owns the permit.
    let report = tokio::spawn(async move {
        let _permit = permit;
        let prepared = tokio::task::spawn_blocking(move || {
            crate::author_json::prepare_uploaded(request.document.as_bytes(), release)
        })
        .await
        .map_err(|_| AppError::Unavailable)?;
        let document = match prepared {
            Ok(document) => document,
            Err(report) => return Ok(report),
        };
        let tx = backend
            .db
            .begin_with_config(
                Some(IsolationLevel::RepeatableRead),
                Some(sea_orm::AccessMode::ReadOnly),
            )
            .await
            .map_err(|_| AppError::Unavailable)?;
        let report = if release {
            crate::content::check_registered_release(&tx, backend.product, &document, &root).await?
        } else {
            crate::author_import::check_registered(&tx, backend.product, &document, &root).await?
        };
        tx.rollback().await.map_err(|_| AppError::Unavailable)?;
        Ok(report)
    })
    .await
    .map_err(|_| AppError::Unavailable)??;
    Ok(Json(report))
}
async fn import_lesson(
    auth: AdminAuth,
    State(backend): State<Store>,
    axum::Extension(permits): axum::Extension<std::sync::Arc<tokio::sync::Semaphore>>,
    Json(request): Json<AdminDocumentRequest>,
) -> Result<Json<AdminImportResult>, AppError> {
    let operator = auth.require_operator().await?;
    reason(&request.reason)?;
    let _permit = permits.try_acquire().map_err(|_| AppError::RateLimited)?;
    let source = tokio::task::spawn_blocking(move || {
        let source = crate::author_json::parse_document(request.document.as_bytes())?;
        crate::validate_source_schema(source.clone())?;
        Ok::<_, anyhow::Error>(source)
    })
    .await
    .map_err(|_| AppError::Unavailable)?
    .map_err(|_| AppError::InvalidInput)?;
    let imported = crate::author_import::import_operator(
        &backend.db,
        backend.product,
        source,
        &operator,
        &request.reason,
    )
    .await
    .map_err(|error| {
        if error.is::<crate::author_import::RevisionConflict>() {
            return AppError::Conflict;
        }
        error
            .downcast_ref::<AppError>()
            .map_or(AppError::InvalidInput, |error| match error {
                AppError::Forbidden => AppError::Forbidden,
                AppError::Unauthorized => AppError::Unauthorized,
                AppError::NotFound => AppError::NotFound,
                _ => AppError::Unavailable,
            })
    })?;
    Ok(Json(imported))
}
async fn stage(
    auth: AdminAuth,
    State(backend): State<Store>,
    axum::Extension(root): axum::Extension<std::path::PathBuf>,
    axum::Extension(permits): axum::Extension<std::sync::Arc<tokio::sync::Semaphore>>,
    Json(request): Json<AdminDocumentRequest>,
) -> Result<Json<String>, AppError> {
    let operator = auth.require_operator().await?;
    reason(&request.reason)?;
    let _permit = permits.try_acquire().map_err(|_| AppError::RateLimited)?;
    let manifest: crate::content::ReleaseManifest = tokio::task::spawn_blocking(move || {
        let value = crate::author_json::parse_document(request.document.as_bytes())?;
        let manifest: crate::content::ReleaseManifest = crate::author_json::from_value(value, "")?;
        manifest.validate_author()?;
        Ok::<_, anyhow::Error>(manifest)
    })
    .await
    .map_err(|_| AppError::Unavailable)?
    .map_err(|_| AppError::InvalidInput)?;
    crate::content::stage_operator(
        &backend.db,
        backend.product,
        &manifest,
        &operator,
        &request.reason,
        &root,
    )
    .await?;
    Ok(Json(manifest.id))
}
pub(crate) use crate::account_admin::reason;
pub(crate) fn revision(id: &str, rev: u32) -> Result<(), AppError> {
    if !brioche_course_contract::valid_content_id(id)
        || !brioche_course_contract::valid_content_revision(rev)
    {
        return Err(AppError::InvalidInput);
    }
    Ok(())
}
use crate::account_admin::generation;
pub(crate) async fn approved<C: ConnectionTrait>(
    db: &C,
    product: Option<crate::product::ProductId>,
    id: &str,
    rev: u32,
    source: &Value,
) -> Result<bool, AppError> {
    let latest = one(db,&format!("SELECT approved FROM editorial_reviews WHERE lesson_id=$1 AND revision=$2{} ORDER BY version DESC LIMIT 1",product_filter(product,"product_id")),vec![id.into(),(rev as i32).into()]).await?;
    let editorial_approved = match latest {
        Some(row) => field(&row, "approved"),
        None => Ok(matches!(
            crate::author_source::editorial(source)
                .map_err(|_| AppError::InvalidInput)?
                .status,
            crate::author_source::EditorialStatus::Reviewed
        )),
    }?;
    Ok(editorial_approved
        && crate::lesson_audio_reviews::accepted(db, product, id, rev, source).await?)
}
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OverviewQuery {
    lesson_after_id: Option<String>,
    lesson_after_revision: Option<u32>,
    release_after_id: Option<String>,
    lesson_q: Option<String>,
    release_q: Option<String>,
}
impl OverviewQuery {
    fn validate(&self) -> Result<(), AppError> {
        match (&self.lesson_after_id, self.lesson_after_revision) {
            (None, None) => {}
            (Some(id), Some(rev)) => revision(id, rev)?,
            _ => return Err(AppError::InvalidInput),
        }
        if self
            .release_after_id
            .as_ref()
            .is_some_and(|id| !brioche_course_contract::valid_content_id(id))
        {
            return Err(AppError::InvalidInput);
        }
        for q in [&self.lesson_q, &self.release_q].into_iter().flatten() {
            if q.len() > 200 || q.chars().any(char::is_control) {
                return Err(AppError::InvalidInput);
            }
        }
        Ok(())
    }
}
async fn overview(
    auth: AdminAuth,
    State(backend): State<Store>,
    Query(query): Query<OverviewQuery>,
) -> Result<Json<AdminOverview>, AppError> {
    auth.require_operator().await?;
    query.validate()?;
    let tx = backend
        .db
        .begin_with_config(Some(IsolationLevel::RepeatableRead), None)
        .await
        .map_err(|_| AppError::Unavailable)?;
    let state = one(
        &tx,
        &format!(
            "SELECT active_release,generation FROM content_state WHERE {}",
            backend.state_selector()
        ),
        vec![],
    )
    .await?
    .ok_or(AppError::Unavailable)?;
    let lesson_scope = product_filter(backend.product, "r.product_id");
    let same_withdrawal = if backend.product.is_some() {
        " AND w.product_id=r.product_id"
    } else {
        ""
    };
    let same_review = if backend.product.is_some() {
        " AND product_id=r.product_id"
    } else {
        ""
    };
    let rows=tx.query_all_raw(Statement::from_sql_and_values(DbBackend::Postgres,
        format!("SELECT r.lesson_id,r.revision,r.public_document,r.published,r.server_document AS source,r.server_document->'editorial' AS editorial,v.version,v.approved,v.reason,EXISTS(SELECT 1 FROM content_withdrawals w WHERE (w.lesson_id,w.revision)=(r.lesson_id,r.revision){same_withdrawal}) AS withdrawn FROM lesson_revisions r LEFT JOIN LATERAL (SELECT version,approved,reason FROM editorial_reviews WHERE (lesson_id,revision)=(r.lesson_id,r.revision){same_review} ORDER BY version DESC LIMIT 1) v ON true WHERE (r.lesson_id>$1 OR (r.lesson_id=$1 AND r.revision<$2)) AND ($3='' OR strpos(lower(r.lesson_id),lower($3))>0 OR strpos(lower(r.public_document->'title'->>'zh'),lower($3))>0 OR strpos(lower(r.public_document->'title'->>'fr'),lower($3))>0 OR strpos(lower(r.public_document->>'levelId'),lower($3))>0 OR strpos(lower(r.public_document->>'unitId'),lower($3))>0) {lesson_scope} ORDER BY r.lesson_id,r.revision DESC LIMIT 21"),
        vec![query.lesson_after_id.unwrap_or_default().into(),(query.lesson_after_revision.unwrap_or(0) as i32).into(),query.lesson_q.unwrap_or_default().trim().to_owned().into()])).await.map_err(|_|AppError::Unavailable)?;
    let lesson_more = rows.len() > 20;
    let mut lessons = Vec::new();
    for row in rows.into_iter().take(20) {
        let public: Value = field(&row, "public_document")?;
        let editorial: Value = field(&row, "editorial")?;
        let source: Value = field(&row, "source")?;
        let content_approved =
            field::<Option<bool>>(&row, "approved")?.unwrap_or(editorial["status"] == "reviewed");
        let audio_required = crate::lesson_audio_reviews::required(&source);
        let audio_accepted = audio_required
            && crate::lesson_audio_reviews::accepted(
                &tx,
                backend.product,
                &field::<String>(&row, "lesson_id")?,
                field::<i32>(&row, "revision")? as u32,
                &source,
            )
            .await?;
        lessons.push(AdminLesson {
            id: field(&row, "lesson_id")?,
            revision: field::<i32>(&row, "revision")? as u32,
            title: public["title"]["zh"]
                .as_str()
                .ok_or(AppError::Unavailable)?
                .into(),
            level: public["levelId"]
                .as_str()
                .ok_or(AppError::Unavailable)?
                .into(),
            unit: public["unitId"]
                .as_str()
                .ok_or(AppError::Unavailable)?
                .into(),
            published: field(&row, "published")?,
            withdrawn: field(&row, "withdrawn")?,
            approved: content_approved && (!audio_required || audio_accepted),
            content_approved,
            audio_required,
            audio_accepted,
            review_version: field::<Option<i32>>(&row, "version")?.unwrap_or(0) as u32,
            review_note: field::<Option<String>>(&row, "reason")?
                .unwrap_or_else(|| editorial["note"].as_str().unwrap_or("").into()),
        });
    }
    let lesson_next = if lesson_more {
        lessons.last().map(|lesson| AdminLessonCursor {
            id: lesson.id.clone(),
            revision: lesson.revision,
        })
    } else {
        None
    };
    let rows=tx.query_all_raw(Statement::from_sql_and_values(DbBackend::Postgres,format!("SELECT r.id,count(e.lesson_id) AS lesson_count FROM content_releases r LEFT JOIN release_entries e ON e.release_id=r.id{} WHERE r.id>$1 AND ($2='' OR strpos(lower(r.id),lower($2))>0){} GROUP BY r.id ORDER BY r.id LIMIT 21",if backend.product.is_some(){" AND e.product_id=r.product_id"}else{""},product_filter(backend.product,"r.product_id")), vec![query.release_after_id.unwrap_or_default().into(),query.release_q.unwrap_or_default().trim().to_owned().into()])).await.map_err(|_|AppError::Unavailable)?;
    let release_more = rows.len() > 20;
    let releases = rows
        .iter()
        .take(20)
        .map(|row| {
            Ok(AdminRelease {
                id: field(row, "id")?,
                lesson_count: u32::try_from(field::<i64>(row, "lesson_count")?)
                    .map_err(|_| AppError::Unavailable)?,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let release_next = if release_more {
        releases.last().map(|r| r.id.clone())
    } else {
        None
    };
    let result = AdminOverview {
        generation: field::<i64>(&state, "generation")?.to_string(),
        active_release: field(&state, "active_release")?,
        lessons,
        releases,
        lesson_next,
        release_next,
    };
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(result))
}
async fn review(
    auth: AdminAuth,
    State(backend): State<Store>,
    Path((id, rev)): Path<(String, u32)>,
    Json(request): Json<AdminReviewRequest>,
) -> Result<Json<AdminReviewRequest>, AppError> {
    let operator = auth.require_operator().await?;
    revision(&id, rev)?;
    reason(&request.reason)?;
    let actor = owner(&auth)?;
    let tx = backend
        .db
        .begin()
        .await
        .map_err(|_| AppError::Unavailable)?;
    operator.lock_content(&tx).await?;
    one(
        &tx,
        &format!(
            "SELECT generation FROM content_state WHERE {} FOR UPDATE",
            backend.state_selector()
        ),
        vec![],
    )
    .await?
    .ok_or(AppError::Unavailable)?;
    let row=one(&tx,&format!("SELECT published,server_document,EXISTS(SELECT 1 FROM content_withdrawals w WHERE lesson_id=$1 AND revision=$2{}) AS withdrawn FROM lesson_revisions r WHERE lesson_id=$1 AND revision=$2{} FOR UPDATE",product_filter(backend.product,"w.product_id"),product_filter(backend.product,"r.product_id")),vec![id.clone().into(),(rev as i32).into()]).await?.ok_or(AppError::NotFound)?;
    if field::<bool>(&row, "withdrawn")? {
        return Err(AppError::Gone);
    }
    // Published snapshots keep their approval; removal uses the explicit withdrawal operation.
    if field::<bool>(&row, "published")? {
        return Err(AppError::Conflict);
    }
    let latest=one(&tx,&format!("SELECT version,approved,reason FROM editorial_reviews WHERE lesson_id=$1 AND revision=$2{} ORDER BY version DESC LIMIT 1",product_filter(backend.product,"product_id")),vec![id.clone().into(),(rev as i32).into()]).await?;
    let current = latest
        .as_ref()
        .map(|row| field::<i32>(row, "version"))
        .transpose()?
        .unwrap_or(0) as u32;
    if current != request.version {
        // An identical retry of the immediately preceding decision is acknowledged without another row.
        if current == request.version.saturating_add(1)
            && latest.as_ref().is_some_and(|row| {
                field::<bool>(row, "approved").ok() == Some(request.approved)
                    && field::<String>(row, "reason").ok().as_deref() == Some(&request.reason)
            })
        {
            let original=one(&tx,&format!("SELECT actor_id FROM editorial_reviews WHERE lesson_id=$1 AND revision=$2 AND version=$3{}",product_filter(backend.product,"product_id")),vec![id.into(),(rev as i32).into(),(current as i32).into()]).await?.ok_or(AppError::Unavailable)?;
            if field::<i64>(&original, "actor_id")? == actor {
                return Ok(Json(AdminReviewRequest {
                    version: current,
                    ..request
                }));
            }
        }
        return Err(AppError::Conflict);
    }
    if request.approved
        && !crate::lesson_audio_reviews::accepted(
            &tx,
            backend.product,
            &id,
            rev,
            &field::<Value>(&row, "server_document")?,
        )
        .await?
    {
        return Err(AppError::InvalidInput);
    }
    let next = i32::try_from(current)
        .ok()
        .and_then(|v| v.checked_add(1))
        .ok_or(AppError::Unavailable)?;
    if let Some(product) = backend.product {
        exec(&tx,"INSERT INTO editorial_reviews(lesson_id,revision,version,approved,actor_id,reason,product_id) VALUES($1,$2,$3,$4,$5,$6,$7)",vec![id.into(),(rev as i32).into(),next.into(),request.approved.into(),actor.into(),request.reason.clone().into(),product.as_str().into()]).await?;
    } else {
        exec(&tx,"INSERT INTO editorial_reviews(lesson_id,revision,version,approved,actor_id,reason) VALUES($1,$2,$3,$4,$5,$6)",vec![id.into(),(rev as i32).into(),next.into(),request.approved.into(),actor.into(),request.reason.clone().into()]).await?;
    }
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(AdminReviewRequest {
        version: next as u32,
        ..request
    }))
}
async fn activate(
    auth: AdminAuth,
    State(backend): State<Store>,
    axum::Extension(root): axum::Extension<std::path::PathBuf>,
    Json(request): Json<AdminActivateRequest>,
) -> Result<Json<String>, AppError> {
    let operator = auth.require_operator().await?;
    reason(&request.reason)?;
    let result = crate::content::activate_operator(
        &backend.db,
        backend.product,
        &request.release_id,
        generation(&request.generation)?,
        &operator,
        &request.reason,
        &root,
    )
    .await?;
    Ok(Json(result.to_string()))
}
async fn withdraw(
    auth: AdminAuth,
    State(backend): State<Store>,
    Path((id, rev)): Path<(String, u32)>,
    Json(request): Json<AdminWithdrawRequest>,
) -> Result<Json<String>, AppError> {
    let operator = auth.require_operator().await?;
    revision(&id, rev)?;
    reason(&request.reason)?;
    let result = crate::content::withdraw_operator(
        &backend.db,
        backend.product,
        &id,
        rev,
        generation(&request.generation)?,
        &operator,
        &request.reason,
    )
    .await?;
    Ok(Json(result.to_string()))
}
