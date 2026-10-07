use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
        CREATE TABLE speech_package_imports (
            id TEXT PRIMARY KEY CHECK(id ~ '^[a-f0-9]{32}$'),
            alignment_id TEXT NOT NULL REFERENCES speech_alignments(id),
            actor_id BIGINT NOT NULL REFERENCES users(id),
            request JSONB NOT NULL, result JSONB NOT NULL, manifest JSONB NOT NULL,
            reason TEXT NOT NULL, lesson_id TEXT NOT NULL, revision INTEGER NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
            FOREIGN KEY(lesson_id,revision) REFERENCES lesson_revisions(lesson_id,revision),
            UNIQUE(lesson_id,revision)
        );
        CREATE INDEX speech_package_imports_alignment ON speech_package_imports(alignment_id,id);
        CREATE TRIGGER immutable_speech_package_imports BEFORE UPDATE OR DELETE ON speech_package_imports FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        "#).await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
        DO $$ BEGIN IF EXISTS(SELECT 1 FROM speech_package_imports) THEN RAISE EXCEPTION 'speech package audit must be retained'; END IF; END $$;
        DROP TABLE speech_package_imports;
        "#).await?;
        Ok(())
    }
}
