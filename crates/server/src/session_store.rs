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
    product: Option<crate::product::ProductId>,
}
impl std::fmt::Debug for PgSessionStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PgSessionStore").finish_non_exhaustive()
    }
}
impl PgSessionStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db, product: None }
    }
    pub fn for_product(db: DatabaseConnection, product: crate::product::ProductId) -> Self {
        Self {
            db,
            product: Some(product),
        }
    }
    fn data(&self, record: &Record) -> Result<serde_json::Value> {
        let mut data = serde_json::to_value(&record.data).map_err(|_| backend())?;
        if let Some(product) = self.product {
            if !self.allows(&data) && data.get("chef.product").is_some() {
                return Err(backend());
            }
            data.as_object_mut()
                .ok_or_else(backend)?
                .insert("chef.product".into(), serde_json::json!(product.as_str()));
        }
        Ok(data)
    }
    fn allows(&self, data: &serde_json::Value) -> bool {
        match self.product {
            None => true,
            Some(product) => match data.get("chef.product") {
                Some(value) => value.as_str() == Some(product.as_str()),
                // Existing sessions belong only to the original French product.
                None => product == crate::product::ProductId::Brioche,
            },
        }
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
            let data = self.data(record)?;
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
        let data = self.data(record)?;
        // An in-flight response must never resurrect a session deleted by logout or rotation.
        let result = self.db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,
            "UPDATE browser_sessions SET data = $2, expires_at = to_timestamp($3::bigint) WHERE id_hash = $1 AND expires_at > CURRENT_TIMESTAMP AND ($4::text IS NULL OR data->>'chef.product' = $4 OR ($4 = 'brioche' AND NOT data ? 'chef.product'))",
            [hash(&record.id).into(), data.into(), record.expiry_date.unix_timestamp().into(), self.product.map(|product| product.as_str().to_owned()).into()]))
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
        if !self.allows(&data) {
            // Treat a foreign cookie as anonymous, never revoke its other-product record.
            return Ok(None);
        }
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
                "DELETE FROM browser_sessions WHERE id_hash = $1 AND ($2::text IS NULL OR data->>'chef.product' = $2 OR ($2 = 'brioche' AND NOT data ? 'chef.product'))",
                [hash(id).into(), self.product.map(|product| product.as_str().to_owned()).into()],
            ))
            .await
            .map_err(|_| backend())?;
        Ok(())
    }
}
