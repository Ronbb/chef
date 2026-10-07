use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
            ALTER TABLE identity_tokens ADD COLUMN product_id TEXT NOT NULL DEFAULT 'brioche'
                CHECK(product_id IN ('brioche','hargow'));
            ALTER TABLE account_admin_audit ADD COLUMN product_id TEXT NOT NULL DEFAULT 'brioche'
                CHECK(product_id IN ('brioche','hargow'));
            CREATE INDEX identity_tokens_product_pending ON identity_tokens(product_id,kind,expires_at)
                WHERE consumed_at IS NULL;
            CREATE INDEX account_admin_audit_product ON account_admin_audit(product_id,created_at,id);
        "#).await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
            DO $$ BEGIN
                IF EXISTS(SELECT 1 FROM identity_tokens WHERE product_id <> 'brioche')
                    OR EXISTS(SELECT 1 FROM account_admin_audit WHERE product_id <> 'brioche')
                THEN RAISE EXCEPTION 'Identity product rollback requires preserving scoped tokens and audit'; END IF;
            END $$;
            DROP INDEX account_admin_audit_product;
            DROP INDEX identity_tokens_product_pending;
            ALTER TABLE account_admin_audit DROP COLUMN product_id;
            ALTER TABLE identity_tokens DROP COLUMN product_id;
        "#).await?;
        Ok(())
    }
}
