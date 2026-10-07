use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
        CREATE TABLE lesson_direct_publications (
            lesson_id TEXT NOT NULL, revision INTEGER NOT NULL,
            lesson_hash CHAR(64) NOT NULL, actor_id BIGINT NOT NULL REFERENCES users(id),
            reason TEXT NOT NULL, request JSONB NOT NULL,
            review_version INTEGER NOT NULL CHECK(review_version >= 0),
            created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
            PRIMARY KEY(lesson_id,revision),
            FOREIGN KEY(lesson_id,revision) REFERENCES lesson_revisions(lesson_id,revision)
        );
        CREATE TRIGGER immutable_lesson_direct_publications BEFORE UPDATE OR DELETE ON lesson_direct_publications FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        "#).await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
        DO $$ BEGIN IF EXISTS(SELECT 1 FROM lesson_direct_publications) THEN RAISE EXCEPTION 'direct publication audit must be retained'; END IF; END $$;
        DROP TABLE lesson_direct_publications;
        "#).await?;
        Ok(())
    }
}
