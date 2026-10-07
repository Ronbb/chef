use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
            CREATE TABLE users (
                id BIGSERIAL PRIMARY KEY,
                email TEXT NOT NULL UNIQUE CHECK (email = lower(email) AND length(email) <= 254),
                password_hash TEXT NOT NULL,
                display_name TEXT NOT NULL CHECK (length(display_name) BETWEEN 1 AND 80),
                role TEXT NOT NULL DEFAULT 'learner' CHECK (role IN ('learner','operator')),
                settings JSONB NOT NULL DEFAULT '{}'::jsonb,
                created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE identity_tokens (
                token_hash VARCHAR(64) PRIMARY KEY,
                kind TEXT NOT NULL CHECK (kind IN ('invite','reset')),
                email TEXT NOT NULL,
                user_id BIGINT REFERENCES users(id),
                role TEXT NOT NULL DEFAULT 'learner' CHECK (role IN ('learner','operator')),
                expires_at TIMESTAMPTZ NOT NULL,
                consumed_at TIMESTAMPTZ,
                CHECK ((kind = 'invite' AND user_id IS NULL) OR (kind = 'reset' AND user_id IS NOT NULL))
            );
            CREATE INDEX identity_tokens_expiry ON identity_tokens(expires_at);
            CREATE INDEX identity_tokens_user ON identity_tokens(user_id);
            CREATE TABLE auth_throttle (
                key_hash VARCHAR(64) PRIMARY KEY,
                attempts INTEGER NOT NULL,
                resets_at TIMESTAMPTZ NOT NULL
            );
        "#).await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "DROP TABLE auth_throttle; DROP TABLE identity_tokens; DROP TABLE users;",
            )
            .await?;
        Ok(())
    }
}
