use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
        CREATE TABLE speech_alignments (
            id TEXT PRIMARY KEY CHECK(id ~ '^[a-f0-9]{32}$'),
            plan_id TEXT NOT NULL REFERENCES course_speech_plans(id),
            request JSONB NOT NULL, report JSONB NOT NULL,
            report_hash TEXT NOT NULL CHECK(report_hash ~ '^[a-f0-9]{64}$'),
            actor_id BIGINT NOT NULL REFERENCES users(id), reason TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
        );
        CREATE INDEX speech_alignments_plan ON speech_alignments(plan_id,id);
        CREATE TABLE speech_alignment_reviews (
            alignment_id TEXT NOT NULL REFERENCES speech_alignments(id),
            clip_id TEXT NOT NULL REFERENCES course_speech_clips(id),
            accepted BOOLEAN NOT NULL, words JSONB NOT NULL CHECK(jsonb_typeof(words)='array'),
            request JSONB NOT NULL, actor_id BIGINT NOT NULL REFERENCES users(id), reason TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
            PRIMARY KEY(alignment_id,clip_id),
            CHECK(NOT accepted OR jsonb_array_length(words)>0)
        );
        CREATE TRIGGER immutable_speech_alignments BEFORE UPDATE OR DELETE ON speech_alignments FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        CREATE TRIGGER immutable_speech_alignment_reviews BEFORE UPDATE OR DELETE ON speech_alignment_reviews FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        "#).await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
        DO $$ BEGIN IF EXISTS(SELECT 1 FROM speech_alignments) THEN RAISE EXCEPTION 'alignment audit must be retained'; END IF; END $$;
        DROP TABLE speech_alignment_reviews,speech_alignments;
        "#).await?;
        Ok(())
    }
}
