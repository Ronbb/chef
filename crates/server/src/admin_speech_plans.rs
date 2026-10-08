//! Immutable private course plans. Saving a plan never initiates a paid provider call.
use crate::{
    AppError,
    admin_auth::AdminAuth,
    identity::Backend,
    learning::{exec, field, one, product_filter},
    voice_references::hex,
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use brioche_course_contract::{
    AdminCharacterVoice, AdminSpeechOptions, AdminSpeechPlan, AdminSpeechPlanRequest,
    AdminSpeechPlans, AdminSpeechPreviewRequest, AdminSpeechTarget,
};
use sea_orm::{
    ConnectionTrait, DbBackend, IsolationLevel, QueryResult, Statement, TransactionTrait,
};
use serde_json::Value;

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
        .route("/api/v1/operator/speech-plans", get(list).post(save))
        .route("/api/v1/operator/speech-plans/preview", post(preview))
        .route("/api/v1/operator/speech-plans/{id}", get(read))
        .route(
            "/api/v1/operator/lessons/{id}/revisions/{revision}/speech-options",
            get(options),
        )
        .with_state(Store { db, product })
}
fn lesson_key(id: &str, revision: u32) -> Result<(), AppError> {
    if !brioche_course_contract::valid_content_id(id)
        || !brioche_course_contract::valid_content_revision(revision)
    {
        return Err(AppError::InvalidInput);
    }
    Ok(())
}
async fn source(
    db: &impl ConnectionTrait,
    product: Option<crate::product::ProductId>,
    id: &str,
    revision: u32,
) -> Result<Value, AppError> {
    lesson_key(id, revision)?;
    let row = one(db, &format!("SELECT server_document FROM lesson_revisions r WHERE lesson_id=$1 AND revision=$2{} AND NOT EXISTS(SELECT 1 FROM content_withdrawals w WHERE (w.lesson_id,w.revision)=(r.lesson_id,r.revision){})",product_filter(product,"r.product_id"),if product.is_some(){" AND w.product_id=r.product_id"}else{""}), vec![id.into(), (revision as i32).into()]).await?.ok_or(AppError::NotFound)?;
    field(&row, "server_document")
}
fn voice(row: &QueryResult) -> Result<AdminCharacterVoice, AppError> {
    Ok(AdminCharacterVoice {
        character: serde_json::from_value(field(row, "snapshot")?)
            .map_err(|_| AppError::Unavailable)?,
        avatar_revision: field::<i32>(row, "avatar_revision")? as u32,
        voice_revision: field::<i32>(row, "voice_revision")? as u32,
        profile: field::<Option<Value>>(row, "profile")?
            .map(serde_json::from_value)
            .transpose()
            .map_err(|_| AppError::Unavailable)?,
    })
}
async fn options(
    auth: AdminAuth,
    State(b): State<Store>,
    Path((id, revision)): Path<(String, u32)>,
    Query(_query): Query<ItemQuery>,
) -> Result<Json<AdminSpeechOptions>, AppError> {
    auth.require_operator().await?;
    let tx =
        b.db.begin_with_config(Some(IsolationLevel::RepeatableRead), None)
            .await
            .map_err(|_| AppError::Unavailable)?;
    let src = source(&tx, b.product, &id, revision).await?;
    let lesson = crate::project_source(src).map_err(|_| AppError::Unavailable)?;
    let mut voices = Vec::new();
    for c in &lesson.cast {
        let row=one(&tx,&format!("SELECT c.snapshot,c.avatar_revision,COALESCE(v.revision,0) AS voice_revision,v.profile FROM character_revisions c LEFT JOIN LATERAL (SELECT revision,profile FROM character_voice_profiles WHERE character_id=c.character_id AND character_revision=c.revision{} ORDER BY revision DESC LIMIT 1) v ON true WHERE c.character_id=$1 AND c.revision=$2{}",if b.product.is_some(){" AND product_id=c.product_id"}else{""},product_filter(b.product,"c.product_id")),vec![c.character_id.clone().into(),(c.revision as i32).into()]).await?.ok_or(AppError::Unavailable)?;
        voices.push(voice(&row)?);
    }
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(AdminSpeechOptions { lesson, voices }))
}
async fn compile(
    db: &impl ConnectionTrait,
    product: Option<crate::product::ProductId>,
    request: &AdminSpeechPreviewRequest,
) -> Result<(crate::speech_plan::Plan, AdminSpeechPlan), AppError> {
    if request.selection.voices.is_empty() || request.selection.voices.len() > 100 {
        return Err(AppError::InvalidInput);
    }
    let src = source(db, product, &request.lesson_id, request.lesson_revision).await?;
    let lesson = crate::project_source(src.clone()).map_err(|_| AppError::Unavailable)?;
    let mut items = Vec::new();
    for k in &request.selection.voices {
        lesson_key(&k.character_id, k.character_revision)?;
        if !brioche_course_contract::valid_content_revision(k.voice_revision) {
            return Err(AppError::InvalidInput);
        }
        let row=one(db,&format!("SELECT c.snapshot,c.avatar_revision,v.revision AS voice_revision,v.profile FROM character_revisions c JOIN character_voice_profiles v ON v.character_id=c.character_id AND v.character_revision=c.revision{} WHERE c.character_id=$1 AND c.revision=$2 AND v.revision=$3{}",if product.is_some(){" AND v.product_id=c.product_id"}else{""},product_filter(product,"c.product_id")),vec![k.character_id.clone().into(),(k.character_revision as i32).into(),(k.voice_revision as i32).into()]).await?.ok_or(AppError::InvalidInput)?;
        items.push(voice(&row)?);
    }
    let plan = crate::speech_plan::compile(
        &lesson,
        &src,
        &crate::speech_plan::Config {
            items: items.clone(),
            knowledge_narrator: request.selection.knowledge_narrator.clone(),
            emotions: request.selection.emotions.clone(),
        },
    )
    .map_err(|_| AppError::InvalidInput)?;
    let view = AdminSpeechPlan {
        id: None,
        lesson_id: plan.lesson_id.clone(),
        lesson_revision: plan.lesson_revision,
        source_hash: plan.source_hash.clone(),
        plan_hash: plan.plan_hash.clone(),
        request_count: plan.requests.len() as u32,
        total_request_characters: plan.total_request_characters as u32,
        selection: request.selection.clone(),
        voices: items,
        targets: plan
            .targets
            .iter()
            .map(|t| AdminSpeechTarget {
                pointer: t.pointer.clone(),
                entry_id: t.entry_id.clone(),
                text: t.text.clone(),
                voice: t.voice.clone(),
                emotion: t.emotion.clone(),
                generation_key: t.generation_key.clone(),
                word_count: t.words.len() as u32,
            })
            .collect(),
        created_at: None,
    };
    Ok((plan, view))
}
async fn preview(
    auth: AdminAuth,
    State(b): State<Store>,
    Json(request): Json<AdminSpeechPreviewRequest>,
) -> Result<Json<AdminSpeechPlan>, AppError> {
    let operator = auth.require_operator().await?;
    Ok(Json(preview_authorized(&b, &operator, &request).await?))
}

/// Compile a private plan from registered, fixed versions without generating audio.
pub async fn preview_for_actor(
    b: &Backend,
    actor: i64,
    request: &AdminSpeechPreviewRequest,
) -> Result<AdminSpeechPlan, AppError> {
    let operator = crate::product_memberships::require_operator(
        &b.db,
        crate::product::ProductId::Brioche,
        actor,
    )
    .await?;
    preview_authorized(
        &Store {
            db: b.db.clone(),
            product: None,
        },
        &operator,
        request,
    )
    .await
}
async fn preview_authorized(
    b: &Store,
    operator: &crate::product_memberships::Operator,
    request: &AdminSpeechPreviewRequest,
) -> Result<AdminSpeechPlan, AppError> {
    if b.product.is_some_and(|product| product != operator.product) {
        return Err(AppError::Forbidden);
    }
    let tx =
        b.db.begin_with_config(Some(IsolationLevel::RepeatableRead), None)
            .await
            .map_err(|_| AppError::Unavailable)?;
    operator.lock_content(&tx).await?;
    let (_, view) = compile(&tx, b.product, request).await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(view)
}
fn projection(product: Option<crate::product::ProductId>) -> String {
    format!(
        r#"SELECT p.summary,p.id,to_char(p.created_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"') AS created_at FROM course_speech_plans p WHERE NOT EXISTS(SELECT 1 FROM content_withdrawals w WHERE (w.lesson_id,w.revision)=(p.lesson_id,p.lesson_revision){}){}"#,
        if product.is_some() {
            " AND w.product_id=p.product_id"
        } else {
            ""
        },
        product_filter(product, "p.product_id")
    )
}
fn item(row: &QueryResult) -> Result<AdminSpeechPlan, AppError> {
    let mut summary: AdminSpeechPlan =
        serde_json::from_value(field(row, "summary")?).map_err(|_| AppError::Unavailable)?;
    summary.id = Some(field(row, "id")?);
    summary.created_at = Some(field(row, "created_at")?);
    Ok(summary)
}
async fn load(
    db: &impl ConnectionTrait,
    product: Option<crate::product::ProductId>,
    id: &str,
) -> Result<AdminSpeechPlan, AppError> {
    if !hex(id, 32) {
        return Err(AppError::InvalidInput);
    }
    let row = one(
        db,
        &format!("{} AND p.id=$1", projection(product)),
        vec![id.into()],
    )
    .await?
    .ok_or(AppError::NotFound)?;
    item(&row)
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemQuery {}
async fn read(
    auth: AdminAuth,
    State(b): State<Store>,
    Path(id): Path<String>,
    Query(_query): Query<ItemQuery>,
) -> Result<Json<AdminSpeechPlan>, AppError> {
    auth.require_operator().await?;
    Ok(Json(load(&b.db, b.product, &id).await?))
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Cursor {
    lesson_id: String,
    lesson_revision: u32,
    after_id: Option<String>,
}
async fn list(
    auth: AdminAuth,
    State(b): State<Store>,
    Query(cursor): Query<Cursor>,
) -> Result<Json<AdminSpeechPlans>, AppError> {
    auth.require_operator().await?;
    lesson_key(&cursor.lesson_id, cursor.lesson_revision)?;
    let after = cursor.after_id.unwrap_or_default();
    if !after.is_empty() && !hex(&after, 32) {
        return Err(AppError::InvalidInput);
    }
    let rows =
        b.db.query_all_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            format!(
                "{} AND p.lesson_id=$1 AND p.lesson_revision=$2 AND p.id>$3 ORDER BY p.id LIMIT 21",
                projection(b.product)
            ),
            vec![
                cursor.lesson_id.into(),
                (cursor.lesson_revision as i32).into(),
                after.into(),
            ],
        ))
        .await
        .map_err(|_| AppError::Unavailable)?;
    let items = rows
        .iter()
        .take(20)
        .map(item)
        .collect::<Result<Vec<_>, _>>()?;
    let next = if rows.len() > 20 {
        items.last().and_then(|i| i.id.clone())
    } else {
        None
    };
    Ok(Json(AdminSpeechPlans { items, next }))
}
async fn save(
    auth: AdminAuth,
    State(b): State<Store>,
    Json(request): Json<AdminSpeechPlanRequest>,
) -> Result<Json<AdminSpeechPlan>, AppError> {
    let operator = auth.require_operator().await?;
    Ok(Json(save_for_actor(&b, &operator, request).await?))
}

/// Trusted local entry point; records its origin and shares the HTTP transaction.
pub async fn save_local(
    b: &Backend,
    actor: i64,
    mut request: AdminSpeechPlanRequest,
) -> Result<AdminSpeechPlan, AppError> {
    request.reason = format!("[local-cli] {}", request.reason);
    let operator = crate::product_memberships::require_operator(
        &b.db,
        crate::product::ProductId::Brioche,
        actor,
    )
    .await?;
    save_for_actor(
        &Store {
            db: b.db.clone(),
            product: None,
        },
        &operator,
        request,
    )
    .await
}

async fn save_for_actor(
    b: &Store,
    operator: &crate::product_memberships::Operator,
    request: AdminSpeechPlanRequest,
) -> Result<AdminSpeechPlan, AppError> {
    if b.product.is_some_and(|product| product != operator.product) {
        return Err(AppError::Forbidden);
    }
    let actor = operator.actor;
    crate::admin::reason(&request.reason)?;
    if !hex(&request.id, 32) || !hex(&request.expected_plan_hash, 64) {
        return Err(AppError::InvalidInput);
    }
    let tx = b.db.begin().await.map_err(|_| AppError::Unavailable)?;
    operator.lock_content(&tx).await?;
    let request_json = serde_json::to_value(&request).map_err(|_| AppError::InvalidInput)?;
    // Keep the global guard on old/partial layouts; retries never load foreign payloads.
    let local_ids = b.product.is_some()
        && crate::product_keys::supports_local_ids(&tx, crate::product_keys::LocalIdTable::Plan)
            .await?;
    if let Some(product) = b.product.filter(|_| !local_ids)
        && one(
            &tx,
            "SELECT 1 FROM course_speech_plans WHERE id=$1 AND product_id<>$2",
            vec![request.id.clone().into(), product.as_str().into()],
        )
        .await?
        .is_some()
    {
        return Err(AppError::NotFound);
    }
    if let Some(existing) = one(
        &tx,
        &format!(
            "SELECT actor_id,request FROM course_speech_plans WHERE id=$1{}",
            product_filter(b.product, "product_id")
        ),
        vec![request.id.clone().into()],
    )
    .await?
    {
        if field::<i64>(&existing, "actor_id")? != actor
            || field::<Value>(&existing, "request")? != request_json
        {
            return Err(AppError::Conflict);
        }
        return load(&tx, b.product, &request.id).await;
    }
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
    let (plan, view) = compile(&tx, b.product, &request.preview).await?;
    if plan.plan_hash != request.expected_plan_hash {
        return Err(AppError::Conflict);
    }
    let mut values = vec![
        request.id.clone().into(),
        request.preview.lesson_id.into(),
        (request.preview.lesson_revision as i32).into(),
        request_json.into(),
        serde_json::to_value(plan)
            .map_err(|_| AppError::Unavailable)?
            .into(),
        serde_json::to_value(view)
            .map_err(|_| AppError::Unavailable)?
            .into(),
        actor.into(),
        request.reason.into(),
    ];
    let sql = if let Some(product) = b.product {
        values.push(product.as_str().into());
        "INSERT INTO course_speech_plans(id,lesson_id,lesson_revision,request,plan,summary,actor_id,reason,product_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)"
    } else {
        "INSERT INTO course_speech_plans(id,lesson_id,lesson_revision,request,plan,summary,actor_id,reason) VALUES($1,$2,$3,$4,$5,$6,$7,$8)"
    };
    exec(&tx, sql, values).await?;
    let result = load(&tx, b.product, &request.id).await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(result)
}
