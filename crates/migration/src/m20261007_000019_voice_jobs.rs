use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
        CREATE TABLE voice_clone_jobs (
            id TEXT PRIMARY KEY CHECK(id ~ '^[a-f0-9]{32}$'),
            grant_id TEXT NOT NULL UNIQUE REFERENCES voice_reference_grants(id),
            prefix TEXT NOT NULL UNIQUE CHECK(prefix ~ '^[a-zA-Z0-9]{1,10}$'),
            actor_id BIGINT NOT NULL REFERENCES users(id), reason TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE voice_clone_events (
            job_id TEXT NOT NULL REFERENCES voice_clone_jobs(id), version INTEGER NOT NULL CHECK(version>0),
            status TEXT NOT NULL CHECK(status IN ('submitted','unknown','failed','processing','checking','ready','unavailable','modelMismatch','checkFailed')),
            voice_id TEXT, request_id TEXT, actor_id BIGINT REFERENCES users(id), reason TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY(job_id,version)
        );
        CREATE TRIGGER immutable_voice_clone_jobs BEFORE UPDATE OR DELETE ON voice_clone_jobs FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        CREATE TRIGGER immutable_voice_clone_events BEFORE UPDATE OR DELETE ON voice_clone_events FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        "#).await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
        DO $$ BEGIN IF EXISTS(SELECT 1 FROM voice_clone_jobs) THEN RAISE EXCEPTION 'voice clone audit must be retained'; END IF; END $$;
        DROP TABLE voice_clone_events; DROP TABLE voice_clone_jobs;
        "#).await?;
        Ok(())
    }
}
