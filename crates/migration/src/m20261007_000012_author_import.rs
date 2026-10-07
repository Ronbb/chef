use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
        CREATE TABLE lesson_import_audit (
            lesson_id TEXT NOT NULL, revision INTEGER NOT NULL,
            actor TEXT NOT NULL, reason TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY(lesson_id,revision),
            FOREIGN KEY(lesson_id,revision) REFERENCES lesson_revisions(lesson_id,revision)
        );
        CREATE TRIGGER immutable_lesson_import_audit BEFORE UPDATE OR DELETE ON lesson_import_audit
            FOR EACH ROW EXECUTE FUNCTION reject_content_edit();
        "#,
            )
            .await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE lesson_import_audit")
            .await?;
        Ok(())
    }
}
