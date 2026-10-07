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
            CREATE TABLE chef_schema_layout (
                singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK(singleton),
                learning_schema TEXT NOT NULL,
                identity_schema TEXT NOT NULL CHECK(identity_schema<>learning_schema),
                split_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TRIGGER immutable_schema_layout BEFORE UPDATE OR DELETE ON chef_schema_layout
                FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        "#,
            )
            .await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
            DO $$ BEGIN
                IF EXISTS(SELECT 1 FROM chef_schema_layout) THEN
                    RAISE EXCEPTION 'Split identity schema requires preserving layout and identity data';
                END IF;
            END $$;
            DROP TABLE chef_schema_layout;
        "#).await?;
        Ok(())
    }
}
