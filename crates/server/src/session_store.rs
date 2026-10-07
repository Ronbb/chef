//! A thin SeaORM adapter for tower-sessions; the existing pool owns all persistence.
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};
use sha2::{Digest, Sha256};
use tower_sessions::{
    SessionStore,
    session::{Id, Record},
    session_store::{Error, Result},
};

#[derive(Clone)]
pub struct PgSessionStore {
    db: DatabaseConnection,
}
impl std::fmt::Debug for PgSessionStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PgSessionStore").finish_non_exhaustive()
    }
}
impl PgSessionStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
    /// Expired records are never loaded, even before physical cleanup.
    pub async fn delete_expired(&self) -> Result<u64> {
        let result = self
            .db
            .execute_unprepared(
                "DELETE FROM browser_sessions WHERE expires_at <= CURRENT_TIMESTAMP",
            )
            .await
            .map_err(|_| Error::Backend("session cleanup unavailable".into()))?;
        Ok(result.rows_affected())
    }
}
pub(crate) fn hash(id: &Id) -> String {
    format!("{:x}", Sha256::digest(id.to_string().as_bytes()))
}
fn backend() -> Error {
    Error::Backend("session storage unavailable".into())
}
#[async_trait::async_trait]
impl SessionStore for PgSessionStore {
    async fn create(&self, record: &mut Record) -> Result<()> {
        // Do not overwrite on a collision. Retry with a new library-generated opaque ID.
        for _ in 0..8 {
            let data = serde_json::to_value(&record.data).map_err(|_| backend())?;
            let result = self.db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,
                "INSERT INTO browser_sessions (id_hash, data, expires_at) VALUES ($1, $2, to_timestamp($3::bigint)) ON CONFLICT (id_hash) DO NOTHING",
                [hash(&record.id).into(), data.into(), record.expiry_date.unix_timestamp().into()]))
                .await.map_err(|_| backend())?;
            if result.rows_affected() == 1 {
                return Ok(());
            }
            record.id = Id::default();
        }
        Err(backend())
    }
    async fn save(&self, record: &Record) -> Result<()> {
        let data = serde_json::to_value(&record.data).map_err(|_| backend())?;
        // An in-flight response must never resurrect a session deleted by logout or rotation.
        let result = self.db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,
            "UPDATE browser_sessions SET data = $2, expires_at = to_timestamp($3::bigint) WHERE id_hash = $1 AND expires_at > CURRENT_TIMESTAMP",
            [hash(&record.id).into(), data.into(), record.expiry_date.unix_timestamp().into()]))
            .await.map_err(|_| backend())?;
        if result.rows_affected() != 1 {
            return Err(Error::Backend("session expired or revoked".into()));
        }
        Ok(())
    }
    async fn load(&self, id: &Id) -> Result<Option<Record>> {
        let row = self.db.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,
            "SELECT data, floor(extract(epoch FROM expires_at))::bigint AS expiry FROM browser_sessions WHERE id_hash = $1 AND expires_at > CURRENT_TIMESTAMP",
            [hash(id).into()])).await.map_err(|_| backend())?;
        let Some(row) = row else {
            return Ok(None);
        };
        let data: serde_json::Value = row.try_get("", "data").map_err(|_| backend())?;
        let expiry: i64 = row.try_get("", "expiry").map_err(|_| backend())?;
        Ok(Some(Record {
            id: *id,
            data: serde_json::from_value(data).map_err(|_| backend())?,
            expiry_date: time::OffsetDateTime::from_unix_timestamp(expiry)
                .map_err(|_| backend())?,
        }))
    }
    async fn delete(&self, id: &Id) -> Result<()> {
        self.db
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "DELETE FROM browser_sessions WHERE id_hash = $1",
                [hash(id).into()],
            ))
            .await
            .map_err(|_| backend())?;
        Ok(())
    }
}
