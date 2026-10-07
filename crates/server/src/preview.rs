//! Imported immutable revisions are visible only to an authenticated operator.
use crate::{
    AppError,
    admin_auth::AdminAuth,
    learning::{field, one},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use brioche_course_contract::{
    Catalog, GradeRequest, GradeResult, Level, PreviewRelease, PublicLesson, Unit,
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
}
pub(crate) fn router<S: Clone + Send + Sync + 'static>(
    root: std::path::PathBuf,
    db: sea_orm::DatabaseConnection,
) -> Router<S> {
    Router::new()
        .route("/api/v1/operator/releases/{id}", get(release))
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
        .layer(axum::Extension(PreviewMedia {
            root,
            permits: std::sync::Arc::new(tokio::sync::Semaphore::new(2)),
        }))
        .with_state(Store { db })
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
        "SELECT manifest FROM content_releases WHERE id=$1",
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
        "SELECT r.public_document,EXISTS(SELECT 1 FROM content_withdrawals w WHERE (w.lesson_id,w.revision)=(r.lesson_id,r.revision)) AS withdrawn FROM release_entries e JOIN lesson_revisions r ON (r.lesson_id,r.revision)=(e.lesson_id,e.revision) WHERE e.release_id=$1 ORDER BY e.position",
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
                let lesson: PublicLesson = serde_json::from_value(field(&row, "public_document")?)
                    .map_err(|_| AppError::Unavailable)?;
                lesson.validate().map_err(|_| AppError::Unavailable)?;
                if lesson.id != entry.lesson_id
                    || lesson.revision != entry.revision
                    || lesson.level_id != level.id
                    || lesson.unit_id != unit.id
                {
                    return Err(AppError::Unavailable);
                }
                if field::<bool>(&row, "withdrawn")? {
                    withdrawn.push(lesson.id.clone());
                }
                lessons.push(lesson.summary());
            }
            units.push(Unit {
                id: unit.id,
                title_zh: unit.title_zh,
                lessons,
            });
        }
        levels.push(Level {
            id: level.id,
            label: level.label,
            units,
        });
    }
    if rows.next().is_some() {
        return Err(AppError::Unavailable);
    }
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(PreviewRelease {
        id,
        catalog: Catalog {
            levels,
            development_fixture: false,
        },
        withdrawn_lesson_ids: withdrawn,
    }))
}
async fn read(backend: &Store, id: &str, revision: u32) -> Result<PublicLesson, AppError> {
    if !valid_id(id) || revision == 0 || revision > i32::MAX as u32 {
        return Err(AppError::InvalidInput);
    }
    let row = one(&backend.db,
        "SELECT public_document,EXISTS(SELECT 1 FROM content_withdrawals w WHERE (w.lesson_id,w.revision)=(r.lesson_id,r.revision)) AS withdrawn FROM lesson_revisions r WHERE lesson_id=$1 AND revision=$2",
        vec![id.into(),(revision as i32).into()]).await?.ok_or(AppError::NotFound)?;
    if field::<bool>(&row, "withdrawn")? {
        return Err(AppError::Gone);
    }
    let mut lesson: PublicLesson = serde_json::from_value(field(&row, "public_document")?)
        .map_err(|_| AppError::Unavailable)?;
    lesson.validate().map_err(|_| AppError::Unavailable)?;
    if lesson.id != id || lesson.revision != revision {
        return Err(AppError::Unavailable);
    }
    for asset in &mut lesson.media {
        let name = asset
            .url
            .strip_prefix("/api/media/")
            .ok_or(AppError::Unavailable)?;
        if name.contains('/') || name.contains('?') || name.contains('#') {
            return Err(AppError::Unavailable);
        }
        asset.url = format!("/api/v1/operator/lessons/{id}/revisions/{revision}/media/{name}");
    }
    for asset in &mut lesson.audio {
        let name = asset
            .url
            .strip_prefix("/api/audio/")
            .ok_or(AppError::Unavailable)?;
        if name.contains('/') || name.contains('?') || name.contains('#') {
            return Err(AppError::Unavailable);
        }
        asset.url = format!("/api/v1/operator/lessons/{id}/revisions/{revision}/audio/{name}");
    }
    for recording in lesson
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
        )
    {
        let asset = lesson
            .audio
            .iter()
            .find(|asset| asset.asset_id == recording.asset.asset_id)
            .ok_or(AppError::Unavailable)?;
        recording.asset.url = asset.url.clone();
    }
    Ok(lesson)
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
        "SELECT public_document,server_document,EXISTS(SELECT 1 FROM content_withdrawals w WHERE (w.lesson_id,w.revision)=(r.lesson_id,r.revision)) AS withdrawn FROM lesson_revisions r WHERE lesson_id=$1 AND revision=$2",
        vec![id.clone().into(),(revision as i32).into()]).await?.ok_or(AppError::NotFound)?;
    if field::<bool>(&row, "withdrawn")? {
        return Err(AppError::Gone);
    }
    let lesson: PublicLesson = serde_json::from_value(field(&row, "public_document")?)
        .map_err(|_| AppError::Unavailable)?;
    lesson.validate().map_err(|_| AppError::Unavailable)?;
    if lesson.id != id || lesson.revision != revision {
        return Err(AppError::Unavailable);
    }
    let source: serde_json::Value = field(&row, "server_document")?;
    let grader =
        crate::grading::Grader::from_source(&lesson, &source).map_err(|_| AppError::Unavailable)?;
    let result = grader
        .grade(&lesson, &request.exercise_id, &request.answer)
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
    let lesson = read(&backend, &id, revision).await?;
    let asset = lesson
        .media
        .into_iter()
        .find(|asset| asset.url.rsplit('/').next() == Some(name.as_str()))
        .ok_or(AppError::NotFound)?;
    crate::media::asset_response(config.root, asset, config.permits).await
}

async fn audio(
    auth: AdminAuth,
    axum::Extension(config): axum::Extension<PreviewMedia>,
    State(backend): State<Store>,
    Path((id, revision, name)): Path<(String, u32, String)>,
    headers: axum::http::HeaderMap,
) -> Result<axum::response::Response, AppError> {
    auth.require_operator().await?;
    let lesson = read(&backend, &id, revision).await?;
    let asset = lesson
        .audio
        .into_iter()
        .find(|asset| asset.url.rsplit('/').next() == Some(name.as_str()))
        .ok_or(AppError::NotFound)?;
    crate::recording::asset_response(config.root, asset, config.permits, headers).await
}
