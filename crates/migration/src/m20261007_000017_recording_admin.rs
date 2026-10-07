use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared("ALTER TABLE audio_import_audit ADD COLUMN actor_id BIGINT REFERENCES users(id), ADD COLUMN reason TEXT, ADD COLUMN target TEXT, ADD CONSTRAINT audio_operator_audit CHECK ((actor_id IS NULL AND reason IS NULL AND target IS NULL) OR (actor_id IS NOT NULL AND reason IS NOT NULL AND length(btrim(reason))>0 AND target IS NOT NULL AND length(btrim(target))>0))").await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared("DO $$ BEGIN IF EXISTS(SELECT 1 FROM audio_import_audit WHERE actor_id IS NOT NULL) THEN RAISE EXCEPTION 'audio operator audit exists'; END IF; END $$; ALTER TABLE audio_import_audit DROP CONSTRAINT audio_operator_audit, DROP COLUMN actor_id, DROP COLUMN reason, DROP COLUMN target").await?;
        Ok(())
    }
}
