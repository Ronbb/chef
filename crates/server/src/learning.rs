//! Owned, revision-pinned learning transactions. Client scores are never accepted.
use crate::{
    AppError,
    grading::{GradeError, Grader},
    identity::{AuthSession, Backend},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post, put},
};
use axum_login::AuthUser;
use brioche_course_contract::*;
use sea_orm::{
    ConnectionTrait, DatabaseTransaction, DbBackend, QueryResult, Statement, TransactionTrait,
    Value,
};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

const STAMP: &str = "YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"";
pub(crate) fn owner(auth: &AuthSession) -> Result<i64, AppError> {
    auth.user
        .as_ref()
        .map(AuthUser::id)
        .ok_or(AppError::Unauthorized)
}
pub(crate) fn random_id() -> Result<String, AppError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| AppError::Unavailable)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
pub(crate) fn validate_key(key: &str) -> Result<(), AppError> {
    if !(16..=128).contains(&key.len())
        || !key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(AppError::InvalidInput);
    }
    Ok(())
}
pub(crate) fn hash<T: Serialize>(request: &T) -> Result<String, AppError> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(request).map_err(|_| AppError::Unavailable)?)
    ))
}
pub(crate) async fn one<C: ConnectionTrait>(
    db: &C,
    sql: &str,
    values: Vec<Value>,
) -> Result<Option<QueryResult>, AppError> {
    db.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        sql,
        values,
    ))
    .await
    .map_err(|_| AppError::Unavailable)
}
pub(crate) async fn exec<C: ConnectionTrait>(
    db: &C,
    sql: &str,
    values: Vec<Value>,
) -> Result<u64, AppError> {
    Ok(db
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            values,
        ))
        .await
        .map_err(|_| AppError::Unavailable)?
        .rows_affected())
}
pub(crate) fn field<T: sea_orm::TryGetable>(row: &QueryResult, name: &str) -> Result<T, AppError> {
    row.try_get("", name).map_err(|_| AppError::Unavailable)
}
pub(crate) async fn replay<T: DeserializeOwned>(
    tx: &DatabaseTransaction,
    user: i64,
    scope: &str,
    key: &str,
    fingerprint: &str,
) -> Result<Option<T>, AppError> {
    validate_key(key)?;
    let Some(row) = one(tx,"SELECT request_hash,result FROM learning_operations WHERE user_id=$1 AND scope=$2 AND idempotency_key=$3",vec![user.into(),scope.into(),key.into()]).await? else { return Ok(None); };
    if field::<String>(&row, "request_hash")? != fingerprint {
        return Err(AppError::Conflict);
    }
    Ok(Some(
        serde_json::from_value(field(&row, "result")?).map_err(|_| AppError::Unavailable)?,
    ))
}
pub(crate) async fn record<T: Serialize>(
    tx: &DatabaseTransaction,
    user: i64,
    scope: &str,
    key: &str,
    fingerprint: &str,
    result: &T,
) -> Result<(), AppError> {
    exec(tx,"INSERT INTO learning_operations (user_id,scope,idempotency_key,request_hash,result) VALUES ($1,$2,$3,$4,$5)",vec![user.into(),scope.into(),key.into(),fingerprint.into(),serde_json::to_value(result).map_err(|_| AppError::Unavailable)?.into()]).await?;
    Ok(())
}
struct SessionRow {
    id: String,
    user: i64,
    lesson: PublicLesson,
    source: serde_json::Value,
    version: u32,
    last: Option<String>,
    completed: Option<String>,
    first_completed: Option<String>,
}
async fn load<C: ConnectionTrait>(
    db: &C,
    user: i64,
    id: &str,
    lock: bool,
) -> Result<SessionRow, AppError> {
    let sql = format!(
        "SELECT s.id,s.lesson_id,s.revision,s.schema_version,s.version,s.last_step_id,r.public_document,r.server_document,r.published,to_char(s.completed_at AT TIME ZONE 'UTC','{STAMP}') AS completed,to_char(p.first_completed_at AT TIME ZONE 'UTC','{STAMP}') AS first_completed FROM learning_sessions s JOIN lesson_revisions r ON (r.lesson_id,r.revision)=(s.lesson_id,s.revision) LEFT JOIN lesson_progress p ON (p.user_id,p.lesson_id)=(s.user_id,s.lesson_id) WHERE s.user_id=$1 AND s.id=$2 {}",
        if lock {
            "FOR UPDATE OF s FOR SHARE OF r"
        } else {
            ""
        }
    );
    let row = one(db, &sql, vec![user.into(), id.into()])
        .await?
        .ok_or(AppError::NotFound)?;
    if !field::<bool>(&row, "published")? {
        return Err(AppError::Gone);
    }
    let lesson: PublicLesson = serde_json::from_value(field(&row, "public_document")?)
        .map_err(|_| AppError::Unavailable)?;
    lesson.validate().map_err(|_| AppError::Unavailable)?;
    if lesson.id != field::<String>(&row, "lesson_id")?
        || lesson.revision
            != u32::try_from(field::<i32>(&row, "revision")?).map_err(|_| AppError::Unavailable)?
        || lesson.schema_version != field::<String>(&row, "schema_version")?
    {
        return Err(AppError::Unavailable);
    }
    Ok(SessionRow {
        id: field(&row, "id")?,
        user,
        lesson,
        source: field(&row, "server_document")?,
        version: u32::try_from(field::<i32>(&row, "version")?)
            .map_err(|_| AppError::Unavailable)?,
        last: field(&row, "last_step_id")?,
        completed: field(&row, "completed")?,
        first_completed: field(&row, "first_completed")?,
    })
}
async fn progress<C: ConnectionTrait>(
    db: &C,
    session: &SessionRow,
) -> Result<LearningState, AppError> {
    let steps = db
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT step_id FROM step_progress WHERE session_id=$1",
            [session.id.clone().into()],
        ))
        .await
        .map_err(|_| AppError::Unavailable)?;
    let steps = steps
        .iter()
        .map(|r| field::<String>(r, "step_id"))
        .collect::<Result<BTreeSet<_>, _>>()?;
    let hints = db
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT exercise_id FROM exercise_hints WHERE session_id=$1 ORDER BY exercise_id",
            [session.id.clone().into()],
        ))
        .await
        .map_err(|_| AppError::Unavailable)?;
    let rows = db.query_all_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT id,exercise_id,attempt_index,answer,result,hint_used FROM exercise_attempts WHERE session_id=$1 AND user_id=$2 ORDER BY exercise_id,attempt_index",[session.id.clone().into(),session.user.into()])).await.map_err(|_| AppError::Unavailable)?;
    let attempts = rows
        .iter()
        .map(|r| {
            Ok(AttemptRecord {
                id: field(r, "id")?,
                exercise_id: field(r, "exercise_id")?,
                attempt_index: u32::try_from(field::<i32>(r, "attempt_index")?)
                    .map_err(|_| AppError::Unavailable)?,
                answer: serde_json::from_value(field(r, "answer")?)
                    .map_err(|_| AppError::Unavailable)?,
                result: serde_json::from_value(field(r, "result")?)
                    .map_err(|_| AppError::Unavailable)?,
                hint_used: field(r, "hint_used")?,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(LearningState {
        id: session.id.clone(),
        lesson_id: session.lesson.id.clone(),
        revision: session.lesson.revision,
        version: session.version,
        last_step_id: session.last.clone(),
        confirmed_step_ids: session
            .lesson
            .steps
            .iter()
            .filter(|s| steps.contains(&s.id))
            .map(|s| s.id.clone())
            .collect(),
        hinted_exercise_ids: hints
            .iter()
            .map(|r| field(r, "exercise_id"))
            .collect::<Result<_, _>>()?,
        attempts,
        completed_at: session.completed.clone(),
        first_completed_at: session.first_completed.clone(),
    })
}
fn check_version(session: &SessionRow, version: u32) -> Result<(), AppError> {
    if session.version != version || version >= i32::MAX as u32 {
        return Err(AppError::Conflict);
    }
    Ok(())
}
fn prerequisites(
    lesson: &PublicLesson,
    state: &LearningState,
    step_index: usize,
) -> Result<(), AppError> {
    if lesson.steps[..step_index].iter().any(|s| {
        lesson.completion.required_step_ids.contains(&s.id)
            && !state.confirmed_step_ids.contains(&s.id)
    }) {
        return Err(AppError::Conflict);
    }
    Ok(())
}
async fn bump(
    tx: &DatabaseTransaction,
    session: &SessionRow,
    step: Option<&str>,
    complete: bool,
) -> Result<(), AppError> {
    let changed = exec(tx,"UPDATE learning_sessions SET version=version+1,last_step_id=COALESCE($3,last_step_id),completed_at=CASE WHEN $4 THEN COALESCE(completed_at,CURRENT_TIMESTAMP) ELSE completed_at END,updated_at=CURRENT_TIMESTAMP WHERE user_id=$1 AND id=$2 AND version=$5",vec![session.user.into(),session.id.clone().into(),step.map(str::to_owned).into(),complete.into(),i32::try_from(session.version).map_err(|_| AppError::Unavailable)?.into()]).await?;
    if changed != 1 {
        return Err(AppError::Conflict);
    }
    Ok(())
}
async fn begin(
    backend: &Backend,
    user: i64,
    id: &str,
    key: &str,
) -> Result<(DatabaseTransaction, SessionRow), AppError> {
    validate_key(key)?;
    let tx = backend
        .db
        .begin()
        .await
        .map_err(|_| AppError::Unavailable)?;
    let session = load(&tx, user, id, true).await?;
    Ok((tx, session))
}
async fn start(
    auth: AuthSession,
    State(backend): State<Backend>,
    Json(request): Json<StartLearningRequest>,
) -> Result<Json<LearningSession>, AppError> {
    let user = owner(&auth)?;
    validate_key(&request.idempotency_key)?;
    if request.lesson_id.is_empty()
        || request.lesson_id.len() > 200
        || request.schema_version != "1.0"
    {
        return Err(AppError::InvalidInput);
    }
    let fingerprint = hash(&request)?;
    let tx = backend
        .db
        .begin()
        .await
        .map_err(|_| AppError::Unavailable)?;
    // Lock idempotency scope first, then the user's lesson, even for different request keys.
    exec(
        &tx,
        "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
        vec![format!("learning-start-key:{user}:{}", request.idempotency_key).into()],
    )
    .await?;
    if let Some(cached) =
        replay::<LearningSession>(&tx, user, "start", &request.idempotency_key, &fingerprint)
            .await?
    {
        load(&tx, user, &cached.progress.id, false).await?; // Withdrawn snapshots never escape through a replay.
        return Ok(Json(cached));
    }
    exec(
        &tx,
        "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
        vec![format!("learning-start-lesson:{user}:{}", request.lesson_id).into()],
    )
    .await?;
    let existing = one(&tx,"SELECT id FROM learning_sessions WHERE user_id=$1 AND lesson_id=$2 AND completed_at IS NULL",vec![user.into(),request.lesson_id.clone().into()]).await?;
    let id = if let Some(existing) = existing {
        field::<String>(&existing, "id")?
    } else {
        let state = one(
            &tx,
            "SELECT active_release FROM content_state WHERE singleton FOR SHARE",
            vec![],
        )
        .await?
        .ok_or(AppError::Unavailable)?;
        let release: Option<String> = field(&state, "active_release")?;
        let row = one(&tx,"SELECT r.revision,r.public_document FROM release_entries e JOIN lesson_revisions r ON (r.lesson_id,r.revision)=(e.lesson_id,e.revision) WHERE e.release_id=$1 AND e.lesson_id=$2 AND r.published FOR SHARE OF r",vec![release.into(),request.lesson_id.clone().into()]).await?.ok_or(AppError::NotFound)?;
        let lesson: PublicLesson = serde_json::from_value(field(&row, "public_document")?)
            .map_err(|_| AppError::Unavailable)?;
        lesson.validate().map_err(|_| AppError::Unavailable)?;
        if lesson.schema_version != request.schema_version {
            return Err(AppError::Conflict);
        }
        let id = random_id()?;
        exec(&tx,"INSERT INTO learning_sessions (id,user_id,lesson_id,revision,schema_version,last_step_id) VALUES ($1,$2,$3,$4,$5,$6)",vec![id.clone().into(),user.into(),request.lesson_id.clone().into(),field::<i32>(&row,"revision")?.into(),request.schema_version.clone().into(),lesson.steps.first().map(|s| s.id.clone()).into()]).await?;
        exec(&tx,"INSERT INTO lesson_progress (user_id,lesson_id,last_session_id) VALUES ($1,$2,$3) ON CONFLICT (user_id,lesson_id) DO UPDATE SET last_session_id=EXCLUDED.last_session_id",vec![user.into(),request.lesson_id.clone().into(),id.clone().into()]).await?;
        id
    };
    let session = load(&tx, user, &id, true).await?;
    if session.lesson.schema_version != request.schema_version {
        return Err(AppError::Conflict);
    }
    let result = LearningSession {
        progress: progress(&tx, &session).await?,
        lesson: session.lesson,
    };
    record(
        &tx,
        user,
        "start",
        &request.idempotency_key,
        &fingerprint,
        &result,
    )
    .await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(result))
}
async fn get_session(
    auth: AuthSession,
    State(backend): State<Backend>,
    Path(id): Path<String>,
) -> Result<Json<LearningSession>, AppError> {
    let user = owner(&auth)?;
    let tx = backend
        .db
        .begin()
        .await
        .map_err(|_| AppError::Unavailable)?;
    // The row lock makes the document and progress a consistent read while a writer commits.
    let session = load(&tx, user, &id, true).await?;
    let result = LearningSession {
        progress: progress(&tx, &session).await?,
        lesson: session.lesson,
    };
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(result))
}
async fn confirm_step(
    auth: AuthSession,
    State(backend): State<Backend>,
    Path((id, step)): Path<(String, String)>,
    Json(request): Json<LearningWriteRequest>,
) -> Result<Json<LearningState>, AppError> {
    let user = owner(&auth)?;
    let scope = format!("{id}:step:{step}");
    let fingerprint = hash(&request)?;
    let (tx, session) = begin(&backend, user, &id, &request.idempotency_key).await?;
    if let Some(cached) = replay(&tx, user, &scope, &request.idempotency_key, &fingerprint).await? {
        return Ok(Json(cached));
    }
    check_version(&session, request.version)?;
    let index = session
        .lesson
        .steps
        .iter()
        .position(|s| s.id == step)
        .ok_or(AppError::NotFound)?;
    let state = progress(&tx, &session).await?;
    if !state.confirmed_step_ids.contains(&step) {
        if session.completed.is_some() {
            return Err(AppError::Conflict);
        }
        prerequisites(&session.lesson, &state, index)?;
        let target = &session.lesson.steps[index];
        if session
            .lesson
            .completion
            .required_exercise_ids
            .iter()
            .any(|exercise| {
                target.block_ids.contains(exercise)
                    && !state.attempts.iter().any(|a| &a.exercise_id == exercise)
            })
        {
            return Err(AppError::Conflict);
        }
        exec(
            &tx,
            "INSERT INTO step_progress (session_id,step_id) VALUES ($1,$2)",
            vec![id.clone().into(), step.clone().into()],
        )
        .await?;
        // Resume at the following step; optional steps can be revisited without erasing confirmations.
        let next = session
            .lesson
            .steps
            .get(index + 1)
            .map(|s| s.id.as_str())
            .unwrap_or(&step);
        bump(&tx, &session, Some(next), false).await?;
    }
    let state = progress(&tx, &load(&tx, user, &id, false).await?).await?;
    record(
        &tx,
        user,
        &scope,
        &request.idempotency_key,
        &fingerprint,
        &state,
    )
    .await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(state))
}
async fn submit(
    auth: AuthSession,
    State(backend): State<Backend>,
    Path(id): Path<String>,
    Json(request): Json<SubmitAttemptRequest>,
) -> Result<Json<AttemptResult>, AppError> {
    let user = owner(&auth)?;
    let scope = format!("{id}:attempt");
    let fingerprint = hash(&request)?;
    let (tx, session) = begin(&backend, user, &id, &request.idempotency_key).await?;
    if let Some(cached) = replay(&tx, user, &scope, &request.idempotency_key, &fingerprint).await? {
        return Ok(Json(cached));
    }
    check_version(&session, request.version)?;
    if session.completed.is_some() {
        return Err(AppError::Conflict);
    }
    let index = session
        .lesson
        .steps
        .iter()
        .position(|s| s.block_ids.contains(&request.exercise_id))
        .ok_or(AppError::NotFound)?;
    let state = progress(&tx, &session).await?;
    prerequisites(&session.lesson, &state, index)?;
    let result = Grader::from_source(&session.lesson, &session.source)
        .map_err(|_| AppError::Unavailable)?
        .grade(&session.lesson, &request.exercise_id, &request.answer)
        .map_err(|e| match e {
            GradeError::UnknownExercise => AppError::NotFound,
            GradeError::InvalidAnswer => AppError::InvalidAnswer,
            GradeError::InvalidContent => AppError::Unavailable,
        })?;
    let attempt_index = state
        .attempts
        .iter()
        .filter(|a| a.exercise_id == request.exercise_id)
        .count()
        + 1;
    if attempt_index > 100 {
        return Err(AppError::RateLimited);
    }
    exec(&tx,"INSERT INTO exercise_attempts (id,session_id,user_id,exercise_id,attempt_index,answer,result,hint_used) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",vec![random_id()?.into(),id.clone().into(),user.into(),request.exercise_id.clone().into(),(attempt_index as i32).into(),serde_json::to_value(&request.answer).map_err(|_| AppError::Unavailable)?.into(),serde_json::to_value(&result).map_err(|_| AppError::Unavailable)?.into(),state.hinted_exercise_ids.contains(&request.exercise_id).into()]).await?;
    bump(&tx, &session, Some(&session.lesson.steps[index].id), false).await?;
    let response = AttemptResult {
        result,
        progress: progress(&tx, &load(&tx, user, &id, false).await?).await?,
    };
    record(
        &tx,
        user,
        &scope,
        &request.idempotency_key,
        &fingerprint,
        &response,
    )
    .await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(response))
}
async fn hint(
    auth: AuthSession,
    State(backend): State<Backend>,
    Path((id, exercise)): Path<(String, String)>,
    Json(request): Json<LearningWriteRequest>,
) -> Result<Json<HintResult>, AppError> {
    let user = owner(&auth)?;
    let scope = format!("{id}:hint:{exercise}");
    let fingerprint = hash(&request)?;
    let (tx, session) = begin(&backend, user, &id, &request.idempotency_key).await?;
    if let Some(cached) = replay(&tx, user, &scope, &request.idempotency_key, &fingerprint).await? {
        return Ok(Json(cached));
    }
    check_version(&session, request.version)?;
    if session.completed.is_some() {
        return Err(AppError::Conflict);
    }
    let hint = session
        .lesson
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Exercise {
                id,
                exercise: Exercise::FillBlank { hint_zh, .. },
            } if id == &exercise && !hint_zh.trim().is_empty() => Some(hint_zh.clone()),
            _ => None,
        })
        .ok_or(AppError::NotFound)?;
    let index = session
        .lesson
        .steps
        .iter()
        .position(|s| s.block_ids.contains(&exercise))
        .ok_or(AppError::Unavailable)?;
    prerequisites(&session.lesson, &progress(&tx, &session).await?, index)?;
    if exec(
        &tx,
        "INSERT INTO exercise_hints (session_id,exercise_id) VALUES ($1,$2) ON CONFLICT DO NOTHING",
        vec![id.clone().into(), exercise.into()],
    )
    .await?
        > 0
    {
        bump(&tx, &session, Some(&session.lesson.steps[index].id), false).await?;
    }
    let response = HintResult {
        hint_zh: hint,
        progress: progress(&tx, &load(&tx, user, &id, false).await?).await?,
    };
    record(
        &tx,
        user,
        &scope,
        &request.idempotency_key,
        &fingerprint,
        &response,
    )
    .await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(response))
}
async fn complete(
    auth: AuthSession,
    State(backend): State<Backend>,
    Path(id): Path<String>,
    Json(request): Json<LearningWriteRequest>,
) -> Result<Json<LearningState>, AppError> {
    let user = owner(&auth)?;
    let scope = format!("{id}:complete");
    let fingerprint = hash(&request)?;
    let (tx, session) = begin(&backend, user, &id, &request.idempotency_key).await?;
    if let Some(cached) = replay(&tx, user, &scope, &request.idempotency_key, &fingerprint).await? {
        return Ok(Json(cached));
    }
    check_version(&session, request.version)?;
    if session.completed.is_none() {
        let state = progress(&tx, &session).await?;
        if session
            .lesson
            .completion
            .required_step_ids
            .iter()
            .any(|s| !state.confirmed_step_ids.contains(s))
            || session
                .lesson
                .completion
                .required_exercise_ids
                .iter()
                .any(|e| !state.attempts.iter().any(|a| &a.exercise_id == e))
        {
            return Err(AppError::Conflict);
        }
        bump(&tx, &session, None, true).await?;
        exec(&tx,"UPDATE lesson_progress SET first_completed_at=COALESCE(first_completed_at,CURRENT_TIMESTAMP),latest_completed_revision=$3 WHERE user_id=$1 AND lesson_id=$2",vec![user.into(),session.lesson.id.clone().into(),i32::try_from(session.lesson.revision).map_err(|_| AppError::Unavailable)?.into()]).await?;
        // Deterministic knowledge lock order avoids deadlocks when different lessons share expressions.
        for knowledge in session
            .lesson
            .review_item_ids
            .iter()
            .collect::<BTreeSet<_>>()
        {
            let vocabulary = session
                .lesson
                .knowledge
                .vocabulary
                .iter()
                .find(|v| &v.id == knowledge)
                .ok_or(AppError::Unavailable)?;
            exec(&tx,"INSERT INTO review_cards (id,user_id,knowledge_id,source_lesson_id,source_revision,snapshot) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (user_id,knowledge_id) DO NOTHING",vec![random_id()?.into(),user.into(),knowledge.clone().into(),session.lesson.id.clone().into(),i32::try_from(session.lesson.revision).map_err(|_| AppError::Unavailable)?.into(),serde_json::to_value(vocabulary).map_err(|_| AppError::Unavailable)?.into()]).await?;
        }
    }
    let state = progress(&tx, &load(&tx, user, &id, false).await?).await?;
    record(
        &tx,
        user,
        &scope,
        &request.idempotency_key,
        &fingerprint,
        &state,
    )
    .await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(state))
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    cursor: Option<String>,
}
async fn overview(
    auth: AuthSession,
    State(backend): State<Backend>,
    Query(page): Query<Page>,
) -> Result<Json<LearningOverview>, AppError> {
    let user = owner(&auth)?;
    let (stamp, id) = if let Some(cursor) = page.cursor {
        let (stamp, id) = cursor.split_once('@').ok_or(AppError::InvalidInput)?;
        if stamp.len() > 40
            || stamp.parse::<jiff::Timestamp>().is_err()
            || id.len() != 32
            || !id.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(AppError::InvalidInput);
        }
        (Some(stamp.to_owned()), Some(id.to_owned()))
    } else {
        (None, None)
    };
    let sql = format!(
        "SELECT s.id,s.lesson_id,s.revision,s.last_step_id,r.public_document->'title' AS title,to_char(s.completed_at AT TIME ZONE 'UTC','{STAMP}') AS completed,to_char(p.first_completed_at AT TIME ZONE 'UTC','{STAMP}') AS first_completed,to_char(s.updated_at AT TIME ZONE 'UTC','{STAMP}') AS updated FROM lesson_progress p JOIN learning_sessions s ON s.id=p.last_session_id AND s.user_id=p.user_id JOIN lesson_revisions r ON (r.lesson_id,r.revision)=(s.lesson_id,s.revision) WHERE p.user_id=$1 AND r.published=true AND ($2::timestamptz IS NULL OR (s.updated_at,s.id)<($2::timestamptz,$3::text)) ORDER BY s.updated_at DESC,s.id DESC LIMIT 21"
    );
    let rows = backend
        .db
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            &sql,
            [user.into(), stamp.into(), id.into()],
        ))
        .await
        .map_err(|_| AppError::Unavailable)?;
    let more = rows.len() > 20;
    let items = rows
        .iter()
        .take(20)
        .map(|r| {
            Ok(LearningOverviewItem {
                session_id: field(r, "id")?,
                lesson_id: field(r, "lesson_id")?,
                revision: u32::try_from(field::<i32>(r, "revision")?)
                    .map_err(|_| AppError::Unavailable)?,
                title: serde_json::from_value(field(r, "title")?)
                    .map_err(|_| AppError::Unavailable)?,
                last_step_id: field(r, "last_step_id")?,
                completed_at: field(r, "completed")?,
                first_completed_at: field(r, "first_completed")?,
                updated_at: field(r, "updated")?,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let next_cursor = if more {
        items
            .last()
            .map(|i| format!("{}@{}", i.updated_at, i.session_id))
    } else {
        None
    };
    let count = one(&backend.db,"SELECT count(*)::bigint AS completed FROM lesson_progress WHERE user_id=$1 AND first_completed_at IS NOT NULL",vec![user.into()]).await?.ok_or(AppError::Unavailable)?;
    Ok(Json(LearningOverview {
        items,
        next_cursor,
        completed_lessons: u32::try_from(field::<i64>(&count, "completed")?)
            .map_err(|_| AppError::Unavailable)?,
    }))
}
pub fn router() -> Router<Backend> {
    Router::new()
        .route("/api/v1/learning-sessions", post(start))
        .route("/api/v1/learning-sessions/{id}", get(get_session))
        .route(
            "/api/v1/learning-sessions/{id}/steps/{step}",
            put(confirm_step),
        )
        .route("/api/v1/learning-sessions/{id}/attempts", post(submit))
        .route(
            "/api/v1/learning-sessions/{id}/hints/{exercise}",
            post(hint),
        )
        .route("/api/v1/learning-sessions/{id}/complete", post(complete))
        .route("/api/v1/me/learning", get(overview))
}
