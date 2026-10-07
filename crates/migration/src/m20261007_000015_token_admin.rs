use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared("ALTER TABLE account_admin_audit DROP CONSTRAINT account_admin_audit_action_check; ALTER TABLE account_admin_audit ADD CONSTRAINT account_admin_audit_action_check CHECK(action IN ('invite','reset','role','sessions','revokeInvite','revokeReset'));").await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Refuse downgrade if new audit records exist; never delete history to fit the old constraint.
        manager.get_connection().execute_unprepared("ALTER TABLE account_admin_audit DROP CONSTRAINT account_admin_audit_action_check; ALTER TABLE account_admin_audit ADD CONSTRAINT account_admin_audit_action_check CHECK(action IN ('invite','reset','role','sessions'));").await?;
        Ok(())
    }
}
