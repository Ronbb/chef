use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
        CREATE TABLE lesson_audio_reviews (
            lesson_id TEXT NOT NULL, revision INTEGER NOT NULL, version INTEGER NOT NULL CHECK(version>0),
            lesson_hash CHAR(64) NOT NULL, accepted BOOLEAN NOT NULL, heard BOOLEAN NOT NULL,
            actor_id BIGINT NOT NULL REFERENCES users(id), reason TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
            PRIMARY KEY(lesson_id,revision,version),
            FOREIGN KEY(lesson_id,revision) REFERENCES lesson_revisions(lesson_id,revision),
            CHECK(NOT accepted OR heard)
        );
        CREATE TRIGGER immutable_lesson_audio_reviews BEFORE UPDATE OR DELETE ON lesson_audio_reviews FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        "#).await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
        DO $$ BEGIN IF EXISTS(SELECT 1 FROM lesson_audio_reviews) THEN RAISE EXCEPTION 'lesson audio audit must be retained'; END IF; END $$;
        DROP TABLE lesson_audio_reviews;
        "#).await?;
        Ok(())
    }
}
