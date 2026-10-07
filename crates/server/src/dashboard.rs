//! Read-only study facts. Goal minutes are a preference, never fabricated measured duration.
use crate::{
    AppError,
    identity::Backend,
    learning::{field, one, owner},
    learning_identity::LearningAuth as AuthSession,
    library::STAMP,
};
use axum::{Json, Router, extract::State, routing::get};
use brioche_course_contract::*;
use jiff::{Timestamp, ToSpan, civil::Date};
use sea_orm::{
    ConnectionTrait, DatabaseTransaction, DbBackend, IsolationLevel, Statement, TransactionTrait,
    Value,
};
pub fn router() -> Router<Backend> {
    Router::new().route("/api/v1/me/dashboard", get(dashboard))
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
        "WITH bounds(day,start_at,end_at) AS (VALUES {}), events(stamp,kind) AS (SELECT p.confirmed_at,'step' FROM step_progress p JOIN learning_sessions s ON s.id=p.session_id WHERE s.user_id=$1 UNION ALL SELECT created_at,'exercise' FROM exercise_attempts WHERE user_id=$1 UNION ALL SELECT reviewed_at,'review' FROM review_attempts WHERE user_id=$1 UNION ALL SELECT first_completed_at,'complete' FROM lesson_progress WHERE user_id=$1 AND first_completed_at IS NOT NULL) SELECT b.day,count(*) FILTER(WHERE e.kind='step')::bigint AS steps,count(*) FILTER(WHERE e.kind='exercise')::bigint AS exercises,count(*) FILTER(WHERE e.kind='review')::bigint AS reviews,count(*) FILTER(WHERE e.kind='complete')::bigint AS completed FROM bounds b LEFT JOIN events e ON e.stamp>=b.start_at AND e.stamp<b.end_at GROUP BY b.day ORDER BY b.day",
        bounds.join(",")
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
    State(backend): State<Backend>,
) -> Result<Json<StudyDashboard>, AppError> {
    let user = owner(&auth)?;
    let tx = backend
        .db
        .begin_with_config(Some(IsolationLevel::RepeatableRead), None)
        .await
        .map_err(|_| AppError::Unavailable)?;
    let settings = crate::product_settings::read(&tx, crate::product::ProductId::Brioche, user)
        .await?
        .settings;
    let now = Timestamp::now();
    let (today, days) = days(&tx, user, now, &settings.time_zone).await?;
    let sql = format!(
        "SELECT s.id,s.lesson_id,s.revision,s.last_step_id,r.public_document->'title' AS title,to_char(s.completed_at AT TIME ZONE 'UTC','{STAMP}') AS completed,to_char(p.first_completed_at AT TIME ZONE 'UTC','{STAMP}') AS first_completed,to_char(s.updated_at AT TIME ZONE 'UTC','{STAMP}') AS updated FROM lesson_progress p JOIN learning_sessions s ON s.id=p.last_session_id AND s.user_id=p.user_id JOIN lesson_revisions r ON (r.lesson_id,r.revision)=(s.lesson_id,s.revision) WHERE p.user_id=$1 AND r.published ORDER BY s.updated_at DESC,s.id DESC"
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
        .map(|row| {
            Ok(LearningOverviewItem {
                session_id: field(row, "id")?,
                lesson_id: field(row, "lesson_id")?,
                revision: u32::try_from(field::<i32>(row, "revision")?)
                    .map_err(|_| AppError::Unavailable)?,
                title: serde_json::from_value(field(row, "title")?)
                    .map_err(|_| AppError::Unavailable)?,
                last_step_id: field(row, "last_step_id")?,
                completed_at: field(row, "completed")?,
                first_completed_at: field(row, "first_completed")?,
                updated_at: field(row, "updated")?,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let resume = course_states
        .iter()
        .find(|state| state.completed_at.is_none())
        .cloned();
    let totals=one(&tx,&format!("SELECT count(*) FILTER (WHERE c.due_at <= $2::timestamptz)::bigint AS due,to_char(min(c.due_at) FILTER (WHERE c.due_at > $2::timestamptz) AT TIME ZONE 'UTC','{STAMP}') AS next FROM review_cards c JOIN lesson_revisions r ON (r.lesson_id,r.revision)=(c.source_lesson_id,c.source_revision) WHERE c.user_id=$1 AND NOT c.suspended AND r.published"),vec![user.into(),now.to_string().into()]).await?.ok_or(AppError::Unavailable)?;
    let completed=one(&tx,"SELECT count(*)::bigint AS n FROM lesson_progress WHERE user_id=$1 AND first_completed_at IS NOT NULL",vec![user.into()]).await?.ok_or(AppError::Unavailable)?;
    // Prefer unfinished courses in the active release's explicit editorial order.
    let recommendation=one(&tx,"SELECT r.public_document,EXISTS(SELECT 1 FROM lesson_progress p WHERE p.user_id=$1 AND p.lesson_id=r.lesson_id AND p.first_completed_at IS NOT NULL) AS learned FROM content_state s JOIN release_entries e ON e.release_id=s.active_release JOIN lesson_revisions r ON (r.lesson_id,r.revision)=(e.lesson_id,e.revision) WHERE s.singleton AND r.published ORDER BY learned,e.position LIMIT 1",vec![user.into()]).await?;
    let (recommended_lesson, all_available_completed) = if let Some(row) = recommendation {
        let lesson: PublicLesson = serde_json::from_value(field(&row, "public_document")?)
            .map_err(|_| AppError::Unavailable)?;
        lesson.validate().map_err(|_| AppError::Unavailable)?;
        (Some(lesson.summary()), field(&row, "learned")?)
    } else {
        (None, false)
    };
    let result = StudyDashboard {
        catalog: crate::content::catalog(&tx).await?,
        local_date: today.to_string(),
        time_zone: settings.time_zone,
        week_start: days
            .first()
            .ok_or(AppError::Unavailable)?
            .local_date
            .clone(),
        active_days: days.iter().filter(|day| day.active).count() as u8,
        days,
        weekly_goal_days: settings.weekly_days,
        daily_goal_minutes: settings.daily_minutes,
        due_reviews: count(field(&totals, "due")?)?,
        next_review_at: field(&totals, "next")?,
        completed_lessons: count(field(&completed, "n")?)?,
        resume,
        recommended_lesson,
        all_available_completed,
        course_states,
    };
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
