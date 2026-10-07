use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
        CREATE TABLE character_voice_profiles (
            character_id TEXT NOT NULL, character_revision INTEGER NOT NULL,
            revision INTEGER NOT NULL CHECK(revision>0), profile JSONB NOT NULL,
            actor_id BIGINT NOT NULL REFERENCES users(id), reason TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY(character_id,character_revision,revision),
            FOREIGN KEY(character_id,character_revision) REFERENCES character_revisions(character_id,revision)
        );
        CREATE TRIGGER immutable_character_voices BEFORE UPDATE OR DELETE ON character_voice_profiles
            FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        "#).await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE character_voice_profiles")
            .await?;
        Ok(())
    }
}
