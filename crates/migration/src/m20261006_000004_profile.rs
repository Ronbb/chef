use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
            ALTER TABLE users ADD COLUMN profile_version INTEGER NOT NULL DEFAULT 1 CHECK (profile_version > 0);
            ALTER TABLE users ALTER COLUMN settings SET DEFAULT '{"timeZone":"Asia/Shanghai","weeklyDays":5,"dailyMinutes":10,"showTranslation":false,"speechRate":1.0}'::jsonb;
            UPDATE users SET settings = '{"timeZone":"Asia/Shanghai","weeklyDays":5,"dailyMinutes":10,"showTranslation":false,"speechRate":1.0}'::jsonb || settings;
        "#).await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared("ALTER TABLE users DROP COLUMN profile_version; ALTER TABLE users ALTER COLUMN settings SET DEFAULT '{}'::jsonb;").await?;
        Ok(())
    }
}
