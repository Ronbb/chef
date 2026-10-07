use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
        CREATE TABLE editorial_reviews (
            lesson_id TEXT NOT NULL, revision INTEGER NOT NULL,
            version INTEGER NOT NULL CHECK(version>0),
            approved BOOLEAN NOT NULL,
            actor_id BIGINT NOT NULL REFERENCES users(id),
            reason TEXT NOT NULL CHECK(length(btrim(reason))>0 AND octet_length(reason)<=1000),
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY(lesson_id,revision,version),
            FOREIGN KEY(lesson_id,revision) REFERENCES lesson_revisions(lesson_id,revision)
        );
        CREATE TRIGGER immutable_editorial_reviews BEFORE UPDATE OR DELETE ON editorial_reviews
            FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        "#,
            )
            .await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE editorial_reviews")
            .await?;
        Ok(())
    }
}
