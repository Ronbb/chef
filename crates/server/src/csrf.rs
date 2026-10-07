//! Session-bound CSRF for both anonymous and authenticated write requests.
use crate::AppError;
use axum::{
    Json,
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};
use brioche_course_contract::CsrfToken;
use std::{collections::HashSet, sync::Arc};
use subtle::ConstantTimeEq;
use tower_sessions::Session;

const KEY: &str = "brioche.csrf";
#[derive(Clone, Debug)]
pub struct CsrfPolicy {
    origins: HashSet<String>,
}
impl CsrfPolicy {
    /// Origins are explicit configuration, not untrusted forwarded headers.
    pub fn new(origins: impl IntoIterator<Item = String>) -> anyhow::Result<Self> {
        let mut allowed = HashSet::new();
        for origin in origins {
            let parsed = url::Url::parse(&origin)?;
            anyhow::ensure!(
                matches!(parsed.scheme(), "http" | "https")
                    && parsed.host_str().is_some()
                    && parsed.username().is_empty()
                    && parsed.password().is_none()
                    && parsed.path() == "/"
                    && parsed.query().is_none()
                    && parsed.fragment().is_none(),
                "invalid trusted origin"
            );
            allowed.insert(parsed.origin().ascii_serialization());
        }
        anyhow::ensure!(
            !allowed.is_empty(),
            "at least one trusted origin is required"
        );
        Ok(Self { origins: allowed })
    }
    pub fn allows(&self, origin: &str) -> bool {
        self.origins.contains(origin)
    }
}
fn nonce() -> Result<String, AppError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| AppError::Unavailable)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}
pub async fn rotate(session: &Session) -> Result<String, AppError> {
    let token = nonce()?;
    session
        .insert(KEY, &token)
        .await
        .map_err(|_| AppError::Unavailable)?;
    Ok(token)
}
pub async fn bootstrap(session: Session) -> Result<impl IntoResponse, AppError> {
    let token = match session
        .get::<String>(KEY)
        .await
        .map_err(|_| AppError::Unavailable)?
    {
        Some(token) => token,
        None => rotate(&session).await?,
    };
    Ok((
        [("Cache-Control", "private, no-store"), ("Vary", "Cookie")],
        Json(CsrfToken { csrf_token: token }),
    ))
}
pub async fn protect(
    State(policy): State<Arc<CsrfPolicy>>,
    session: Session,
    request: Request,
    next: Next,
) -> Response {
    let validation = validate_write(
        &policy,
        &session,
        request.headers(),
        !matches!(request.method().as_str(), "GET" | "HEAD" | "OPTIONS"),
    )
    .await;
    let mut response = match validation {
        Ok(()) => next.run(request).await,
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
async fn validate_write(
    policy: &CsrfPolicy,
    session: &Session,
    headers: &axum::http::HeaderMap,
    writing: bool,
) -> Result<(), AppError> {
    if writing {
        let origin = headers
            .get("origin")
            .and_then(|v| v.to_str().ok())
            .ok_or(AppError::Forbidden)?;
        if !policy.allows(origin) {
            return Err(AppError::Forbidden);
        }
        let actual = headers
            .get("x-csrf-token")
            .and_then(|v| v.to_str().ok())
            .ok_or(AppError::Forbidden)?;
        let expected = session
            .get::<String>(KEY)
            .await
            .map_err(|_| AppError::Unavailable)?
            .ok_or(AppError::Forbidden)?;
        if actual.len() != 64 || !bool::from(actual.as_bytes().ct_eq(expected.as_bytes())) {
            return Err(AppError::Forbidden);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn origin_policy_is_exact_and_rejects_non_origins() {
        let policy = CsrfPolicy::new([
            "https://brioche.example".into(),
            "http://localhost:5173".into(),
        ])
        .unwrap();
        assert!(policy.allows("http://localhost:5173"));
        for origin in [
            "null",
            "https://brioche.example.evil",
            "http://brioche.example",
            "https://brioche.example:8443",
        ] {
            assert!(!policy.allows(origin));
        }
        for origin in [
            "https://a.example/path",
            "https://user@a.example",
            "https://a.example?x",
            "file:///a",
            "https://a.example/#x",
        ] {
            assert!(CsrfPolicy::new([origin.into()]).is_err());
        }
        assert!(CsrfPolicy::new([]).is_err());
    }
}
