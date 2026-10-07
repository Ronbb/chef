//! Private assembly artifacts and atomic draft imports. Neither operation publishes a course.
use crate::{
    AppError,
    identity::{AuthSession, Backend, require_operator},
    learning::{exec, field, one, owner},
    voice_references::{hex, lock_operator},
};
use axum::{
    Extension, Json, Router,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue},
    response::Response,
    routing::{get, post},
};
use brioche_course_contract::{
    AdminSpeechPackageImport, AdminSpeechPackageRequest, AdminSpeechPackageResult,
    AdminSpeechPackageResults, AudioAsset, AudioCue, AudioTrack, AudioWordRange, Block,
};
use sea_orm::{ConnectionTrait, DbBackend, IsolationLevel, Statement, TransactionTrait};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path as FilePath, PathBuf},
    sync::Arc,
};

const MAX_PACKAGE: usize = 128 * 1024 * 1024;
pub fn router() -> Router<Backend> {
    Router::new()
        .route(
            "/api/v1/operator/speech-alignments/{id}/package",
            post(export),
        )
        .route(
            "/api/v1/operator/speech-alignments/{id}/package/import",
            post(import),
        )
        .route(
            "/api/v1/operator/speech-alignments/{id}/packages",
            get(list),
        )
}
fn import_error(error: anyhow::Error) -> AppError {
    if error.is::<sea_orm::DbErr>() {
        return AppError::Unavailable;
    }
    if error.is::<crate::author_import::RevisionConflict>() {
        return AppError::Conflict;
    }
    match error.downcast_ref::<AppError>() {
        Some(AppError::Forbidden) => AppError::Forbidden,
        Some(AppError::Conflict) => AppError::Conflict,
        Some(_) => AppError::Unavailable,
        None => AppError::InvalidInput,
    }
}
async fn replay(
    db: &impl ConnectionTrait,
    alignment: &str,
    actor: i64,
    request: &AdminSpeechPackageImport,
) -> Result<Option<AdminSpeechPackageResult>, AppError> {
    let row = one(
        db,
        "SELECT actor_id,alignment_id,request,result FROM speech_package_imports WHERE id=$1",
        vec![request.id.clone().into()],
    )
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    if field::<i64>(&row, "actor_id")? != actor
        || field::<String>(&row, "alignment_id")? != alignment
        || field::<Value>(&row, "request")?
            != serde_json::to_value(request).map_err(|_| AppError::Unavailable)?
    {
        return Err(AppError::Conflict);
    }
    Ok(Some(
        serde_json::from_value(field(&row, "result")?).map_err(|_| AppError::Unavailable)?,
    ))
}
async fn import(
    auth: AuthSession,
    State(b): State<Backend>,
    Path(id): Path<String>,
    Extension(root): Extension<PathBuf>,
    Extension(permits): Extension<Arc<tokio::sync::Semaphore>>,
    Json(request): Json<AdminSpeechPackageImport>,
) -> Result<Json<AdminSpeechPackageResult>, AppError> {
    require_operator(&auth).await?;
    settings(&request.package)?;
    if !hex(&id, 32) || !hex(&request.id, 32) {
        return Err(AppError::InvalidInput);
    }
    let actor = owner(&auth)?;
    let tx =
        b.db.begin_with_config(Some(IsolationLevel::RepeatableRead), None)
            .await
            .map_err(|_| AppError::Unavailable)?;
    lock_operator(&tx, actor).await?;
    if let Some(result) = replay(&tx, &id, actor, &request).await? {
        tx.commit().await.map_err(|_| AppError::Unavailable)?;
        return Ok(Json(result));
    }
    let original = snapshot(&tx, &id, &request.package).await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    let _permit = permits
        .try_acquire_many_owned(2)
        .map_err(|_| AppError::RateLimited)?;
    let expected = original.clone();
    let config = request.package.clone();
    let (assembled, recordings) = tokio::task::spawn_blocking(move || -> Result<_, AppError> {
        let mut assembled = assemble(&root, original, &config, actor)?;
        let recordings = crate::recording::prepare_recordings(
            &assembled.bundle,
            &root,
            &format!("user:{actor}"),
            |spec, _| {
                let bytes = assembled
                    .files
                    .get(&spec.file)
                    .ok_or_else(|| anyhow::anyhow!("missing assembled recording"))?;
                let info = crate::audio::inspect(bytes, &spec.mime_type)?;
                Ok((bytes.clone(), info))
            },
        )
        .map_err(|_| AppError::Unavailable)?;
        assembled.files.clear();
        Ok((assembled, recordings))
    })
    .await
    .map_err(|_| AppError::Unavailable)??;
    let tx = b.db.begin().await.map_err(|_| AppError::Unavailable)?;
    lock_operator(&tx, actor).await?;
    exec(
        &tx,
        "SELECT singleton FROM content_state WHERE singleton FOR UPDATE",
        vec![],
    )
    .await?;
    if let Some(result) = replay(&tx, &id, actor, &request).await? {
        tx.commit().await.map_err(|_| AppError::Unavailable)?;
        return Ok(Json(result));
    }
    if snapshot(&tx, &id, &request.package).await? != expected {
        return Err(AppError::Conflict);
    }
    let actor_name = format!("user:{actor}");
    crate::recording::register_transaction(
        &tx,
        &assembled.bundle,
        recordings,
        &actor_name,
        Some((actor, &request.package.reason)),
        true,
    )
    .await
    .map_err(import_error)?;
    let imported = crate::author_import::import_transaction(
        &tx,
        assembled.source,
        &actor_name,
        &request.package.reason,
        false,
    )
    .await
    .map_err(import_error)?;
    let result = AdminSpeechPackageResult {
        id: request.id.clone(),
        lesson_id: imported.lesson_id,
        revision: imported.revision,
        recording_count: assembled.bundle.assets.len() as u32,
    };
    exec(&tx,"INSERT INTO speech_package_imports(id,alignment_id,actor_id,request,result,manifest,reason,lesson_id,revision) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
        vec![request.id.clone().into(),id.into(),actor.into(),serde_json::to_value(&request).map_err(|_|AppError::Unavailable)?.into(),
        serde_json::to_value(&result).map_err(|_|AppError::Unavailable)?.into(),assembled.manifest.into(),request.package.reason.into(),
        result.lesson_id.clone().into(),(result.revision as i32).into()]).await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(result))
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    after: Option<String>,
}
async fn list(
    auth: AuthSession,
    State(b): State<Backend>,
    Path(id): Path<String>,
    Query(query): Query<Cursor>,
) -> Result<Json<AdminSpeechPackageResults>, AppError> {
    require_operator(&auth).await?;
    let after = query.after.unwrap_or_default();
    if !hex(&id, 32) || (!after.is_empty() && !hex(&after, 32)) {
        return Err(AppError::InvalidInput);
    }
    one(
        &b.db,
        "SELECT id FROM speech_alignments WHERE id=$1",
        vec![id.clone().into()],
    )
    .await?
    .ok_or(AppError::NotFound)?;
    let rows=b.db.query_all_raw(Statement::from_sql_and_values(DbBackend::Postgres,
        "SELECT result FROM speech_package_imports WHERE alignment_id=$1 AND id>$2 ORDER BY id LIMIT 21",vec![id.into(),after.into()]))
        .await.map_err(|_|AppError::Unavailable)?;
    let items = rows
        .iter()
        .take(20)
        .map(|row| serde_json::from_value(field(row, "result")?).map_err(|_| AppError::Unavailable))
        .collect::<Result<Vec<AdminSpeechPackageResult>, AppError>>()?;
    let next = if rows.len() > 20 {
        items.last().map(|r| r.id.clone())
    } else {
        None
    };
    Ok(Json(AdminSpeechPackageResults { items, next }))
}
pub(crate) fn settings(r: &AdminSpeechPackageRequest) -> Result<(), AppError> {
    if !hex(&r.expected_report_hash, 64)
        || !brioche_course_contract::valid_content_revision(r.lesson_revision)
        || r.gap_ms > 1000
        || !r.rights_confirmed
    {
        return Err(AppError::InvalidInput);
    }
    for value in [&r.source, &r.license, &r.creator, &r.credit_zh, &r.reason] {
        crate::admin::reason(value)?;
    }
    if r.credit_zh.len() > 2000 {
        return Err(AppError::InvalidInput);
    }
    Ok(())
}
async fn snapshot(
    db: &impl ConnectionTrait,
    id: &str,
    r: &AdminSpeechPackageRequest,
) -> Result<Value, AppError> {
    let mut snapshot =
        crate::speech_alignments::package_snapshot(db, id, &r.expected_report_hash).await?;
    let plan = &snapshot["plan"];
    let lesson = plan["lessonId"].as_str().ok_or(AppError::Unavailable)?;
    let revision = plan["lessonRevision"]
        .as_u64()
        .and_then(|n| i32::try_from(n).ok())
        .ok_or(AppError::Unavailable)?;
    let row = one(
        db,
        "SELECT server_document FROM lesson_revisions WHERE lesson_id=$1 AND revision=$2",
        vec![lesson.into(), revision.into()],
    )
    .await?
    .ok_or(AppError::NotFound)?;
    let source: Value = field(&row, "server_document")?;
    let hash =
        crate::media::digest(&serde_json::to_vec(&source).map_err(|_| AppError::Unavailable)?);
    if plan["sourceHash"] != hash {
        return Err(AppError::Conflict);
    }
    let latest = one(
        db,
        "SELECT MAX(revision) AS revision FROM lesson_revisions WHERE lesson_id=$1",
        vec![lesson.into()],
    )
    .await?
    .ok_or(AppError::Unavailable)?;
    if i64::from(r.lesson_revision) <= i64::from(field::<i32>(&latest, "revision")?) {
        return Err(AppError::Conflict);
    }
    snapshot["source"] = source;
    Ok(snapshot)
}
async fn export(
    auth: AuthSession,
    State(b): State<Backend>,
    Path(id): Path<String>,
    Extension(root): Extension<PathBuf>,
    Extension(permits): Extension<Arc<tokio::sync::Semaphore>>,
    Json(request): Json<AdminSpeechPackageRequest>,
) -> Result<Response, AppError> {
    require_operator(&auth).await?;
    settings(&request)?;
    let actor = owner(&auth)?;
    // Assembly holds originals, decoded PCM and the archive in memory. Reserve
    // both shared media slots so two maximum-sized packages cannot overlap.
    let _permit = permits
        .try_acquire_many_owned(2)
        .map_err(|_| AppError::RateLimited)?;
    let tx =
        b.db.begin_with_config(Some(IsolationLevel::RepeatableRead), None)
            .await
            .map_err(|_| AppError::Unavailable)?;
    let original = snapshot(&tx, &id, &request).await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    let expected = original.clone();
    let config = request.clone();
    let bytes = tokio::task::spawn_blocking(move || pack(&root, original, &config, actor))
        .await
        .map_err(|_| AppError::Unavailable)??;
    let tx = b.db.begin().await.map_err(|_| AppError::Unavailable)?;
    lock_operator(&tx, actor).await?;
    exec(
        &tx,
        "SELECT singleton FROM content_state WHERE singleton FOR UPDATE",
        vec![],
    )
    .await?;
    if snapshot(&tx, &id, &request).await? != expected {
        return Err(AppError::Conflict);
    }
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        HeaderValue::from_static("application/x-tar"),
    );
    headers.insert(
        "cache-control",
        HeaderValue::from_static("private, no-store"),
    );
    headers.insert(
        "content-disposition",
        HeaderValue::from_str(&format!(
            "attachment; filename=\"speech-package-{id}-v{}.tar\"",
            request.lesson_revision
        ))
        .map_err(|_| AppError::Unavailable)?,
    );
    let mut response = Response::new(Body::from(bytes));
    *response.headers_mut() = headers;
    Ok(response)
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, AppError> {
    v[key].as_str().ok_or(AppError::Unavailable)
}
fn number(v: &Value, key: &str) -> Result<u32, AppError> {
    v[key]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .ok_or(AppError::Unavailable)
}
fn pcm(bytes: &[u8]) -> Result<(Vec<u8>, Vec<Value>), AppError> {
    let info = crate::audio::inspect(bytes, "audio/wav").map_err(|_| AppError::Unavailable)?;
    if info.sample_rate != 24000 || info.channels != 1 {
        return Err(AppError::Unavailable);
    }
    let mut offset = 12;
    let mut data = None;
    let mut tags = Vec::new();
    while offset < bytes.len() {
        let size = u32::from_le_bytes(
            bytes[offset + 4..offset + 8]
                .try_into()
                .map_err(|_| AppError::Unavailable)?,
        ) as usize;
        let chunk = &bytes[offset + 8..offset + 8 + size];
        match &bytes[offset..offset + 4] {
            b"fmt " => {
                if chunk.len() < 16
                    || chunk[..2] != 1u16.to_le_bytes()
                    || chunk[14..16] != 16u16.to_le_bytes()
                {
                    return Err(AppError::Unavailable);
                }
            }
            b"data" => data = Some(chunk.to_vec()),
            _ => tags.push(json!({"chunkId":bytes[offset..offset+4],"bytes":chunk})),
        }
        offset += 8 + size + size % 2;
    }
    Ok((data.ok_or(AppError::Unavailable)?, tags))
}
fn wave(data: &[u8], provenance: Value) -> Result<Vec<u8>, AppError> {
    let tag = serde_json::to_vec(&provenance).map_err(|_| AppError::Unavailable)?;
    if tag.len() > 60 * 1024 {
        return Err(AppError::InvalidInput);
    }
    let mut bytes = b"RIFF\0\0\0\0WAVE".to_vec();
    let mut format = Vec::new();
    format.extend(1u16.to_le_bytes());
    format.extend(1u16.to_le_bytes());
    format.extend(24000u32.to_le_bytes());
    format.extend(48000u32.to_le_bytes());
    format.extend(2u16.to_le_bytes());
    format.extend(16u16.to_le_bytes());
    for (id, body) in [
        (b"fmt ", format.as_slice()),
        (b"AIGC", tag.as_slice()),
        (b"data", data),
    ] {
        bytes.extend(id);
        bytes.extend((body.len() as u32).to_le_bytes());
        bytes.extend(body);
        if body.len() % 2 != 0 {
            bytes.push(0);
        }
    }
    if bytes.len() > crate::audio::MAX_BYTES {
        return Err(AppError::InvalidInput);
    }
    let length = (bytes.len() - 8) as u32;
    bytes[4..8].copy_from_slice(&length.to_le_bytes());
    crate::audio::inspect(&bytes, "audio/wav").map_err(|_| AppError::Unavailable)?;
    Ok(bytes)
}
fn add_file(
    files: &mut BTreeMap<String, Vec<u8>>,
    name: String,
    bytes: Vec<u8>,
) -> Result<(), AppError> {
    if let Some(existing) = files.get(&name) {
        if existing != &bytes {
            return Err(AppError::Unavailable);
        }
        return Ok(());
    }
    let total = files.values().map(Vec::len).sum::<usize>();
    if total
        .checked_add(bytes.len() + 2048)
        .is_none_or(|n| n > MAX_PACKAGE)
    {
        return Err(AppError::InvalidInput);
    }
    files.insert(name, bytes);
    Ok(())
}
fn asset(
    bytes: Vec<u8>,
    r: &AdminSpeechPackageRequest,
    files: &mut BTreeMap<String, Vec<u8>>,
    assets: &mut BTreeMap<String, Value>,
) -> Result<AudioAsset, AppError> {
    let sha = crate::media::digest(&bytes);
    let info = crate::audio::inspect(&bytes, "audio/wav").map_err(|_| AppError::Unavailable)?;
    let identity = json!({"sha256":sha,"source":r.source,"license":r.license,"creator":r.creator,"creditZh":r.credit_zh});
    let id = format!(
        "speech-{}",
        crate::media::digest(&serde_json::to_vec(&identity).map_err(|_| AppError::Unavailable)?)
    );
    let file = format!("recordings/{sha}.wav");
    let public = AudioAsset {
        asset_id: id.clone(),
        revision: 1,
        sha256: sha.clone(),
        mime_type: "audio/wav".into(),
        duration_ms: info.duration_ms,
        credit_zh: r.credit_zh.clone(),
        url: format!("/api/audio/{sha}.wav"),
    };
    assets.insert(id.clone(),json!({"assetId":id,"revision":1,"sha256":sha,"mimeType":"audio/wav","durationMs":info.duration_ms,"creditZh":r.credit_zh,"file":file,"status":"ready","source":r.source,"license":r.license,"creator":r.creator,"rightsConfirmed":true}));
    add_file(files, file, bytes)?;
    Ok(public)
}
struct Assembled {
    files: BTreeMap<String, Vec<u8>>,
    source: Value,
    bundle: crate::recording::AudioBundle,
    manifest: Value,
}
fn assemble(
    root: &FilePath,
    mut manifest: Value,
    r: &AdminSpeechPackageRequest,
    actor: i64,
) -> Result<Assembled, AppError> {
    settings(r)?;
    let mut source = manifest["source"].clone();
    let lesson = crate::project_source(source.clone()).map_err(|_| AppError::Unavailable)?;
    if r.lesson_revision <= lesson.revision {
        return Err(AppError::Conflict);
    }
    let mut files = BTreeMap::new();
    let mut recordings = BTreeMap::new();
    let mut pcm_clips = BTreeMap::new();
    let mut metadata = BTreeMap::new();
    let mut clip_index = BTreeMap::new();
    for clip in manifest["clips"]
        .as_array_mut()
        .ok_or(AppError::Unavailable)?
    {
        let key = text(clip, "generationKey")?.to_owned();
        let (sha, bytes) = crate::speech_media::read(root, &clip["result"])?;
        let (data, tags) = pcm(&bytes)?;
        pcm_clips.insert(key.clone(), data);
        metadata.insert(key.clone(), tags);
        let path = format!("original/{sha}.wav");
        add_file(&mut files, path.clone(), bytes.clone())?;
        clip["file"] = json!(path);
        let original = text(&clip["result"], "providerSha256")?.to_owned();
        let raw = crate::media::stored_bytes(root, &original, "wav")
            .map_err(|_| AppError::Unavailable)?;
        if crate::media::digest(&raw) != original {
            return Err(AppError::Unavailable);
        }
        let raw_path = format!("original/{original}.wav");
        add_file(&mut files, raw_path.clone(), raw)?;
        clip["providerFile"] = json!(raw_path);
        recordings.insert(key.clone(), bytes);
        clip_index.insert(key, clip.clone());
    }
    let targets = manifest["plan"]["targets"]
        .as_array()
        .ok_or(AppError::Unavailable)?;
    let mut used = BTreeSet::new();
    let mut assets = BTreeMap::new();
    let mut descriptors = BTreeMap::new();
    let mut tracks = Vec::new();
    for block in &lesson.blocks {
        let (id, entries): (&String, Vec<_>) = match block {
            Block::Dialogue { id, turns, .. } => {
                (id, turns.iter().map(|t| (&t.id, &t.segments)).collect())
            }
            Block::Article { id, paragraphs, .. } => (
                id,
                paragraphs.iter().map(|p| (&p.id, &p.segments)).collect(),
            ),
            _ => continue,
        };
        let mut data = Vec::new();
        let mut cues = Vec::new();
        let mut provenance = Vec::new();
        for (index, (entry, segments)) in entries.iter().enumerate() {
            let candidates: Vec<_> = targets
                .iter()
                .filter(|t| t["blockId"] == *id && t["entryId"] == **entry)
                .collect();
            if candidates.len() != 1 {
                return Err(AppError::Conflict);
            }
            let t = candidates[0];
            let pointer = text(t, "pointer")?;
            if !used.insert(pointer.to_owned()) {
                return Err(AppError::Conflict);
            }
            let expected_text: String = segments.iter().map(|s| s.text.as_str()).collect();
            if t["text"] != expected_text {
                return Err(AppError::Conflict);
            }
            let key = text(t, "generationKey")?;
            let clip = clip_index.get(key).ok_or(AppError::Unavailable)?;
            let payload = pcm_clips.get(key).ok_or(AppError::Unavailable)?;
            if index > 0 {
                data.resize(data.len() + r.gap_ms as usize * 48, 0);
            }
            let offset = u32::try_from(data.len() / 48).map_err(|_| AppError::InvalidInput)?;
            let duration = number(&clip["result"], "durationMs")?;
            if data
                .len()
                .checked_add(payload.len() + 48)
                .is_none_or(|n| n > crate::audio::MAX_BYTES)
            {
                return Err(AppError::InvalidInput);
            }
            data.extend(payload);
            data.resize(data.len().div_ceil(48) * 48, 0);
            cues.push(AudioCue {
                entry_id: (**entry).clone(),
                segment_id: None,
                word_range: None,
                start_ms: offset,
                end_ms: offset + duration,
            });
            let reviewed = clip["words"].as_array().ok_or(AppError::Unavailable)?;
            let mut groups = BTreeMap::<String, Vec<AudioCue>>::new();
            for word in t["words"].as_array().ok_or(AppError::Unavailable)? {
                let w = reviewed
                    .iter()
                    .find(|w| {
                        w["start"] == word["entryStart"]
                            && w["end"] == word["entryEnd"]
                            && w["text"] == word["text"]
                    })
                    .ok_or(AppError::Conflict)?;
                let start = number(w, "startMs")?;
                let end = number(w, "endMs")?;
                if start >= end || end > duration {
                    return Err(AppError::Conflict);
                }
                let segment = text(word, "segmentId")?.to_owned();
                groups.entry(segment.clone()).or_default().push(AudioCue {
                    entry_id: (**entry).clone(),
                    segment_id: Some(segment),
                    word_range: Some(AudioWordRange {
                        start: number(word, "segmentStart")?,
                        end: number(word, "segmentEnd")?,
                    }),
                    start_ms: offset + start,
                    end_ms: offset + end,
                });
            }
            for (segment, words) in groups {
                cues.push(AudioCue {
                    entry_id: (**entry).clone(),
                    segment_id: Some(segment),
                    word_range: None,
                    start_ms: words.first().ok_or(AppError::Unavailable)?.start_ms,
                    end_ms: words.last().ok_or(AppError::Unavailable)?.end_ms,
                });
                cues.extend(words);
            }
            provenance.push(json!({"clipId":clip["id"],"sourceSha256":clip["result"]["sha256"],"offsetMs":offset,"durationMs":duration,"originalMetadata":metadata[key]}));
        }
        let bytes = wave(
            &data,
            json!({"derivedFromAiSpeech":true,"operation":"brioche-pcm-concat-1","gapMs":r.gap_ms,"alignmentId":manifest["alignmentId"],"blockId":id,"sources":provenance}),
        )?;
        let audio = asset(bytes, r, &mut files, &mut assets)?;
        tracks.push(AudioTrack {
            block_id: id.clone(),
            asset_id: audio.asset_id.clone(),
            cues,
        });
        descriptors.insert(audio.asset_id.clone(), audio);
    }
    for t in targets.iter().filter(|t| t["blockId"].is_null()) {
        let pointer = text(t, "pointer")?;
        if !used.insert(pointer.to_owned()) || !pointer.starts_with("/knowledge/") {
            return Err(AppError::Conflict);
        }
        let original = source
            .pointer(pointer)
            .and_then(Value::as_str)
            .ok_or(AppError::Conflict)?;
        if t["text"] != original {
            return Err(AppError::Conflict);
        }
        let key = text(t, "generationKey")?;
        let audio = asset(
            recordings.get(key).ok_or(AppError::Unavailable)?.clone(),
            r,
            &mut files,
            &mut assets,
        )?;
        let parent = pointer.rsplit_once('/').ok_or(AppError::Unavailable)?.0;
        let node = source
            .pointer_mut(parent)
            .and_then(Value::as_object_mut)
            .ok_or(AppError::Conflict)?;
        node.insert(
            "recording".into(),
            json!({"asset":audio,"startMs":0,"endMs":audio.duration_ms}),
        );
        descriptors.insert(audio.asset_id.clone(), audio);
    }
    let expected = lesson
        .blocks
        .iter()
        .map(|b| match b {
            Block::Dialogue { turns, .. } => turns.len(),
            Block::Article { paragraphs, .. } => paragraphs.len(),
            _ => 0,
        })
        .sum::<usize>()
        + lesson.knowledge.vocabulary.len()
        + lesson
            .knowledge
            .grammar
            .iter()
            .map(|g| g.examples.len())
            .sum::<usize>();
    if used.len() != targets.len() || targets.len() != expected {
        return Err(AppError::Conflict);
    }
    source["revision"] = json!(r.lesson_revision);
    source["editorial"] =
        json!({"status":"draft","note":format!("录音包组装，待最终试听与审批。{}",r.reason)});
    source["audio"] = serde_json::to_value(descriptors.values().collect::<Vec<_>>())
        .map_err(|_| AppError::Unavailable)?;
    source["audioRefs"] = json!(
        descriptors
            .values()
            .map(|a| json!({"assetId":a.asset_id,"revision":a.revision}))
            .collect::<Vec<_>>()
    );
    source["audioTracks"] = serde_json::to_value(tracks).map_err(|_| AppError::Unavailable)?;
    let public = crate::project_source(source.clone()).map_err(|_| AppError::Unavailable)?;
    public.validate().map_err(|_| AppError::Conflict)?;
    crate::grading::Grader::from_author_source(&public, &source).map_err(|_| AppError::Conflict)?;
    let bundle = json!({"schemaVersion":"1.0","assets":assets.values().collect::<Vec<_>>()});
    let typed: crate::recording::AudioBundle =
        serde_json::from_value(bundle.clone()).map_err(|_| AppError::Unavailable)?;
    typed
        .validate_author(&format!("user:{actor}"))
        .map_err(|_| AppError::InvalidInput)?;
    manifest["assembly"] = json!({"compiler":"brioche-pcm-concat-1","request":r,"actorId":actor,
        "registrationRequired":true,"finalListeningRequired":true,"approvalRequired":true,"publicationRequired":true});
    add_file(
        &mut files,
        "lesson.json".into(),
        serde_json::to_vec_pretty(&source).map_err(|_| AppError::Unavailable)?,
    )?;
    add_file(
        &mut files,
        "audio-bundle.json".into(),
        serde_json::to_vec_pretty(&bundle).map_err(|_| AppError::Unavailable)?,
    )?;
    add_file(
        &mut files,
        "manifest.json".into(),
        serde_json::to_vec_pretty(&manifest).map_err(|_| AppError::Unavailable)?,
    )?;
    Ok(Assembled {
        files,
        source,
        bundle: typed,
        manifest,
    })
}
fn pack(
    root: &FilePath,
    manifest: Value,
    request: &AdminSpeechPackageRequest,
    actor: i64,
) -> Result<Vec<u8>, AppError> {
    let assembled = assemble(root, manifest, request, actor)?;
    archive_files(assembled.files)
}
/// Reuse PCM assembly and validators with an explicit owner policy, not a human review.
pub(crate) fn pack_automatic(
    root: &FilePath,
    manifest: Value,
    request: &AdminSpeechPackageRequest,
    actor: i64,
) -> Result<Vec<u8>, AppError> {
    let mut assembled = assemble(root, manifest, request, actor)?;
    assembled.source["editorial"] = json!({"status":"reviewed","note":format!("所有者授权直接发布；未声明人工试听或独立专家审校。{}",request.reason)});
    crate::author_source::editorial(&assembled.source).map_err(|_| AppError::InvalidInput)?;
    assembled.manifest["assembly"]["finalListeningRequired"] = json!(false);
    assembled.manifest["assembly"]["approvalRequired"] = json!(false);
    assembled.manifest["assembly"]["publicationPolicy"] = json!("owner-direct-publish");
    assembled.manifest["assembly"]["humanListeningAsserted"] = json!(false);
    assembled.files.insert(
        "lesson.json".into(),
        serde_json::to_vec_pretty(&assembled.source).map_err(|_| AppError::Unavailable)?,
    );
    assembled.files.insert(
        "manifest.json".into(),
        serde_json::to_vec_pretty(&assembled.manifest).map_err(|_| AppError::Unavailable)?,
    );
    archive_files(assembled.files)
}
fn archive_files(files: BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, AppError> {
    let mut tar = tar::Builder::new(Vec::new());
    for (name, bytes) in files {
        if tar
            .get_ref()
            .len()
            .checked_add(bytes.len() + 2048)
            .is_none_or(|n| n > MAX_PACKAGE)
        {
            return Err(AppError::InvalidInput);
        }
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o600);
        header.set_mtime(0);
        header.set_entry_type(tar::EntryType::Regular);
        tar.append_data(&mut header, name, bytes.as_slice())
            .map_err(|_| AppError::Unavailable)?;
    }
    tar.into_inner().map_err(|_| AppError::Unavailable)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn derived_pcm_keeps_samples_and_original_ai_metadata() {
        let data: Vec<_> = (0..240)
            .flat_map(|n| (n as i16 - 120).to_le_bytes())
            .collect();
        let first = wave(
            &data,
            json!({"Label":"1","ContentProducer":"Synthetic fixture"}),
        )
        .unwrap();
        let (samples, tags) = pcm(&first).unwrap();
        assert_eq!(samples, data);
        let mut joined = samples.clone();
        joined.extend(vec![0; 48 * 250]);
        joined.extend(samples.clone());
        let derived = wave(
            &joined,
            json!({"derivedFromAiSpeech":true,"operation":"brioche-pcm-concat-1","sources":tags}),
        )
        .unwrap();
        let (actual, metadata) = pcm(&derived).unwrap();
        assert_eq!(&actual[..data.len()], data);
        assert!(
            actual[data.len()..data.len() + 48 * 250]
                .iter()
                .all(|b| *b == 0)
        );
        assert_eq!(&actual[data.len() + 48 * 250..], data);
        assert_eq!(
            crate::audio::inspect(&derived, "audio/wav")
                .unwrap()
                .duration_ms,
            270
        );
        let embedded = metadata[0]["bytes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_u64().unwrap() as u8)
            .collect::<Vec<_>>();
        let label: Value = serde_json::from_slice(&embedded).unwrap();
        let preserved = label["sources"][0]["bytes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_u64().unwrap() as u8)
            .collect::<Vec<_>>();
        assert_eq!(
            serde_json::from_slice::<Value>(&preserved).unwrap()["ContentProducer"],
            "Synthetic fixture"
        );
    }
    #[test]
    fn assembly_never_silently_discards_overlarge_ai_metadata_or_changes_format() {
        assert!(wave(&[0; 48], json!({"label":"x".repeat(61*1024)})).is_err());
        let mut bytes = wave(&[0; 48], json!({"fixture":true})).unwrap();
        bytes[24..28].copy_from_slice(&16000u32.to_le_bytes());
        bytes[28..32].copy_from_slice(&32000u32.to_le_bytes());
        assert!(pcm(&bytes).is_err());
    }
}
