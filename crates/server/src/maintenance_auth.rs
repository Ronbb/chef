//! Private session credentials for trusted split CLI operations; never actor authority by email.
use crate::{learning_identity::Client, product::ProductId, product_memberships::Operator};
use anyhow::Result;
use axum::http::{HeaderMap, HeaderValue};
use serde::Deserialize;
use std::io::Read;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Credentials {
    cookie: String,
    csrf_token: String,
}
pub(crate) async fn operator(product: ProductId, expected_email: &str) -> Result<Operator> {
    let origin = std::env::var("PUBLIC_APP_URL")
        .map_err(|_| anyhow::anyhow!("PUBLIC_APP_URL is required for maintenance authorization"))?;
    crate::csrf::CsrfPolicy::new([origin.clone()])
        .map_err(|_| anyhow::anyhow!("Invalid maintenance origin"))?;
    let secure = url::Url::parse(&origin)
        .map_err(|_| anyhow::anyhow!("Invalid maintenance origin"))?
        .scheme()
        == "https";
    let client = Client::new(
        &std::env::var("IDENTITY_INTERNAL_URL")
            .map_err(|_| anyhow::anyhow!("IDENTITY_INTERNAL_URL is required"))?,
        &std::env::var("IDENTITY_INTERNAL_KEY")
            .map_err(|_| anyhow::anyhow!("IDENTITY_INTERNAL_KEY is required"))?,
        product,
        secure,
    )?;
    let path = std::env::var("CHEF_OPERATOR_SESSION_FILE")
        .map_err(|_| anyhow::anyhow!("CHEF_OPERATOR_SESSION_FILE is required"))?;
    let file = std::fs::File::open(path)
        .map_err(|_| anyhow::anyhow!("Private maintenance session unavailable"))?;
    let mut bytes = Vec::new();
    file.take(16385)
        .read_to_end(&mut bytes)
        .map_err(|_| anyhow::anyhow!("Private maintenance session unavailable"))?;
    anyhow::ensure!(bytes.len() <= 16384, "Invalid private maintenance session");
    let credentials: Credentials = serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("Invalid private maintenance session"))?;
    anyhow::ensure!(
        !credentials.cookie.is_empty()
            && credentials.cookie.len() <= 8192
            && !credentials.csrf_token.is_empty()
            && credentials.csrf_token.len() <= 128,
        "Invalid private maintenance session"
    );
    let mut headers = HeaderMap::new();
    for (name, value) in [
        ("cookie", credentials.cookie),
        ("x-csrf-token", credentials.csrf_token),
        ("origin", origin),
    ] {
        let mut value = HeaderValue::from_str(&value)
            .map_err(|_| anyhow::anyhow!("Invalid private maintenance session"))?;
        value.set_sensitive(true);
        headers.insert(name, value);
    }
    let proof = client
        .authorize(&headers, "POST")
        .await
        .map_err(|_| anyhow::anyhow!("Maintenance identity authorization failed"))?;
    proof
        .matches_email(expected_email)
        .map_err(|_| anyhow::anyhow!("Maintenance actor does not match authenticated account"))?;
    proof
        .operator()
        .map_err(|_| anyhow::anyhow!("Maintenance product operator required"))
}
