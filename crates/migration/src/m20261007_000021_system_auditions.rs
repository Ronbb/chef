use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Existing clone rows remain untouched, including their immutable audit trigger.
        manager.get_connection().execute_unprepared(r#"
        ALTER TABLE voice_auditions ALTER clone_job_id DROP NOT NULL, ALTER clone_version DROP NOT NULL;
        ALTER TABLE voice_auditions ADD character_id TEXT, ADD character_revision INTEGER, ADD base_voice_revision INTEGER;
        ALTER TABLE voice_auditions ADD CONSTRAINT audition_character FOREIGN KEY(character_id,character_revision) REFERENCES character_revisions(character_id,revision);
        ALTER TABLE voice_auditions ADD CONSTRAINT audition_source CHECK (
          (clone_job_id IS NOT NULL AND clone_version IS NOT NULL AND character_id IS NULL AND character_revision IS NULL AND base_voice_revision IS NULL)
          OR (clone_job_id IS NULL AND clone_version IS NULL AND character_id IS NOT NULL AND character_revision>0 AND base_voice_revision>=0 AND character_revision IS NOT NULL AND base_voice_revision IS NOT NULL)
        );
        "#).await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
        DO $$ BEGIN IF EXISTS(SELECT 1 FROM voice_auditions WHERE clone_job_id IS NULL) THEN RAISE EXCEPTION 'system audition audit must be retained'; END IF; END $$;
        ALTER TABLE voice_auditions DROP CONSTRAINT audition_source, DROP CONSTRAINT audition_character;
        ALTER TABLE voice_auditions DROP character_id, DROP character_revision, DROP base_voice_revision;
        ALTER TABLE voice_auditions ALTER clone_job_id SET NOT NULL, ALTER clone_version SET NOT NULL;
        "#).await?;
        Ok(())
    }
}
