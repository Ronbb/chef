//! Identity-owned account administration; product scope comes only from server configuration.
use crate::{
    AppError,
    identity::{AuthSession, Backend},
    learning::{exec, field, one, owner},
    product::ProductId,
};
use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait};

pub(crate) fn router(product: ProductId) -> Router<Backend> {
    Router::new()
        .route("/api/v1/operator/accounts", get(accounts))
        .route("/api/v1/operator/accounts/history", get(history))
        .route("/api/v1/operator/accounts/token", post(account_token))
        .route(
            "/api/v1/operator/accounts/pending-tokens",
            get(pending_tokens),
        )
        .route(
            "/api/v1/operator/accounts/pending-tokens/{id}/revoke",
            post(revoke_token),
        )
        .route("/api/v1/operator/accounts/{id}/role", post(account_role))
        .route(
            "/api/v1/operator/accounts/{id}/sessions",
            get(account_sessions),
        )
        .route(
            "/api/v1/operator/accounts/{id}/sessions/{session}/revoke",
            post(revoke_session),
        )
        .layer(Extension(product))
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AccountQuery {
    after_id: Option<String>,
    q: Option<String>,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HistoryQuery {
    before_time: Option<String>,
    before_key: Option<String>,
}
async fn history(
    auth: AuthSession,
    Extension(product): Extension<ProductId>,
    State(backend): State<Backend>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<brioche_course_contract::AdminHistory>, AppError> {
    use brioche_course_contract::{AdminHistory, AdminHistoryCursor, AdminHistoryItem};
    require_operator(&auth, product).await?;
    if query.before_time.is_some() != query.before_key.is_some()
        || query
            .before_time
            .as_ref()
            .is_some_and(|value| value.len() > 40 || value.parse::<jiff::Timestamp>().is_err())
        || query.before_key.as_ref().is_some_and(|value| {
            value.is_empty()
                || value.len() > 256
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-_:".contains(&byte))
        })
    {
        return Err(AppError::InvalidInput);
    }
    let rows=backend.db.query_all_raw(Statement::from_sql_and_values(DbBackend::Postgres,r#"
        WITH events AS (
            SELECT 'account:'||id AS key, CASE WHEN action='invite' AND details->>'role'='operator' THEN 'inviteOperator' ELSE action END AS action,
                target_email AS target,'user:'||actor_id AS actor,reason,created_at
            FROM account_admin_audit WHERE product_id=$3
        )
        SELECT key,action,target,actor,reason,to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"') AS created_at
        FROM events WHERE $1::timestamptz IS NULL OR (created_at,key COLLATE "C") < ($1::timestamptz,$2::text COLLATE "C")
        ORDER BY created_at DESC,key COLLATE "C" DESC LIMIT 21
    "#,vec![query.before_time.into(),query.before_key.into(),product.as_str().into()])).await.map_err(|_|AppError::Unavailable)?;
    let more = rows.len() > 20;
    let items = rows
        .into_iter()
        .take(20)
        .map(|row| {
            Ok(AdminHistoryItem {
                key: field(&row, "key")?,
                action: field(&row, "action")?,
                target: field(&row, "target")?,
                actor: field(&row, "actor")?,
                reason: field(&row, "reason")?,
                created_at: field(&row, "created_at")?,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let next = if more {
        items.last().map(|item| AdminHistoryCursor {
            before_time: item.created_at.clone(),
            before_key: item.key.clone(),
        })
    } else {
        None
    };
    Ok(Json(AdminHistory { items, next }))
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionQuery {
    after_id: Option<String>,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TokenQuery {
    after_id: Option<String>,
    kind: Option<brioche_course_contract::AdminTokenKind>,
}
async fn pending_tokens(
    auth: AuthSession,
    Extension(product): Extension<ProductId>,
    State(backend): State<Backend>,
    Query(query): Query<TokenQuery>,
) -> Result<Json<brioche_course_contract::AdminPendingTokens>, AppError> {
    use brioche_course_contract::{
        AdminAccountRole, AdminPendingToken, AdminPendingTokens, AdminTokenKind,
    };
    require_operator(&auth, product).await?;
    let after = query.after_id.unwrap_or_default();
    if !after.is_empty() {
        session_key(&after)?;
    }
    let kind = query.kind.map(|kind| match kind {
        AdminTokenKind::Invite => "invite",
        AdminTokenKind::Reset => "reset",
    });
    let rows=backend.db.query_all_raw(Statement::from_sql_and_values(DbBackend::Postgres,r#"WITH pending AS (SELECT encode(sha256(convert_to(token_hash,'UTF8')),'hex') AS id,email,kind,role,expires_at FROM identity_tokens WHERE product_id=$3 AND consumed_at IS NULL AND expires_at>CURRENT_TIMESTAMP AND ($1::text IS NULL OR kind=$1)) SELECT id,email,kind,role,to_char(expires_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US"Z"') AS expires_at FROM pending WHERE id>$2 ORDER BY id LIMIT 21"#,vec![kind.into(),after.into(),product.as_str().into()])).await.map_err(|_|AppError::Unavailable)?;
    let more = rows.len() > 20;
    let items: Vec<AdminPendingToken> = rows
        .into_iter()
        .take(20)
        .map(|row| {
            Ok(AdminPendingToken {
                id: field(&row, "id")?,
                email: field(&row, "email")?,
                kind: match field::<String>(&row, "kind")?.as_str() {
                    "invite" => AdminTokenKind::Invite,
                    "reset" => AdminTokenKind::Reset,
                    _ => return Err(AppError::Unavailable),
                },
                role: match field::<String>(&row, "role")?.as_str() {
                    "operator" => AdminAccountRole::Operator,
                    "learner" => AdminAccountRole::Learner,
                    _ => return Err(AppError::Unavailable),
                },
                expires_at: field(&row, "expires_at")?,
            })
        })
        .collect::<Result<_, AppError>>()?;
    let next_id = if more {
        items.last().map(|item| item.id.clone())
    } else {
        None
    };
    Ok(Json(AdminPendingTokens { items, next_id }))
}
async fn revoke_token(
    auth: AuthSession,
    Extension(product): Extension<ProductId>,
    State(backend): State<Backend>,
    Path(id): Path<String>,
    Json(request): Json<brioche_course_contract::AdminRevokeTokenRequest>,
) -> Result<Json<bool>, AppError> {
    require_operator(&auth, product).await?;
    session_key(&id)?;
    reason(&request.reason)?;
    let actor = owner(&auth)?;
    let tx = backend
        .db
        .begin()
        .await
        .map_err(|_| AppError::Unavailable)?;
    exec(
        &tx,
        "SELECT pg_advisory_xact_lock(hashtextextended('account-admin',0))",
        vec![],
    )
    .await?;
    crate::product_memberships::require_operator(&tx, product, actor).await?;
    let row=one(&tx,"SELECT email FROM identity_tokens WHERE product_id=$2 AND encode(sha256(convert_to(token_hash,'UTF8')),'hex')=$1 AND consumed_at IS NULL AND expires_at>CURRENT_TIMESTAMP",vec![id.clone().into(),product.as_str().into()]).await?.ok_or(AppError::NotFound)?;
    let email: String = field(&row, "email")?;
    // Same mailbox lock as issue/accept/reset: revocation and consumption have one winner.
    exec(
        &tx,
        "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
        vec![email.clone().into()],
    )
    .await?;
    let revoked=one(&tx,"UPDATE identity_tokens SET consumed_at=CURRENT_TIMESTAMP WHERE product_id=$2 AND encode(sha256(convert_to(token_hash,'UTF8')),'hex')=$1 AND consumed_at IS NULL AND expires_at>CURRENT_TIMESTAMP RETURNING kind",vec![id.clone().into(),product.as_str().into()]).await?.ok_or(AppError::NotFound)?;
    let kind: String = field(&revoked, "kind")?;
    let action = match kind.as_str() {
        "invite" => "revokeInvite",
        "reset" => "revokeReset",
        _ => return Err(AppError::Unavailable),
    };
    exec(&tx,"INSERT INTO account_admin_audit(action,actor_id,target_email,details,reason,product_id) VALUES($1,$2,$3,$4,$5,$6)",vec![action.into(),actor.into(),email.into(),serde_json::json!({"recordId":id,"kind":kind}).into(),request.reason.into(),product.as_str().into()]).await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(true))
}
fn session_key(value: &str) -> Result<(), AppError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(AppError::InvalidInput);
    }
    Ok(())
}
async fn account_sessions(
    auth: AuthSession,
    Extension(product): Extension<ProductId>,
    State(backend): State<Backend>,
    Path(id): Path<String>,
    Query(query): Query<SessionQuery>,
) -> Result<Json<brioche_course_contract::AdminSessions>, AppError> {
    use brioche_course_contract::{AdminAccount, AdminSession, AdminSessions};
    require_operator(&auth, product).await?;
    let user_id = generation(&id)?;
    if user_id == 0 {
        return Err(AppError::InvalidInput);
    }
    let after = query.after_id.unwrap_or_default();
    if !after.is_empty() {
        session_key(&after)?;
    }
    let row = one(
        &backend.db,
        "SELECT u.email,u.display_name,COALESCE(m.role,'learner') AS role FROM users u LEFT JOIN product_memberships m ON m.user_id=u.id AND m.product_id=$2 WHERE u.id=$1",
        vec![user_id.into(),product.as_str().into()],
    )
    .await?
    .ok_or(AppError::NotFound)?;
    let account = AdminAccount {
        id: id.clone(),
        email: field(&row, "email")?,
        display_name: field(&row, "display_name")?,
        role: field(&row, "role")?,
    };
    let current = auth.session.id().map(|id| crate::session_store::hash(&id));
    let rows=backend.db.query_all_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT id_hash,to_char(expires_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS expires_at FROM browser_sessions WHERE data #>> '{brioche.auth,user_id}'=$1 AND (data->>'chef.product'=$3 OR ($3='brioche' AND NOT data ? 'chef.product')) AND expires_at>CURRENT_TIMESTAMP AND id_hash>$2 ORDER BY id_hash LIMIT 21",vec![id.into(),after.into(),product.as_str().into()])).await.map_err(|_|AppError::Unavailable)?;
    let more = rows.len() > 20;
    let mut items = Vec::new();
    for row in rows.into_iter().take(20) {
        let key: String = field(&row, "id_hash")?;
        items.push(AdminSession {
            current: current.as_ref() == Some(&key),
            id: key,
            expires_at: field(&row, "expires_at")?,
        });
    }
    let next_id = if more {
        items.last().map(|item| item.id.clone())
    } else {
        None
    };
    Ok(Json(AdminSessions {
        account,
        items,
        next_id,
    }))
}
async fn revoke_session(
    mut auth: AuthSession,
    Extension(product): Extension<ProductId>,
    State(backend): State<Backend>,
    Path((id, key)): Path<(String, String)>,
    Json(request): Json<brioche_course_contract::AdminRevokeSessionRequest>,
) -> Result<Json<brioche_course_contract::AdminRevokeSessionResult>, AppError> {
    require_operator(&auth, product).await?;
    reason(&request.reason)?;
    let target = generation(&id)?;
    if target == 0 {
        return Err(AppError::InvalidInput);
    }
    session_key(&key)?;
    let actor = owner(&auth)?;
    let current = auth
        .session
        .id()
        .is_some_and(|session| crate::session_store::hash(&session) == key);
    let tx = backend
        .db
        .begin()
        .await
        .map_err(|_| AppError::Unavailable)?;
    exec(
        &tx,
        "SELECT pg_advisory_xact_lock(hashtextextended('account-admin',0))",
        vec![],
    )
    .await?;
    crate::product_memberships::require_operator(&tx, product, actor).await?;
    let user = one(
        &tx,
        "SELECT email FROM users WHERE id=$1",
        vec![target.into()],
    )
    .await?
    .ok_or(AppError::NotFound)?;
    let deleted=tx.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"DELETE FROM browser_sessions WHERE id_hash=$1 AND data #>> '{brioche.auth,user_id}'=$2 AND (data->>'chef.product'=$3 OR ($3='brioche' AND NOT data ? 'chef.product')) RETURNING id_hash",vec![key.clone().into(),id.clone().into(),product.as_str().into()])).await.map_err(|_|AppError::Unavailable)?;
    if deleted.rows_affected() != 1 {
        return Err(AppError::NotFound);
    }
    exec(&tx,"INSERT INTO account_admin_audit(action,actor_id,target_email,reason,details,product_id) VALUES('sessions',$1,$2,$3,$4,$5)",vec![actor.into(),field::<String>(&user,"email")?.into(),request.reason.into(),serde_json::json!({"userId":id,"sessionRecord":key,"current":current}).into(),product.as_str().into()]).await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    if current {
        auth.logout().await.map_err(|_| AppError::Unavailable)?;
    }
    Ok(Json(brioche_course_contract::AdminRevokeSessionResult {
        current,
    }))
}
async fn accounts(
    auth: AuthSession,
    Extension(product): Extension<ProductId>,
    State(backend): State<Backend>,
    Query(query): Query<AccountQuery>,
) -> Result<Json<brioche_course_contract::AdminAccounts>, AppError> {
    use brioche_course_contract::{AdminAccount, AdminAccounts};
    require_operator(&auth, product).await?;
    let after = generation(query.after_id.as_deref().unwrap_or("0"))?;
    let search = query.q.unwrap_or_default().trim().to_owned();
    if search.len() > 300 || search.chars().count() > 100 || search.chars().any(char::is_control) {
        return Err(AppError::InvalidInput);
    }
    let rows=backend.db.query_all_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT u.id,u.email,u.display_name,COALESCE(m.role,'learner') AS role FROM users u LEFT JOIN product_memberships m ON m.user_id=u.id AND m.product_id=$3 WHERE u.id>$1 AND ($2='' OR strpos(lower(u.email||' '||u.display_name),lower($2))>0) ORDER BY u.id LIMIT 21",vec![after.into(),search.into(),product.as_str().into()])).await.map_err(|_|AppError::Unavailable)?;
    let has_more = rows.len() > 20;
    let mut items = Vec::new();
    for row in rows.into_iter().take(20) {
        items.push(AdminAccount {
            id: field::<i64>(&row, "id")?.to_string(),
            email: field(&row, "email")?,
            display_name: field(&row, "display_name")?,
            role: field(&row, "role")?,
        });
    }
    let next_id = if has_more {
        items.last().map(|item| item.id.clone())
    } else {
        None
    };
    Ok(Json(AdminAccounts { items, next_id }))
}
async fn account_token(
    auth: AuthSession,
    Extension(product): Extension<ProductId>,
    State(backend): State<Backend>,
    Json(request): Json<brioche_course_contract::AdminTokenRequest>,
) -> Result<Json<brioche_course_contract::AdminTokenResult>, AppError> {
    use brioche_course_contract::{AdminTokenKind, AdminTokenResult};
    require_operator(&auth, product).await?;
    let reset = matches!(request.kind, AdminTokenKind::Reset);
    if reset && request.operator {
        return Err(AppError::InvalidInput);
    }
    let email = crate::identity::normalize_email(&request.email)?;
    let token = backend
        .issue_operator_token(
            product,
            &email,
            reset,
            request.operator,
            owner(&auth)?,
            &request.reason,
        )
        .await?;
    Ok(Json(AdminTokenResult {
        token,
        email,
        kind: request.kind,
        expires_in_seconds: if reset { 1800 } else { 172800 },
    }))
}
async fn account_role(
    auth: AuthSession,
    Extension(product): Extension<ProductId>,
    State(backend): State<Backend>,
    Path(id): Path<String>,
    Json(request): Json<brioche_course_contract::AdminRoleRequest>,
) -> Result<Json<brioche_course_contract::AdminAccount>, AppError> {
    use brioche_course_contract::{AdminAccount, AdminAccountRole};
    require_operator(&auth, product).await?;
    reason(&request.reason)?;
    let target = generation(&id)?;
    if target == 0 {
        return Err(AppError::InvalidInput);
    }
    let role_name = |role| match role {
        AdminAccountRole::Learner => "learner",
        AdminAccountRole::Operator => "operator",
    };
    let expected = role_name(request.expected_role);
    let desired = role_name(request.role);
    let actor = owner(&auth)?;
    let tx = backend
        .db
        .begin()
        .await
        .map_err(|_| AppError::Unavailable)?;
    exec(
        &tx,
        "SELECT pg_advisory_xact_lock(hashtextextended('account-admin',0))",
        vec![],
    )
    .await?;
    crate::product_memberships::require_operator(&tx, product, actor).await?;
    let row = one(
        &tx,
        "SELECT email,display_name,role FROM users WHERE id=$1 FOR UPDATE",
        vec![target.into()],
    )
    .await?
    .ok_or(AppError::NotFound)?;
    let membership = crate::product_memberships::read(&tx, product, target).await?;
    let current = membership.role;
    if current != expected {
        return Err(AppError::Conflict);
    }
    let email: String = field(&row, "email")?;
    if current != desired {
        if current == "operator" {
            let count = one(
                &tx,
                "SELECT count(*) AS n FROM product_memberships WHERE product_id=$1 AND role='operator'",
                vec![product.as_str().into()],
            )
            .await?
            .ok_or(AppError::Unavailable)?;
            if field::<i64>(&count, "n")? <= 1 {
                return Err(AppError::Conflict);
            }
        }
        exec(
            &tx,
            "INSERT INTO product_memberships(product_id,user_id,role,version) VALUES($4,$1,$2,$3) ON CONFLICT(product_id,user_id) DO UPDATE SET role=EXCLUDED.role,version=EXCLUDED.version",
            vec![target.into(), desired.into(), i32::try_from(membership.version.checked_add(1).ok_or(AppError::Conflict)?).map_err(|_| AppError::Conflict)?.into(), product.as_str().into()],
        )
        .await?;
        exec(&tx, "INSERT INTO product_membership_audit(product_id,actor_id,target_id,old_role,new_role,old_version,new_version,reason) VALUES($8,$1,$2,$3,$4,$5,$6,$7)", vec![actor.into(),target.into(),if membership.version==0 { None::<String> } else { Some(current.clone()) }.into(),desired.into(),i32::try_from(membership.version).map_err(|_|AppError::Conflict)?.into(),i32::try_from(membership.version+1).map_err(|_|AppError::Conflict)?.into(),request.reason.clone().into(),product.as_str().into()]).await?;
        exec(&tx, "INSERT INTO account_admin_audit(action,actor_id,target_email,reason,details,product_id) VALUES('role',$1,$2,$3,$4,$5)", vec![actor.into(),email.clone().into(),request.reason.into(),serde_json::json!({"userId":id,"from":current,"to":desired}).into(),product.as_str().into()]).await?;
    }
    let account = AdminAccount {
        id,
        email,
        display_name: field(&row, "display_name")?,
        role: desired.into(),
    };
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(account))
}
pub(crate) fn reason(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() || value.len() > 1000 || value.chars().any(char::is_control) {
        return Err(AppError::InvalidInput);
    }
    Ok(())
}
pub(crate) fn generation(value: &str) -> Result<i64, AppError> {
    let result = value.parse::<i64>().map_err(|_| AppError::InvalidInput)?;
    if result < 0 || result.to_string() != value {
        return Err(AppError::InvalidInput);
    }
    Ok(result)
}

async fn require_operator(auth: &AuthSession, product: ProductId) -> Result<(), AppError> {
    crate::product_memberships::require_operator(&auth.backend.db, product, owner(auth)?).await?;
    Ok(())
}
