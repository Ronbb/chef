use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
        CREATE TABLE course_speech_clips (
            id TEXT PRIMARY KEY CHECK(id ~ '^[a-f0-9]{32}$'),
            plan_id TEXT NOT NULL REFERENCES course_speech_plans(id),
            generation_key TEXT NOT NULL CHECK(generation_key ~ '^[a-f0-9]{64}$'),
            request JSONB NOT NULL,
            actor_id BIGINT NOT NULL REFERENCES users(id), reason TEXT NOT NULL,
            reused_from TEXT REFERENCES course_speech_clips(id),
            created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
            CHECK(reused_from IS NULL OR reused_from<>id)
        );
        CREATE INDEX course_speech_clips_key ON course_speech_clips(generation_key,created_at DESC,id DESC);
        CREATE TABLE course_speech_clip_events (
            clip_id TEXT NOT NULL REFERENCES course_speech_clips(id),
            version INTEGER NOT NULL CHECK(version IN (1,2)),
            status TEXT NOT NULL CHECK(status IN ('submitted','ready','failed','unknown')),
            result JSONB, created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
            PRIMARY KEY(clip_id,version), CHECK((status='ready')=(result IS NOT NULL)),
            CHECK(version=1 OR status<>'submitted')
        );
        CREATE TABLE course_speech_clip_reviews (
            clip_id TEXT PRIMARY KEY REFERENCES course_speech_clips(id),
            accepted BOOLEAN NOT NULL,
            actor_id BIGINT NOT NULL REFERENCES users(id), reason TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
        );
        CREATE TRIGGER immutable_course_speech_clips BEFORE UPDATE OR DELETE ON course_speech_clips FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        CREATE TRIGGER immutable_course_speech_clip_events BEFORE UPDATE OR DELETE ON course_speech_clip_events FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        CREATE TRIGGER immutable_course_speech_clip_reviews BEFORE UPDATE OR DELETE ON course_speech_clip_reviews FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        "#).await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
        DO $$ BEGIN IF EXISTS(SELECT 1 FROM course_speech_clips) THEN RAISE EXCEPTION 'course speech clips audit must be retained'; END IF; END $$;
        DROP TABLE course_speech_clip_reviews,course_speech_clip_events,course_speech_clips;
        "#).await?;
        Ok(())
    }
}
