use axum::{Json, Router, extract::State, routing::get};
use chef_engine::{
    csrf::CsrfPolicy,
    identity::Backend,
    identity_service::{self, ServiceConfig},
    product::ProductId,
};
use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection};

async fn ready(
    State(db): State<DatabaseConnection>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    db.execute_unprepared(
        "SELECT users.id FROM users, browser_sessions, identity_tokens, auth_throttle LIMIT 0",
    )
    .await
    .map_err(|_| axum::http::StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Json(serde_json::json!({"status":"ready"})))
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter("chef_engine=info,tower_http=info")
        .init();
    let product = match std::env::var("IDENTITY_PRODUCT")
        .as_deref()
        .unwrap_or("brioche")
    {
        "brioche" => ProductId::Brioche,
        "hargow" => ProductId::Hargow,
        _ => anyhow::bail!("Invalid configured identity product"),
    };
    let config = ServiceConfig::new(
        product,
        &std::env::var("IDENTITY_INTERNAL_KEY")
            .map_err(|_| anyhow::anyhow!("IDENTITY_INTERNAL_KEY is required"))?,
    )?;
    let public_url = std::env::var("PUBLIC_APP_URL")
        .map_err(|_| anyhow::anyhow!("PUBLIC_APP_URL is required"))?;
    let secure = url::Url::parse(&public_url)
        .map_err(|_| anyhow::anyhow!("Invalid identity public origin"))?
        .scheme()
        == "https";
    let mut origins = vec![public_url];
    if let Ok(additional) = std::env::var("ADDITIONAL_APP_ORIGINS") {
        origins.extend(
            additional
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
        );
    }
    let policy = CsrfPolicy::new(origins)
        .map_err(|_| anyhow::anyhow!("Invalid trusted identity origins"))?;
    let mut options = ConnectOptions::new(
        std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL is required"))?,
    );
    options.sqlx_logging(false);
    let db = Database::connect(options)
        .await
        .map_err(|_| anyhow::anyhow!("Identity database connection unavailable"))?;
    // Explicit existing migrations are required; this process never schema-syncs.
    let backend = Backend::new(db.clone()).await?;
    let health = Router::new()
        .route(
            "/health",
            get(|| async { Json(serde_json::json!({"status":"ok"})) }),
        )
        .route("/ready", get(ready))
        .with_state(db);
    let app = identity_service::router(backend, policy, secure, config).merge(health);
    let listener = tokio::net::TcpListener::bind(
        std::env::var("IDENTITY_BIND").unwrap_or_else(|_| "0.0.0.0:3002".into()),
    )
    .await?;
    tracing::info!(address=%listener.local_addr()?, "Identity service listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(chef_engine::command::shutdown())
        .await?;
    Ok(())
}
