use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{DbBackend, Statement};
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        let schema = db
            .query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT current_schema() AS schema_name",
            ))
            .await?
            .ok_or_else(|| DbErr::Custom("Migration schema unavailable".into()))?
            .try_get::<String>("", "schema_name")?;
        let schema = format!("\"{}\"", schema.replace('"', "\"\""));
        let lesson=format!("BEGIN PERFORM 1 FROM {schema}.lesson_revisions WHERE lesson_id=$1 AND revision=$2 FOR SHARE; END").replace('\'',"''");
        let state =
            format!("SELECT active_release FROM {schema}.content_state WHERE singleton FOR SHARE")
                .replace('\'', "''");
        db.execute_unprepared(&format!(
            r#"
            CREATE FUNCTION {schema}.chef_lock_lesson(TEXT,INTEGER) RETURNS VOID
                LANGUAGE plpgsql VOLATILE SECURITY DEFINER SET search_path=pg_catalog AS '{lesson}';
            CREATE FUNCTION {schema}.chef_lock_release_state() RETURNS TEXT
                LANGUAGE sql VOLATILE SECURITY DEFINER SET search_path=pg_catalog AS '{state}';
            REVOKE ALL ON FUNCTION {schema}.chef_lock_lesson(TEXT,INTEGER) FROM PUBLIC;
            REVOKE ALL ON FUNCTION {schema}.chef_lock_release_state() FROM PUBLIC;
        "#
        ))
        .await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared("DROP FUNCTION chef_lock_lesson(TEXT,INTEGER); DROP FUNCTION chef_lock_release_state()").await?;
        Ok(())
    }
}
