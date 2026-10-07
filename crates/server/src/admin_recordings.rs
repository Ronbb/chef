//! Registered recordings stay private until a course explicitly publishes them.
use crate::{
    AppError,
    admin_auth::AdminAuth,
    learning::{field, one},
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::HeaderMap,
    routing::get,
};
use brioche_course_contract::{AdminAssetCursor, AdminRecording, AdminRecordings, AudioAsset};
use sea_orm::{ConnectionTrait, DbBackend, Statement};

#[derive(Clone)]
struct Store {
    db: sea_orm::DatabaseConnection,
}
pub(crate) fn router<S: Clone + Send + Sync + 'static>(
    db: sea_orm::DatabaseConnection,
) -> Router<S> {
    Router::new()
        .route(
            "/api/v1/operator/recordings",
            get(list)
                .post(upload)
                .layer(DefaultBodyLimit::max(34 * 1024 * 1024)),
        )
        .route(
            "/api/v1/operator/recordings/{id}/{revision}/file",
            get(file),
        )
        .with_state(Store { db })
}
async fn upload(
    auth: AdminAuth,
    State(backend): State<Store>,
    axum::Extension(root): axum::Extension<std::path::PathBuf>,
    axum::Extension(permits): axum::Extension<std::sync::Arc<tokio::sync::Semaphore>>,
    mut multipart: Multipart,
) -> Result<Json<AdminAssetCursor>, AppError> {
    let operator = auth.require_operator().await?;
    let _permit = permits
        .try_acquire_owned()
        .map_err(|_| AppError::RateLimited)?;
    let mut document = None;
    let mut bytes = None;
    while let Some(mut part) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::InvalidInput)?
    {
        let name = part.name().unwrap_or_default().to_owned();
        let limit = match name.as_str() {
            "document" if document.is_none() => 16 * 1024,
            "file" if bytes.is_none() => crate::audio::MAX_BYTES,
            _ => return Err(AppError::InvalidInput),
        };
        let mut data = Vec::new();
        while let Some(chunk) = part.chunk().await.map_err(|_| AppError::InvalidInput)? {
            if data.len().saturating_add(chunk.len()) > limit {
                return Err(AppError::InvalidInput);
            }
            data.extend_from_slice(&chunk);
        }
        if name == "document" {
            document = Some(data);
        } else {
            bytes = Some(data);
        }
    }
    let value = crate::author_json::parse_document(&document.ok_or(AppError::InvalidInput)?)
        .map_err(|_| AppError::InvalidInput)?;
    let request: brioche_course_contract::AdminRecordingUpload =
        serde_json::from_value(value).map_err(|_| AppError::InvalidInput)?;
    crate::admin::reason(&request.reason)?;
    let bytes = bytes
        .filter(|b| !b.is_empty())
        .ok_or(AppError::InvalidInput)?;
    let mut spec = crate::recording::AudioSpec {
        asset_id: request.asset_id.clone(),
        revision: request.revision,
        sha256: "0".repeat(64),
        mime_type: request.mime_type.clone(),
        duration_ms: 1,
        credit_zh: request.credit_zh,
        file: "upload".into(),
        status: "ready".into(),
        source: request.source,
        license: request.license,
        creator: request.creator,
        rights_confirmed: request.rights_confirmed,
    };
    let mut bundle = crate::recording::AudioBundle {
        schema_version: "1.0".into(),
        assets: vec![spec.clone()],
    };
    bundle
        .validate_author("web-upload")
        .map_err(|_| AppError::InvalidInput)?;
    let scratch_id = crate::learning::random_id()?;
    let mime = request.mime_type;
    let (scratch, sha, info) = tokio::task::spawn_blocking(move || -> Result<_, AppError> {
        let path = std::env::temp_dir().join(format!("brioche-recording-upload-{scratch_id}"));
        std::fs::create_dir(&path).map_err(|_| AppError::Unavailable)?;
        let scratch = crate::admin_assets::Scratch(path);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&scratch.0, std::fs::Permissions::from_mode(0o700))
                .map_err(|_| AppError::Unavailable)?;
        }
        std::fs::write(scratch.0.join("upload"), bytes).map_err(|_| AppError::Unavailable)?;
        let (data, info) = crate::audio::inspect_file(&scratch.0.join("upload"), &mime)
            .map_err(|_| AppError::InvalidInput)?;
        Ok((scratch, crate::media::digest(&data), info))
    })
    .await
    .map_err(|_| AppError::Unavailable)??;
    spec.sha256 = sha;
    spec.duration_ms = info.duration_ms;
    bundle.assets[0] = spec;
    crate::recording::import_operator_bundle(
        &backend.db,
        bundle,
        &scratch.0,
        &root,
        &operator,
        &request.reason,
    )
    .await
    .map_err(|e| match e.downcast_ref::<AppError>() {
        Some(AppError::Forbidden) => AppError::Forbidden,
        Some(AppError::Unauthorized) => AppError::Unauthorized,
        Some(AppError::Conflict) => AppError::Conflict,
        Some(_) => AppError::Unavailable,
        None => AppError::InvalidInput,
    })?;
    Ok(Json(AdminAssetCursor {
        asset_id: request.asset_id,
        revision: request.revision,
    }))
}
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecordingQuery {
    after_id: Option<String>,
    after_revision: Option<u32>,
    q: Option<String>,
}
impl RecordingQuery {
    fn validate(&self) -> Result<(), AppError> {
        match (&self.after_id, self.after_revision) {
            (None, None) => {}
            (Some(id), Some(revision)) if valid(id, revision) => {}
            _ => return Err(AppError::InvalidInput),
        }
        if self
            .q
            .as_ref()
            .is_some_and(|q| q.len() > 200 || q.chars().any(char::is_control))
        {
            return Err(AppError::InvalidInput);
        }
        Ok(())
    }
}
fn valid(id: &str, revision: u32) -> bool {
    brioche_course_contract::valid_content_id(id)
        && brioche_course_contract::valid_content_revision(revision)
}
async fn list(
    auth: AdminAuth,
    State(backend): State<Store>,
    Query(query): Query<RecordingQuery>,
) -> Result<Json<AdminRecordings>, AppError> {
    auth.require_operator().await?;
    query.validate()?;
    let rows=backend.db.query_all_raw(Statement::from_sql_and_values(DbBackend::Postgres, r#"
        SELECT descriptor,provenance->>'source' AS source,provenance->>'license' AS license,
        provenance->>'creator' AS creator,(provenance->>'rightsConfirmed')::boolean AS rights_confirmed,
        byte_size,sample_rate,channels FROM audio_assets
        WHERE (asset_id,revision)>($1,$2)
        AND ($3='' OR strpos(lower(asset_id),lower($3))>0 OR strpos(lower(descriptor->>'creditZh'),lower($3))>0)
        ORDER BY asset_id,revision LIMIT 21
    "#,vec![query.after_id.unwrap_or_default().into(),(query.after_revision.unwrap_or(0) as i32).into(),query.q.unwrap_or_default().trim().to_owned().into()])).await.map_err(|_|AppError::Unavailable)?;
    let more = rows.len() > 20;
    let items = rows
        .into_iter()
        .take(20)
        .map(|row| -> Result<AdminRecording, AppError> {
            let mut asset: AudioAsset = serde_json::from_value(field(&row, "descriptor")?)
                .map_err(|_| AppError::Unavailable)?;
            asset.url = format!(
                "/api/v1/operator/recordings/{}/{}/file",
                asset.asset_id, asset.revision
            );
            Ok(AdminRecording {
                asset,
                source: field(&row, "source")?,
                license: field(&row, "license")?,
                creator: field(&row, "creator")?,
                rights_confirmed: field(&row, "rights_confirmed")?,
                byte_size: u32::try_from(field::<i64>(&row, "byte_size")?)
                    .map_err(|_| AppError::Unavailable)?,
                sample_rate: u32::try_from(field::<i32>(&row, "sample_rate")?)
                    .map_err(|_| AppError::Unavailable)?,
                channels: u32::try_from(field::<i32>(&row, "channels")?)
                    .map_err(|_| AppError::Unavailable)?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let next = if more {
        items.last().map(|i| AdminAssetCursor {
            asset_id: i.asset.asset_id.clone(),
            revision: i.asset.revision,
        })
    } else {
        None
    };
    Ok(Json(AdminRecordings { items, next }))
}
async fn file(
    auth: AdminAuth,
    State(backend): State<Store>,
    Path((id, revision)): Path<(String, u32)>,
    axum::Extension(root): axum::Extension<std::path::PathBuf>,
    axum::Extension(permits): axum::Extension<std::sync::Arc<tokio::sync::Semaphore>>,
    headers: HeaderMap,
) -> Result<axum::response::Response, AppError> {
    auth.require_operator().await?;
    if !valid(&id, revision) {
        return Err(AppError::InvalidInput);
    }
    let row = one(
        &backend.db,
        "SELECT descriptor FROM audio_assets WHERE asset_id=$1 AND revision=$2",
        vec![id.into(), (revision as i32).into()],
    )
    .await?
    .ok_or(AppError::NotFound)?;
    let descriptor =
        serde_json::from_value(field(&row, "descriptor")?).map_err(|_| AppError::Unavailable)?;
    crate::recording::asset_response(root, descriptor, permits, headers).await
}
