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
    /// Trusted schemes and hosts are explicit configuration; ports are ignored.
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
            allowed.insert(host_origin(parsed));
        }
        anyhow::ensure!(
            !allowed.is_empty(),
            "at least one trusted origin is required"
        );
        Ok(Self { origins: allowed })
    }
    pub fn allows(&self, origin: &str) -> bool {
        let Ok(parsed) = url::Url::parse(origin) else {
            return false;
        };
        // Validate the raw origin shape before URL normalization drops ports or paths.
        // Explicit default ports are valid too; credentials and URL components are not.
        let Some((scheme, authority)) = origin.split_once("://") else {
            return false;
        };
        if !matches!(scheme, "http" | "https")
            || authority.is_empty()
            || authority.contains(['/', '\\', '?', '#', '@'])
            || origin.chars().any(char::is_whitespace)
        {
            return false;
        }
        self.origins.contains(&host_origin(parsed))
    }
}
fn host_origin(mut origin: url::Url) -> String {
    origin.set_port(None).expect("HTTP origin supports ports");
    origin.origin().ascii_serialization()
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
pub(crate) async fn validate_write(
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
    fn origin_policy_ignores_ports_but_preserves_scheme_host_and_origin_shape() {
        let policy = CsrfPolicy::new([
            "https://brioche.example".into(),
            "http://localhost:5173".into(),
            "https://hargow.example:443".into(),
            "http://[::1]:5173".into(),
        ])
        .unwrap();
        assert!(policy.allows("http://localhost:5173"));
        assert!(policy.allows("http://localhost:30075"));
        assert!(policy.allows("http://localhost"));
        assert!(policy.allows("https://brioche.example:8443"));
        assert!(policy.allows("https://brioche.example:30075"));
        assert!(policy.allows("https://brioche.example:443"));
        assert!(policy.allows("https://hargow.example"));
        assert!(policy.allows("https://hargow.example:8443"));
        assert!(policy.allows("http://[::1]:30075"));
        for origin in [
            "null",
            "https://brioche.example.evil",
            "http://brioche.example",
            "https://localhost:5173",
            "https://user@brioche.example:8443",
            "https://brioche.example:8443/path",
            "https://brioche.example:8443/",
            "https://brioche.example:8443?x",
            "https://brioche.example:8443#x",
            "https://brioche.example:99999",
            "https://brioche.example:8443\\path",
            " https://brioche.example",
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
