use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
        CREATE TABLE voice_reference_grants (
            id TEXT PRIMARY KEY CHECK(id ~ '^[a-f0-9]{32}$'),
            token_hash TEXT NOT NULL UNIQUE CHECK(token_hash ~ '^[a-f0-9]{64}$'),
            character_id TEXT NOT NULL, character_revision INTEGER NOT NULL, voice_revision INTEGER NOT NULL,
            asset_id TEXT NOT NULL, asset_revision INTEGER NOT NULL,
            descriptor JSONB NOT NULL, reference JSONB NOT NULL,
            actor_id BIGINT NOT NULL REFERENCES users(id), reason TEXT NOT NULL,
            model TEXT NOT NULL CHECK(model='qwen-audio-3.1-tts-flash'),
            single_speaker_confirmed BOOLEAN NOT NULL CHECK(single_speaker_confirmed),
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            expires_at TIMESTAMPTZ NOT NULL CHECK(expires_at>created_at AND expires_at<=created_at+interval '15 minutes'),
            FOREIGN KEY(character_id,character_revision,voice_revision) REFERENCES character_voice_profiles(character_id,character_revision,revision),
            FOREIGN KEY(asset_id,asset_revision) REFERENCES audio_assets(asset_id,revision)
        );
        CREATE TABLE voice_reference_revocations (
            grant_id TEXT PRIMARY KEY REFERENCES voice_reference_grants(id),
            actor_id BIGINT NOT NULL REFERENCES users(id), reason TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE voice_reference_reads (
            id BIGSERIAL PRIMARY KEY, grant_id TEXT NOT NULL REFERENCES voice_reference_grants(id),
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        CREATE INDEX voice_reference_reads_grant ON voice_reference_reads(grant_id);
        CREATE TRIGGER immutable_voice_reference_grants BEFORE UPDATE OR DELETE ON voice_reference_grants
            FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        CREATE TRIGGER immutable_voice_reference_revocations BEFORE UPDATE OR DELETE ON voice_reference_revocations
            FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        CREATE TRIGGER immutable_voice_reference_reads BEFORE UPDATE OR DELETE ON voice_reference_reads
            FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        "#).await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
        DO $$ BEGIN
            IF EXISTS(SELECT 1 FROM voice_reference_grants) THEN
                RAISE EXCEPTION 'voice reference authorization audit must be retained';
            END IF;
        END $$;
        DROP TABLE voice_reference_reads;
        DROP TABLE voice_reference_revocations;
        DROP TABLE voice_reference_grants;
        "#,
            )
            .await?;
        Ok(())
    }
}
