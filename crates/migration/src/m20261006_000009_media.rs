use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
        CREATE TABLE media_assets (
            asset_id TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision>0),
            descriptor JSONB NOT NULL, provenance JSONB NOT NULL,
            sha256 CHAR(64) NOT NULL, extension TEXT NOT NULL CHECK(extension IN('svg','png','jpg','webp')),
            byte_size BIGINT NOT NULL CHECK(byte_size>0 AND byte_size<=33554432),
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY(asset_id,revision)
        );
        CREATE INDEX media_hash ON media_assets(sha256);
        CREATE TABLE character_revisions (
            character_id TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision>0),
            snapshot JSONB NOT NULL, avatar_id TEXT NOT NULL, avatar_revision INTEGER NOT NULL,
            PRIMARY KEY(character_id,revision),
            FOREIGN KEY(avatar_id,avatar_revision) REFERENCES media_assets(asset_id,revision)
        );
        CREATE TABLE asset_import_audit (
            id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
            actor TEXT NOT NULL, bundle_hash CHAR(64) NOT NULL,
            asset_count INTEGER NOT NULL, character_count INTEGER NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TRIGGER immutable_media BEFORE UPDATE OR DELETE ON media_assets FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        CREATE TRIGGER immutable_characters BEFORE UPDATE OR DELETE ON character_revisions FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        CREATE TRIGGER immutable_asset_audit BEFORE UPDATE OR DELETE ON asset_import_audit FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        "#).await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE asset_import_audit,character_revisions,media_assets")
            .await?;
        Ok(())
    }
}
