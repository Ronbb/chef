//! Forward-only maintenance after the verified migration32 boundary.
use sea_orm_migration::sea_orm::{
    self, ConnectionTrait, DbBackend, DbErr, Statement, TransactionTrait,
};

struct Step {
    version: &'static str,
    identity: bool,
    sql: &'static str,
}
const STEPS: &[Step] = &[
    Step {
        version: "identity_000001_throttle_expiry",
        identity: true,
        sql: "CREATE INDEX chef_throttle_expiry ON auth_throttle(resets_at)",
    },
    Step {
        version: "learning_000001_attempt_owner_time",
        identity: false,
        sql: "CREATE INDEX chef_attempt_owner_time ON exercise_attempts(user_id,created_at)",
    },
    Step {
        version: "learning_000002_product_facts",
        identity: false,
        sql: include_str!("learning_product_facts.sql"),
    },
];
fn error(message: &str) -> DbErr {
    DbErr::Custom(message.into())
}
fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 63
        && s.as_bytes()[0].is_ascii_lowercase()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Same database/owner, explicit recorded schemas; no runtime role or search-path fallback.
pub async fn up(
    db: &sea_orm::DatabaseConnection,
    learning: &str,
    identity: &str,
) -> Result<(), DbErr> {
    if !identifier(learning) || !identifier(identity) {
        return Err(error("Invalid migration schema"));
    }
    if learning == identity {
        return Err(error("Split migration layout required"));
    }
    let tx = db.begin().await?;
    tx.execute_unprepared("SET LOCAL lock_timeout='5s'; SET LOCAL statement_timeout='30s'; SELECT pg_advisory_xact_lock(hashtextextended('chef-schema-split',0))").await?;
    let current = tx
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT current_schema() AS name",
        ))
        .await?
        .ok_or_else(|| error("Migration schema missing"))?
        .try_get::<String>("", "name")?;
    if current != learning {
        return Err(error("Migration connection must select learning schema"));
    }
    let row = tx
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            format!(
                "SELECT version FROM \"{learning}\".seaql_migrations ORDER BY version DESC LIMIT 1"
            ),
        ))
        .await?
        .ok_or_else(|| error("Migration32 history required"))?;
    if row.try_get::<String>("", "version")? != "m20261008_000032_schema_layout" {
        return Err(error("Verified migration32 boundary required"));
    }
    let layout=tx.query_one_raw(Statement::from_string(DbBackend::Postgres,format!("SELECT learning_schema,identity_schema FROM \"{learning}\".chef_schema_layout WHERE singleton"))).await?;
    match layout {
        Some(row)
            if row.try_get::<String>("", "learning_schema")? == learning
                && row.try_get::<String>("", "identity_schema")? == identity => {}
        _ => return Err(error("Recorded migration layout mismatch")),
    }
    for (schema, tables) in [
        (
            identity,
            vec![
                "users",
                "browser_sessions",
                "identity_tokens",
                "auth_throttle",
                "product_memberships",
                "product_membership_audit",
                "account_admin_audit",
            ],
        ),
        (
            learning,
            vec![
                "exercise_attempts",
                "seaql_migrations",
                "chef_schema_layout",
            ],
        ),
    ] {
        let count=tx.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,"SELECT count(*)::bigint AS n FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relname=ANY($2::text[]) AND c.relkind='r' AND c.relowner=(SELECT oid FROM pg_catalog.pg_roles WHERE rolname=current_user)",[schema.into(),tables.iter().map(|s|s.to_string()).collect::<Vec<_>>().into()])).await?.ok_or_else(||error("Owner unavailable"))?.try_get::<i64>("","n")?;
        if count != tables.len() as i64 {
            return Err(error("Migration owner required for both scopes"));
        }
    }
    tx.execute_unprepared(&format!("CREATE TABLE IF NOT EXISTS \"{learning}\".chef_layout_migrations(version TEXT PRIMARY KEY,scope TEXT NOT NULL CHECK(scope IN ('identity','learning')),schema_name TEXT NOT NULL,definition TEXT NOT NULL,applied_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP)")).await?;
    // Reject unknown or changed definitions before executing any pending DDL.
    let rows=tx.query_all_raw(Statement::from_string(DbBackend::Postgres,format!("SELECT version,scope,schema_name,definition FROM \"{learning}\".chef_layout_migrations ORDER BY version"))).await?;
    let mut applied = std::collections::BTreeSet::new();
    for row in rows {
        let version = row.try_get::<String>("", "version")?;
        let step = STEPS
            .iter()
            .find(|s| s.version == version)
            .ok_or_else(|| error("Unknown layout migration history"))?;
        if row.try_get::<String>("", "scope")?
            != if step.identity {
                "identity"
            } else {
                "learning"
            }
            || row.try_get::<String>("", "schema_name")?
                != if step.identity { identity } else { learning }
            || row.try_get::<String>("", "definition")? != step.sql.replace("\r\n", "\n")
        {
            return Err(error("Layout migration definition mismatch"));
        }
        applied.insert(version);
    }
    for step in STEPS {
        if applied.contains(step.version) {
            continue;
        }
        let schema = if step.identity { identity } else { learning };
        // A Windows checkout must record the same definition as a Linux build.
        let definition = step.sql.replace("\r\n", "\n");
        tx.execute_unprepared(&format!(
            "SET LOCAL search_path TO \"{schema}\"; {}",
            definition
        ))
        .await?;
        tx.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,format!("INSERT INTO \"{learning}\".chef_layout_migrations(version,scope,schema_name,definition) VALUES($1,$2,$3,$4)"),[step.version.into(),if step.identity {"identity"} else {"learning"}.into(),schema.into(),definition.into()])).await?;
    }
    tx.commit().await?;
    Ok(())
}
