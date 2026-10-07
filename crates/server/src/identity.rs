use crate::{
    AppError,
    csrf::{self, CsrfPolicy},
    password::PasswordService,
    session_store::PgSessionStore,
};
use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use axum_login::{AuthManagerLayerBuilder, AuthUser, AuthnBackend};
use brioche_course_contract::{
    AcceptInviteRequest, AuthResult, CsrfToken, LoginRequest, ResetPasswordRequest,
    UpdateProfileRequest, UserProfile, UserSettings,
};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend, QueryResult, Statement, TransactionTrait,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tower_sessions::{Expiry, SessionManagerLayer, cookie::SameSite};

const AUTH_KEY: &str = "brioche.auth";
#[derive(Clone)]
pub struct User {
    id: i64,
    email: String,
    display_name: String,
    role: String,
    password_hash: String,
    version: u32,
}
impl std::fmt::Debug for User {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("User")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}
impl User {
    async fn profile(&self, db: &DatabaseConnection) -> Result<UserProfile, AppError> {
        let preferences =
            crate::product_settings::read(db, crate::product::ProductId::Brioche, self.id).await?;
        Ok(UserProfile {
            id: self.id.to_string(),
            email: self.email.clone(),
            display_name: self.display_name.clone(),
            role: crate::product_memberships::read(db, crate::product::ProductId::Brioche, self.id)
                .await?
                .role,
            settings: preferences.settings,
            version: preferences.version,
        })
    }
    fn account(&self) -> crate::identity_service::AccountProfile {
        crate::identity_service::AccountProfile {
            id: self.id.to_string(),
            email: self.email.clone(),
            display_name: self.display_name.clone(),
            role: self.role.clone(),
            version: self.version,
        }
    }
}
impl AuthUser for User {
    type Id = i64;
    fn id(&self) -> i64 {
        self.id
    }
    fn session_auth_hash(&self) -> &[u8] {
        self.password_hash.as_bytes()
    }
}
#[derive(Clone)]
pub struct Backend {
    pub(crate) db: DatabaseConnection,
    passwords: PasswordService,
    dummy_hash: String,
}
fn row_user(row: QueryResult) -> Result<User, AppError> {
    Ok(User {
        id: row.try_get("", "id").map_err(|_| AppError::Unavailable)?,
        email: row
            .try_get("", "email")
            .map_err(|_| AppError::Unavailable)?,
        display_name: row
            .try_get("", "display_name")
            .map_err(|_| AppError::Unavailable)?,
        role: row.try_get("", "role").map_err(|_| AppError::Unavailable)?,
        password_hash: row
            .try_get("", "password_hash")
            .map_err(|_| AppError::Unavailable)?,
        version: u32::try_from(
            row.try_get::<i32>("", "profile_version")
                .map_err(|_| AppError::Unavailable)?,
        )
        .map_err(|_| AppError::Unavailable)?,
    })
}
pub fn normalize_email(email: &str) -> Result<String, AppError> {
    let email = email.trim().to_ascii_lowercase();
    let Some((local, domain)) = email.split_once('@') else {
        return Err(AppError::InvalidInput);
    };
    if email.len() > 254
        || local.is_empty()
        || local.len() > 64
        || !domain.contains('.')
        || domain.starts_with('.')
        || domain.ends_with('.')
        || email.contains(char::is_whitespace)
        || email.contains(['\r', '\n', '\0'])
        || !email.is_ascii()
        || domain.contains('@')
    {
        return Err(AppError::InvalidInput);
    }
    Ok(email)
}
fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn token_hash(token: &str) -> Result<String, AppError> {
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AppError::InvalidInput);
    }
    Ok(digest(token))
}
impl Backend {
    pub async fn new(db: DatabaseConnection) -> Result<Self, AppError> {
        let passwords = PasswordService::default();
        // A non-existent account performs the same expensive verification as a known account.
        let dummy_hash = passwords
            .hash("nonexistent account dummy credential".into())
            .await?;
        Ok(Self {
            db,
            passwords,
            dummy_hash,
        })
    }
    async fn throttle(&self, key: &str) -> Result<(), AppError> {
        let row = self.db.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,
            "INSERT INTO auth_throttle (key_hash, attempts, resets_at) VALUES ($1,1,CURRENT_TIMESTAMP + interval '10 minutes') ON CONFLICT (key_hash) DO UPDATE SET attempts = CASE WHEN auth_throttle.resets_at <= CURRENT_TIMESTAMP THEN 1 ELSE LEAST(auth_throttle.attempts + 1,11) END, resets_at = CASE WHEN auth_throttle.resets_at <= CURRENT_TIMESTAMP THEN CURRENT_TIMESTAMP + interval '10 minutes' ELSE auth_throttle.resets_at END RETURNING attempts",
            [digest(key).into()])).await.map_err(|_| AppError::Unavailable)?.ok_or(AppError::Unavailable)?;
        let attempts: i32 = row
            .try_get("", "attempts")
            .map_err(|_| AppError::Unavailable)?;
        if attempts > 10 {
            return Err(AppError::RateLimited);
        }
        Ok(())
    }
    async fn validate_token(
        &self,
        hash: &str,
        kind: &str,
        product: crate::product::ProductId,
    ) -> Result<(), AppError> {
        let row = self.db.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,
            "SELECT token_hash FROM identity_tokens WHERE token_hash=$1 AND kind=$2 AND product_id=$3 AND consumed_at IS NULL AND expires_at>CURRENT_TIMESTAMP",
            [hash.into(), kind.into(), product.as_str().into()])).await.map_err(|_| AppError::Unavailable)?;
        if row.is_none() {
            return Err(AppError::InvalidInput);
        }
        Ok(())
    }
    pub async fn accept_invite(&self, request: AcceptInviteRequest) -> Result<User, AppError> {
        self.accept_invite_scoped(request, crate::product::ProductId::Brioche)
            .await
    }
    pub(crate) async fn accept_invite_scoped(
        &self,
        request: AcceptInviteRequest,
        product: crate::product::ProductId,
    ) -> Result<User, AppError> {
        let email = normalize_email(&request.email)?;
        let name = request.display_name.trim();
        if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
            return Err(AppError::InvalidInput);
        }
        let hash = token_hash(&request.token)?;
        self.validate_token(&hash, "invite", product).await?;
        self.throttle(&format!("invite:{hash}")).await?;
        let password_hash = self.passwords.hash(request.password).await?;
        let tx = self.db.begin().await.map_err(|_| AppError::Unavailable)?;
        tx.execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
            [email.clone().into()],
        ))
        .await
        .map_err(|_| AppError::Unavailable)?;
        let token = tx.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,
            "SELECT email, role FROM identity_tokens WHERE token_hash=$1 AND product_id=$2 AND kind='invite' AND consumed_at IS NULL AND expires_at>CURRENT_TIMESTAMP FOR UPDATE",
            [hash.clone().into(), product.as_str().into()])).await.map_err(|_| AppError::Unavailable)?.ok_or(AppError::InvalidInput)?;
        let invited_email: String = token
            .try_get("", "email")
            .map_err(|_| AppError::Unavailable)?;
        if email != invited_email {
            return Err(AppError::InvalidInput);
        }
        let role: String = token
            .try_get("", "role")
            .map_err(|_| AppError::Unavailable)?;
        let row = tx.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,
            "INSERT INTO users (email,password_hash,display_name,role) VALUES ($1,$2,$3,$4) ON CONFLICT (email) DO NOTHING RETURNING id,email,password_hash,display_name,role,profile_version",
            [email.into(), password_hash.into(), name.into(), (if product == crate::product::ProductId::Brioche { role.clone() } else { "learner".into() }).into()])).await.map_err(|_| AppError::Unavailable)?.ok_or(AppError::InvalidInput)?;
        tx.execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE identity_tokens SET consumed_at=CURRENT_TIMESTAMP WHERE token_hash=$1",
            [hash.into()],
        ))
        .await
        .map_err(|_| AppError::Unavailable)?;
        let user = row_user(row)?;
        // A scoped invitation grants membership only in its issuing product.
        tx.execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "INSERT INTO product_memberships(product_id,user_id,role) VALUES($3,$1,$2)",
            [user.id.into(), role.into(), product.as_str().into()],
        ))
        .await
        .map_err(|_| AppError::Unavailable)?;
        tx.commit().await.map_err(|_| AppError::Unavailable)?;
        Ok(user)
    }
    pub async fn reset_password(&self, request: ResetPasswordRequest) -> Result<(), AppError> {
        self.reset_password_scoped(request, crate::product::ProductId::Brioche)
            .await
    }
    async fn reset_password_scoped(
        &self,
        request: ResetPasswordRequest,
        product: crate::product::ProductId,
    ) -> Result<(), AppError> {
        let hash = token_hash(&request.token)?;
        self.validate_token(&hash, "reset", product).await?;
        self.throttle(&format!("reset:{hash}")).await?;
        let password_hash = self.passwords.hash(request.password).await?;
        let tx = self.db.begin().await.map_err(|_| AppError::Unavailable)?;
        let row = tx
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT email FROM identity_tokens WHERE token_hash=$1 AND product_id=$2",
                [hash.clone().into(), product.as_str().into()],
            ))
            .await
            .map_err(|_| AppError::Unavailable)?
            .ok_or(AppError::InvalidInput)?;
        let email: String = row
            .try_get("", "email")
            .map_err(|_| AppError::Unavailable)?;
        tx.execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
            [email.into()],
        ))
        .await
        .map_err(|_| AppError::Unavailable)?;
        let token = tx.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,
            "SELECT user_id FROM identity_tokens WHERE token_hash=$1 AND product_id=$2 AND kind='reset' AND consumed_at IS NULL AND expires_at>CURRENT_TIMESTAMP FOR UPDATE",
            [hash.into(), product.as_str().into()])).await.map_err(|_| AppError::Unavailable)?.ok_or(AppError::InvalidInput)?;
        let user_id: i64 = token
            .try_get("", "user_id")
            .map_err(|_| AppError::Unavailable)?;
        tx.execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE users SET password_hash=$2 WHERE id=$1",
            [user_id.into(), password_hash.into()],
        ))
        .await
        .map_err(|_| AppError::Unavailable)?;
        tx.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,
            "UPDATE identity_tokens SET consumed_at=CURRENT_TIMESTAMP WHERE user_id=$1 AND kind='reset' AND consumed_at IS NULL", [user_id.into()])).await.map_err(|_| AppError::Unavailable)?;
        tx.execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "DELETE FROM browser_sessions WHERE data #>> '{brioche.auth,user_id}' = $1",
            [user_id.to_string().into()],
        ))
        .await
        .map_err(|_| AppError::Unavailable)?;
        tx.commit().await.map_err(|_| AppError::Unavailable)?;
        Ok(())
    }
    pub async fn issue_token(
        &self,
        email: &str,
        reset: bool,
        operator: bool,
    ) -> Result<String, AppError> {
        self.issue_token_impl(
            email,
            reset,
            operator,
            crate::product::ProductId::Brioche,
            None,
        )
        .await
    }
    pub(crate) async fn issue_operator_token(
        &self,
        product: crate::product::ProductId,
        email: &str,
        reset: bool,
        operator: bool,
        actor: i64,
        reason: &str,
    ) -> Result<String, AppError> {
        crate::account_admin::reason(reason)?;
        self.issue_token_impl(email, reset, operator, product, Some((actor, reason)))
            .await
    }
    async fn issue_token_impl(
        &self,
        email: &str,
        reset: bool,
        operator: bool,
        product: crate::product::ProductId,
        audit: Option<(i64, &str)>,
    ) -> Result<String, AppError> {
        let email = normalize_email(email)?;
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|_| AppError::Unavailable)?;
        let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let tx = self.db.begin().await.map_err(|_| AppError::Unavailable)?;
        if let Some((actor, _)) = audit {
            crate::product_memberships::lock_operator(&tx, product, actor).await?;
        }
        tx.execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
            [email.clone().into()],
        ))
        .await
        .map_err(|_| AppError::Unavailable)?;
        let user = tx
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT id FROM users WHERE email=$1 FOR UPDATE",
                [email.clone().into()],
            ))
            .await
            .map_err(|_| AppError::Unavailable)?;
        if reset != user.is_some() {
            return Err(AppError::InvalidInput);
        }
        let user_id: Option<i64> = user
            .map(|r| r.try_get("", "id"))
            .transpose()
            .map_err(|_| AppError::Unavailable)?;
        let kind = if reset { "reset" } else { "invite" };
        tx.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,
            "UPDATE identity_tokens SET consumed_at=CURRENT_TIMESTAMP WHERE email=$1 AND kind=$2 AND product_id=$3 AND consumed_at IS NULL",
            [email.clone().into(), kind.into(), product.as_str().into()])).await.map_err(|_| AppError::Unavailable)?;
        tx.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,
            "INSERT INTO identity_tokens (token_hash,kind,email,user_id,role,expires_at,product_id) VALUES ($1,$2,$3,$4,$5,CURRENT_TIMESTAMP + ($6::bigint * interval '1 second'),$7)",
            [digest(&token).into(), kind.into(), email.clone().into(), user_id.into(), (if operator { "operator" } else { "learner" }).into(), (if reset { 1800_i64 } else { 172800_i64 }).into(), product.as_str().into()])).await.map_err(|_| AppError::Unavailable)?;
        if let Some((actor, reason)) = audit {
            let details = if reset {
                serde_json::json!({})
            } else {
                serde_json::json!({"role":if operator {"operator"} else {"learner"}})
            };
            crate::learning::exec(&tx,"INSERT INTO account_admin_audit(action,actor_id,target_email,reason,details,product_id) VALUES($1,$2,$3,$4,$5,$6)",vec![kind.into(),actor.into(),email.into(),reason.into(),details.into(),product.as_str().into()]).await?;
        }
        tx.commit().await.map_err(|_| AppError::Unavailable)?;
        Ok(token)
    }
}
impl AuthnBackend for Backend {
    type User = User;
    type Credentials = LoginRequest;
    type Error = AppError;
    async fn authenticate(&self, request: LoginRequest) -> Result<Option<User>, AppError> {
        let email = normalize_email(&request.email).map_err(|_| AppError::Unauthorized)?;
        self.throttle(&format!("login:{email}")).await?;
        let row = self
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT id,email,password_hash,display_name,role,profile_version FROM users WHERE email=$1",
                [email.into()],
            ))
            .await
            .map_err(|_| AppError::Unavailable)?;
        let user = row.map(row_user).transpose()?;
        let encoded = user
            .as_ref()
            .map(|u| u.password_hash.clone())
            .unwrap_or_else(|| self.dummy_hash.clone());
        if !self.passwords.verify(request.password, encoded).await? {
            return Ok(None);
        }
        Ok(user)
    }
    async fn get_user(&self, id: &i64) -> Result<Option<User>, AppError> {
        self.db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT id,email,password_hash,display_name,role,profile_version FROM users WHERE id=$1",
                [(*id).into()],
            ))
            .await
            .map_err(|_| AppError::Unavailable)?
            .map(row_user)
            .transpose()
    }
}
pub type AuthSession = axum_login::AuthSession<Backend>;
pub(crate) async fn require_operator(
    auth: &AuthSession,
) -> Result<crate::product_memberships::Operator, AppError> {
    let user = auth.user.as_ref().ok_or(AppError::Unauthorized)?;
    crate::product_memberships::require_operator(
        &auth.backend.db,
        crate::product::ProductId::Brioche,
        user.id,
    )
    .await
}
async fn establish(auth: &mut AuthSession, user: User) -> Result<Json<AuthResult>, AppError> {
    crate::product_settings::ensure(
        &auth.backend.db,
        crate::product::ProductId::Brioche,
        user.id,
    )
    .await?;
    let csrf_token = establish_session(auth, &user).await?;
    Ok(Json(AuthResult {
        user: user.profile(&auth.backend.db).await?,
        csrf_token,
    }))
}
async fn establish_session(auth: &mut AuthSession, user: &User) -> Result<String, AppError> {
    if auth.user.is_some() {
        auth.session
            .cycle_id()
            .await
            .map_err(|_| AppError::Unavailable)?;
    }
    auth.login(user).await.map_err(|_| AppError::Unavailable)?;
    csrf::rotate(&auth.session).await
}
async fn login(
    mut auth: AuthSession,
    Json(request): Json<LoginRequest>,
) -> Result<Json<AuthResult>, AppError> {
    let user = auth
        .authenticate(request)
        .await
        .map_err(|e| match e {
            axum_login::Error::Backend(error) => error,
            _ => AppError::Unavailable,
        })?
        .ok_or(AppError::Unauthorized)?;
    establish(&mut auth, user).await
}
async fn accept(
    mut auth: AuthSession,
    State(backend): State<Backend>,
    Json(request): Json<AcceptInviteRequest>,
) -> Result<Json<AuthResult>, AppError> {
    let user = backend.accept_invite(request).await?;
    establish(&mut auth, user).await
}
async fn logout(mut auth: AuthSession) -> Result<Json<CsrfToken>, AppError> {
    auth.logout().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(CsrfToken {
        csrf_token: csrf::rotate(&auth.session).await?,
    }))
}
async fn reset(
    mut auth: AuthSession,
    State(backend): State<Backend>,
    axum::Extension(product): axum::Extension<crate::product::ProductId>,
    Json(request): Json<ResetPasswordRequest>,
) -> Result<Json<CsrfToken>, AppError> {
    backend.reset_password_scoped(request, product).await?;
    logout_after_reset(&mut auth).await
}
async fn logout_after_reset(auth: &mut AuthSession) -> Result<Json<CsrfToken>, AppError> {
    auth.logout().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(CsrfToken {
        csrf_token: csrf::rotate(&auth.session).await?,
    }))
}
async fn me(auth: AuthSession) -> Result<Json<UserProfile>, AppError> {
    Ok(Json(
        auth.user
            .ok_or(AppError::Unauthorized)?
            .profile(&auth.backend.db)
            .await?,
    ))
}

async fn update_profile(
    auth: AuthSession,
    State(backend): State<Backend>,
    Json(request): Json<UpdateProfileRequest>,
) -> Result<Json<UserProfile>, AppError> {
    let user = auth.user.ok_or(AppError::Unauthorized)?;
    let tx = backend
        .db
        .begin()
        .await
        .map_err(|_| AppError::Unavailable)?;
    // Serialize legacy combined name/preferences writes without mixing product revisions.
    tx.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT id FROM users WHERE id=$1 FOR UPDATE",
        [user.id.into()],
    ))
    .await
    .map_err(|_| AppError::Unavailable)?
    .ok_or(AppError::Unauthorized)?;
    let preferences =
        crate::product_settings::read(&tx, crate::product::ProductId::Brioche, user.id).await?;
    if request.version != preferences.version {
        return Err(AppError::Conflict);
    }
    let changes_name = request.display_name.is_some();
    let (name, settings) = profile_changes(&user.display_name, preferences.settings, request)?;
    let version = crate::product_settings::save(
        &tx,
        crate::product::ProductId::Brioche,
        user.id,
        preferences.version,
        &settings,
    )
    .await?;
    let updated = if changes_name {
        let row = tx.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,
            "UPDATE users SET display_name=$2,profile_version=profile_version+1 WHERE id=$1 AND profile_version < 2147483647 RETURNING id,email,password_hash,display_name,role,profile_version",
            [user.id.into(), name.into()])).await.map_err(|_| AppError::Unavailable)?.ok_or(AppError::Conflict)?;
        row_user(row)?
    } else {
        user
    };
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(UserProfile {
        id: updated.id.to_string(),
        email: updated.email,
        display_name: updated.display_name,
        role: crate::product_memberships::read(
            &backend.db,
            crate::product::ProductId::Brioche,
            updated.id,
        )
        .await?
        .role,
        settings,
        version,
    }))
}

pub(crate) fn profile_changes(
    display_name: &str,
    mut settings: UserSettings,
    request: UpdateProfileRequest,
) -> Result<(String, UserSettings), AppError> {
    if request.display_name.is_none()
        && request.time_zone.is_none()
        && request.weekly_days.is_none()
        && request.daily_minutes.is_none()
        && request.show_translation.is_none()
        && request.speech_rate.is_none()
    {
        return Err(AppError::InvalidInput);
    }
    let name = request
        .display_name
        .as_deref()
        .unwrap_or(display_name)
        .trim()
        .to_owned();
    if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err(AppError::InvalidInput);
    }
    if let Some(zone) = request.time_zone {
        // Only IANA database identifiers, never a machine-local zone or a UTC offset.
        if zone.len() > 100 || zone != zone.trim() || jiff::tz::db().get(&zone).is_err() {
            return Err(AppError::InvalidInput);
        }
        settings.time_zone = zone;
    }
    if let Some(days) = request.weekly_days {
        if ![3, 5, 7].contains(&days) {
            return Err(AppError::InvalidInput);
        }
        settings.weekly_days = days;
    }
    if let Some(minutes) = request.daily_minutes {
        if ![5, 10, 15].contains(&minutes) {
            return Err(AppError::InvalidInput);
        }
        settings.daily_minutes = minutes;
    }
    if let Some(show) = request.show_translation {
        settings.show_translation = show;
    }
    if let Some(rate) = request.speech_rate {
        if ![0.75, 1.0, 1.25, 1.5].contains(&rate) {
            return Err(AppError::InvalidInput);
        }
        settings.speech_rate = rate;
    }
    Ok((name, settings))
}

pub fn router(backend: Backend, policy: CsrfPolicy, secure: bool) -> Router {
    router_with_media_root(backend, policy, secure, crate::media::media_root())
}
pub fn router_with_media_root(
    backend: Backend,
    policy: CsrfPolicy,
    secure: bool,
    root: std::path::PathBuf,
) -> Router {
    let routes = Router::new()
        .route("/api/v1/auth/csrf", get(csrf::bootstrap))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/accept-invite", post(accept))
        .route("/api/v1/auth/reset-password", post(reset))
        .route("/api/v1/me", get(me))
        .route("/api/v1/account", get(account_me).patch(account_update))
        .route("/api/v1/me/settings", axum::routing::patch(update_profile))
        .merge(crate::learning_store::routes().with_state(
            crate::learning_store::LearningStore::new(backend.db.clone()),
        ))
        .merge(crate::admin::router(root.clone(), backend.db.clone()))
        .merge(crate::preview::router(root))
        .layer(axum::Extension(crate::product::ProductId::Brioche));
    protect_routes(
        routes,
        backend,
        policy,
        secure,
        crate::product::ProductId::Brioche,
    )
}

/// Identity process exposes account data only, without learning or content routes.
pub fn account_router(
    backend: Backend,
    policy: CsrfPolicy,
    secure: bool,
    product: crate::product::ProductId,
) -> Router {
    let routes = Router::new()
        .route("/api/v1/auth/csrf", get(csrf::bootstrap))
        .route("/api/v1/auth/login", post(account_login))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/accept-invite", post(account_accept))
        .route("/api/v1/auth/reset-password", post(reset))
        .route("/api/v1/account", get(account_me).patch(account_update))
        .merge(crate::product_memberships::router(backend.clone(), product))
        .merge(crate::account_admin::router(product))
        .layer(axum::Extension(product));
    protect_routes(routes, backend, policy, secure, product)
}

async fn account_login(
    mut auth: AuthSession,
    Json(request): Json<LoginRequest>,
) -> Result<Json<crate::identity_service::AccountAuthResult>, AppError> {
    let user = auth
        .authenticate(request)
        .await
        .map_err(|error| match error {
            axum_login::Error::Backend(error) => error,
            _ => AppError::Unavailable,
        })?
        .ok_or(AppError::Unauthorized)?;
    let csrf_token = establish_session(&mut auth, &user).await?;
    Ok(Json(crate::identity_service::AccountAuthResult {
        user: user.account(),
        csrf_token,
    }))
}
async fn account_accept(
    mut auth: AuthSession,
    State(backend): State<Backend>,
    axum::Extension(product): axum::Extension<crate::product::ProductId>,
    Json(request): Json<AcceptInviteRequest>,
) -> Result<Json<crate::identity_service::AccountAuthResult>, AppError> {
    let user = backend.accept_invite_scoped(request, product).await?;
    let csrf_token = establish_session(&mut auth, &user).await?;
    Ok(Json(crate::identity_service::AccountAuthResult {
        user: user.account(),
        csrf_token,
    }))
}
async fn account_me(
    auth: AuthSession,
) -> Result<Json<crate::identity_service::AccountProfile>, AppError> {
    Ok(Json(account_identity(auth)?))
}
async fn account_update(
    auth: AuthSession,
    State(backend): State<Backend>,
    Json(request): Json<brioche_course_contract::AccountProfileUpdateRequest>,
) -> Result<Json<brioche_course_contract::AccountProfile>, AppError> {
    let id = crate::learning::owner(&auth)?;
    let name = request.display_name.trim();
    if name.is_empty()
        || name.chars().count() > 80
        || name.chars().any(char::is_control)
        || request.expected_account_version == 0
        || request.expected_account_version > 2147483647
    {
        return Err(AppError::InvalidInput);
    }
    let row=backend.db.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,
        "UPDATE users SET display_name=$2,profile_version=profile_version+1 WHERE id=$1 AND profile_version=$3 AND profile_version<2147483647 RETURNING id,email,display_name,role,profile_version",
        [id.into(),name.into(),i32::try_from(request.expected_account_version).map_err(|_|AppError::InvalidInput)?.into()]))
        .await.map_err(|_|AppError::Unavailable)?.ok_or(AppError::Conflict)?;
    Ok(Json(brioche_course_contract::AccountProfile {
        id: row
            .try_get::<i64>("", "id")
            .map_err(|_| AppError::Unavailable)?
            .to_string(),
        email: row
            .try_get("", "email")
            .map_err(|_| AppError::Unavailable)?,
        display_name: row
            .try_get("", "display_name")
            .map_err(|_| AppError::Unavailable)?,
        role: row.try_get("", "role").map_err(|_| AppError::Unavailable)?,
        version: u32::try_from(
            row.try_get::<i32>("", "profile_version")
                .map_err(|_| AppError::Unavailable)?,
        )
        .map_err(|_| AppError::Unavailable)?,
    }))
}
pub(crate) fn account_identity(
    auth: AuthSession,
) -> Result<crate::identity_service::AccountProfile, AppError> {
    Ok(auth.user.ok_or(AppError::Unauthorized)?.account())
}
pub(crate) fn protect_routes(
    routes: Router<Backend>,
    backend: Backend,
    policy: CsrfPolicy,
    secure: bool,
    product: crate::product::ProductId,
) -> Router {
    let session_layer =
        SessionManagerLayer::new(PgSessionStore::for_product(backend.db.clone(), product))
            .with_name(product.cookie_name(secure))
            .with_secure(secure)
            .with_http_only(true)
            .with_same_site(SameSite::Lax)
            .with_expiry(Expiry::OnInactivity(time::Duration::days(14)));
    let auth_layer = AuthManagerLayerBuilder::new(backend.clone(), session_layer)
        .with_data_key(AUTH_KEY)
        .build();
    routes
        .layer(axum::middleware::from_fn_with_state(
            Arc::new(policy),
            csrf::protect,
        ))
        .layer(auth_layer)
        .layer(axum::extract::DefaultBodyLimit::max(16 * 1024))
        .with_state(backend)
}
