use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
        CREATE TABLE content_releases (
            id TEXT PRIMARY KEY, manifest JSONB NOT NULL, content_hash CHAR(64) NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE release_entries (
            release_id TEXT NOT NULL REFERENCES content_releases(id),
            lesson_id TEXT NOT NULL, revision INTEGER NOT NULL,
            position INTEGER NOT NULL CHECK(position>=0),
            PRIMARY KEY(release_id,lesson_id), UNIQUE(release_id,position),
            FOREIGN KEY(lesson_id,revision) REFERENCES lesson_revisions(lesson_id,revision)
        );
        CREATE TABLE content_state (
            singleton BOOLEAN PRIMARY KEY DEFAULT true CHECK(singleton),
            active_release TEXT REFERENCES content_releases(id),
            generation BIGINT NOT NULL DEFAULT 0 CHECK(generation>=0)
        );
        INSERT INTO content_state(singleton) VALUES(true);
        CREATE TABLE content_withdrawals (
            lesson_id TEXT NOT NULL, revision INTEGER NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY(lesson_id,revision),
            FOREIGN KEY(lesson_id,revision) REFERENCES lesson_revisions(lesson_id,revision)
        );
        CREATE TABLE content_audit (
            id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
            action TEXT NOT NULL CHECK(action IN('stage','activate','withdraw')),
            actor TEXT NOT NULL, reason TEXT NOT NULL,
            release_id TEXT REFERENCES content_releases(id),
            lesson_id TEXT, revision INTEGER, generation BIGINT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        CREATE FUNCTION reject_content_edit() RETURNS trigger LANGUAGE plpgsql AS $$
        BEGIN RAISE EXCEPTION 'immutable content must use a new revision'; END $$;
        CREATE FUNCTION protect_lesson_snapshot() RETURNS trigger LANGUAGE plpgsql AS $$
        BEGIN
            IF NEW.lesson_id IS DISTINCT FROM OLD.lesson_id OR NEW.revision IS DISTINCT FROM OLD.revision
                OR NEW.public_document IS DISTINCT FROM OLD.public_document
                OR NEW.server_document IS DISTINCT FROM OLD.server_document THEN
                RAISE EXCEPTION 'immutable lesson snapshot';
            END IF;
            IF NEW.published AND EXISTS(SELECT 1 FROM content_withdrawals w
                WHERE w.lesson_id=NEW.lesson_id AND w.revision=NEW.revision) THEN
                RAISE EXCEPTION 'withdrawn revision cannot be restored';
            END IF;
            RETURN NEW;
        END $$;
        CREATE TRIGGER protect_lesson_snapshot BEFORE UPDATE ON lesson_revisions FOR EACH ROW EXECUTE FUNCTION protect_lesson_snapshot();
        CREATE TRIGGER immutable_lesson_delete BEFORE DELETE ON lesson_revisions FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        CREATE TRIGGER immutable_releases BEFORE UPDATE OR DELETE ON content_releases FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        CREATE TRIGGER immutable_entries BEFORE UPDATE OR DELETE ON release_entries FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        CREATE TRIGGER immutable_withdrawals BEFORE UPDATE OR DELETE ON content_withdrawals FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        CREATE TRIGGER immutable_audit BEFORE UPDATE OR DELETE ON content_audit FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        CREATE FUNCTION protect_release_insert() RETURNS trigger LANGUAGE plpgsql AS $$
        BEGIN
            IF EXISTS(SELECT 1 FROM content_audit WHERE release_id=NEW.release_id AND action='stage') THEN
                RAISE EXCEPTION 'staged directory is immutable';
            END IF;
            RETURN NEW;
        END $$;
        CREATE TRIGGER immutable_entry_insert BEFORE INSERT ON release_entries FOR EACH ROW EXECUTE FUNCTION protect_release_insert();
        "#).await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared("DROP TRIGGER protect_lesson_snapshot ON lesson_revisions; DROP TRIGGER immutable_lesson_delete ON lesson_revisions; DROP FUNCTION protect_lesson_snapshot(); DROP TABLE content_audit,content_withdrawals,content_state,release_entries,content_releases; DROP FUNCTION reject_content_edit(); DROP FUNCTION protect_release_insert()").await?;
        Ok(())
    }
}
