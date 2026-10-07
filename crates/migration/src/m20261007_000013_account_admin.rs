use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
            CREATE TABLE account_admin_audit (
                id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
                action TEXT NOT NULL CHECK(action IN ('invite','reset','role','sessions')),
                actor_id BIGINT NOT NULL REFERENCES users(id),
                target_email TEXT NOT NULL,
                details JSONB NOT NULL DEFAULT '{}'::jsonb,
                reason TEXT NOT NULL CHECK(length(btrim(reason))>0 AND octet_length(reason)<=1000),
                created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TRIGGER immutable_account_admin_audit BEFORE UPDATE OR DELETE ON account_admin_audit
                FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        "#).await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE account_admin_audit")
            .await?;
        Ok(())
    }
}
