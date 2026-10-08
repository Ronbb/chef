//! Audited identity maintenance; no learning database access or HTTP authorization bypass.
use crate::{
    identity::{Backend, normalize_email},
    product::ProductId,
};
use anyhow::{Context, Result, bail};
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
};
use std::{io::Write, path::PathBuf};

/// Runs explicit token commands; service startup retains its separate configuration.
pub async fn run_if_requested(product: ProductId) -> Result<bool> {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        return Ok(false);
    };
    if command == "serve" {
        anyhow::ensure!(args.next().is_none(), "usage: chef-identity serve");
        return Ok(false);
    }
    anyhow::ensure!(
        matches!(
            command.as_str(),
            "invite" | "reset-password" | "bootstrap-operator"
        ),
        "Unknown identity command"
    );
    let args: Vec<_> = args.collect();
    if command == "bootstrap-operator" {
        anyhow::ensure!(
            args.len() == 2,
            "usage: chef-identity bootstrap-operator <existing-account-email> <reason>"
        );
        let email =
            normalize_email(&args[0]).map_err(|_| anyhow::anyhow!("Invalid target email"))?;
        crate::account_admin::reason(&args[1])
            .map_err(|_| anyhow::anyhow!("Invalid bootstrap reason"))?;
        let db = maintenance_database().await?;
        crate::product_memberships::bootstrap_owner(&db, product, &email, &args[1]).await
            .map_err(|_| anyhow::anyhow!("First operator bootstrap not confirmed; verify identity table ownership, existing account and product history"))?;
        tracing::info!("first product operator initialized with membership audit");
        return Ok(true);
    }
    let reset = command == "reset-password";
    anyhow::ensure!(
        args.len() == 4 || (!reset && args.len() == 5 && args[4] == "--operator"),
        "usage: chef-identity invite|reset-password <email> <new-private-output-file> <operator-email> <reason> [invite only: --operator]"
    );
    let email = normalize_email(&args[0]).map_err(|_| anyhow::anyhow!("Invalid target email"))?;
    let actor_email =
        normalize_email(&args[2]).map_err(|_| anyhow::anyhow!("Invalid operator email"))?;
    crate::account_admin::reason(&args[3]).map_err(|_| anyhow::anyhow!("Invalid token reason"))?;
    let output = PathBuf::from(&args[1]);
    let base = std::env::var("PUBLIC_APP_URL").context("PUBLIC_APP_URL is required")?;
    crate::csrf::CsrfPolicy::new([base.clone()])
        .map_err(|_| anyhow::anyhow!("Invalid identity public origin"))?;
    let mut link =
        url::Url::parse(&base).map_err(|_| anyhow::anyhow!("Invalid identity public origin"))?;
    let db = maintenance_database().await?;
    let actor = db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT id FROM users WHERE email=$1",
            [actor_email.into()],
        ))
        .await
        .map_err(|_| anyhow::anyhow!("Identity actor unavailable"))?
        .context("Identity actor unavailable")?
        .try_get::<i64>("", "id")?;
    crate::product_memberships::require_operator(&db, product, actor)
        .await
        .map_err(|_| anyhow::anyhow!("Operator membership required for configured product"))?;
    let backend = Backend::new(db)
        .await
        .map_err(|_| anyhow::anyhow!("Identity backend unavailable"))?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&output)
        .context("Cannot create exclusive private token output")?;
    let token = match backend
        .issue_operator_token(product, &email, reset, args.len() == 5, actor, &args[3])
        .await
    {
        Ok(token) => token,
        Err(_) => {
            drop(file);
            let _ = std::fs::remove_file(&output);
            bail!("Token issuance not confirmed; inspect pending tokens before retrying")
        }
    };
    link.set_path(if reset { "/reset-password" } else { "/invite" });
    link.set_fragment(Some(
        &url::form_urlencoded::Serializer::new(String::new())
            .append_pair("token", &token)
            .append_pair("email", &email)
            .finish(),
    ));
    if writeln!(file, "{link}")
        .and_then(|_| file.sync_all())
        .is_err()
    {
        bail!(
            "Token issued but private output was not saved; inspect pending tokens before retrying"
        );
    }
    tracing::info!("one-time link written to the requested private file");
    Ok(true)
}

async fn maintenance_database() -> Result<DatabaseConnection> {
    let schema = std::env::var("IDENTITY_DATABASE_SCHEMA")
        .context("IDENTITY_DATABASE_SCHEMA is required for identity maintenance")?;
    let mut options =
        ConnectOptions::new(std::env::var("DATABASE_URL").context("DATABASE_URL is required")?);
    options
        .sqlx_logging(false)
        .max_connections(2)
        .connect_timeout(std::time::Duration::from_secs(5))
        .acquire_timeout(std::time::Duration::from_secs(5));
    crate::database_scope::apply(&mut options, Some(&schema))?;
    let db = Database::connect(options)
        .await
        .map_err(|_| anyhow::anyhow!("Identity database connection unavailable"))?;
    let row=db.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT current_schema() AS schema,(SELECT count(*) FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=current_schema() AND c.relkind='r' AND c.relname IN ('users','browser_sessions','identity_tokens','auth_throttle','product_memberships','product_membership_audit','account_admin_audit')) AS tables,NOT EXISTS(SELECT 1 FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=current_schema() AND c.relname IN ('lesson_revisions','learning_sessions','content_state')) AS identity_only")).await.map_err(|_|anyhow::anyhow!("Identity maintenance layout unavailable"))?.context("Identity maintenance layout unavailable")?;
    anyhow::ensure!(
        row.try_get::<String>("", "schema")? == schema
            && row.try_get::<i64>("", "tables")? == 7
            && row.try_get::<bool>("", "identity_only")?,
        "Identity maintenance requires a separate identity schema"
    );
    Ok(db)
}
