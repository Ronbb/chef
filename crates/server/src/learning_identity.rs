//! Request-local identity verification. No credentials, sessions or authorization cache in learning.
use crate::{
    AppError, identity_service::SessionIdentity, learning_store::LearningStore, product::ProductId,
};
use axum::{
    Router,
    extract::{FromRequestParts, Request, State},
    http::{HeaderMap, request::Parts},
    middleware::Next,
    response::{IntoResponse, Response},
};
use axum_login::AuthUser;
use std::time::Duration;

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    endpoint: url::Url,
    credential: reqwest::header::HeaderValue,
    product: ProductId,
    secure: bool,
    permits: std::sync::Arc<tokio::sync::Semaphore>,
}
impl Client {
    pub(crate) async fn is_operator(&self, actor: i64) -> Result<bool, AppError> {
        if actor <= 0 {
            return Err(AppError::Unavailable);
        }
        let _permit = self
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| AppError::Unavailable)?;
        let mut endpoint = self.endpoint.clone();
        endpoint.set_path(&format!("/internal/v1/operators/{actor}"));
        let response = self
            .http
            .get(endpoint)
            .header("authorization", self.credential.clone())
            .header("x-chef-product", self.product.as_str())
            .send()
            .await
            .map_err(|_| AppError::Unavailable)?;
        match response.status().as_u16() {
            204 => Ok(true),
            404 => Ok(false),
            _ => Err(AppError::Unavailable),
        }
    }
    pub(crate) fn product(&self) -> ProductId {
        self.product
    }
    pub fn new(origin: &str, key: &str, product: ProductId, secure: bool) -> anyhow::Result<Self> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut endpoint =
            url::Url::parse(origin).map_err(|_| anyhow::anyhow!("Invalid identity endpoint"))?;
        anyhow::ensure!(
            matches!(endpoint.scheme(), "http" | "https")
                && endpoint.host_str().is_some()
                && endpoint.username().is_empty()
                && endpoint.password().is_none()
                && endpoint.path() == "/"
                && endpoint.query().is_none()
                && endpoint.fragment().is_none(),
            "Invalid identity endpoint"
        );
        anyhow::ensure!(
            key.len() == 64 && key.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "Invalid identity credential"
        );
        endpoint.set_path("/internal/v1/session");
        let mut credential = reqwest::header::HeaderValue::from_str(&format!("Bearer {key}"))?;
        credential.set_sensitive(true);
        let http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(1))
            .timeout(Duration::from_secs(2))
            .build()?;
        Ok(Self {
            http,
            endpoint,
            credential,
            product,
            secure,
            permits: std::sync::Arc::new(tokio::sync::Semaphore::new(32)),
        })
    }
    async fn verify(&self, headers: &HeaderMap, method: &str) -> Result<SessionIdentity, AppError> {
        let _permit = self
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| AppError::Unavailable)?;
        // Forward only this product's single session cookie, never other browser cookies.
        let name = self.product.cookie_name(self.secure);
        let mut cookies = Vec::new();
        for value in headers.get_all("cookie").iter() {
            let value = value.to_str().map_err(|_| AppError::Unauthorized)?;
            if value.len() > 8192 {
                return Err(AppError::Unauthorized);
            }
            for item in value.split(';') {
                if let Some((key, value)) = item.trim().split_once('=')
                    && key == name
                {
                    cookies.push(value);
                }
            }
        }
        if cookies.len() != 1 || cookies[0].is_empty() || cookies[0].len() > 128 {
            return Err(AppError::Unauthorized);
        }
        let mut request = self
            .http
            .get(self.endpoint.clone())
            .header("authorization", self.credential.clone())
            .header("x-chef-product", self.product.as_str())
            .header("x-chef-request-method", method)
            .header("cookie", format!("{name}={}", cookies[0]));
        for header in ["origin", "x-csrf-token"] {
            if headers.get_all(header).iter().count() > 1 {
                return Err(AppError::Forbidden);
            }
            if let Some(value) = headers.get(header) {
                request = request.header(header, value.clone());
            }
        }
        let mut response = request.send().await.map_err(|_| AppError::Unavailable)?;
        match response.status().as_u16() {
            200 => {}
            401 => return Err(AppError::Unauthorized),
            403 => return Err(AppError::Forbidden),
            _ => return Err(AppError::Unavailable),
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| AppError::Unavailable)? {
            if bytes.len() + chunk.len() > 4096 {
                return Err(AppError::Unavailable);
            }
            bytes.extend_from_slice(&chunk);
        }
        let verified: SessionIdentity =
            serde_json::from_slice(&bytes).map_err(|_| AppError::Unavailable)?;
        let id = verified
            .account
            .id
            .parse::<i64>()
            .map_err(|_| AppError::Unavailable)?;
        if verified.product != self.product
            || id <= 0
            || verified.account.id != id.to_string()
            || !matches!(verified.account.role.as_str(), "learner" | "operator")
            || verified.account.version == 0
            || !matches!(verified.membership.role.as_str(), "learner" | "operator")
            || (verified.membership.role == "operator" && verified.membership.version == 0)
        {
            return Err(AppError::Unavailable);
        }
        Ok(verified)
    }
}
#[derive(Clone)]
pub struct LearningAuth {
    pub(crate) identity: SessionIdentity,
}

/// Never deserialized or logged; request-local credentials only, no authorization cache.
#[derive(Clone)]
pub(crate) struct RemoteAuthorization {
    client: Client,
    headers: HeaderMap,
    method: String,
    identity: SessionIdentity,
}
impl RemoteAuthorization {
    pub(crate) async fn is_operator(&self, actor: i64) -> Result<bool, AppError> {
        self.client.is_operator(actor).await
    }
    pub(crate) fn actor(&self) -> i64 {
        self.identity
            .account
            .id
            .parse()
            .expect("verified account ID")
    }
    pub(crate) fn operator(&self) -> Result<crate::product_memberships::Operator, AppError> {
        if self.identity.membership.role != "operator" {
            return Err(AppError::Forbidden);
        }
        Ok(crate::product_memberships::Operator {
            product: self.identity.product,
            actor: self.actor(),
            remote: Some(self.clone()),
        })
    }
    pub(crate) async fn recheck(&self) -> Result<(), AppError> {
        let identity = self.client.verify(&self.headers, &self.method).await?;
        if identity.account.id != self.identity.account.id || identity.membership.role != "operator"
        {
            return Err(AppError::Forbidden);
        }
        Ok(())
    }
}
impl<S: Send + Sync> FromRequestParts<S> for LearningAuth {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, AppError> {
        if let Some(value) = parts.extensions.get::<LearningAuth>() {
            return Ok(value.clone());
        }
        // Legacy deployments retain the existing auth layer; remote routers always gate first.
        let auth = crate::identity::AuthSession::from_request_parts(parts, state)
            .await
            .map_err(|_| AppError::Unavailable)?;
        let membership = crate::product_memberships::read(
            &auth.backend.db,
            ProductId::Brioche,
            crate::learning::owner(&auth)?,
        )
        .await?;
        let identity = SessionIdentity {
            product: ProductId::Brioche,
            account: crate::identity::account_identity(auth)?,
            membership,
        };
        Ok(Self { identity })
    }
}
pub trait AccountOwner {
    fn account_id(&self) -> Result<i64, AppError>;
}
impl AccountOwner for crate::identity::AuthSession {
    fn account_id(&self) -> Result<i64, AppError> {
        self.user
            .as_ref()
            .map(AuthUser::id)
            .ok_or(AppError::Unauthorized)
    }
}
impl AccountOwner for LearningAuth {
    fn account_id(&self) -> Result<i64, AppError> {
        self.identity
            .account
            .id
            .parse()
            .map_err(|_| AppError::Unavailable)
    }
}
async fn gate(State(client): State<Client>, mut request: Request, next: Next) -> Response {
    let result = client
        .verify(request.headers(), request.method().as_str())
        .await;
    let mut response = match result {
        Ok(identity) => {
            let mut headers = HeaderMap::new();
            for name in ["cookie", "origin", "x-csrf-token"] {
                for value in request.headers().get_all(name) {
                    headers.append(name, value.clone());
                }
            }
            let method = request.method().as_str().to_owned();
            request.extensions_mut().insert(RemoteAuthorization {
                client: client.clone(),
                headers,
                method,
                identity: identity.clone(),
            });
            request.extensions_mut().insert(LearningAuth { identity });
            next.run(request).await
        }
        Err(error) => error.into_response(),
    };
    response
        .headers_mut()
        .insert("cache-control", "private, no-store".parse().unwrap());
    response
        .headers_mut()
        .append("vary", "Cookie".parse().unwrap());
    response
}
async fn profile(
    auth: LearningAuth,
    State(backend): State<LearningStore>,
) -> Result<axum::Json<brioche_course_contract::UserProfile>, AppError> {
    let preferences =
        crate::product_settings::read(&backend.db, auth.identity.product, auth.account_id()?)
            .await?;
    Ok(axum::Json(profile_payload(auth.identity, preferences)))
}
fn profile_payload(
    identity: SessionIdentity,
    preferences: crate::product_settings::Preferences,
) -> brioche_course_contract::UserProfile {
    brioche_course_contract::UserProfile {
        id: identity.account.id,
        email: identity.account.email,
        display_name: identity.account.display_name,
        role: identity.membership.role,
        settings: preferences.settings,
        version: preferences.version,
    }
}
async fn settings(
    auth: LearningAuth,
    State(backend): State<LearningStore>,
    axum::Json(request): axum::Json<brioche_course_contract::UpdateProfileRequest>,
) -> Result<axum::Json<brioche_course_contract::UserProfile>, AppError> {
    // Shared account edits belong to identity, never a learning database write.
    if request.display_name.is_some() {
        return Err(AppError::InvalidInput);
    }
    use sea_orm::TransactionTrait;
    let tx = backend
        .db
        .begin()
        .await
        .map_err(|_| AppError::Unavailable)?;
    let user = auth.account_id()?;
    let preferences = crate::product_settings::read(&tx, auth.identity.product, user).await?;
    if preferences.version != request.version {
        return Err(AppError::Conflict);
    }
    let (_, settings) = crate::identity::profile_changes(
        &auth.identity.account.display_name,
        preferences.settings,
        request,
    )?;
    let version = crate::product_settings::save(
        &tx,
        auth.identity.product,
        user,
        preferences.version,
        &settings,
    )
    .await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(axum::Json(profile_payload(
        auth.identity,
        crate::product_settings::Preferences { settings, version },
    )))
}
/// Until full product facts isolation is migrated, remote business routes are Brioche-only.
pub fn router(db: sea_orm::DatabaseConnection, client: Client) -> anyhow::Result<Router> {
    anyhow::ensure!(
        client.product == ProductId::Brioche,
        "Product learning data migration incomplete"
    );
    Ok(Router::new()
        .route("/api/v1/me", axum::routing::get(profile))
        .route("/api/v1/me/settings", axum::routing::patch(settings))
        .merge(crate::learning_store::routes())
        .layer(axum::extract::DefaultBodyLimit::max(16 * 1024))
        .with_state(LearningStore::new(db))
        .route_layer(axum::middleware::from_fn_with_state(client, gate)))
}
pub(crate) fn protect(router: Router, client: Client) -> Router {
    router.route_layer(axum::middleware::from_fn_with_state(client, gate))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Json, http::StatusCode, routing::get};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tower::ServiceExt;
    #[tokio::test]
    async fn actual_http_boundary_is_bounded_private_and_fails_closed() {
        let mode = Arc::new(AtomicUsize::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let handler_mode = mode.clone();
        let handler_calls = calls.clone();
        let identity=Router::new().route("/internal/v1/session",get(move |headers: HeaderMap| {
            let mode=handler_mode.load(Ordering::SeqCst);
            let calls=handler_calls.clone();
            async move {
                calls.fetch_add(1,Ordering::SeqCst);
                assert_eq!(headers["cookie"], "brioche.sid=test-session");
                assert_eq!(headers["x-chef-product"], "brioche");
                assert_eq!(headers["x-chef-request-method"], "GET");
                assert!(headers.get("x-spoofed-identity").is_none());
                let value=serde_json::json!({"product":if mode==1 {"hargow"} else {"brioche"},"account":{"id":"101","email":"learner@example.test","displayName":"Test","role":"learner","version":1},"membership":{"role":"learner","version":0}});
                match mode {
                    2 => (StatusCode::OK,"{\"product\":\"brioche\"}".to_string()).into_response(),
                    3 => (StatusCode::OK,"x".repeat(5000)).into_response(),
                    4 => StatusCode::SERVICE_UNAVAILABLE.into_response(),
                    5 => (StatusCode::FOUND,[("location","/redirected")]).into_response(),
                    6 => { tokio::time::sleep(Duration::from_secs(3)).await; Json(value).into_response() },
                    7 => StatusCode::UNAUTHORIZED.into_response(),
                    _ => Json(value).into_response(),
                }
            }
        }));
        let identity=identity.route("/redirected",get(|| async { Json(serde_json::json!({"product":"brioche","account":{"id":"101","email":"learner@example.test","displayName":"Test","role":"learner","version":1},"membership":{"role":"learner","version":0}})) }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, identity).await.unwrap() });
        let client = Client::new(&origin, &"1".repeat(64), ProductId::Brioche, false).unwrap();
        let business = Router::new()
            .route(
                "/private",
                get(|auth: LearningAuth| async move { Json(auth.account_id().unwrap()) }),
            )
            .route_layer(axum::middleware::from_fn_with_state(client.clone(), gate));
        let request = |cookie: &str| {
            axum::http::Request::builder()
                .uri("/private")
                .header("cookie", cookie)
                .header("x-spoofed-identity", "operator")
                .body(axum::body::Body::empty())
                .unwrap()
        };
        for (mode_value, expected) in [
            (0, 200),
            (1, 503),
            (2, 503),
            (3, 503),
            (4, 503),
            (5, 503),
            (6, 503),
            (7, 401),
        ] {
            mode.store(mode_value, Ordering::SeqCst);
            let response = business
                .clone()
                .oneshot(request("unrelated=secret; brioche.sid=test-session"))
                .await
                .unwrap();
            assert_eq!(response.status().as_u16(), expected);
            assert_eq!(response.headers()["cache-control"], "private, no-store");
            assert!(response.headers().get("set-cookie").is_none());
        }
        let count = calls.load(Ordering::SeqCst);
        for cookie in [
            "hargow.sid=test-session",
            "brioche.sid=a; brioche.sid=b",
            "",
        ] {
            assert_eq!(
                business
                    .clone()
                    .oneshot(request(cookie))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::UNAUTHORIZED
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst), count);
        task.abort();
        let _ = task.await;
        assert_eq!(
            business
                .oneshot(request("brioche.sid=test-session"))
                .await
                .unwrap()
                .status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert!(
            Client::new(
                "http://username@localhost/",
                &"1".repeat(64),
                ProductId::Brioche,
                false
            )
            .is_err()
        );
        assert!(
            Client::new(
                "http://localhost/?target=hargow",
                &"1".repeat(64),
                ProductId::Brioche,
                false
            )
            .is_err()
        );
    }
}
