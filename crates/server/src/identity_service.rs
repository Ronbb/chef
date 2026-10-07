//! Standalone identity HTTP boundary; no curriculum, media or learning routes.
use crate::{
    AppError,
    csrf::CsrfPolicy,
    identity::{self, AuthSession, Backend},
    product::ProductId,
};
use axum::{
    Json, Router,
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::get,
};
use brioche_course_contract::{AuthResult, UserProfile};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountProfile {
    pub id: String,
    pub email: String,
    pub display_name: String,
    /// Global account role; consumers must separately authorize product operations.
    pub role: String,
    pub version: u32,
}
impl From<UserProfile> for AccountProfile {
    fn from(user: UserProfile) -> Self {
        Self {
            id: user.id,
            email: user.email,
            display_name: user.display_name,
            role: user.role,
            version: user.version,
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountAuthResult {
    pub user: AccountProfile,
    pub csrf_token: String,
}
impl From<AuthResult> for AccountAuthResult {
    fn from(result: AuthResult) -> Self {
        Self {
            user: result.user.into(),
            csrf_token: result.csrf_token,
        }
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionIdentity {
    pub product: ProductId,
    pub account: AccountProfile,
}

#[derive(Clone)]
pub struct ServiceConfig {
    product: ProductId,
    // Store only the digest; never derive Debug or serialize the service credential.
    key_hash: [u8; 32],
}
impl ServiceConfig {
    pub fn new(product: ProductId, key: &str) -> anyhow::Result<Self> {
        anyhow::ensure!(
            key.len() == 64 && key.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "Invalid identity service credential"
        );
        Ok(Self {
            product,
            key_hash: Sha256::digest(key.as_bytes()).into(),
        })
    }
}
async fn service_only(
    State(config): State<ServiceConfig>,
    request: Request,
    next: Next,
) -> Response {
    let authorized = request.headers().get_all("authorization").iter().count() == 1
        && request
            .headers()
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .filter(|value| value.len() == 64)
            .map(|value| bool::from(config.key_hash.ct_eq(&Sha256::digest(value.as_bytes())[..])))
            .unwrap_or(false);
    let product = request
        .headers()
        .get("x-chef-product")
        .and_then(|value| value.to_str().ok());
    let mut response = if !authorized {
        AppError::Unauthorized.into_response()
    } else if request.headers().get_all("x-chef-product").iter().count() != 1
        || product != Some(config.product.as_str())
    {
        AppError::Forbidden.into_response()
    } else {
        next.run(request).await
    };
    response
        .headers_mut()
        .insert("cache-control", "private, no-store".parse().unwrap());
    response
}
async fn introspect(
    State(config): State<ServiceConfig>,
    auth: AuthSession,
) -> Result<Json<SessionIdentity>, AppError> {
    let account = identity::account_identity(auth)?;
    Ok(Json(SessionIdentity {
        product: config.product,
        account,
    }))
}
pub fn router(backend: Backend, policy: CsrfPolicy, secure: bool, config: ServiceConfig) -> Router {
    let internal = Router::new()
        .route("/internal/v1/session", get(introspect))
        .with_state(config.clone());
    // The credential gate wraps authentication, rejecting invalid clients before DB reads.
    let internal = identity::protect_routes(
        internal,
        backend.clone(),
        policy.clone(),
        secure,
        config.product,
    )
    .route_layer(axum::middleware::from_fn_with_state(
        config.clone(),
        service_only,
    ));
    identity::account_router(backend, policy, secure, config.product).merge(internal)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn internal_credentials_and_product_are_checked_before_disconnected_storage() {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;
        let key = "1".repeat(64);
        let backend = Backend::new(Default::default()).await.unwrap();
        let app = router(
            backend,
            CsrfPolicy::new(["http://localhost:5173".into()]).unwrap(),
            false,
            ServiceConfig::new(ProductId::Brioche, &key).unwrap(),
        );
        for (credential, product, status) in
            [("bad", "brioche", 401), (key.as_str(), "hargow", 403)]
        {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/internal/v1/session")
                        .header(
                            "cookie",
                            format!("brioche.sid={}", tower_sessions::session::Id::default()),
                        )
                        .header("authorization", format!("Bearer {credential}"))
                        .header("x-chef-product", product)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status().as_u16(), status);
            assert_eq!(response.headers()["cache-control"], "private, no-store");
            assert!(response.headers().get("set-cookie").is_none());
        }
    }
    #[test]
    fn account_boundary_excludes_learning_settings_and_rejects_extra_fields() {
        let user = AccountProfile {
            id: "1".into(),
            email: "learner@example.test".into(),
            display_name: "Learner".into(),
            role: "learner".into(),
            version: 1,
        };
        let mut value = serde_json::to_value(user).unwrap();
        assert!(value.get("settings").is_none());
        assert!(value.get("passwordHash").is_none());
        value["settings"] = serde_json::json!({"showTranslation": true});
        assert!(serde_json::from_value::<AccountProfile>(value).is_err());
        assert!(ServiceConfig::new(ProductId::Brioche, "short-private-value").is_err());
    }
}
