//! Imported immutable revisions are visible only to an authenticated operator.
use crate::{
    AppError,
    admin_auth::AdminAuth,
    author_source::CheckedLesson,
    learning::{field, one, product_filter},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use brioche_course_contract::{
    GradeRequest, GradeResult, PreviewRelease, PublicLesson,
    neutral::{NeutralLesson, NeutralPreviewRelease},
};
use sea_orm::{ConnectionTrait, DbBackend, IsolationLevel, Statement, TransactionTrait};

#[derive(Clone)]
struct PreviewMedia {
    root: std::path::PathBuf,
    permits: std::sync::Arc<tokio::sync::Semaphore>,
}
#[derive(Clone)]
struct Store {
    db: sea_orm::DatabaseConnection,
    product: Option<crate::product::ProductId>,
}
pub(crate) fn router<S: Clone + Send + Sync + 'static>(
    root: std::path::PathBuf,
    db: sea_orm::DatabaseConnection,
    product: Option<crate::product::ProductId>,
) -> Router<S> {
    Router::new()
        .route("/api/v1/operator/releases/{id}", get(release))
        .route("/api/v2/operator/releases/{id}", get(release_neutral))
        .route(
            "/api/v2/operator/lessons/{id}/revisions/{revision}",
            get(lesson_neutral),
        )
        .route(
            "/api/v2/operator/lessons/{id}/revisions/{revision}/grade",
            post(grade_neutral),
        )
        .route(
            "/api/v2/operator/lessons/{id}/revisions/{revision}/media/{name}",
            get(media),
        )
        .route(
            "/api/v2/operator/lessons/{id}/revisions/{revision}/audio/{name}",
            get(audio),
        )
        .route(
            "/api/v1/operator/lessons/{id}/revisions/{revision}/grade",
            post(grade),
        )
        .route(
            "/api/v1/operator/lessons/{id}/revisions/{revision}",
            get(lesson),
        )
        .route(
            "/api/v1/operator/lessons/{id}/revisions/{revision}/media/{name}",
            get(media),
        )
        .route(
            "/api/v1/operator/lessons/{id}/revisions/{revision}/audio/{name}",
            get(audio),
        )
        .route_layer(axum::middleware::from_fn(reject_query))
        .layer(axum::Extension(PreviewMedia {
            root,
            permits: std::sync::Arc::new(tokio::sync::Semaphore::new(2)),
        }))
        .with_state(Store { db, product })
}

// Product comes only from trusted service configuration, never preview URLs.
async fn reject_query(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, AppError> {
    if request.uri().query().is_some() {
        return Err(AppError::InvalidInput);
    }
    Ok(next.run(request).await)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 100
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

async fn release(
    auth: AdminAuth,
    State(backend): State<Store>,
    Path(id): Path<String>,
) -> Result<Json<PreviewRelease>, AppError> {
    auth.require_operator().await?;
    Ok(Json(
        serde_json::from_value(read_release(&backend, id, false).await?)
            .map_err(|_| AppError::Unavailable)?,
    ))
}
async fn release_neutral(
    auth: AdminAuth,
    State(backend): State<Store>,
    Path(id): Path<String>,
) -> Result<Json<NeutralPreviewRelease>, AppError> {
    auth.require_operator().await?;
    Ok(Json(
        serde_json::from_value(read_release(&backend, id, true).await?)
            .map_err(|_| AppError::Unavailable)?,
    ))
}
async fn read_release(
    backend: &Store,
    id: String,
    neutral: bool,
) -> Result<serde_json::Value, AppError> {
    if !valid_id(&id) {
        return Err(AppError::InvalidInput);
    }
    let tx = backend
        .db
        .begin_with_config(Some(IsolationLevel::RepeatableRead), None)
        .await
        .map_err(|_| AppError::Unavailable)?;
    let row = one(
        &tx,
        &format!(
            "SELECT manifest FROM content_releases WHERE id=$1{}",
            product_filter(backend.product, "product_id")
        ),
        vec![id.clone().into()],
    )
    .await?
    .ok_or(AppError::NotFound)?;
    let manifest: crate::content::ReleaseManifest =
        serde_json::from_value(field(&row, "manifest")?).map_err(|_| AppError::Unavailable)?;
    manifest.validate().map_err(|_| AppError::Unavailable)?;
    if manifest.id != id {
        return Err(AppError::Unavailable);
    }
    let rows = tx.query_all_raw(Statement::from_sql_and_values(DbBackend::Postgres,
        format!("SELECT r.public_document,EXISTS(SELECT 1 FROM content_withdrawals w WHERE (w.lesson_id,w.revision)=(r.lesson_id,r.revision){}) AS withdrawn FROM release_entries e JOIN lesson_revisions r ON (r.lesson_id,r.revision)=(e.lesson_id,e.revision) WHERE e.release_id=$1{}{} ORDER BY e.position", product_filter(backend.product, "w.product_id"), product_filter(backend.product, "e.product_id"), product_filter(backend.product, "r.product_id")),
        [id.clone().into()])).await.map_err(|_| AppError::Unavailable)?;
    let mut rows = rows.into_iter();
    let mut levels = Vec::new();
    let mut withdrawn = Vec::new();
    for level in manifest.levels {
        let mut units = Vec::new();
        for unit in level.units {
            let mut lessons = Vec::new();
            for entry in unit.lessons {
                let row = rows.next().ok_or(AppError::Unavailable)?;
                let lesson = CheckedLesson::from_public_document(field(&row, "public_document")?)
                    .map_err(|_| AppError::Unavailable)?;
                if lesson.id() != entry.lesson_id
                    || lesson.revision() != entry.revision
                    || lesson.level_id() != level.id
                    || lesson.unit_id() != unit.id
                {
                    return Err(AppError::Unavailable);
                }
                if field::<bool>(&row, "withdrawn")? {
                    withdrawn.push(lesson.id().to_owned());
                }
                lessons.push(lesson.summary_document(neutral)?);
            }
            units.push(serde_json::json!({"id":unit.id,"titleZh":unit.title_zh,"lessons":lessons}));
        }
        levels.push(serde_json::json!({"id":level.id,"label":level.label,"units":units}));
    }
    if rows.next().is_some() {
        return Err(AppError::Unavailable);
    }
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(
        serde_json::json!({"id":id,"catalog":{"levels":levels,"developmentFixture":false},"withdrawnLessonIds":withdrawn}),
    )
}
async fn read_checked(backend: &Store, id: &str, revision: u32) -> Result<CheckedLesson, AppError> {
    if !valid_id(id) || revision == 0 || revision > i32::MAX as u32 {
        return Err(AppError::InvalidInput);
    }
    let row = one(&backend.db,
        &format!("SELECT public_document,EXISTS(SELECT 1 FROM content_withdrawals w WHERE (w.lesson_id,w.revision)=(r.lesson_id,r.revision){}) AS withdrawn FROM lesson_revisions r WHERE lesson_id=$1 AND revision=$2{}", product_filter(backend.product, "w.product_id"), product_filter(backend.product, "r.product_id")),
        vec![id.into(),(revision as i32).into()]).await?.ok_or(AppError::NotFound)?;
    if field::<bool>(&row, "withdrawn")? {
        return Err(AppError::Gone);
    }
    let lesson = CheckedLesson::from_public_document(field(&row, "public_document")?)
        .map_err(|_| AppError::Unavailable)?;
    if lesson.id() != id || lesson.revision() != revision {
        return Err(AppError::Unavailable);
    }
    Ok(lesson)
}
async fn read(backend: &Store, id: &str, revision: u32) -> Result<PublicLesson, AppError> {
    let CheckedLesson::Legacy(mut lesson) = read_checked(backend, id, revision).await? else {
        return Err(AppError::Conflict);
    };
    rewrite_assets(&mut lesson.media, &mut lesson.audio, id, revision, "v1")?;
    rewrite_recordings(
        lesson
            .knowledge
            .vocabulary
            .iter_mut()
            .filter_map(|v| v.recording.as_mut())
            .chain(
                lesson
                    .knowledge
                    .grammar
                    .iter_mut()
                    .flat_map(|g| g.examples.iter_mut().filter_map(|e| e.recording.as_mut())),
            ),
        &lesson.audio,
    )?;
    Ok(lesson)
}
fn rewrite_assets(
    media: &mut [brioche_course_contract::MediaAsset],
    audio: &mut [brioche_course_contract::AudioAsset],
    id: &str,
    revision: u32,
    version: &str,
) -> Result<(), AppError> {
    for asset in media {
        let name = asset
            .url
            .strip_prefix("/api/media/")
            .ok_or(AppError::Unavailable)?;
        if name.contains('/') || name.contains('?') || name.contains('#') {
            return Err(AppError::Unavailable);
        }
        asset.url =
            format!("/api/{version}/operator/lessons/{id}/revisions/{revision}/media/{name}");
    }
    for asset in audio {
        let name = asset
            .url
            .strip_prefix("/api/audio/")
            .ok_or(AppError::Unavailable)?;
        if name.contains('/') || name.contains('?') || name.contains('#') {
            return Err(AppError::Unavailable);
        }
        asset.url =
            format!("/api/{version}/operator/lessons/{id}/revisions/{revision}/audio/{name}");
    }
    Ok(())
}
fn rewrite_recordings<'a>(
    recordings: impl Iterator<Item = &'a mut brioche_course_contract::KnowledgeRecording>,
    audio: &[brioche_course_contract::AudioAsset],
) -> Result<(), AppError> {
    for recording in recordings {
        let asset = audio
            .iter()
            .find(|asset| asset.asset_id == recording.asset.asset_id)
            .ok_or(AppError::Unavailable)?;
        recording.asset.url = asset.url.clone();
    }
    Ok(())
}
async fn lesson_neutral(
    auth: AdminAuth,
    State(backend): State<Store>,
    Path((id, revision)): Path<(String, u32)>,
) -> Result<Json<NeutralLesson>, AppError> {
    auth.require_operator().await?;
    let checked = read_checked(&backend, &id, revision).await?;
    let mut lesson = brioche_course_contract::neutral::decode_public(
        checked
            .public_document()
            .map_err(|_| AppError::Unavailable)?,
    )
    .map_err(|_| AppError::Unavailable)?;
    rewrite_assets(&mut lesson.media, &mut lesson.audio, &id, revision, "v2")?;
    rewrite_recordings(
        lesson
            .knowledge
            .vocabulary
            .iter_mut()
            .filter_map(|v| v.recording.as_mut())
            .chain(
                lesson
                    .knowledge
                    .grammar
                    .iter_mut()
                    .flat_map(|g| g.examples.iter_mut().filter_map(|e| e.recording.as_mut())),
            ),
        &lesson.audio,
    )?;
    Ok(Json(lesson))
}
async fn lesson(
    auth: AdminAuth,
    State(backend): State<Store>,
    Path((id, revision)): Path<(String, u32)>,
) -> Result<Json<PublicLesson>, AppError> {
    auth.require_operator().await?;
    Ok(Json(read(&backend, &id, revision).await?))
}

async fn grade(
    auth: AdminAuth,
    State(backend): State<Store>,
    Path((id, revision)): Path<(String, u32)>,
    Json(request): Json<GradeRequest>,
) -> Result<Json<GradeResult>, AppError> {
    grade_checked(auth, backend, id, revision, request, false).await
}
async fn grade_neutral(
    auth: AdminAuth,
    State(backend): State<Store>,
    Path((id, revision)): Path<(String, u32)>,
    Json(request): Json<GradeRequest>,
) -> Result<Json<GradeResult>, AppError> {
    grade_checked(auth, backend, id, revision, request, true).await
}
async fn grade_checked(
    auth: AdminAuth,
    backend: Store,
    id: String,
    revision: u32,
    request: GradeRequest,
    neutral: bool,
) -> Result<Json<GradeResult>, AppError> {
    let operator = auth.require_operator().await?;
    if !valid_id(&id) || revision == 0 || revision > i32::MAX as u32 || request.revision != revision
    {
        return Err(AppError::InvalidInput);
    }
    let tx = backend
        .db
        .begin()
        .await
        .map_err(|_| AppError::Unavailable)?;
    operator.lock_content(&tx).await?;
    let row = one(&tx,
        &format!("SELECT public_document,server_document,EXISTS(SELECT 1 FROM content_withdrawals w WHERE (w.lesson_id,w.revision)=(r.lesson_id,r.revision){}) AS withdrawn FROM lesson_revisions r WHERE lesson_id=$1 AND revision=$2{}", product_filter(backend.product, "w.product_id"), product_filter(backend.product, "r.product_id")),
        vec![id.clone().into(),(revision as i32).into()]).await?.ok_or(AppError::NotFound)?;
    if field::<bool>(&row, "withdrawn")? {
        return Err(AppError::Gone);
    }
    let lesson = CheckedLesson::from_public_document(field(&row, "public_document")?)
        .map_err(|_| AppError::Unavailable)?;
    if lesson.id() != id || lesson.revision() != revision {
        return Err(AppError::Unavailable);
    }
    if !neutral && !matches!(lesson, CheckedLesson::Legacy(_)) {
        return Err(AppError::Conflict);
    }
    let source: serde_json::Value = field(&row, "server_document")?;
    let result = lesson
        .grade(&source, &request.exercise_id, &request.answer)
        .map_err(|error| match error {
            crate::grading::GradeError::InvalidContent => AppError::Unavailable,
            crate::grading::GradeError::UnknownExercise => AppError::NotFound,
            crate::grading::GradeError::InvalidAnswer => AppError::InvalidAnswer,
        })?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(result))
}
async fn media(
    auth: AdminAuth,
    axum::Extension(config): axum::Extension<PreviewMedia>,
    State(backend): State<Store>,
    Path((id, revision, name)): Path<(String, u32, String)>,
) -> Result<axum::response::Response, AppError> {
    auth.require_operator().await?;
    let lesson = read_checked(&backend, &id, revision).await?;
    let asset = lesson
        .media()
        .iter()
        .find(|asset| asset.url.rsplit('/').next() == Some(name.as_str()))
        .ok_or(AppError::NotFound)?;
    crate::media::asset_response(config.root, asset.clone(), config.permits).await
}

async fn audio(
    auth: AdminAuth,
    axum::Extension(config): axum::Extension<PreviewMedia>,
    State(backend): State<Store>,
    Path((id, revision, name)): Path<(String, u32, String)>,
    headers: axum::http::HeaderMap,
) -> Result<axum::response::Response, AppError> {
    auth.require_operator().await?;
    let lesson = read_checked(&backend, &id, revision).await?;
    let asset = lesson
        .audio()
        .iter()
        .find(|asset| asset.url.rsplit('/').next() == Some(name.as_str()))
        .ok_or(AppError::NotFound)?;
    crate::recording::asset_response(config.root, asset.clone(), config.permits, headers).await
}
