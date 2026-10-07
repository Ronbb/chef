//! Private, immutable voice directions bound to registered character snapshots.
use crate::{
    AppError,
    identity::{AuthSession, Backend, require_operator},
    learning::{exec, field, one, owner},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use brioche_course_contract::{
    AdminCharacterVoice, AdminCharacterVoiceRequest, AdminCharacterVoices, CharacterVoiceProfile,
};
use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait};

pub fn router() -> Router<Backend> {
    Router::new()
        .route(
            "/api/v1/operator/characters/revisions",
            axum::routing::post(append_character),
        )
        .route(
            "/api/v1/operator/characters/{id}/{revision}",
            get(character_version),
        )
        .route("/api/v1/operator/characters", get(list).post(append))
        .route(
            "/api/v1/operator/characters/{id}/{character_revision}/avatar",
            get(avatar),
        )
        .route(
            "/api/v1/operator/characters/{id}/{character_revision}/voices/{voice_revision}",
            get(version),
        )
}
async fn character_version(
    auth: AuthSession,
    State(backend): State<Backend>,
    Path((id, revision)): Path<(String, u32)>,
) -> Result<Json<AdminCharacterVoice>, AppError> {
    require_operator(&auth)?;
    id_revision(&id, revision)?;
    let row=one(&backend.db,"SELECT c.snapshot,c.avatar_revision,COALESCE(v.revision,0) AS voice_revision,v.profile FROM character_revisions c LEFT JOIN LATERAL (SELECT revision,profile FROM character_voice_profiles WHERE character_id=c.character_id AND character_revision=c.revision ORDER BY revision DESC LIMIT 1) v ON true WHERE c.character_id=$1 AND c.revision=$2",vec![id.into(),(revision as i32).into()]).await?.ok_or(AppError::NotFound)?;
    Ok(Json(item(&row)?))
}
async fn append_character(
    auth: AuthSession,
    State(backend): State<Backend>,
    axum::Extension(root): axum::Extension<std::path::PathBuf>,
    Json(request): Json<brioche_course_contract::AdminCharacterRequest>,
) -> Result<Json<AdminCharacterVoice>, AppError> {
    require_operator(&auth)?;
    crate::admin::reason(&request.reason)?;
    let revision = request
        .expected_revision
        .checked_add(1)
        .ok_or(AppError::InvalidInput)?;
    id_revision(&request.character_id, revision)?;
    id_revision(&request.avatar_id, request.avatar_revision)?;
    let character = brioche_course_contract::Character {
        character_id: request.character_id,
        revision,
        display_name: request.display_name,
        avatar_id: request.avatar_id,
        speech_locale: brioche_course_contract::CHARACTER_SPEECH_LOCALE.into(),
    };
    crate::media::import_operator_character(
        &backend.db,
        crate::media::CharacterSpec {
            snapshot: character.clone(),
            avatar_revision: request.avatar_revision,
        },
        &root,
        owner(&auth)?,
        request.expected_revision,
        &request.reason,
    )
    .await
    .map_err(|e| match e.downcast_ref::<AppError>() {
        Some(AppError::Forbidden) => AppError::Forbidden,
        Some(AppError::Conflict) => AppError::Conflict,
        Some(_) => AppError::Unavailable,
        None => AppError::InvalidInput,
    })?;
    Ok(Json(AdminCharacterVoice {
        character,
        avatar_revision: request.avatar_revision,
        voice_revision: 0,
        profile: None,
    }))
}
async fn avatar(
    auth: AuthSession,
    State(backend): State<Backend>,
    Path((id, revision)): Path<(String, u32)>,
    axum::Extension(root): axum::Extension<std::path::PathBuf>,
    axum::Extension(permits): axum::Extension<std::sync::Arc<tokio::sync::Semaphore>>,
) -> Result<axum::response::Response, AppError> {
    require_operator(&auth)?;
    id_revision(&id, revision)?;
    let row=one(&backend.db,"SELECT m.descriptor FROM character_revisions c JOIN media_assets m ON m.asset_id=c.avatar_id AND m.revision=c.avatar_revision WHERE c.character_id=$1 AND c.revision=$2",vec![id.into(),(revision as i32).into()]).await?.ok_or(AppError::NotFound)?;
    let descriptor =
        serde_json::from_value(field(&row, "descriptor")?).map_err(|_| AppError::Unavailable)?;
    crate::media::asset_response(root, descriptor, permits).await
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Cursor {
    after_id: Option<String>,
}
fn id_revision(id: &str, revision: u32) -> Result<(), AppError> {
    if !brioche_course_contract::valid_content_id(id)
        || !brioche_course_contract::valid_content_revision(revision)
    {
        return Err(AppError::InvalidInput);
    }
    Ok(())
}
pub fn validate(profile: &CharacterVoiceProfile) -> Result<(), AppError> {
    for value in [
        &profile.personality,
        &profile.speaking_style,
        &profile.default_emotion,
    ] {
        if value.trim().is_empty() || value.len() > 2000 || value.chars().any(char::is_control) {
            return Err(AppError::InvalidInput);
        }
    }
    // Store future provider/model combinations, but the generator must explicitly support them.
    for value in [&profile.provider, &profile.model, &profile.voice_id] {
        if value.is_empty()
            || value.len() > 200
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        {
            return Err(AppError::InvalidInput);
        }
    }
    if profile.locale != "fr-FR"
        || !profile.rate.is_finite()
        || !(0.5..=2.0).contains(&profile.rate)
        || !["system", "cloned"].contains(&profile.voice_kind.as_str())
    {
        return Err(AppError::InvalidInput);
    }
    if let Some(reference) = &profile.reference_audio {
        id_revision(&reference.asset_id, reference.revision)?;
        for value in [&reference.transcript, &reference.cloning_permission] {
            if value.trim().is_empty() || value.len() > 4000 || value.chars().any(char::is_control)
            {
                return Err(AppError::InvalidInput);
            }
        }
    } else if profile.voice_kind == "cloned" {
        return Err(AppError::InvalidInput);
    }
    Ok(())
}
fn item(row: &sea_orm::QueryResult) -> Result<AdminCharacterVoice, AppError> {
    let profile: Option<serde_json::Value> = field(row, "profile")?;
    Ok(AdminCharacterVoice {
        character: serde_json::from_value(field(row, "snapshot")?)
            .map_err(|_| AppError::Unavailable)?,
        avatar_revision: field::<i32>(row, "avatar_revision")? as u32,
        voice_revision: field::<i32>(row, "voice_revision")? as u32,
        profile: profile
            .map(serde_json::from_value)
            .transpose()
            .map_err(|_| AppError::Unavailable)?,
    })
}
async fn list(
    auth: AuthSession,
    State(backend): State<Backend>,
    Query(cursor): Query<Cursor>,
) -> Result<Json<AdminCharacterVoices>, AppError> {
    require_operator(&auth)?;
    let after = cursor.after_id.unwrap_or_default();
    if !after.is_empty() && !brioche_course_contract::valid_content_id(&after) {
        return Err(AppError::InvalidInput);
    }
    let rows=backend.db.query_all_raw(Statement::from_sql_and_values(DbBackend::Postgres,r#"
      SELECT c.snapshot,c.avatar_revision,COALESCE(v.revision,0) AS voice_revision,v.profile
      FROM (SELECT DISTINCT ON(character_id) * FROM character_revisions WHERE character_id>$1 ORDER BY character_id,revision DESC) c
      LEFT JOIN LATERAL (SELECT revision,profile FROM character_voice_profiles WHERE character_id=c.character_id AND character_revision=c.revision ORDER BY revision DESC LIMIT 1) v ON true
      ORDER BY c.character_id LIMIT 21
    "#,vec![after.into()])).await.map_err(|_|AppError::Unavailable)?;
    let more = rows.len() > 20;
    let items: Vec<_> = rows
        .into_iter()
        .take(20)
        .map(|row| item(&row))
        .collect::<Result<_, _>>()?;
    let next_id = if more {
        items.last().map(|i| i.character.character_id.clone())
    } else {
        None
    };
    Ok(Json(AdminCharacterVoices { items, next_id }))
}
async fn version(
    auth: AuthSession,
    State(backend): State<Backend>,
    Path((id, character_revision, voice_revision)): Path<(String, u32, u32)>,
) -> Result<Json<AdminCharacterVoice>, AppError> {
    require_operator(&auth)?;
    id_revision(&id, character_revision)?;
    id_revision(&id, voice_revision)?;
    let row=one(&backend.db,"SELECT c.snapshot,c.avatar_revision,v.revision AS voice_revision,v.profile FROM character_revisions c JOIN character_voice_profiles v ON v.character_id=c.character_id AND v.character_revision=c.revision WHERE c.character_id=$1 AND c.revision=$2 AND v.revision=$3",vec![id.into(),(character_revision as i32).into(),(voice_revision as i32).into()]).await?.ok_or(AppError::NotFound)?;
    Ok(Json(item(&row)?))
}
async fn append(
    auth: AuthSession,
    State(backend): State<Backend>,
    Json(request): Json<AdminCharacterVoiceRequest>,
) -> Result<Json<AdminCharacterVoice>, AppError> {
    require_operator(&auth)?;
    Ok(Json(
        append_profile(&backend.db, owner(&auth)?, request).await?,
    ))
}
pub async fn append_profile(
    db: &sea_orm::DatabaseConnection,
    actor: i64,
    request: AdminCharacterVoiceRequest,
) -> Result<AdminCharacterVoice, AppError> {
    let tx = db.begin().await.map_err(|_| AppError::Unavailable)?;
    let result = append_profile_in(&tx, actor, request).await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(result)
}
pub(crate) async fn append_profile_in(
    tx: &impl ConnectionTrait,
    actor: i64,
    request: AdminCharacterVoiceRequest,
) -> Result<AdminCharacterVoice, AppError> {
    crate::admin::reason(&request.reason)?;
    id_revision(&request.character_id, request.character_revision)?;
    validate(&request.profile)?;
    let next = request
        .expected_voice_revision
        .checked_add(1)
        .ok_or(AppError::InvalidInput)?;
    id_revision(&request.character_id, next)?;
    // Serializes profiles and rechecks permissions in the same lock as role changes.
    exec(
        tx,
        "SELECT pg_advisory_xact_lock(hashtextextended('account-admin',0))",
        vec![],
    )
    .await?;
    let operator = one(tx, "SELECT role FROM users WHERE id=$1", vec![actor.into()])
        .await?
        .ok_or(AppError::Forbidden)?;
    if field::<String>(&operator, "role")? != "operator" {
        return Err(AppError::Forbidden);
    }
    let row=one(tx,"SELECT snapshot,avatar_revision FROM character_revisions WHERE character_id=$1 AND revision=$2",vec![request.character_id.clone().into(),(request.character_revision as i32).into()]).await?.ok_or(AppError::NotFound)?;
    let latest=one(tx,"SELECT COALESCE(max(revision),0) AS revision FROM character_voice_profiles WHERE character_id=$1 AND character_revision=$2",vec![request.character_id.clone().into(),(request.character_revision as i32).into()]).await?.ok_or(AppError::Unavailable)?;
    if field::<i32>(&latest, "revision")? as u32 != request.expected_voice_revision {
        return Err(AppError::Conflict);
    }
    if let Some(reference) = &request.profile.reference_audio {
        let audio = one(
            tx,
            "SELECT duration_ms FROM audio_assets WHERE asset_id=$1 AND revision=$2",
            vec![
                reference.asset_id.clone().into(),
                (reference.revision as i32).into(),
            ],
        )
        .await?
        .ok_or(AppError::NotFound)?;
        // Reference registration alone does not prove cloning consent or provider enrollment.
        if field::<i32>(&audio, "duration_ms")? > 30000 {
            return Err(AppError::InvalidInput);
        }
    }
    exec(tx,"INSERT INTO character_voice_profiles(character_id,character_revision,revision,profile,actor_id,reason) VALUES($1,$2,$3,$4,$5,$6)",vec![request.character_id.into(),(request.character_revision as i32).into(),(next as i32).into(),serde_json::to_value(&request.profile).map_err(|_|AppError::InvalidInput)?.into(),actor.into(),request.reason.into()]).await?;
    let result = AdminCharacterVoice {
        character: serde_json::from_value(field(&row, "snapshot")?)
            .map_err(|_| AppError::Unavailable)?,
        avatar_revision: field::<i32>(&row, "avatar_revision")? as u32,
        voice_revision: next,
        profile: Some(request.profile),
    };
    Ok(result)
}
