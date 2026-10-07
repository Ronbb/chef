//! Private provider boundary. Never log credentials, raw provider bodies or reference URLs.
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
mod speech;
pub(crate) use speech::normalize_wave;
pub use speech::{Speech, SpeechRequest};
pub const MODEL: &str = "qwen-audio-3.1-tts-flash";
#[derive(Clone, Copy, Debug)]
pub enum ProviderError {
    Rejected,
    Unknown,
}
#[derive(Clone)]
pub struct Receipt {
    pub voice_id: String,
    pub request_id: String,
}
pub struct Details {
    pub model: String,
    pub status: String,
    pub request_id: String,
}
#[async_trait::async_trait]
pub trait Transport: Send + Sync {
    async fn create(&self, prefix: &str, reference_url: &str) -> Result<Receipt, ProviderError>;
    async fn query(&self, voice_id: &str) -> Result<Details, ProviderError>;
    /// A paid request. Callers must persist their attempt before invoking it; never retry automatically.
    async fn synthesize(&self, _request: &SpeechRequest) -> Result<Speech, ProviderError> {
        Err(ProviderError::Rejected)
    }
}
#[derive(Clone)]
pub struct Service {
    pub(crate) transport: Arc<dyn Transport>,
    pub(crate) origin: String,
    pub(crate) permits: Arc<tokio::sync::Semaphore>,
}
impl Service {
    /// Trusted process configuration/injection only. Public requests never supply an origin or provider endpoint.
    pub fn new(transport: Arc<dyn Transport>, origin: &str) -> Result<Self, ProviderError> {
        let url = url::Url::parse(origin).map_err(|_| ProviderError::Rejected)?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
        {
            return Err(ProviderError::Rejected);
        }
        Ok(Self {
            transport,
            origin: url.origin().ascii_serialization(),
            permits: Arc::new(tokio::sync::Semaphore::new(2)),
        })
    }
    pub fn from_env() -> Result<Option<Self>, ProviderError> {
        let Ok(key) = std::env::var("DASHSCOPE_API_KEY") else {
            return Ok(None);
        };
        if key.is_empty() {
            return Ok(None);
        }
        let base = std::env::var("DASHSCOPE_BASE_URL")
            .or_else(|_| {
                std::env::var("QWEN_WORKSPACE_ID")
                    .map(|workspace| format!("https://{workspace}.cn-beijing.maas.aliyuncs.com"))
            })
            .map_err(|_| ProviderError::Rejected)?;
        let api = Api::new(&base, key)?;
        let origin = std::env::var("PUBLIC_APP_URL").map_err(|_| ProviderError::Rejected)?;
        Self::new(Arc::new(api), &origin).map(Some)
    }
}
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
}
struct Api {
    client: reqwest::Client,
    endpoint: String,
    key: String,
}
impl Api {
    fn new(base: &str, key: String) -> Result<Self, ProviderError> {
        let url = url::Url::parse(base).map_err(|_| ProviderError::Rejected)?;
        let workspace = url
            .host_str()
            .and_then(|h| h.strip_suffix(".cn-beijing.maas.aliyuncs.com"))
            .ok_or(ProviderError::Rejected)?;
        if url.scheme() != "https"
            || url.port().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || workspace.is_empty()
            || workspace.len() > 100
            || !workspace
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            || ![
                "/",
                "/api/v1",
                "/api/v1/",
                "/compatible-mode/v1",
                "/compatible-mode/v1/",
            ]
            .contains(&url.path())
            || key.trim().is_empty()
            || key.len() > 4096
            || key.bytes().any(|b| b.is_ascii_control())
        {
            return Err(ProviderError::Rejected);
        }
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::builder()
            .https_only(true)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| ProviderError::Rejected)?;
        Ok(Self {
            client,
            endpoint: format!(
                "{}/api/v1/services/audio/tts/customization",
                url.origin().ascii_serialization()
            ),
            key,
        })
    }
    async fn call(&self, input: Value) -> Result<Value, ProviderError> {
        self.call_at(
            &self.endpoint,
            json!({"model":"voice-enrollment","input":input}),
            Duration::from_secs(30),
        )
        .await
    }
    async fn call_at(
        &self,
        endpoint: &str,
        body: Value,
        timeout: Duration,
    ) -> Result<Value, ProviderError> {
        // Do not propagate reqwest::Error: its Display/Debug may include the sensitive request URL.
        let mut response = self
            .client
            .post(endpoint)
            .bearer_auth(&self.key)
            .timeout(timeout)
            .json(&body)
            .send()
            .await
            .map_err(|_| ProviderError::Unknown)?;
        if !response.status().is_success() {
            return Err(
                if matches!(response.status().as_u16(), 400 | 401 | 403 | 404 | 422) {
                    ProviderError::Rejected
                } else {
                    ProviderError::Unknown
                },
            );
        }
        if response.content_length().is_some_and(|n| n > 256_000) {
            return Err(ProviderError::Unknown);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| ProviderError::Unknown)? {
            if bytes.len() + chunk.len() > 256_000 {
                return Err(ProviderError::Unknown);
            }
            bytes.extend_from_slice(&chunk);
        }
        let data: Value = serde_json::from_slice(&bytes).map_err(|_| ProviderError::Unknown)?;
        if !data["output"].is_object()
            || data.get("code").is_some()
            || !data["request_id"].as_str().is_some_and(valid_id)
        {
            return Err(ProviderError::Unknown);
        }
        Ok(data)
    }
}
fn parse_receipt(data: &Value, prefix: &str) -> Result<Receipt, ProviderError> {
    let voice = data["output"]["voice_id"]
        .as_str()
        .filter(|s| valid_id(s) && s.starts_with(&format!("{MODEL}-{prefix}-")))
        .ok_or(ProviderError::Unknown)?;
    let request_id = data["request_id"]
        .as_str()
        .filter(|s| valid_id(s))
        .ok_or(ProviderError::Unknown)?;
    Ok(Receipt {
        voice_id: voice.into(),
        request_id: request_id.into(),
    })
}
fn parse_details(data: &Value) -> Result<Details, ProviderError> {
    let model = data["output"]["target_model"]
        .as_str()
        .filter(|s| valid_id(s))
        .ok_or(ProviderError::Unknown)?;
    let status = data["output"]["status"]
        .as_str()
        .filter(|s| ["DEPLOYING", "OK", "UNDEPLOYED"].contains(s))
        .ok_or(ProviderError::Unknown)?;
    let request_id = data["request_id"]
        .as_str()
        .filter(|s| valid_id(s))
        .ok_or(ProviderError::Unknown)?;
    Ok(Details {
        model: model.into(),
        status: status.into(),
        request_id: request_id.into(),
    })
}
#[async_trait::async_trait]
impl Transport for Api {
    async fn create(&self, prefix: &str, reference_url: &str) -> Result<Receipt, ProviderError> {
        if prefix.is_empty()
            || prefix.len() > 10
            || !prefix.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            return Err(ProviderError::Rejected);
        }
        let data=self.call(json!({"action":"create_voice","target_model":MODEL,"prefix":prefix,"url":reference_url,"language_hints":["fr"],"max_prompt_audio_length":30.0,"enable_preprocess":false,"enable_volume_normalization":"false"})).await?;
        parse_receipt(&data, prefix)
    }
    async fn query(&self, voice_id: &str) -> Result<Details, ProviderError> {
        if !valid_id(voice_id) {
            return Err(ProviderError::Rejected);
        }
        parse_details(
            &self
                .call(json!({"action":"query_voice","voice_id":voice_id}))
                .await?,
        )
    }
    async fn synthesize(&self, request: &SpeechRequest) -> Result<Speech, ProviderError> {
        speech::synthesize(self, request).await
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn http_create_and_query_use_bounded_non_retrying_requests() {
        let response = Arc::new(std::sync::Mutex::new((
            200u16,
            json!({"request_id":"create-request","output":{"voice_id":format!("{MODEL}-abc-test"),"resource_link":"discard"}}),
        )));
        let calls = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
        let r = response.clone();
        let c = calls.clone();
        let app = axum::Router::new().route(
            "/",
            axum::routing::post(
                move |headers: axum::http::HeaderMap, axum::Json(body): axum::Json<Value>| {
                    let r = r.clone();
                    let c = c.clone();
                    async move {
                        assert_eq!(headers["authorization"], "Bearer synthetic-key");
                        c.lock().unwrap().push(body);
                        let (status, value) = r.lock().unwrap().clone();
                        (
                            axum::http::StatusCode::from_u16(status).unwrap(),
                            axum::Json(value),
                        )
                    }
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        // The production constructor rejects HTTP. This private test exercises the wire protocol without real credentials.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let api = Api {
            client: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .retry(reqwest::retry::never())
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap(),
            endpoint: format!("http://{address}"),
            key: "synthetic-key".into(),
        };
        let receipt = api
            .create("abc", "https://example.test/reference")
            .await
            .unwrap();
        assert_eq!(receipt.voice_id, format!("{MODEL}-abc-test"));
        let body = calls.lock().unwrap()[0].clone();
        assert_eq!(body["model"], "voice-enrollment");
        assert_eq!(body["input"]["action"], "create_voice");
        assert_eq!(body["input"]["target_model"], MODEL);
        assert_eq!(body["input"]["language_hints"], json!(["fr"]));
        *response.lock().unwrap() = (
            200,
            json!({"request_id":"query-request","output":{"target_model":MODEL,"status":"OK","resource_link":"discard"}}),
        );
        assert_eq!(api.query(&receipt.voice_id).await.unwrap().status, "OK");
        *response.lock().unwrap() = (503, json!({"message":"raw secret"}));
        assert!(matches!(
            api.create("abc", "https://example.test/reference").await,
            Err(ProviderError::Unknown)
        ));
        assert_eq!(calls.lock().unwrap().len(), 3);
        *response.lock().unwrap() = (422, json!({"message":"raw secret"}));
        assert!(matches!(
            api.create("abc", "https://example.test/reference").await,
            Err(ProviderError::Rejected)
        ));
        *response.lock().unwrap() = (
            200,
            json!({"request_id":"test","output":{"payload":"x".repeat(256001)}}),
        );
        assert!(api.query(&receipt.voice_id).await.is_err());
        *response.lock().unwrap() = (302, json!({}));
        assert!(api.query(&receipt.voice_id).await.is_err());
        assert_eq!(calls.lock().unwrap().len(), 6);
        server.abort();
        let _ = server.await;
    }
    #[test]
    fn endpoint_is_pinned_and_receipts_are_bound_and_sanitized() {
        assert!(
            Api::new(
                "https://test.cn-beijing.maas.aliyuncs.com/compatible-mode/v1",
                "test-key".into()
            )
            .is_ok()
        );
        for base in [
            "http://test.cn-beijing.maas.aliyuncs.com",
            "https://bad.example",
            "https://test.cn-beijing.maas.aliyuncs.com@bad.example",
            "https://test.cn-beijing.maas.aliyuncs.com/other",
            "https://test.cn-beijing.maas.aliyuncs.com?key=x",
        ] {
            assert!(Api::new(base, "test-key".into()).is_err());
        }
        let data = json!({"request_id":"test-request","output":{"voice_id":format!("{MODEL}-abc-123"),"resource_link":"secret","status":"OK","target_model":MODEL}});
        assert_eq!(
            parse_receipt(&data, "abc").unwrap().voice_id,
            format!("{MODEL}-abc-123")
        );
        assert!(parse_receipt(&data, "different").is_err());
        assert_eq!(parse_details(&data).unwrap().status, "OK");
        assert!(
            parse_details(
                &json!({"request_id":"test","output":{"target_model":MODEL,"status":"UNKNOWN"}})
            )
            .is_err()
        );
    }
}
