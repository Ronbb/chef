use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
        CREATE TABLE voice_auditions (
            id TEXT PRIMARY KEY CHECK(id ~ '^[a-f0-9]{32}$'),
            clone_job_id TEXT NOT NULL REFERENCES voice_clone_jobs(id),
            clone_version INTEGER NOT NULL CHECK(clone_version>0),
            profile JSONB NOT NULL, parameters JSONB NOT NULL,
            actor_id BIGINT NOT NULL REFERENCES users(id), reason TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE voice_audition_events (
            audition_id TEXT NOT NULL REFERENCES voice_auditions(id), version INTEGER NOT NULL CHECK(version>0),
            status TEXT NOT NULL CHECK(status IN ('submitted','unknown','failed','ready')), result JSONB,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP, PRIMARY KEY(audition_id,version),
            CHECK((status='ready')=(result IS NOT NULL))
        );
        CREATE TABLE voice_audition_reviews (
            audition_id TEXT PRIMARY KEY REFERENCES voice_auditions(id), accepted BOOLEAN NOT NULL,
            character_id TEXT NOT NULL, character_revision INTEGER NOT NULL, voice_revision INTEGER,
            actor_id BIGINT NOT NULL REFERENCES users(id), reason TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(character_id,character_revision,voice_revision) REFERENCES character_voice_profiles(character_id,character_revision,revision),
            CHECK(accepted=(voice_revision IS NOT NULL))
        );
        CREATE TRIGGER immutable_voice_auditions BEFORE UPDATE OR DELETE ON voice_auditions FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        CREATE TRIGGER immutable_voice_audition_events BEFORE UPDATE OR DELETE ON voice_audition_events FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        CREATE TRIGGER immutable_voice_audition_reviews BEFORE UPDATE OR DELETE ON voice_audition_reviews FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        "#).await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
        DO $$ BEGIN IF EXISTS(SELECT 1 FROM voice_auditions) THEN RAISE EXCEPTION 'voice audition audit must be retained'; END IF; END $$;
        DROP TABLE voice_audition_reviews; DROP TABLE voice_audition_events; DROP TABLE voice_auditions;
        "#).await?;
        Ok(())
    }
}
