use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
            CREATE TABLE learning_sessions (
                id VARCHAR(32) PRIMARY KEY,
                user_id BIGINT NOT NULL REFERENCES users(id),
                lesson_id TEXT NOT NULL,
                revision INTEGER NOT NULL,
                schema_version TEXT NOT NULL,
                version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
                last_step_id TEXT,
                completed_at TIMESTAMPTZ,
                created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE (id, user_id),
                UNIQUE (id, user_id, lesson_id),
                FOREIGN KEY (lesson_id,revision) REFERENCES lesson_revisions(lesson_id,revision)
            );
            CREATE UNIQUE INDEX learning_one_active ON learning_sessions(user_id,lesson_id) WHERE completed_at IS NULL;
            CREATE INDEX learning_owner_recent ON learning_sessions(user_id,updated_at DESC,id DESC);
            CREATE TABLE lesson_progress (
                user_id BIGINT NOT NULL REFERENCES users(id),
                lesson_id TEXT NOT NULL,
                last_session_id VARCHAR(32) NOT NULL,
                first_completed_at TIMESTAMPTZ,
                latest_completed_revision INTEGER,
                PRIMARY KEY (user_id,lesson_id),
                FOREIGN KEY (last_session_id,user_id,lesson_id) REFERENCES learning_sessions(id,user_id,lesson_id)
            );
            CREATE TABLE step_progress (
                session_id VARCHAR(32) NOT NULL REFERENCES learning_sessions(id),
                step_id TEXT NOT NULL,
                confirmed_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (session_id,step_id)
            );
            CREATE TABLE exercise_hints (
                session_id VARCHAR(32) NOT NULL REFERENCES learning_sessions(id),
                exercise_id TEXT NOT NULL,
                seen_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (session_id,exercise_id)
            );
            CREATE TABLE exercise_attempts (
                id VARCHAR(32) PRIMARY KEY,
                session_id VARCHAR(32) NOT NULL,
                user_id BIGINT NOT NULL,
                exercise_id TEXT NOT NULL,
                attempt_index INTEGER NOT NULL CHECK (attempt_index BETWEEN 1 AND 100),
                answer JSONB NOT NULL,
                result JSONB NOT NULL,
                hint_used BOOLEAN NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (session_id,user_id) REFERENCES learning_sessions(id,user_id),
                UNIQUE (session_id,exercise_id,attempt_index)
            );
            CREATE TABLE learning_operations (
                user_id BIGINT NOT NULL REFERENCES users(id),
                scope TEXT NOT NULL,
                idempotency_key VARCHAR(128) NOT NULL,
                request_hash VARCHAR(64) NOT NULL,
                result JSONB NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (user_id,scope,idempotency_key)
            );
            CREATE TABLE review_cards (
                id VARCHAR(32) PRIMARY KEY,
                user_id BIGINT NOT NULL REFERENCES users(id),
                knowledge_id TEXT NOT NULL,
                source_lesson_id TEXT NOT NULL,
                source_revision INTEGER NOT NULL,
                snapshot JSONB NOT NULL,
                stage SMALLINT NOT NULL DEFAULT -1 CHECK (stage BETWEEN -1 AND 4),
                due_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
                suspended BOOLEAN NOT NULL DEFAULT false,
                created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE (user_id,knowledge_id),
                FOREIGN KEY (source_lesson_id,source_revision) REFERENCES lesson_revisions(lesson_id,revision)
            );
            CREATE INDEX review_owner_due ON review_cards(user_id,due_at,id) WHERE suspended=false;
        "#).await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared("DROP TABLE review_cards; DROP TABLE learning_operations; DROP TABLE exercise_attempts; DROP TABLE exercise_hints; DROP TABLE step_progress; DROP TABLE lesson_progress; DROP TABLE learning_sessions;").await?;
        Ok(())
    }
}
