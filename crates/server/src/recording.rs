//! Immutable recording registry. Registration does not make a recording public.
use crate::{
    audio,
    learning::{exec, field, hash, one},
    media,
};
use anyhow::{Context, Result, ensure};
use brioche_course_contract::{AudioAsset, PublicLesson};
use sea_orm::{ConnectionTrait, DatabaseConnection, TransactionTrait};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Component, Path},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioBundle {
    pub schema_version: String,
    pub assets: Vec<AudioSpec>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioSpec {
    pub asset_id: String,
    pub revision: u32,
    pub sha256: String,
    pub mime_type: String,
    pub duration_ms: u32,
    pub credit_zh: String,
    pub file: String,
    pub status: String,
    pub source: String,
    pub license: String,
    pub creator: String,
    pub rights_confirmed: bool,
}

impl AudioBundle {
    pub fn validate_author(&self, actor: &str) -> Result<()> {
        ensure!(self.schema_version == "1.0", "/schemaVersion: expected 1.0");
        ensure!(
            (1..=500).contains(&self.assets.len()),
            "/assets: expected 1..500 recordings"
        );
        ensure!(media::text(actor), "/: invalid import actor");
        let mut ids = BTreeSet::new();
        for (index, spec) in self.assets.iter().enumerate() {
            let p = format!("/assets/{index}");
            ensure!(
                media::valid_id(&spec.asset_id),
                "{p}/assetId: invalid recording ID"
            );
            ensure!(
                brioche_course_contract::valid_content_revision(spec.revision),
                "{p}/revision: outside database range"
            );
            ensure!(
                ids.insert((&spec.asset_id, spec.revision)),
                "{p}/assetId: duplicate recording revision"
            );
            ensure!(
                spec.status == "ready",
                "{p}/status: recording must be ready"
            );
            ensure!(
                spec.rights_confirmed,
                "{p}/rightsConfirmed: confirmed rights required"
            );
            for (field, value) in [
                ("source", &spec.source),
                ("license", &spec.license),
                ("creator", &spec.creator),
                ("creditZh", &spec.credit_zh),
            ] {
                ensure!(
                    media::text(value),
                    "{p}/{field}: expected nonempty provenance text"
                );
            }
            ensure!(
                spec.sha256.len() == 64
                    && spec
                        .sha256
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "{p}/sha256: expected lowercase SHA-256"
            );
            ensure!(
                (1..=1_800_000).contains(&spec.duration_ms),
                "{p}/durationMs: duration out of range"
            );
            audio::extension(&spec.mime_type)
                .with_context(|| format!("{p}/mimeType: unsupported recording MIME"))?;
            ensure!(
                !spec.file.is_empty()
                    && Path::new(&spec.file)
                        .components()
                        .all(|c| matches!(c, Component::Normal(_))),
                "{p}/file: expected relative path without traversal"
            );
        }
        Ok(())
    }
}

/// Read-only metadata and complete file/decode checks, one bounded recording at a time.
/// Call from a blocking worker; registration and publication are separate operations.
pub fn check_bundle(bundle: &AudioBundle, source_root: &Path) -> Result<()> {
    bundle.validate_author("offline-check")?;
    let root = source_root
        .canonicalize()
        .context("/: source directory unavailable")?;
    ensure!(root.is_dir(), "/: expected source directory");
    for (index, spec) in bundle.assets.iter().enumerate() {
        inspect_source_file(&root, spec, index)?;
    }
    Ok(())
}

fn inspect_source_file(
    root: &Path,
    spec: &AudioSpec,
    index: usize,
) -> Result<(Vec<u8>, audio::RecordingInfo)> {
    let p = format!("/assets/{index}");
    let path = root
        .join(&spec.file)
        .canonicalize()
        .with_context(|| format!("{p}/file: recording file unavailable"))?;
    ensure!(
        path.starts_with(root),
        "{p}/file: recording escapes source directory"
    );
    let (bytes, info) = audio::inspect_file(&path, &spec.mime_type)
        .with_context(|| format!("{p}/file: invalid recording file"))?;
    ensure!(
        media::digest(&bytes) == spec.sha256,
        "{p}/sha256: recording hash mismatch"
    );
    ensure!(
        info.duration_ms == spec.duration_ms,
        "{p}/durationMs: recording duration does not match decoded frames"
    );
    Ok((bytes, info))
}

pub async fn import_bundle(
    db: &DatabaseConnection,
    bundle: AudioBundle,
    source_root: &Path,
    store: &Path,
    actor: &str,
) -> Result<()> {
    import_bundle_impl(db, bundle, source_root, store, actor, None).await
}
pub(crate) async fn import_operator_bundle(
    db: &DatabaseConnection,
    bundle: AudioBundle,
    source_root: &Path,
    store: &Path,
    actor: i64,
    reason: &str,
) -> Result<()> {
    crate::admin::reason(reason)?;
    import_bundle_impl(
        db,
        bundle,
        source_root,
        store,
        &format!("user:{actor}"),
        Some((actor, reason)),
    )
    .await
}
async fn import_bundle_impl(
    db: &DatabaseConnection,
    bundle: AudioBundle,
    source_root: &Path,
    store: &Path,
    actor: &str,
    operator: Option<(i64, &str)>,
) -> Result<()> {
    bundle.validate_author(actor)?;
    let source_root = source_root.canonicalize()?;
    let store = store.to_path_buf();
    let prepared_bundle = bundle.clone();
    let prepared_actor = actor.to_owned();
    let recordings = tokio::task::spawn_blocking(move || -> Result<Vec<_>> {
        prepare_recordings(&prepared_bundle, &store, &prepared_actor, |spec, index| {
            inspect_source_file(&source_root, spec, index)
        })
    })
    .await??;
    let tx = db.begin().await?;
    if let Some((actor, _)) = operator {
        crate::product_memberships::lock_operator(&tx, crate::product::ProductId::Brioche, actor)
            .await?;
    }
    one(
        &tx,
        "SELECT generation FROM content_state WHERE singleton FOR UPDATE",
        vec![],
    )
    .await
    .map_err(anyhow::Error::msg)?
    .context("content state missing")?;
    register_transaction(&tx, &bundle, recordings, actor, operator, false).await?;
    tx.commit().await?;
    Ok(())
}
pub(crate) type PreparedRecordings = Vec<(AudioSpec, AudioAsset, usize, audio::RecordingInfo)>;
/// Blocking file preparation shared by uploads, CLI bundles and assembled courses.
pub(crate) fn prepare_recordings(
    bundle: &AudioBundle,
    store: &Path,
    actor: &str,
    mut read: impl FnMut(&AudioSpec, usize) -> Result<(Vec<u8>, audio::RecordingInfo)>,
) -> Result<PreparedRecordings> {
    bundle.validate_author(actor)?;
    let mut result = Vec::new();
    for (index, spec) in bundle.assets.iter().enumerate() {
        let (bytes, info) = read(spec, index)?;
        ensure!(
            media::digest(&bytes) == spec.sha256 && info.duration_ms == spec.duration_ms,
            "/assets/{index}: recording does not match fixed metadata"
        );
        let ext = audio::extension(&spec.mime_type)?;
        media::store_file(store, &bytes, &spec.sha256, ext)?;
        let descriptor = AudioAsset {
            asset_id: spec.asset_id.clone(),
            revision: spec.revision,
            sha256: spec.sha256.clone(),
            mime_type: spec.mime_type.clone(),
            duration_ms: info.duration_ms,
            credit_zh: spec.credit_zh.clone(),
            url: format!("/api/audio/{}.{}", spec.sha256, ext),
        };
        result.push((spec.clone(), descriptor, bytes.len(), info));
    }
    Ok(result)
}
/// The caller must hold the content lock and reauthorize its operator.
pub(crate) async fn register_transaction(
    db: &impl ConnectionTrait,
    bundle: &AudioBundle,
    recordings: PreparedRecordings,
    actor: &str,
    operator: Option<(i64, &str)>,
    reuse_identical: bool,
) -> Result<()> {
    bundle.validate_author(actor)?;
    let bundle_hash = hash(bundle).map_err(anyhow::Error::msg)?;
    let mut reused = BTreeSet::new();
    for (index, (spec, _, _, _)) in recordings.iter().enumerate() {
        let existing = one(
            db,
            "SELECT descriptor,provenance,byte_size,sample_rate,channels FROM audio_assets WHERE asset_id=$1 AND revision=$2",
            vec![spec.asset_id.clone().into(), (spec.revision as i32).into()],
        )
        .await
        .map_err(anyhow::Error::msg)?;
        if let Some(row) = existing.as_ref().filter(|_| reuse_identical) {
            let (_, descriptor, size, info) = &recordings[index];
            let registered: AudioSpec = serde_json::from_value(field(row, "provenance")?)?;
            let mut candidate = spec.clone();
            // A local bundle path is transport metadata, not recording identity
            // or rights. Keep the original registered path and audit unchanged.
            candidate.file = registered.file.clone();
            if field::<serde_json::Value>(row, "descriptor")? != serde_json::to_value(descriptor)?
                || field::<serde_json::Value>(row, "provenance")?
                    != serde_json::to_value(candidate)?
                || field::<i64>(row, "byte_size")? != *size as i64
                || field::<i32>(row, "sample_rate")? != info.sample_rate as i32
                || field::<i32>(row, "channels")? != info.channels as i32
            {
                return Err(crate::AppError::Conflict.into());
            }
            reused.insert((spec.asset_id.clone(), spec.revision));
            continue;
        }
        if existing.is_some() && operator.is_some() {
            return Err(crate::AppError::Conflict.into());
        }
        ensure!(
            existing.is_none(),
            "/assets/{index}/revision: recording revision already registered"
        );
    }
    for (spec, descriptor, size, info) in recordings {
        if reused.contains(&(spec.asset_id.clone(), spec.revision)) {
            continue;
        }
        exec(db, "INSERT INTO audio_assets(asset_id,revision,descriptor,provenance,sha256,extension,byte_size,duration_ms,sample_rate,channels) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)", vec![
            spec.asset_id.clone().into(), (spec.revision as i32).into(), serde_json::to_value(descriptor)?.into(),
            serde_json::to_value(&spec)?.into(), spec.sha256.into(), audio::extension(&spec.mime_type)?.into(),
            (size as i64).into(), (info.duration_ms as i32).into(), (info.sample_rate as i32).into(), (info.channels as i32).into(),
        ]).await.map_err(anyhow::Error::msg)?;
    }
    exec(
        db,
        "INSERT INTO audio_import_audit(actor,bundle_hash,asset_count,actor_id,reason,target) VALUES($1,$2,$3,$4,$5,$6)",
        vec![
            actor.into(),
            bundle_hash.into(),
            (bundle.assets.len() as i32).into(),
            operator.map(|(actor,_)|actor).into(),
            operator.map(|(_,reason)|reason.to_owned()).into(),
            operator.map(|_|bundle.assets.iter().map(|s|format!("{} v{}",s.asset_id,s.revision)).collect::<Vec<_>>().join(", ")).into(),
        ],
    )
    .await
    .map_err(anyhow::Error::msg)?;
    Ok(())
}

pub fn source_audio_refs(source: &serde_json::Value) -> Result<Vec<media::AssetRef>> {
    media::source_refs(source, "audioRefs")
}

pub async fn hydrate_source<C: ConnectionTrait>(
    db: &C,
    mut source: serde_json::Value,
) -> Result<serde_json::Value> {
    let refs = source_audio_refs(&source)?;
    if source.get("audioRefs").is_none() {
        return Ok(source);
    }
    let mut descriptors = Vec::new();
    for (index, reference) in refs.into_iter().enumerate() {
        let row = one(
            db,
            "SELECT descriptor FROM audio_assets WHERE asset_id=$1 AND revision=$2",
            vec![
                reference.asset_id.into(),
                (reference.revision as i32).into(),
            ],
        )
        .await
        .map_err(anyhow::Error::msg)?
        .with_context(|| {
            format!("/audioRefs/{index}/revision: registered recording revision missing")
        })?;
        descriptors
            .push(field::<serde_json::Value>(&row, "descriptor").map_err(anyhow::Error::msg)?);
    }
    source["audio"] = serde_json::Value::Array(descriptors);
    Ok(source)
}

/// Rechecks immutable registration and stored bytes before staging/activating a release.
pub async fn validate_lesson<C: ConnectionTrait>(
    db: &C,
    lesson: &PublicLesson,
    root: &Path,
) -> Result<(), crate::AppError> {
    validate_lesson_detailed(db, lesson, root)
        .await
        .map_err(|error| error.runtime)
}
pub(crate) async fn validate_lesson_detailed<C: ConnectionTrait>(
    db: &C,
    lesson: &PublicLesson,
    root: &Path,
) -> Result<(), media::PublicationFailure> {
    for (index, asset) in lesson.audio.iter().enumerate() {
        let pointer = format!("/audio/{index}");
        let row = one(db, "SELECT descriptor,provenance,byte_size,sample_rate,channels FROM audio_assets WHERE asset_id=$1 AND revision=$2", vec![asset.asset_id.clone().into(), (asset.revision as i32).into()]).await?.ok_or_else(|| media::PublicationFailure::at(&format!("{pointer}/revision"), "recording revision is not registered"))?;
        if field::<serde_json::Value>(&row, "descriptor")?
            != serde_json::to_value(asset).map_err(|_| crate::AppError::Unavailable)?
        {
            return Err(media::PublicationFailure::at(
                &pointer,
                "recording descriptor does not match registered revision",
            ));
        }
        let spec: AudioSpec = serde_json::from_value(field(&row, "provenance")?)
            .map_err(|_| crate::AppError::Unavailable)?;
        AudioBundle {
            schema_version: "1.0".into(),
            assets: vec![spec],
        }
        .validate_author("publication-validation")
        .map_err(|_| {
            media::PublicationFailure::at(
                &pointer,
                "registered recording provenance fails publication validation",
            )
        })?;
        let expected_size = field::<i64>(&row, "byte_size")?;
        let expected_rate = field::<i32>(&row, "sample_rate")?;
        let expected_channels = field::<i32>(&row, "channels")?;
        let root = root.to_path_buf();
        let asset = asset.clone();
        tokio::task::spawn_blocking(move || -> std::result::Result<(), &'static str> {
            let ext =
                audio::extension(&asset.mime_type).map_err(|_| "unsupported recording format")?;
            let bytes = media::stored_bytes(&root, &asset.sha256, ext)
                .map_err(|_| "stored recording object is missing or unreadable")?;
            if media::digest(&bytes) != asset.sha256 || bytes.len() as i64 != expected_size {
                return Err("stored recording bytes do not match registered revision");
            }
            let info = audio::inspect(&bytes, &asset.mime_type)
                .map_err(|_| "stored recording cannot be decoded")?;
            if info.duration_ms != asset.duration_ms
                || info.sample_rate as i32 != expected_rate
                || info.channels as i32 != expected_channels
            {
                return Err("stored recording decode metadata does not match registered revision");
            }
            Ok(())
        })
        .await
        .map_err(|_| crate::AppError::Unavailable)?
        .map_err(|message| media::PublicationFailure::at(&pointer, message))?;
    }
    Ok(())
}

#[derive(Clone)]
struct AudioState {
    db: DatabaseConnection,
    root: std::path::PathBuf,
    permits: std::sync::Arc<tokio::sync::Semaphore>,
}
pub fn router(db: DatabaseConnection, root: std::path::PathBuf) -> axum::Router {
    axum::Router::new()
        .route("/api/audio/{name}", axum::routing::get(serve))
        .with_state(AudioState {
            db,
            root,
            permits: std::sync::Arc::new(tokio::sync::Semaphore::new(2)),
        })
}
async fn serve(
    axum::extract::State(state): axum::extract::State<AudioState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<axum::response::Response, crate::AppError> {
    let (sha, ext) = name.split_once('.').ok_or(crate::AppError::NotFound)?;
    if sha.len() != 64
        || !sha
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || !matches!(ext, "mp3" | "wav")
    {
        return Err(crate::AppError::NotFound);
    }
    let row = one(&state.db, "SELECT descriptor FROM audio_assets a WHERE sha256=$1 AND extension=$2 AND EXISTS(SELECT 1 FROM lesson_revisions r WHERE r.published AND NOT EXISTS(SELECT 1 FROM content_withdrawals w WHERE (w.lesson_id,w.revision)=(r.lesson_id,r.revision)) AND r.public_document->'audio' @> jsonb_build_array(jsonb_build_object('assetId',a.asset_id,'revision',a.revision))) LIMIT 1", vec![sha.into(),ext.into()]).await?.ok_or(crate::AppError::NotFound)?;
    let descriptor = serde_json::from_value(field(&row, "descriptor")?)
        .map_err(|_| crate::AppError::Unavailable)?;
    asset_response(state.root, descriptor, state.permits, headers).await
}

pub(crate) async fn asset_response(
    root: std::path::PathBuf,
    descriptor: AudioAsset,
    permits: std::sync::Arc<tokio::sync::Semaphore>,
    headers: axum::http::HeaderMap,
) -> Result<axum::response::Response, crate::AppError> {
    let ext = audio::extension(&descriptor.mime_type)
        .map_err(|_| crate::AppError::Unavailable)?
        .to_owned();
    let sha = descriptor.sha256;
    if sha.len() != 64
        || !sha
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(crate::AppError::Unavailable);
    }
    let etag = format!("\"{sha}\"");
    let permit = crate::media_read::acquire(permits).await?;
    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
        let _permit = permit;
        let bytes = media::stored_bytes(&root, &sha, &ext)?;
        ensure!(media::digest(&bytes) == sha, "recording object corrupt");
        Ok(bytes)
    })
    .await
    .map_err(|_| crate::AppError::Unavailable)?
    .map_err(|_| crate::AppError::Unavailable)?;
    bytes_response(descriptor.mime_type, etag, bytes, headers)
}

/// Common bounded byte/Range response after the caller has verified its authorization and hash.
pub(crate) fn bytes_response(
    mime: String,
    etag: String,
    bytes: Vec<u8>,
    headers: axum::http::HeaderMap,
) -> Result<axum::response::Response, crate::AppError> {
    use axum::{
        body::Body,
        http::{StatusCode, header},
        response::Response,
    };
    let size = bytes.len();
    let mut builder = Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::ETAG, &etag)
        .header("x-content-type-options", "nosniff")
        .header("cross-origin-resource-policy", "same-origin");
    let mut span = 0..size;
    if headers.contains_key(header::RANGE)
        && headers
            .get(header::IF_RANGE)
            .is_none_or(|value| value.as_bytes() == etag.as_bytes())
    {
        let values: Vec<_> = headers.get_all(header::RANGE).iter().collect();
        let range = if values.len() == 1 {
            values[0]
                .to_str()
                .ok()
                .and_then(|value| byte_range(value, size))
        } else {
            None
        };
        let Some(range) = range else {
            return builder
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::CONTENT_RANGE, format!("bytes */{size}"))
                .header(header::CONTENT_LENGTH, 0)
                .body(Body::empty())
                .map_err(|_| crate::AppError::Unavailable);
        };
        span = range;
        builder = builder.status(StatusCode::PARTIAL_CONTENT).header(
            header::CONTENT_RANGE,
            format!("bytes {}-{}/{size}", span.start, span.end - 1),
        );
    }
    let length = span.len();
    let body = if span.start == 0 && span.end == size {
        bytes
    } else {
        bytes[span].to_vec()
    };
    builder
        .header(header::CONTENT_LENGTH, length)
        .body(Body::from(body))
        .map_err(|_| crate::AppError::Unavailable)
}

fn byte_range(value: &str, size: usize) -> Option<std::ops::Range<usize>> {
    if value.len() > 100 || size == 0 {
        return None;
    }
    let value = value.strip_prefix("bytes=")?;
    let (start, end) = value.split_once('-')?;
    let number = |text: &str| -> Option<usize> {
        if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
            None
        } else {
            text.parse().ok()
        }
    };
    if start.is_empty() {
        let suffix = number(end)?;
        return (suffix > 0).then_some(size.saturating_sub(suffix)..size);
    }
    let start = number(start)?;
    if start >= size {
        return None;
    }
    let end = if end.is_empty() {
        size - 1
    } else {
        number(end)?.min(size - 1)
    };
    (end >= start).then_some(start..end + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn single_byte_ranges_are_bounded() {
        assert_eq!(byte_range("bytes=2-5", 10), Some(2..6));
        assert_eq!(byte_range("bytes=2-", 10), Some(2..10));
        assert_eq!(byte_range("bytes=-3", 10), Some(7..10));
        assert_eq!(byte_range("bytes=0-999", 10), Some(0..10));
        assert_eq!(byte_range("bytes=-999", 10), Some(0..10));
        for value in [
            "bytes=10-",
            "bytes=5-2",
            "bytes=-0",
            "bytes=",
            "bytes=1-2,3-4",
            "items=1-2",
            "bytes=+1-2",
            "bytes=0-999999999999999999999999999999",
        ] {
            assert_eq!(byte_range(value, 10), None, "{value}");
        }
    }
    #[test]
    fn refuses_missing_rights_and_bad_references() {
        for value in [
            serde_json::json!({"audioRefs":null}),
            serde_json::json!({"audioRefs":[{"assetId":"audio","revision":0}]}),
            serde_json::json!({"audioRefs":[{"assetId":"audio","revision":1},{"assetId":"audio","revision":2}]}),
            serde_json::json!({"audioRefs":[{"assetId":"audio","revision":1,"ignored":true}]}),
        ] {
            assert!(source_audio_refs(&value).is_err());
        }
        let mut bundle = AudioBundle {
            schema_version: "1.0".into(),
            assets: vec![AudioSpec {
                asset_id: "original".into(),
                revision: 1,
                sha256: "0".repeat(64),
                mime_type: "audio/mpeg".into(),
                duration_ms: 1000,
                credit_zh: "原创测试音".into(),
                file: "synthetic.mp3".into(),
                status: "ready".into(),
                source: "local synthesis".into(),
                license: "original protocol fixture".into(),
                creator: "test generator".into(),
                rights_confirmed: true,
            }],
        };
        assert!(bundle.validate_author("operator").is_ok());
        bundle.assets[0].rights_confirmed = false;
        assert!(bundle.validate_author("operator").is_err());
        bundle.assets[0].rights_confirmed = true;
        bundle.assets[0].file = "../escape.mp3".into();
        assert!(bundle.validate_author("operator").is_err());
    }
}
