//! Owned review queue and atomic fixed-interval scheduling.
use crate::{
    AppError,
    identity::{AuthSession, Backend},
    learning::{exec, field, hash, one, owner, random_id, record, replay, validate_key},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post, put},
};
use brioche_course_contract::*;
use jiff::{Timestamp, ToSpan, civil::Date};
use sea_orm::{
    ConnectionTrait, DatabaseTransaction, DbBackend, IsolationLevel, QueryResult, Statement,
    TransactionTrait,
};
use serde::Deserialize;
const STAMP: &str = "YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"";
const COLUMNS: &str = "c.id,c.knowledge_id,c.source_lesson_id,c.source_revision,c.snapshot,c.stage,c.version,c.suspended,r.published";
fn card(row: &QueryResult) -> Result<ReviewCard, AppError> {
    Ok(ReviewCard {
        id: field(row, "id")?,
        knowledge_id: field(row, "knowledge_id")?,
        source_lesson_id: field(row, "source_lesson_id")?,
        source_revision: u32::try_from(field::<i32>(row, "source_revision")?)
            .map_err(|_| AppError::Unavailable)?,
        vocabulary: serde_json::from_value(field(row, "snapshot")?)
            .map_err(|_| AppError::Unavailable)?,
        stage: field(row, "stage")?,
        suspended: field(row, "suspended")?,
        due_at: field(row, "due")?,
        version: u32::try_from(field::<i32>(row, "version")?).map_err(|_| AppError::Unavailable)?,
    })
}
pub(crate) async fn load(
    tx: &DatabaseTransaction,
    user: i64,
    id: &str,
) -> Result<ReviewCard, AppError> {
    let row=one(tx,&format!("SELECT {COLUMNS},to_char(c.due_at AT TIME ZONE 'UTC','{STAMP}') AS due FROM review_cards c JOIN lesson_revisions r ON (r.lesson_id,r.revision)=(c.source_lesson_id,c.source_revision) WHERE c.user_id=$1 AND c.id=$2 FOR UPDATE OF c FOR SHARE OF r"),vec![user.into(),id.into()]).await?.ok_or(AppError::NotFound)?;
    if !field::<bool>(&row, "published")? {
        return Err(AppError::Gone);
    }
    card(&row)
}
/// Civil-day scheduling, not elapsed multiples of 24 hours. Compatible resolves gaps/folds.
pub fn schedule(
    now: Timestamp,
    zone: &str,
    stage: i16,
    rating: &ReviewRating,
) -> Result<(i16, Timestamp), AppError> {
    if !(-1..=4).contains(&stage) {
        return Err(AppError::Unavailable);
    }
    let next = match rating {
        ReviewRating::Again => 0,
        ReviewRating::Remembered => (stage + 1).min(4),
        ReviewRating::Familiar => (stage + 2).min(4),
    };
    let days = [1, 3, 7, 14, 30][next as usize];
    let date = now
        .in_tz(zone)
        .map_err(|_| AppError::Unavailable)?
        .date()
        .checked_add(days.days())
        .map_err(|_| AppError::Unavailable)?;
    let due = date
        .at(9, 0, 0, 0)
        .in_tz(zone)
        .map_err(|_| AppError::Unavailable)?
        .timestamp();
    Ok((next, due))
}
async fn zone(tx: &DatabaseTransaction, user: i64) -> Result<String, AppError> {
    one(
        tx,
        "SELECT id FROM users WHERE id=$1 FOR SHARE",
        vec![user.into()],
    )
    .await?
    .ok_or(AppError::Unauthorized)?;
    Ok(
        crate::product_settings::read(tx, crate::product::ProductId::Brioche, user)
            .await?
            .settings
            .time_zone,
    )
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QueueQuery {
    date: Option<String>,
}
pub fn router() -> Router<Backend> {
    Router::new()
        .route("/api/v1/me/reviews", get(queue))
        .route("/api/v1/me/reviews/{id}", get(detail))
        .route("/api/v1/me/reviews/{id}/attempts", post(attempt))
        .route("/api/v1/me/reviews/{id}/preferences", put(preferences))
        .route("/api/v1/me/review-cards", get(cards))
}
async fn queue(
    auth: AuthSession,
    State(backend): State<Backend>,
    Query(query): Query<QueueQuery>,
) -> Result<Json<ReviewQueue>, AppError> {
    let user = owner(&auth)?;
    let tx = backend
        .db
        .begin_with_config(Some(IsolationLevel::RepeatableRead), None)
        .await
        .map_err(|_| AppError::Unavailable)?;
    let now = Timestamp::now();
    let time_zone = zone(&tx, user).await?;
    let today = now
        .in_tz(&time_zone)
        .map_err(|_| AppError::Unavailable)?
        .date();
    let date = if let Some(date) = query.date {
        date.parse::<Date>().map_err(|_| AppError::InvalidInput)?
    } else {
        today
    };
    if date > today {
        return Err(AppError::InvalidInput);
    }
    let cutoff = if date == today {
        now
    } else {
        date.checked_add(1.days())
            .map_err(|_| AppError::InvalidInput)?
            .at(0, 0, 0, 0)
            .in_tz(&time_zone)
            .map_err(|_| AppError::InvalidInput)?
            .timestamp()
    };
    let rows=tx.query_all_raw(Statement::from_sql_and_values(DbBackend::Postgres,format!("SELECT {COLUMNS},to_char(c.due_at AT TIME ZONE 'UTC','{STAMP}') AS due FROM review_cards c JOIN lesson_revisions r ON (r.lesson_id,r.revision)=(c.source_lesson_id,c.source_revision) WHERE c.user_id=$1 AND NOT c.suspended AND r.published AND c.due_at < $2::timestamptz ORDER BY c.due_at,c.id LIMIT 10"),[user.into(),cutoff.to_string().into()])).await.map_err(|_|AppError::Unavailable)?;
    let totals=one(&tx,&format!("SELECT count(*) FILTER (WHERE c.due_at < $2::timestamptz)::bigint AS count,to_char(min(c.due_at) FILTER (WHERE c.due_at >= $2::timestamptz) AT TIME ZONE 'UTC','{STAMP}') AS next FROM review_cards c JOIN lesson_revisions r ON (r.lesson_id,r.revision)=(c.source_lesson_id,c.source_revision) WHERE c.user_id=$1 AND NOT c.suspended AND r.published"),vec![user.into(),cutoff.to_string().into()]).await?.ok_or(AppError::Unavailable)?;
    let result = ReviewQueue {
        items: rows.iter().map(card).collect::<Result<_, _>>()?,
        due_count: u32::try_from(field::<i64>(&totals, "count")?)
            .map_err(|_| AppError::Unavailable)?,
        next_due_at: field(&totals, "next")?,
        local_date: date.to_string(),
        time_zone,
    };
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(result))
}
async fn detail(
    auth: AuthSession,
    State(backend): State<Backend>,
    Path(id): Path<String>,
) -> Result<Json<ReviewCard>, AppError> {
    let tx = backend
        .db
        .begin()
        .await
        .map_err(|_| AppError::Unavailable)?;
    let result = load(&tx, owner(&auth)?, &id).await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(result))
}
async fn attempt(
    auth: AuthSession,
    State(backend): State<Backend>,
    Path(id): Path<String>,
    Json(request): Json<ReviewAttemptRequest>,
) -> Result<Json<ReviewAttemptResult>, AppError> {
    let user = owner(&auth)?;
    validate_key(&request.idempotency_key)?;
    let tx = backend
        .db
        .begin()
        .await
        .map_err(|_| AppError::Unavailable)?;
    // Settings first: a concurrent timezone edit and this schedule have a defined order.
    let time_zone = zone(&tx, user).await?;
    let old = load(&tx, user, &id).await?;
    let scope = format!("review:{id}:attempt");
    let fingerprint = hash(&request)?;
    if let Some(cached) = replay(&tx, user, &scope, &request.idempotency_key, &fingerprint).await? {
        return Ok(Json(cached));
    }
    if old.suspended {
        return Err(AppError::Conflict);
    }
    if request.card_version != old.version || old.version >= i32::MAX as u32 {
        return Err(AppError::Conflict);
    }
    let now = Timestamp::now();
    if old
        .due_at
        .parse::<Timestamp>()
        .map_err(|_| AppError::Unavailable)?
        > now
    {
        return Err(AppError::Conflict);
    }
    let (stage, due) = schedule(now, &time_zone, old.stage, &request.rating)?;
    let rating = match request.rating {
        ReviewRating::Again => "again",
        ReviewRating::Remembered => "remembered",
        ReviewRating::Familiar => "familiar",
    };
    exec(&tx,"INSERT INTO review_attempts (id,card_id,user_id,rating,old_stage,new_stage,old_version,new_version,due_at,reviewed_at,time_zone) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9::timestamptz,$10::timestamptz,$11)",vec![random_id()?.into(),id.clone().into(),user.into(),rating.into(),old.stage.into(),stage.into(),(old.version as i32).into(),(old.version as i32+1).into(),due.to_string().into(),now.to_string().into(),time_zone.clone().into()]).await?;
    exec(&tx,"UPDATE review_cards SET stage=$3,due_at=$4::timestamptz,version=version+1 WHERE id=$1 AND user_id=$2",vec![id.clone().into(),user.into(),stage.into(),due.to_string().into()]).await?;
    let result = ReviewAttemptResult {
        card: load(&tx, user, &id).await?,
        reviewed_at: now.to_string(),
        time_zone,
    };
    record(
        &tx,
        user,
        &scope,
        &request.idempotency_key,
        &fingerprint,
        &result,
    )
    .await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(result))
}
async fn preferences(
    auth: AuthSession,
    State(backend): State<Backend>,
    Path(id): Path<String>,
    Json(request): Json<ReviewPreferenceRequest>,
) -> Result<Json<ReviewCard>, AppError> {
    let user = owner(&auth)?;
    validate_key(&request.idempotency_key)?;
    let tx = backend
        .db
        .begin()
        .await
        .map_err(|_| AppError::Unavailable)?;
    let old = load(&tx, user, &id).await?;
    let scope = format!("review:{id}:preferences");
    let fingerprint = hash(&request)?;
    if let Some(cached) = replay(&tx, user, &scope, &request.idempotency_key, &fingerprint).await? {
        return Ok(Json(cached));
    }
    if old.version != request.card_version || old.version >= i32::MAX as u32 {
        return Err(AppError::Conflict);
    }
    if old.suspended != request.suspended {
        exec(
            &tx,
            "UPDATE review_cards SET suspended=$3,version=version+1 WHERE user_id=$1 AND id=$2",
            vec![user.into(), id.clone().into(), request.suspended.into()],
        )
        .await?;
    }
    let result = load(&tx, user, &id).await?;
    record(
        &tx,
        user,
        &scope,
        &request.idempotency_key,
        &fingerprint,
        &result,
    )
    .await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(result))
}
async fn cards(
    auth: AuthSession,
    State(backend): State<Backend>,
    Query(page): Query<crate::library::Page>,
) -> Result<Json<ReviewCardsPage>, AppError> {
    let (stamp, id) = crate::library::cursor(page.cursor)?;
    let sql = format!(
        "SELECT {COLUMNS},to_char(c.due_at AT TIME ZONE 'UTC','{STAMP}') AS due,to_char(c.created_at AT TIME ZONE 'UTC','{STAMP}') AS created FROM review_cards c JOIN lesson_revisions r ON (r.lesson_id,r.revision)=(c.source_lesson_id,c.source_revision) WHERE c.user_id=$1 AND r.published AND ($2::timestamptz IS NULL OR (c.created_at,c.id)<($2::timestamptz,$3::text)) ORDER BY c.created_at DESC,c.id DESC LIMIT 21"
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
        .map(card)
        .collect::<Result<Vec<_>, _>>()?;
    let next_cursor = if rows.len() > 20 {
        let last = &rows[19];
        Some(format!(
            "{}@{}",
            field::<String>(last, "created")?,
            field::<String>(last, "id")?
        ))
    } else {
        None
    };
    Ok(Json(ReviewCardsPage { items, next_cursor }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calendar_days_and_dst() {
        let now = "2026-03-28T22:30:00Z".parse().unwrap();
        let (stage, due) = schedule(now, "Europe/Paris", -1, &ReviewRating::Remembered).unwrap();
        assert_eq!(stage, 0);
        assert_eq!(due.to_string(), "2026-03-29T07:00:00Z");
        let (_, due) = schedule(now, "Asia/Shanghai", -1, &ReviewRating::Familiar).unwrap();
        assert_eq!(due.to_string(), "2026-04-01T01:00:00Z");
        for stage in -1..=4 {
            assert_eq!(
                schedule(now, "UTC", stage, &ReviewRating::Again).unwrap().0,
                0
            );
            assert_eq!(
                schedule(now, "UTC", stage, &ReviewRating::Remembered)
                    .unwrap()
                    .0,
                (stage + 1).min(4)
            );
            assert_eq!(
                schedule(now, "UTC", stage, &ReviewRating::Familiar)
                    .unwrap()
                    .0,
                (stage + 2).min(4)
            );
        }
        assert!(schedule(now, "bad/zone", 0, &ReviewRating::Again).is_err());
        assert!(schedule(now, "UTC", 5, &ReviewRating::Again).is_err());
    }
}
