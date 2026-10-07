use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
        CREATE TABLE course_speech_plans (
            id TEXT PRIMARY KEY CHECK(id ~ '^[a-f0-9]{32}$'),
            lesson_id TEXT NOT NULL, lesson_revision INTEGER NOT NULL,
            request JSONB NOT NULL, plan JSONB NOT NULL, summary JSONB NOT NULL,
            actor_id BIGINT NOT NULL REFERENCES users(id), reason TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(lesson_id,lesson_revision) REFERENCES lesson_revisions(lesson_id,revision)
        );
        CREATE INDEX course_speech_plans_lesson ON course_speech_plans(lesson_id,lesson_revision,id);
        CREATE TRIGGER immutable_course_speech_plans BEFORE UPDATE OR DELETE ON course_speech_plans FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        "#).await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
        DO $$ BEGIN IF EXISTS(SELECT 1 FROM course_speech_plans) THEN RAISE EXCEPTION 'course speech plans audit must be retained'; END IF; END $$;
        DROP TABLE course_speech_plans;
        "#).await?;
        Ok(())
    }
}
