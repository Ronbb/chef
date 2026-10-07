//! Explicit maintenance operation using the migration owner, never a runtime route.
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement, TransactionTrait};

const TABLES: [&str; 7] = [
    "users",
    "browser_sessions",
    "identity_tokens",
    "auth_throttle",
    "product_memberships",
    "product_membership_audit",
    "account_admin_audit",
];

pub async fn relocate(db: &DatabaseConnection, source: &str, target: &str) -> anyhow::Result<()> {
    crate::database_scope::validate(source)?;
    crate::database_scope::validate(target)?;
    anyhow::ensure!(
        source != target,
        "Identity and learning schemas must differ"
    );
    let tx = db.begin().await?;
    tx.execute_unprepared("SET LOCAL lock_timeout='5s'; SET LOCAL statement_timeout='30s'")
        .await?;
    tx.execute_unprepared("SELECT pg_advisory_xact_lock(hashtextextended('chef-schema-split',0))")
        .await?;
    let current = tx
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT current_schema() AS name",
        ))
        .await?
        .ok_or_else(|| anyhow::anyhow!("Schema unavailable"))?
        .try_get::<String>("", "name")?;
    anyhow::ensure!(
        current == source,
        "Migration connection must select the source schema"
    );
    let table_names = TABLES
        .iter()
        .map(|table| format!("'{table}'"))
        .collect::<Vec<_>>()
        .join(",");
    let owned = tx.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,
        format!("SELECT count(*)::bigint AS count FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relname IN ({table_names}) AND c.relkind='r' AND c.relowner=(SELECT oid FROM pg_catalog.pg_roles WHERE rolname=current_user)"),
        [source.into()])).await?.unwrap().try_get::<i64>("", "count")?;
    anyhow::ensure!(
        owned == TABLES.len() as i64,
        "All identity tables must belong to the migration owner"
    );
    // Presence and an empty immutable layout establish the exact migration boundary.
    let version = tx
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            format!(
                "SELECT version FROM \"{source}\".seaql_migrations ORDER BY version DESC LIMIT 1"
            ),
        ))
        .await?
        .ok_or_else(|| anyhow::anyhow!("Migration history unavailable"))?
        .try_get::<String>("", "version")?;
    anyhow::ensure!(
        version == "m20261008_000032_schema_layout",
        "Schema split requires the verified migration32 layout"
    );
    let layout = tx
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            format!("SELECT count(*)::bigint AS count FROM \"{source}\".chef_schema_layout"),
        ))
        .await?
        .unwrap()
        .try_get::<i64>("", "count")?;
    anyhow::ensure!(layout == 0, "Identity schema is already split");
    // CREATE, not IF NOT EXISTS: a pre-existing destination must never be adopted.
    tx.execute_unprepared(&format!(
        "CREATE SCHEMA \"{target}\"; REVOKE ALL ON SCHEMA \"{target}\" FROM PUBLIC"
    ))
    .await?;
    let locks = TABLES
        .iter()
        .map(|table| format!("\"{source}\".\"{table}\""))
        .collect::<Vec<_>>()
        .join(",");
    tx.execute_unprepared(&format!("LOCK TABLE {locks} IN ACCESS EXCLUSIVE MODE"))
        .await?;
    for table in TABLES {
        tx.execute_unprepared(&format!(
            "ALTER TABLE \"{source}\".\"{table}\" SET SCHEMA \"{target}\""
        ))
        .await?;
    }
    tx.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,
        format!("INSERT INTO \"{source}\".chef_schema_layout(learning_schema,identity_schema) VALUES($1,$2)"),
        [source.into(), target.into()])).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn require_combined(db: &DatabaseConnection) -> anyhow::Result<()> {
    let exists = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT to_regclass('chef_schema_layout') IS NOT NULL AS present",
        ))
        .await?
        .unwrap()
        .try_get::<bool>("", "present")?;
    if exists {
        let split = db
            .query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT EXISTS(SELECT 1 FROM chef_schema_layout) AS split",
            ))
            .await?
            .unwrap()
            .try_get::<bool>("", "split")?;
        anyhow::ensure!(
            !split,
            "Split schema requires independent identity mode; combined migration and service are disabled"
        );
    }
    Ok(())
}

/// Explicit owner upgrade; the existing combined command remains fail-closed.
pub async fn migrate_layout(db: &DatabaseConnection, learning: &str) -> anyhow::Result<()> {
    crate::database_scope::validate(learning)?;
    let row = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            format!(
                "SELECT identity_schema FROM \"{learning}\".chef_schema_layout WHERE singleton"
            ),
        ))
        .await?;
    let identity = match row {
        Some(row) => row.try_get::<String>("", "identity_schema")?,
        None => anyhow::bail!("Split migration layout required"),
    };
    brioche_migration::layout::up(db, learning, &identity).await?;
    Ok(())
}
