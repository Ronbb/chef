//! Bookmarks preserve their first source snapshot and are independent of review participation.
use crate::{
    AppError,
    learning::{
        exec, field, hash, insert_fact, one, owner, product_filter, product_source_filter,
        random_id, record, replay, validate_key,
    },
    learning_identity::LearningAuth as AuthSession,
    learning_store::LearningStore,
};
use crate::{
    author_source::CheckedLesson,
    knowledge_snapshot::{KnowledgeWire, Snapshot, StoredHistoryItem, StoredSavedItem},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use brioche_course_contract::*;
use sea_orm::{
    ConnectionTrait, DatabaseTransaction, DbBackend, QueryResult, Statement, TransactionTrait,
};
use serde::Deserialize;
pub(crate) const STAMP: &str = "YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"";
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Page {
    pub cursor: Option<String>,
}
pub(crate) fn cursor(value: Option<String>) -> Result<(Option<String>, Option<String>), AppError> {
    let Some(value) = value else {
        return Ok((None, None));
    };
    let (stamp, id) = value.split_once('@').ok_or(AppError::InvalidInput)?;
    if stamp.len() > 40
        || stamp.parse::<jiff::Timestamp>().is_err()
        || id.len() != 32
        || !id.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(AppError::InvalidInput);
    }
    Ok((Some(stamp.into()), Some(id.into())))
}
pub(crate) async fn source(
    tx: &DatabaseTransaction,
    product: Option<crate::product::ProductId>,
    lesson: &str,
    revision: u32,
    knowledge: &str,
) -> Result<Snapshot, AppError> {
    let revision = i32::try_from(revision).map_err(|_| AppError::InvalidInput)?;
    crate::learning_store::lock_lesson(tx, product, lesson, revision).await?;
    let row = one(
        tx,
        &format!("SELECT published,public_document FROM lesson_revisions WHERE lesson_id=$1 AND revision=$2{}", product_filter(product, "product_id")),
        vec![lesson.into(), revision.into()],
    )
    .await?
    .ok_or(AppError::NotFound)?;
    if !field::<bool>(&row, "published")? {
        return Err(AppError::Gone);
    }
    let lesson = CheckedLesson::from_public_document(field(&row, "public_document")?)
        .map_err(|_| AppError::Unavailable)?;
    serde_json::from_value(
        lesson
            .vocabulary_snapshot(knowledge)
            .map_err(|_| AppError::NotFound)?,
    )
    .map_err(|_| AppError::Unavailable)
}
fn saved(row: &QueryResult) -> Result<StoredSavedItem, AppError> {
    let published = field::<bool>(row, "published")?;
    Ok(StoredSavedItem {
        id: field(row, "id")?,
        knowledge_id: field(row, "knowledge_id")?,
        source_lesson_id: field(row, "source_lesson_id")?,
        source_revision: u32::try_from(field::<i32>(row, "source_revision")?)
            .map_err(|_| AppError::Unavailable)?,
        vocabulary: if published {
            Some(Snapshot::from_row(row)?)
        } else {
            None
        },
        saved: field(row, "saved")?,
        withdrawn: !published,
        version: u32::try_from(field::<i32>(row, "version")?).map_err(|_| AppError::Unavailable)?,
        created_at: field(row, "created")?,
    })
}
async fn load(
    tx: &DatabaseTransaction,
    product: Option<crate::product::ProductId>,
    user: i64,
    knowledge: &str,
    lock: bool,
) -> Result<Option<StoredSavedItem>, AppError> {
    if lock && let Some(reference)=one(tx,&format!("SELECT source_lesson_id,source_revision FROM saved_items WHERE user_id=$1 AND knowledge_id=$2{}",product_filter(product,"product_id")),vec![user.into(),knowledge.into()]).await? {
        crate::learning_store::lock_lesson(tx,product,&field::<String>(&reference,"source_lesson_id")?,field(&reference,"source_revision")?).await?;
    }
    let sql = format!(
        "SELECT s.*,r.published,r.public_document->>'schemaVersion' AS source_schema,r.public_document->>'targetLanguage' AS target_language,to_char(s.created_at AT TIME ZONE 'UTC','{STAMP}') AS created FROM saved_items s JOIN lesson_revisions r ON (r.lesson_id,r.revision)=(s.source_lesson_id,s.source_revision) WHERE s.user_id=$1 AND s.knowledge_id=$2{} {}",
        product_source_filter(product, "s.product_id"),
        if lock { "FOR UPDATE OF s" } else { "" }
    );
    one(tx, &sql, vec![user.into(), knowledge.into()])
        .await?
        .map(|row| saved(&row))
        .transpose()
}
fn routes(prefix: &str, wire: KnowledgeWire) -> Router<LearningStore> {
    Router::new()
        .route(&format!("{prefix}/saved-items"), get(list))
        .route(
            &format!("{prefix}/saved-items/{{knowledge}}"),
            get(detail).put(write),
        )
        .route(&format!("{prefix}/review-history"), get(history))
        .route(&format!("{prefix}/review-enrollments"), post(enroll))
        .layer(axum::Extension(wire))
}
pub fn router() -> Router<LearningStore> {
    Router::new()
        .merge(routes("/api/v1/me", KnowledgeWire::Legacy))
        .merge(routes("/api/v2/me", KnowledgeWire::Neutral))
}
async fn detail(
    auth: AuthSession,
    axum::Extension(wire): axum::Extension<KnowledgeWire>,
    State(backend): State<LearningStore>,
    Path(knowledge): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let tx = backend
        .db
        .begin()
        .await
        .map_err(|_| AppError::Unavailable)?;
    let item = load(&tx, backend.product, owner(&auth)?, &knowledge, false)
        .await?
        .ok_or(AppError::NotFound)?;
    let result = wire.response::<SavedItem, neutral::NeutralSavedItem>(&item)?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(result))
}
async fn write(
    auth: AuthSession,
    axum::Extension(wire): axum::Extension<KnowledgeWire>,
    State(backend): State<LearningStore>,
    Path(knowledge): Path<String>,
    Json(request): Json<SavedWriteRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user = owner(&auth)?;
    validate_key(&request.idempotency_key)?;
    if knowledge.is_empty() || knowledge.len() > 256 {
        return Err(AppError::InvalidInput);
    }
    let tx = backend
        .db
        .begin()
        .await
        .map_err(|_| AppError::Unavailable)?;
    let scope = format!("saved:{knowledge}");
    let fingerprint = hash(&request)?;
    one(
        &tx,
        "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
        vec![
            format!(
                "{}saved:{user}:{knowledge}",
                backend
                    .product
                    .map_or_else(String::new, |p| format!("{}:", p.as_str()))
            )
            .into(),
        ],
    )
    .await?;
    let old = load(&tx, backend.product, user, &knowledge, true).await?;
    if let Some(snapshot) = old.as_ref().and_then(|old| old.vocabulary.as_ref()) {
        wire.check(snapshot)?;
    }
    if request.saved && old.as_ref().is_some_and(|old| old.withdrawn) {
        return Err(AppError::Gone);
    }
    if let Some(mut cached) = replay::<StoredSavedItem>(
        &tx,
        backend.product,
        user,
        &scope,
        &request.idempotency_key,
        &fingerprint,
    )
    .await?
    {
        if old.as_ref().is_some_and(|old| old.withdrawn) {
            cached.vocabulary = None;
            cached.withdrawn = true;
        }
        return Ok(Json(
            wire.response::<SavedItem, neutral::NeutralSavedItem>(&cached)?,
        ));
    }
    if request.version != old.as_ref().map_or(0, |item| item.version) {
        return Err(AppError::Conflict);
    }
    if let Some(old) = old {
        if old.saved != request.saved {
            if old.version >= i32::MAX as u32 {
                return Err(AppError::Conflict);
            }
            exec(&tx,&format!("UPDATE saved_items SET saved=$3,version=version+1,updated_at=CURRENT_TIMESTAMP WHERE user_id=$1 AND knowledge_id=$2{}",product_filter(backend.product,"product_id")),vec![user.into(),knowledge.clone().into(),request.saved.into()]).await?;
        }
    } else {
        let vocabulary = source(
            &tx,
            backend.product,
            &request.source_lesson_id,
            request.source_revision,
            &knowledge,
        )
        .await?;
        wire.check(&vocabulary)?;
        insert_fact(
            &tx,
            backend.product,
            "saved_items",
            "id,user_id,knowledge_id,source_lesson_id,source_revision,snapshot,saved",
            vec![
                random_id()?.into(),
                user.into(),
                knowledge.clone().into(),
                request.source_lesson_id.into(),
                (request.source_revision as i32).into(),
                serde_json::to_value(vocabulary)
                    .map_err(|_| AppError::Unavailable)?
                    .into(),
                request.saved.into(),
            ],
            "",
        )
        .await?;
    }
    let result = load(&tx, backend.product, user, &knowledge, false)
        .await?
        .ok_or(AppError::Unavailable)?;
    record(
        &tx,
        backend.product,
        user,
        &scope,
        &request.idempotency_key,
        &fingerprint,
        &result,
    )
    .await?;
    let result = wire.response::<SavedItem, neutral::NeutralSavedItem>(&result)?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(result))
}
async fn list(
    auth: AuthSession,
    axum::Extension(wire): axum::Extension<KnowledgeWire>,
    State(backend): State<LearningStore>,
    Query(page): Query<Page>,
) -> Result<Json<serde_json::Value>, AppError> {
    let (stamp, id) = cursor(page.cursor)?;
    let sql = format!(
        "SELECT s.*,r.published,r.public_document->>'schemaVersion' AS source_schema,r.public_document->>'targetLanguage' AS target_language,to_char(s.created_at AT TIME ZONE 'UTC','{STAMP}') AS created FROM saved_items s JOIN lesson_revisions r ON (r.lesson_id,r.revision)=(s.source_lesson_id,s.source_revision) WHERE s.user_id=$1{} AND s.saved AND ($2::timestamptz IS NULL OR (s.created_at,s.id)<($2::timestamptz,$3::text)) ORDER BY s.created_at DESC,s.id DESC LIMIT 21",
        product_source_filter(backend.product, "s.product_id"),
    );
    let rows = backend
        .db
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            [owner(&auth)?.into(), stamp.into(), id.into()],
        ))
        .await
        .map_err(|_| AppError::Unavailable)?;
    let more = rows.len() > 20;
    let items = rows
        .iter()
        .take(20)
        .map(saved)
        .collect::<Result<Vec<_>, _>>()?;
    let next_cursor = if more {
        items
            .last()
            .map(|item| format!("{}@{}", item.created_at, item.id))
    } else {
        None
    };
    Ok(Json(
        wire.response::<SavedPage, neutral::NeutralSavedPage>(
            &serde_json::json!({"items":items,"nextCursor":next_cursor}),
        )?,
    ))
}
async fn enroll(
    auth: AuthSession,
    axum::Extension(wire): axum::Extension<KnowledgeWire>,
    State(backend): State<LearningStore>,
    Json(request): Json<ReviewEnrollmentRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user = owner(&auth)?;
    validate_key(&request.idempotency_key)?;
    let tx = backend
        .db
        .begin()
        .await
        .map_err(|_| AppError::Unavailable)?;
    let vocabulary = source(
        &tx,
        backend.product,
        &request.source_lesson_id,
        request.source_revision,
        &request.knowledge_id,
    )
    .await?;
    wire.check(&vocabulary)?;
    let scope = format!("enroll:{}", request.knowledge_id);
    let fingerprint = hash(&request)?;
    insert_fact(
        &tx,
        backend.product,
        "review_cards",
        "id,user_id,knowledge_id,source_lesson_id,source_revision,snapshot",
        vec![
            random_id()?.into(),
            user.into(),
            request.knowledge_id.clone().into(),
            request.source_lesson_id.into(),
            (request.source_revision as i32).into(),
            serde_json::to_value(vocabulary)
                .map_err(|_| AppError::Unavailable)?
                .into(),
        ],
        crate::learning::review_conflict(backend.product),
    )
    .await?;
    let row = one(
        &tx,
        &format!(
            "SELECT id FROM review_cards WHERE user_id=$1 AND knowledge_id=$2{} FOR UPDATE",
            product_filter(backend.product, "product_id")
        ),
        vec![user.into(), request.knowledge_id.into()],
    )
    .await?
    .ok_or(AppError::Unavailable)?;
    let id: String = field(&row, "id")?;
    let current = crate::reviews::load(&tx, backend.product, user, &id).await?;
    if let Some(cached) = replay::<serde_json::Value>(
        &tx,
        backend.product,
        user,
        &scope,
        &request.idempotency_key,
        &fingerprint,
    )
    .await?
    {
        return Ok(Json(
            wire.response::<ReviewCard, neutral::NeutralReviewCard>(&cached)?,
        ));
    }
    record(
        &tx,
        backend.product,
        user,
        &scope,
        &request.idempotency_key,
        &fingerprint,
        &current,
    )
    .await?;
    let current = wire.response::<ReviewCard, neutral::NeutralReviewCard>(&current)?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(current))
}
async fn history(
    auth: AuthSession,
    axum::Extension(wire): axum::Extension<KnowledgeWire>,
    State(backend): State<LearningStore>,
    Query(page): Query<Page>,
) -> Result<Json<serde_json::Value>, AppError> {
    let (stamp, id) = cursor(page.cursor)?;
    let sql = format!(
        "SELECT a.*,c.snapshot,r.published,r.public_document->>'schemaVersion' AS source_schema,r.public_document->>'targetLanguage' AS target_language,to_char(a.reviewed_at AT TIME ZONE 'UTC','{STAMP}') AS reviewed,to_char(a.due_at AT TIME ZONE 'UTC','{STAMP}') AS due FROM review_attempts a JOIN review_cards c ON (c.id,c.user_id)=(a.card_id,a.user_id){} JOIN lesson_revisions r ON (r.lesson_id,r.revision)=(c.source_lesson_id,c.source_revision) WHERE a.user_id=$1{} AND ($2::timestamptz IS NULL OR (a.reviewed_at,a.id)<($2::timestamptz,$3::text)) ORDER BY a.reviewed_at DESC,a.id DESC LIMIT 21",
        if backend.product.is_some() {
            " AND c.product_id=a.product_id"
        } else {
            ""
        },
        product_source_filter(backend.product, "a.product_id"),
    );
    let rows = backend
        .db
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            [owner(&auth)?.into(), stamp.into(), id.into()],
        ))
        .await
        .map_err(|_| AppError::Unavailable)?;
    let items = rows
        .iter()
        .take(20)
        .map(|row| {
            let published = field::<bool>(row, "published")?;
            Ok(StoredHistoryItem {
                id: field(row, "id")?,
                card_id: field(row, "card_id")?,
                vocabulary: if published {
                    Some(Snapshot::from_row(row)?)
                } else {
                    None
                },
                withdrawn: !published,
                rating: serde_json::from_value(serde_json::Value::String(field(row, "rating")?))
                    .map_err(|_| AppError::Unavailable)?,
                old_stage: field(row, "old_stage")?,
                new_stage: field(row, "new_stage")?,
                reviewed_at: field(row, "reviewed")?,
                due_at: field(row, "due")?,
                time_zone: field(row, "time_zone")?,
                algorithm_version: field(row, "algorithm_version")?,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let next_cursor = if rows.len() > 20 {
        items
            .last()
            .map(|item| format!("{}@{}", item.reviewed_at, item.id))
    } else {
        None
    };
    Ok(Json(
        wire.response::<ReviewHistoryPage, neutral::NeutralReviewHistoryPage>(
            &serde_json::json!({"items":items,"nextCursor":next_cursor}),
        )?,
    ))
}
