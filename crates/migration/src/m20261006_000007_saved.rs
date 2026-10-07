use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
        CREATE TABLE saved_items (
            id VARCHAR(32) PRIMARY KEY,
            user_id BIGINT NOT NULL REFERENCES users(id),
            knowledge_id TEXT NOT NULL,
            source_lesson_id TEXT NOT NULL,
            source_revision INTEGER NOT NULL,
            snapshot JSONB NOT NULL,
            saved BOOLEAN NOT NULL DEFAULT true,
            version INTEGER NOT NULL DEFAULT 1 CHECK(version>0),
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(user_id,knowledge_id),
            FOREIGN KEY(source_lesson_id,source_revision) REFERENCES lesson_revisions(lesson_id,revision)
        );
        CREATE INDEX saved_owner_created ON saved_items(user_id,created_at DESC,id DESC) WHERE saved=true;
        "#).await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE saved_items")
            .await?;
        Ok(())
    }
}
