//! Request metadata only: never raw URLs, headers, submitted values or identity data.
use axum::{
    Router,
    extract::{MatchedPath, Request},
    http::{HeaderValue, Method},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use std::time::Instant;

pub fn observe(app: Router) -> Router {
    app.layer(middleware::from_fn(record))
}
async fn record(request: Request, next: Next) -> Response {
    let id = match crate::learning::random_id() {
        Ok(id) => id,
        Err(error) => {
            tracing::error!("request identifier unavailable");
            return error.into_response();
        }
    };
    let method = match *request.method() {
        Method::GET => "GET",
        Method::POST => "POST",
        Method::PUT => "PUT",
        Method::PATCH => "PATCH",
        Method::DELETE => "DELETE",
        Method::HEAD => "HEAD",
        Method::OPTIONS => "OPTIONS",
        _ => "OTHER",
    };
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|path| path.as_str().to_owned())
        .unwrap_or_else(|| "<unmatched>".into());
    let start = Instant::now();
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        "x-request-id",
        HeaderValue::from_str(&id).expect("generated hex request ID"),
    );
    tracing::info!(request_id=%id, method, route, status=response.status().as_u16(), duration_ms=start.elapsed().as_millis() as u64, "request completed");
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, routing::post};
    use std::{
        io::Write,
        sync::{Arc, Mutex},
    };
    use tower::ServiceExt;
    struct Capture(Arc<Mutex<Vec<u8>>>);
    impl Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    #[tokio::test]
    async fn logs_templates_and_generated_ids_without_private_request_data() {
        let output = Arc::new(Mutex::new(Vec::new()));
        let writer = output.clone();
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_max_level(tracing::Level::INFO)
            .with_writer(move || Capture(writer.clone()))
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);
        let app = observe(
            Router::new()
                .route(
                    "/api/items/{id}",
                    post(|_body: String| async { axum::http::StatusCode::BAD_REQUEST }),
                )
                .layer(axum::extract::DefaultBodyLimit::max(16)),
        );
        let mut ids = Vec::new();
        for (uri, body, expected) in [
            (
                "/api/items/private-path-value?token=private-query-value",
                "private-body",
                400,
            ),
            (
                "/api/items/private-path-value",
                "private-body-value-too-large",
                413,
            ),
            ("/private-fallback-value?token=private-query-value", "", 404),
        ] {
            let response = app
                .clone()
                .oneshot(
                    axum::http::Request::builder()
                        .method("POST")
                        .uri(uri)
                        .header("x-request-id", "untrusted-client-id")
                        .header("cookie", "private-cookie-value")
                        .header("authorization", "Bearer private-auth-value")
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status().as_u16(), expected);
            let id = response.headers()["x-request-id"]
                .to_str()
                .unwrap()
                .to_owned();
            assert_eq!(id.len(), 32);
            assert!(id.bytes().all(|c| c.is_ascii_hexdigit()));
            ids.push(id);
        }
        assert_eq!(
            ids.iter().collect::<std::collections::HashSet<_>>().len(),
            3
        );
        let logs = String::from_utf8(output.lock().unwrap().clone()).unwrap();
        assert!(logs.contains("/api/items/{id}"));
        assert!(logs.contains("<unmatched>"));
        assert!(logs.contains("duration_ms="));
        for id in ids {
            assert!(logs.contains(&id));
        }
        for secret in [
            "private-path-value",
            "private-query-value",
            "private-body",
            "private-cookie-value",
            "private-auth-value",
            "untrusted-client-id",
        ] {
            assert!(!logs.contains(secret));
        }
    }
}
