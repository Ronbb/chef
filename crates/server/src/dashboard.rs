//! Read-only study facts. Goal minutes are a preference, never fabricated measured duration.
use crate::knowledge_snapshot::KnowledgeWire;
use crate::{
    AppError,
    learning::{field, one, owner, product_filter, product_source_filter},
    learning_identity::LearningAuth as AuthSession,
    learning_store::LearningStore,
    library::STAMP,
};
use axum::{Json, Router, extract::State, routing::get};
use brioche_course_contract::*;
use jiff::{Timestamp, ToSpan, civil::Date};
use sea_orm::{
    ConnectionTrait, DatabaseTransaction, DbBackend, IsolationLevel, Statement, TransactionTrait,
    Value,
};
pub fn router() -> Router<LearningStore> {
    Router::new()
        .merge(
            Router::new()
                .route("/api/v1/me/dashboard", get(dashboard))
                .layer(axum::Extension(KnowledgeWire::Legacy)),
        )
        .merge(
            Router::new()
                .route("/api/v2/me/dashboard", get(dashboard))
                .layer(axum::Extension(KnowledgeWire::Neutral)),
        )
}
fn week_dates(now: Timestamp, zone: &str) -> Result<(Date, Vec<Date>), AppError> {
    let today = now.in_tz(zone).map_err(|_| AppError::Unavailable)?.date();
    let offset = today.weekday().to_monday_zero_offset();
    let monday = today
        .checked_sub(i64::from(offset).days())
        .map_err(|_| AppError::Unavailable)?;
    let dates = (0..7)
        .map(|index| {
            monday
                .checked_add(index.days())
                .map_err(|_| AppError::Unavailable)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((today, dates))
}
fn midnight(date: Date, zone: &str) -> Result<String, AppError> {
    // Civil dates are resolved once using the same tz database as review scheduling.
    Ok(date
        .at(0, 0, 0, 0)
        .in_tz(zone)
        .map_err(|_| AppError::Unavailable)?
        .timestamp()
        .to_string())
}
fn count(value: i64) -> Result<u32, AppError> {
    u32::try_from(value).map_err(|_| AppError::Unavailable)
}
async fn days(
    tx: &DatabaseTransaction,
    product: Option<crate::product::ProductId>,
    user: i64,
    now: Timestamp,
    zone: &str,
) -> Result<(Date, Vec<StudyDay>), AppError> {
    let (today, dates) = week_dates(now, zone)?;
    let mut values = vec![Value::from(user)];
    let mut bounds = Vec::new();
    for date in dates {
        let first = values.len() + 1;
        bounds.push(format!(
            "(${first}::text,${}::timestamptz,${}::timestamptz)",
            first + 1,
            first + 2
        ));
        values.extend([
            date.to_string().into(),
            midnight(date, zone)?.into(),
            midnight(
                date.checked_add(1.days())
                    .map_err(|_| AppError::Unavailable)?,
                zone,
            )?
            .into(),
        ]);
    }
    // Union immutable event timestamps, not last_updated (which retries or revisits can move).
    let sql = format!(
        "WITH bounds(day,start_at,end_at) AS (VALUES {}), events(stamp,kind) AS (SELECT p.confirmed_at,'step' FROM step_progress p JOIN learning_sessions s ON s.id=p.session_id{} WHERE s.user_id=$1{} UNION ALL SELECT created_at,'exercise' FROM exercise_attempts WHERE user_id=$1{} UNION ALL SELECT reviewed_at,'review' FROM review_attempts WHERE user_id=$1{} UNION ALL SELECT first_completed_at,'complete' FROM lesson_progress WHERE user_id=$1{} AND first_completed_at IS NOT NULL) SELECT b.day,count(*) FILTER(WHERE e.kind='step')::bigint AS steps,count(*) FILTER(WHERE e.kind='exercise')::bigint AS exercises,count(*) FILTER(WHERE e.kind='review')::bigint AS reviews,count(*) FILTER(WHERE e.kind='complete')::bigint AS completed FROM bounds b LEFT JOIN events e ON e.stamp>=b.start_at AND e.stamp<b.end_at GROUP BY b.day ORDER BY b.day",
        bounds.join(","),
        if product.is_some() {
            " AND s.product_id=p.product_id"
        } else {
            ""
        },
        product_filter(product, "s.product_id"),
        product_filter(product, "product_id"),
        product_filter(product, "product_id"),
        product_filter(product, "product_id"),
    );
    let rows = tx
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            values,
        ))
        .await
        .map_err(|_| AppError::Unavailable)?;
    let days = rows
        .iter()
        .map(|row| {
            let steps = count(field(row, "steps")?)?;
            let exercises = count(field(row, "exercises")?)?;
            let reviews = count(field(row, "reviews")?)?;
            let completed = count(field(row, "completed")?)?;
            Ok(StudyDay {
                local_date: field(row, "day")?,
                confirmed_steps: steps,
                exercise_attempts: exercises,
                review_attempts: reviews,
                completed_lessons: completed,
                active: steps > 0 || exercises > 0 || reviews > 0 || completed > 0,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok((today, days))
}
async fn dashboard(
    auth: AuthSession,
    axum::Extension(wire): axum::Extension<KnowledgeWire>,
    State(backend): State<LearningStore>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user = owner(&auth)?;
    let tx = backend
        .db
        .begin_with_config(Some(IsolationLevel::RepeatableRead), None)
        .await
        .map_err(|_| AppError::Unavailable)?;
    let settings = crate::product_settings::read(
        &tx,
        backend
            .product
            .unwrap_or(crate::product::ProductId::Brioche),
        user,
    )
    .await?
    .settings;
    let now = Timestamp::now();
    let (today, days) = days(&tx, backend.product, user, now, &settings.time_zone).await?;
    let sql = format!(
        "SELECT s.id,s.lesson_id,s.revision,s.last_step_id,r.public_document->'title' AS title,r.public_document->>'schemaVersion' AS course_schema,to_char(s.completed_at AT TIME ZONE 'UTC','{STAMP}') AS completed,to_char(p.first_completed_at AT TIME ZONE 'UTC','{STAMP}') AS first_completed,to_char(s.updated_at AT TIME ZONE 'UTC','{STAMP}') AS updated FROM lesson_progress p JOIN learning_sessions s ON s.id=p.last_session_id AND s.user_id=p.user_id{} JOIN lesson_revisions r ON (r.lesson_id,r.revision)=(s.lesson_id,s.revision) WHERE p.user_id=$1{} AND r.published ORDER BY s.updated_at DESC,s.id DESC",
        if backend.product.is_some() {
            " AND s.product_id=p.product_id"
        } else {
            ""
        },
        product_source_filter(backend.product, "p.product_id"),
    );
    let rows = tx
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            [user.into()],
        ))
        .await
        .map_err(|_| AppError::Unavailable)?;
    let course_states = rows
        .iter()
        .map(|row| crate::knowledge_snapshot::overview_item(row, wire))
        .collect::<Result<Vec<_>, AppError>>()?;
    let resume = course_states
        .iter()
        .find(|state| state.completed_at.is_none())
        .cloned();
    let totals=one(&tx,&format!("SELECT count(*) FILTER (WHERE c.due_at <= $2::timestamptz)::bigint AS due,to_char(min(c.due_at) FILTER (WHERE c.due_at > $2::timestamptz) AT TIME ZONE 'UTC','{STAMP}') AS next FROM review_cards c JOIN lesson_revisions r ON (r.lesson_id,r.revision)=(c.source_lesson_id,c.source_revision) WHERE c.user_id=$1{} AND NOT c.suspended AND r.published",product_source_filter(backend.product,"c.product_id")),vec![user.into(),now.to_string().into()]).await?.ok_or(AppError::Unavailable)?;
    let completed=one(&tx,&format!("SELECT count(*)::bigint AS n FROM lesson_progress WHERE user_id=$1{} AND first_completed_at IS NOT NULL",product_filter(backend.product,"product_id")),vec![user.into()]).await?.ok_or(AppError::Unavailable)?;
    // Prefer unfinished courses in the active release's explicit editorial order.
    let recommendation=one(&tx,&format!("SELECT r.public_document,EXISTS(SELECT 1 FROM lesson_progress p WHERE p.user_id=$1 AND p.lesson_id=r.lesson_id{} AND p.first_completed_at IS NOT NULL) AS learned FROM content_state s JOIN release_entries e ON e.release_id=s.active_release{} JOIN lesson_revisions r ON (r.lesson_id,r.revision)=(e.lesson_id,e.revision){} WHERE {} AND r.published AND NOT EXISTS(SELECT 1 FROM content_withdrawals w WHERE (w.lesson_id,w.revision)=(r.lesson_id,r.revision){}) ORDER BY learned,e.position LIMIT 1",product_filter(backend.product,"p.product_id"),if backend.product.is_some(){" AND e.product_id=s.product_id"}else{""},if backend.product.is_some(){" AND r.product_id=e.product_id"}else{""},backend.product.map_or_else(||"s.singleton".to_owned(),|p|format!("s.product_id='{}'",p.as_str())),if backend.product.is_some(){" AND w.product_id=r.product_id"}else{""}),vec![user.into()]).await?;
    let (recommended_lesson, all_available_completed) = if let Some(row) = recommendation {
        let lesson = crate::author_source::CheckedLesson::from_public_document(field(
            &row,
            "public_document",
        )?)
        .map_err(|_| AppError::Unavailable)?;
        (
            Some(lesson.summary_document(wire.is_neutral())?),
            field(&row, "learned")?,
        )
    } else {
        (None, false)
    };
    let catalog = if wire.is_neutral() {
        serde_json::to_value(
            crate::content::neutral_catalog_matching_for_product(&tx, backend.product, &[]).await?,
        )
    } else {
        serde_json::to_value(
            crate::content::catalog_matching_for_product(&tx, backend.product, &[]).await?,
        )
    }
    .map_err(|_| AppError::Unavailable)?;
    let result = serde_json::json!({"catalog":catalog,"localDate":today.to_string(),"timeZone":settings.time_zone,"weekStart":days.first().ok_or(AppError::Unavailable)?.local_date,"activeDays":days.iter().filter(|d|d.active).count() as u8,"days":days,"weeklyGoalDays":settings.weekly_days,"dailyGoalMinutes":settings.daily_minutes,"dueReviews":count(field(&totals,"due")?)?,"nextReviewAt":field::<Option<String>>(&totals,"next")?,"completedLessons":count(field(&completed,"n")?)?,"resume":resume,"recommendedLesson":recommended_lesson,"allAvailableCompleted":all_available_completed,"courseStates":course_states});
    let result = wire.response::<StudyDashboard, neutral::NeutralStudyDashboard>(&result)?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(result))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn monday_weeks_and_local_boundaries() {
        let now = "2026-03-29T22:30:00Z".parse().unwrap();
        let (today, week) = week_dates(now, "Europe/Paris").unwrap();
        assert_eq!(today.to_string(), "2026-03-30");
        assert_eq!(week[0].to_string(), "2026-03-30");
        assert_eq!(week[6].to_string(), "2026-04-05");
        let (_, week) = week_dates(now, "America/New_York").unwrap();
        assert_eq!(week[0].to_string(), "2026-03-23");
        assert_eq!(
            midnight("2026-03-29".parse().unwrap(), "Europe/Paris").unwrap(),
            "2026-03-28T23:00:00Z"
        );
        assert_eq!(
            midnight("2026-03-30".parse().unwrap(), "Europe/Paris").unwrap(),
            "2026-03-29T22:00:00Z"
        );
        assert!(week_dates(now, "bad/zone").is_err());
    }
}
