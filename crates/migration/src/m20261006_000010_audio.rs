use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
        CREATE TABLE audio_assets (
            asset_id TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision>0),
            descriptor JSONB NOT NULL, provenance JSONB NOT NULL,
            sha256 CHAR(64) NOT NULL CHECK(sha256 ~ '^[0-9a-f]{64}$'),
            extension TEXT NOT NULL CHECK(extension IN('mp3','wav')),
            byte_size BIGINT NOT NULL CHECK(byte_size>0 AND byte_size<=33554432),
            duration_ms INTEGER NOT NULL CHECK(duration_ms>0 AND duration_ms<=1800000),
            sample_rate INTEGER NOT NULL CHECK(sample_rate BETWEEN 8000 AND 96000),
            channels INTEGER NOT NULL CHECK(channels IN(1,2)),
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY(asset_id,revision)
        );
        CREATE INDEX audio_hash ON audio_assets(sha256);
        CREATE TABLE audio_import_audit (
            id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
            actor TEXT NOT NULL, bundle_hash CHAR(64) NOT NULL,
            asset_count INTEGER NOT NULL CHECK(asset_count BETWEEN 1 AND 500),
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TRIGGER immutable_audio BEFORE UPDATE OR DELETE ON audio_assets FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        CREATE TRIGGER immutable_audio_audit BEFORE UPDATE OR DELETE ON audio_import_audit FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        "#).await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE audio_import_audit,audio_assets")
            .await?;
        Ok(())
    }
}
