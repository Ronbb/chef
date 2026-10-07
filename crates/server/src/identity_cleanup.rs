//! Identity-owned expiration maintenance. Learning-only processes never start it.
use sea_orm::{ConnectionTrait, DatabaseConnection};

pub fn spawn(db: DatabaseConnection) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let store = crate::session_store::PgSessionStore::new(db.clone());
        let mut timer = tokio::time::interval(std::time::Duration::from_secs(60));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            timer.tick().await;
            if store.delete_expired().await.is_err() {
                tracing::warn!("session cleanup unavailable");
            }
            if db.execute_unprepared("DELETE FROM auth_throttle WHERE resets_at <= CURRENT_TIMESTAMP; DELETE FROM identity_tokens WHERE expires_at < CURRENT_TIMESTAMP - interval '7 days';").await.is_err() {
                tracing::warn!("identity cleanup unavailable");
            }
        }
    })
}
