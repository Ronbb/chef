use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
            CREATE TABLE product_user_settings (
                product_id TEXT NOT NULL CHECK (product_id IN ('brioche','hargow')),
                user_id BIGINT NOT NULL REFERENCES users(id),
                settings JSONB NOT NULL DEFAULT '{"timeZone":"Asia/Shanghai","weeklyDays":5,"dailyMinutes":10,"showTranslation":false,"speechRate":1.0}'::jsonb,
                version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
                PRIMARY KEY(product_id,user_id)
            );
            INSERT INTO product_user_settings(product_id,user_id,settings,version)
                SELECT 'brioche',id,settings,profile_version FROM users;
            ALTER TABLE users DROP COLUMN settings;
        "#).await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Do not silently discard another product's preferences when rolling back.
        manager.get_connection().execute_unprepared(r#"
            DO $$ BEGIN
                IF EXISTS(SELECT 1 FROM product_user_settings WHERE product_id <> 'brioche') THEN
                    RAISE EXCEPTION 'Product settings rollback requires preserving other product data';
                END IF;
            END $$;
            ALTER TABLE users ADD COLUMN settings JSONB NOT NULL DEFAULT '{"timeZone":"Asia/Shanghai","weeklyDays":5,"dailyMinutes":10,"showTranslation":false,"speechRate":1.0}'::jsonb;
            UPDATE users u SET settings=s.settings,profile_version=GREATEST(u.profile_version,s.version)
                FROM product_user_settings s WHERE s.product_id='brioche' AND s.user_id=u.id;
            DROP TABLE product_user_settings;
        "#).await?;
        Ok(())
    }
}
