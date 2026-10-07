//! Limited bearer delivery for an explicitly authorized fixed reference; never a public registry.
use crate::{
    AppError,
    identity::{AuthSession, Backend, require_operator},
    learning::{exec, field, one, owner},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::get,
};
use brioche_course_contract::{
    AdminReferenceGrant, AdminReferenceGrantRequest, AdminReferenceGrantResult,
    AdminReferenceGrants, AudioAsset, CharacterVoiceProfile,
};
use sea_orm::{ConnectionTrait, DbBackend, QueryResult, Statement, TransactionTrait};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::Semaphore;

pub fn router() -> Router<Backend> {
    Router::new()
        .route("/api/v1/operator/voice-references", get(list).post(issue))
        .route(
            "/api/v1/operator/voice-references/{id}/revoke",
            axum::routing::post(revoke),
        )
        .route("/api/v1/voice-references/{id}/{token}", get(download))
}
pub(crate) fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn grant(row: &QueryResult) -> Result<AdminReferenceGrant, AppError> {
    Ok(AdminReferenceGrant {
        id: field(row, "id")?,
        character_id: field(row, "character_id")?,
        character_revision: field::<i32>(row, "character_revision")? as u32,
        voice_revision: field::<i32>(row, "voice_revision")? as u32,
        asset_id: field(row, "asset_id")?,
        asset_revision: field::<i32>(row, "asset_revision")? as u32,
        model: field(row, "model")?,
        created_at: field(row, "created_at")?,
        expires_at: field(row, "expires_at")?,
        revoked: field(row, "revoked")?,
        read_count: field::<i64>(row, "read_count")? as u32,
    })
}
const PROJECTION: &str = r#"SELECT g.id,g.character_id,g.character_revision,g.voice_revision,g.asset_id,g.asset_revision,g.model,to_char(g.created_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"') AS created_at,to_char(g.expires_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"') AS expires_at,EXISTS(SELECT 1 FROM voice_reference_revocations r WHERE r.grant_id=g.id) AS revoked,(SELECT count(*) FROM voice_reference_reads a WHERE a.grant_id=g.id) AS read_count FROM voice_reference_grants g"#;
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Cursor {
    after_id: Option<String>,
}
async fn list(
    auth: AuthSession,
    State(backend): State<Backend>,
    Query(cursor): Query<Cursor>,
) -> Result<Json<AdminReferenceGrants>, AppError> {
    require_operator(&auth)?;
    let after = cursor.after_id.unwrap_or_default();
    if !after.is_empty() && !hex(&after, 32) {
        return Err(AppError::InvalidInput);
    }
    let rows = backend
        .db
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            format!("{PROJECTION} WHERE g.id>$1 ORDER BY g.id LIMIT 21"),
            vec![after.into()],
        ))
        .await
        .map_err(|_| AppError::Unavailable)?;
    let more = rows.len() > 20;
    let items = rows
        .iter()
        .take(20)
        .map(grant)
        .collect::<Result<Vec<_>, _>>()?;
    let next = if more {
        items.last().map(|g| g.id.clone())
    } else {
        None
    };
    Ok(Json(AdminReferenceGrants { items, next }))
}
pub(crate) async fn lock_operator(
    tx: &sea_orm::DatabaseTransaction,
    actor: i64,
) -> Result<(), AppError> {
    exec(
        tx,
        "SELECT pg_advisory_xact_lock(hashtextextended('account-admin',0))",
        vec![],
    )
    .await?;
    let row = one(tx, "SELECT role FROM users WHERE id=$1", vec![actor.into()])
        .await?
        .ok_or(AppError::Forbidden)?;
    if field::<String>(&row, "role")? != "operator" {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

/// Hash, full decode and provider limits are checked against actual bytes, not just client metadata.
pub(crate) async fn inspect(
    root: PathBuf,
    descriptor: AudioAsset,
    permits: Arc<Semaphore>,
) -> Result<Vec<u8>, AppError> {
    let permit = permits
        .try_acquire_owned()
        .map_err(|_| AppError::RateLimited)?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let extension =
            crate::audio::extension(&descriptor.mime_type).map_err(|_| AppError::InvalidInput)?;
        let bytes = crate::media::stored_bytes(&root, &descriptor.sha256, extension)
            .map_err(|_| AppError::Unavailable)?;
        if bytes.len() > 10_000_000 || crate::media::digest(&bytes) != descriptor.sha256 {
            return Err(AppError::InvalidInput);
        }
        let info = crate::audio::inspect(&bytes, &descriptor.mime_type)
            .map_err(|_| AppError::InvalidInput)?;
        if !(5000..=30000).contains(&info.duration_ms)
            || info.duration_ms != descriptor.duration_ms
            || info.sample_rate < 16000
        {
            return Err(AppError::InvalidInput);
        }
        if descriptor.mime_type == "audio/wav" {
            // inspect() already verifies all chunk boundaries; only PCM 16-bit is accepted by this provider.
            let mut offset = 12;
            let mut pcm16 = false;
            while offset + 8 <= bytes.len() {
                let n =
                    u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
                if &bytes[offset..offset + 4] == b"fmt " && n >= 16 {
                    pcm16 = u16::from_le_bytes(bytes[offset + 8..offset + 10].try_into().unwrap())
                        == 1
                        && u16::from_le_bytes(bytes[offset + 22..offset + 24].try_into().unwrap())
                            == 16;
                    break;
                }
                offset += 8 + n + (n % 2);
            }
            if !pcm16 {
                return Err(AppError::InvalidInput);
            }
        }
        Ok(bytes)
    })
    .await
    .map_err(|_| AppError::Unavailable)?
}

async fn issue(
    auth: AuthSession,
    State(backend): State<Backend>,
    axum::Extension(root): axum::Extension<PathBuf>,
    axum::Extension(permits): axum::Extension<Arc<Semaphore>>,
    Json(request): Json<AdminReferenceGrantRequest>,
) -> Result<Json<AdminReferenceGrantResult>, AppError> {
    require_operator(&auth)?;
    crate::admin::reason(&request.reason)?;
    if !request.single_speaker_confirmed
        || !brioche_course_contract::valid_content_id(&request.character_id)
        || !brioche_course_contract::valid_content_revision(request.character_revision)
        || !brioche_course_contract::valid_content_revision(request.voice_revision)
    {
        return Err(AppError::InvalidInput);
    }
    let actor = owner(&auth)?;
    let tx = backend
        .db
        .begin()
        .await
        .map_err(|_| AppError::Unavailable)?;
    lock_operator(&tx, actor).await?;
    let row=one(&tx,"SELECT profile FROM character_voice_profiles WHERE character_id=$1 AND character_revision=$2 AND revision=$3",
        vec![request.character_id.clone().into(),(request.character_revision as i32).into(),(request.voice_revision as i32).into()]).await?.ok_or(AppError::NotFound)?;
    let profile: CharacterVoiceProfile =
        serde_json::from_value(field(&row, "profile")?).map_err(|_| AppError::Unavailable)?;
    if profile.provider != "qwen"
        || profile.model != "qwen-audio-3.1-tts-flash"
        || profile.locale != "fr-FR"
    {
        return Err(AppError::InvalidInput);
    }
    let reference = profile.reference_audio.ok_or(AppError::InvalidInput)?;
    if reference.transcript.trim().is_empty() || reference.cloning_permission.trim().is_empty() {
        return Err(AppError::InvalidInput);
    }
    let row = one(
        &tx,
        "SELECT descriptor,provenance FROM audio_assets WHERE asset_id=$1 AND revision=$2",
        vec![
            reference.asset_id.clone().into(),
            (reference.revision as i32).into(),
        ],
    )
    .await?
    .ok_or(AppError::NotFound)?;
    let descriptor: AudioAsset =
        serde_json::from_value(field(&row, "descriptor")?).map_err(|_| AppError::Unavailable)?;
    let provenance: serde_json::Value = field(&row, "provenance")?;
    if provenance["rightsConfirmed"] != true
        || descriptor.asset_id != reference.asset_id
        || descriptor.revision != reference.revision
    {
        return Err(AppError::InvalidInput);
    }
    inspect(root, descriptor.clone(), permits).await?;
    let existing=one(&tx,"SELECT id FROM voice_reference_grants g WHERE character_id=$1 AND character_revision=$2 AND voice_revision=$3 AND expires_at>clock_timestamp() AND NOT EXISTS(SELECT 1 FROM voice_reference_revocations r WHERE r.grant_id=g.id) LIMIT 1",
        vec![request.character_id.clone().into(),(request.character_revision as i32).into(),(request.voice_revision as i32).into()]).await?;
    if existing.is_some() {
        return Err(AppError::Conflict);
    }
    let id = crate::learning::random_id()?;
    let token = format!(
        "{}{}",
        crate::learning::random_id()?,
        crate::learning::random_id()?
    );
    exec(&tx,"INSERT INTO voice_reference_grants(id,token_hash,character_id,character_revision,voice_revision,asset_id,asset_revision,descriptor,reference,actor_id,reason,model,single_speaker_confirmed,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,true,CURRENT_TIMESTAMP+interval '15 minutes')",
        vec![id.clone().into(),crate::media::digest(token.as_bytes()).into(),request.character_id.into(),(request.character_revision as i32).into(),(request.voice_revision as i32).into(),reference.asset_id.clone().into(),(reference.revision as i32).into(),serde_json::to_value(descriptor).map_err(|_|AppError::Unavailable)?.into(),serde_json::to_value(reference).map_err(|_|AppError::Unavailable)?.into(),actor.into(),request.reason.into(),profile.model.into()]).await?;
    let row = one(
        &tx,
        &format!("{PROJECTION} WHERE g.id=$1"),
        vec![id.clone().into()],
    )
    .await?
    .ok_or(AppError::Unavailable)?;
    let grant = grant(&row)?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(AdminReferenceGrantResult {
        grant,
        path: format!("/api/v1/voice-references/{id}/{token}"),
    }))
}

async fn revoke(
    auth: AuthSession,
    State(backend): State<Backend>,
    Path(id): Path<String>,
    Json(request): Json<brioche_course_contract::AdminRevokeTokenRequest>,
) -> Result<Json<AdminReferenceGrant>, AppError> {
    require_operator(&auth)?;
    if !hex(&id, 32) {
        return Err(AppError::InvalidInput);
    }
    crate::admin::reason(&request.reason)?;
    let actor = owner(&auth)?;
    let tx = backend
        .db
        .begin()
        .await
        .map_err(|_| AppError::Unavailable)?;
    lock_operator(&tx, actor).await?;
    one(
        &tx,
        "SELECT id FROM voice_reference_grants WHERE id=$1 FOR UPDATE",
        vec![id.clone().into()],
    )
    .await?
    .ok_or(AppError::NotFound)?;
    if one(
        &tx,
        "SELECT grant_id FROM voice_reference_revocations WHERE grant_id=$1",
        vec![id.clone().into()],
    )
    .await?
    .is_some()
    {
        return Err(AppError::Conflict);
    }
    exec(
        &tx,
        "INSERT INTO voice_reference_revocations(grant_id,actor_id,reason) VALUES($1,$2,$3)",
        vec![id.clone().into(), actor.into(), request.reason.into()],
    )
    .await?;
    let row = one(&tx, &format!("{PROJECTION} WHERE g.id=$1"), vec![id.into()])
        .await?
        .ok_or(AppError::Unavailable)?;
    let result = grant(&row)?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(result))
}

async fn download(
    State(backend): State<Backend>,
    Path((id, token)): Path<(String, String)>,
    axum::Extension(root): axum::Extension<PathBuf>,
    axum::Extension(permits): axum::Extension<Arc<Semaphore>>,
    headers: HeaderMap,
) -> Result<axum::response::Response, AppError> {
    if !hex(&id, 32) || !hex(&token, 64) {
        return Err(AppError::NotFound);
    }
    let token_hash = crate::media::digest(token.as_bytes());
    // Unknown bearer URLs must not serialize unrelated account administration.
    if one(&backend.db,"SELECT id FROM voice_reference_grants WHERE id=$1 AND token_hash=$2 AND expires_at>clock_timestamp()",
        vec![id.clone().into(),token_hash.clone().into()]).await?.is_none(){return Err(AppError::NotFound);}
    let tx = backend
        .db
        .begin()
        .await
        .map_err(|_| AppError::Unavailable)?;
    // Same lock order as role changes/revocation. No new fetch can pass after either commits.
    exec(
        &tx,
        "SELECT pg_advisory_xact_lock(hashtextextended('account-admin',0))",
        vec![],
    )
    .await?;
    let row=one(&tx,"SELECT descriptor,actor_id FROM voice_reference_grants g WHERE id=$1 AND token_hash=$2 AND expires_at>clock_timestamp() AND NOT EXISTS(SELECT 1 FROM voice_reference_revocations r WHERE r.grant_id=g.id) AND (SELECT count(*) FROM voice_reference_reads a WHERE a.grant_id=g.id)<32 FOR UPDATE",
        vec![id.clone().into(),token_hash.into()]).await?.ok_or(AppError::NotFound)?;
    let actor: i64 = field(&row, "actor_id")?;
    let user = one(
        &tx,
        "SELECT role FROM users WHERE id=$1",
        vec![actor.into()],
    )
    .await?
    .ok_or(AppError::NotFound)?;
    if field::<String>(&user, "role")? != "operator" {
        return Err(AppError::NotFound);
    }
    let descriptor: AudioAsset =
        serde_json::from_value(field(&row, "descriptor")?).map_err(|_| AppError::Unavailable)?;
    let bytes = inspect(root, descriptor.clone(), permits).await?;
    // Expiry is rechecked after file processing, with the database clock rather than transaction start.
    if one(
        &tx,
        "SELECT id FROM voice_reference_grants WHERE id=$1 AND expires_at>clock_timestamp()",
        vec![id.clone().into()],
    )
    .await?
    .is_none()
    {
        return Err(AppError::NotFound);
    }
    let response = crate::recording::bytes_response(
        descriptor.mime_type,
        format!("\"{}\"", descriptor.sha256),
        bytes,
        headers,
    )?;
    exec(
        &tx,
        "INSERT INTO voice_reference_reads(grant_id) VALUES($1)",
        vec![id.into()],
    )
    .await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(response)
}
