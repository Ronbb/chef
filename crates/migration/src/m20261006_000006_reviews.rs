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
            ALTER TABLE review_cards ADD CONSTRAINT review_card_owner UNIQUE(id,user_id);
            CREATE TABLE review_attempts (
                id VARCHAR(32) PRIMARY KEY,
                card_id VARCHAR(32) NOT NULL,
                user_id BIGINT NOT NULL,
                rating TEXT NOT NULL CHECK(rating IN ('again','remembered','familiar')),
                old_stage SMALLINT NOT NULL CHECK(old_stage BETWEEN -1 AND 4),
                new_stage SMALLINT NOT NULL CHECK(new_stage BETWEEN 0 AND 4),
                old_version INTEGER NOT NULL,
                new_version INTEGER NOT NULL,
                due_at TIMESTAMPTZ NOT NULL,
                reviewed_at TIMESTAMPTZ NOT NULL,
                time_zone TEXT NOT NULL,
                algorithm_version TEXT NOT NULL DEFAULT 'fixed-v1',
                FOREIGN KEY(card_id,user_id) REFERENCES review_cards(id,user_id),
                UNIQUE(card_id,old_version),
                CHECK(new_version=old_version+1)
            );
            CREATE INDEX review_attempt_owner_time ON review_attempts(user_id,reviewed_at DESC);
        "#,
            )
            .await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared("DROP TABLE review_attempts; ALTER TABLE review_cards DROP CONSTRAINT review_card_owner;").await?;
        Ok(())
    }
}
