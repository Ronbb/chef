//! Bounded, private export of an immutable plan and its reviewed audio. Never publication.
use crate::{
    AppError,
    admin_auth::AdminAuth,
    identity::Backend,
    learning::{exec, field},
    speech_clips,
};
use axum::{
    Extension, Router,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue},
    response::Response,
    routing::get,
};
use sea_orm::{ConnectionTrait, IsolationLevel, TransactionTrait};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use unicode_segmentation::UnicodeSegmentation;
const MAX_EXPORT: usize = 128 * 1024 * 1024;
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
        .route("/api/v1/operator/speech-plans/{id}/export", get(export))
        .route(
            "/api/v1/operator/speech-plans/{id}/export-direct",
            get(export_direct),
        )
        .with_state(Store { db, product })
}
pub(crate) async fn snapshot_for_product(
    db: &impl ConnectionTrait,
    product: Option<crate::product::ProductId>,
    id: &str,
) -> Result<Value, AppError> {
    snapshot_policy(db, product, id, true).await
}
pub(crate) async fn snapshot_direct(
    db: &impl ConnectionTrait,
    id: &str,
) -> Result<Value, AppError> {
    snapshot_policy(db, None, id, false).await
}
async fn snapshot_policy(
    db: &impl ConnectionTrait,
    product: Option<crate::product::ProductId>,
    id: &str,
    reviewed: bool,
) -> Result<Value, AppError> {
    let plan = speech_clips::plan_for_product(db, product, id).await?;
    let requests = plan["requests"].as_object().ok_or(AppError::Unavailable)?;
    let mut clips = Vec::new();
    for key in requests.keys() {
        let row = speech_clips::latest_for_product(db, product, key)
            .await?
            .ok_or(AppError::Conflict)?;
        let clip = speech_clips::item(&row)?;
        if clip.status != "ready"
            || clip.accepted == Some(false)
            || (reviewed && clip.accepted != Some(true))
        {
            return Err(AppError::Conflict);
        }
        let result: Value = field::<Option<Value>>(&row, "result")?.ok_or(AppError::Unavailable)?;
        let text = requests[key]["parameters"]["input"]["text"]
            .as_str()
            .ok_or(AppError::Unavailable)?;
        let words: Vec<_> = text
            .unicode_word_indices()
            .map(|(byte, word)| {
                let start = text[..byte].chars().count();
                json!({"text":word,"start":start,"end":start+word.chars().count()})
            })
            .collect();
        let review = if reviewed {
            json!({"actorId":field::<i64>(&row,"review_actor")?,"reason":field::<String>(&row,"review_reason")?})
        } else {
            Value::Null
        };
        clips.push(
            json!({"words":words,"id":clip.id,"generationKey":key,"result":result,"review":review}),
        );
    }
    let mut result = json!({"schemaVersion":"1.0","planId":id,"plan":plan,"clips":clips});
    if !reviewed {
        result["kind"] = json!("brioche-speech-inputs");
        result["publicationPolicy"] = json!("owner-direct-publish");
        result["humanListeningAsserted"] = json!(false);
    }
    Ok(result)
}
fn append(builder: &mut tar::Builder<Vec<u8>>, name: &str, bytes: &[u8]) -> Result<(), AppError> {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o600);
    header.set_mtime(0);
    header.set_entry_type(tar::EntryType::Regular);
    builder
        .append_data(&mut header, name, bytes)
        .map_err(|_| AppError::Unavailable)
}
pub(crate) fn pack(root: &std::path::Path, mut manifest: Value) -> Result<Vec<u8>, AppError> {
    let mut builder = tar::Builder::new(Vec::new());
    let mut files = BTreeMap::<String, String>::new();
    for clip in manifest["clips"]
        .as_array_mut()
        .ok_or(AppError::Unavailable)?
    {
        let result = &clip["result"];
        let (sha, bytes) = crate::speech_media::read(root, result)?;
        let original = result["providerSha256"]
            .as_str()
            .ok_or(AppError::Unavailable)?
            .to_owned();
        let path = format!("media/{sha}.wav");
        if !files.contains_key(&sha) {
            if builder
                .get_ref()
                .len()
                .checked_add(bytes.len() + 1024)
                .is_none_or(|n| n > MAX_EXPORT)
            {
                return Err(AppError::InvalidInput);
            }
            append(&mut builder, &path, &bytes)?;
            files.insert(sha.clone(), path.clone());
        }
        let original_path = format!("media/{original}.wav");
        if !files.contains_key(&original) {
            let raw = crate::media::stored_bytes(root, &original, "wav")
                .map_err(|_| AppError::Unavailable)?;
            // Re-read originals used in archive after the normalized/raw pair validation.
            if crate::media::digest(&raw) != original {
                return Err(AppError::Unavailable);
            }
            if builder
                .get_ref()
                .len()
                .checked_add(raw.len() + 1024)
                .is_none_or(|n| n > MAX_EXPORT)
            {
                return Err(AppError::InvalidInput);
            }
            append(&mut builder, &original_path, &raw)?;
            files.insert(original.clone(), original_path.clone());
        }
        clip["file"] = json!(path);
        clip["providerFile"] = json!(original_path);
    }
    let bytes = serde_json::to_vec_pretty(&manifest).map_err(|_| AppError::Unavailable)?;
    if builder
        .get_ref()
        .len()
        .checked_add(bytes.len() + 2048)
        .is_none_or(|n| n > MAX_EXPORT)
    {
        return Err(AppError::InvalidInput);
    }
    append(&mut builder, "manifest.json", &bytes)?;
    builder.into_inner().map_err(|_| AppError::Unavailable)
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportQuery {}
async fn export(
    auth: AdminAuth,
    State(b): State<Store>,
    Path(id): Path<String>,
    Query(_query): Query<ExportQuery>,
    Extension(root): Extension<PathBuf>,
    Extension(permits): Extension<Arc<tokio::sync::Semaphore>>,
) -> Result<Response, AppError> {
    export_response(auth, b, id, root, permits, true).await
}
async fn export_direct(
    auth: AdminAuth,
    State(b): State<Store>,
    Path(id): Path<String>,
    Query(_query): Query<ExportQuery>,
    Extension(root): Extension<PathBuf>,
    Extension(permits): Extension<Arc<tokio::sync::Semaphore>>,
) -> Result<Response, AppError> {
    export_response(auth, b, id, root, permits, false).await
}
async fn export_response(
    auth: AdminAuth,
    b: Store,
    id: String,
    root: PathBuf,
    permits: Arc<tokio::sync::Semaphore>,
    reviewed: bool,
) -> Result<Response, AppError> {
    let operator = auth.require_operator().await?;
    let _permit = permits
        .try_acquire_owned()
        .map_err(|_| AppError::RateLimited)?;
    let bytes = export_policy(&b, &operator, id.clone(), root, reviewed).await?;
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        HeaderValue::from_static("application/x-tar"),
    );
    headers.insert(
        "content-disposition",
        HeaderValue::from_str(&format!("attachment; filename=\"speech-{id}.tar\""))
            .map_err(|_| AppError::Unavailable)?,
    );
    headers.insert(
        "cache-control",
        HeaderValue::from_static("private, no-store"),
    );
    let mut response = Response::new(Body::from(bytes));
    *response.headers_mut() = headers;
    Ok(response)
}

/// Local private delivery keeps the HTTP export's media and current-operator checks.
pub async fn export_for_actor(
    b: &Backend,
    actor: i64,
    id: String,
    root: PathBuf,
) -> Result<Vec<u8>, AppError> {
    let operator = crate::product_memberships::require_operator(
        &b.db,
        crate::product::ProductId::Brioche,
        actor,
    )
    .await?;
    export_policy(
        &Store {
            db: b.db.clone(),
            product: None,
        },
        &operator,
        id,
        root,
        true,
    )
    .await
}
/// Technical input delivery for the owner's direct publication workflow.
/// Ready clips are required; no human listening declaration is generated.
pub async fn export_direct_for_actor(
    b: &Backend,
    actor: i64,
    id: String,
    root: PathBuf,
) -> Result<Vec<u8>, AppError> {
    let operator = crate::product_memberships::require_operator(
        &b.db,
        crate::product::ProductId::Brioche,
        actor,
    )
    .await?;
    export_policy(
        &Store {
            db: b.db.clone(),
            product: None,
        },
        &operator,
        id,
        root,
        false,
    )
    .await
}
async fn export_policy(
    b: &Store,
    operator: &crate::product_memberships::Operator,
    id: String,
    root: PathBuf,
    reviewed: bool,
) -> Result<Vec<u8>, AppError> {
    if b.product.is_some_and(|product| product != operator.product) {
        return Err(AppError::Forbidden);
    }
    let tx = b.db.begin().await.map_err(|_| AppError::Unavailable)?;
    operator.lock_content(&tx).await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    let tx =
        b.db.begin_with_config(Some(IsolationLevel::RepeatableRead), None)
            .await
            .map_err(|_| AppError::Unavailable)?;
    let manifest = snapshot_policy(&tx, b.product, &id, reviewed).await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    let expected = manifest.clone();
    let bytes = tokio::task::spawn_blocking(move || pack(&root, manifest))
        .await
        .map_err(|_| AppError::Unavailable)??;
    // Do not deliver an archive built under a subsequently revoked role/review/source.
    let tx = b.db.begin().await.map_err(|_| AppError::Unavailable)?;
    operator.lock_content(&tx).await?;
    exec(
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
    .await?;
    if snapshot_policy(&tx, b.product, &id, reviewed).await? != expected {
        return Err(AppError::Conflict);
    }
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(bytes)
}
