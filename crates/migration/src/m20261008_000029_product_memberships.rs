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
            CREATE TABLE product_memberships (
                product_id TEXT NOT NULL CHECK(product_id IN ('brioche','hargow')),
                user_id BIGINT NOT NULL REFERENCES users(id),
                role TEXT NOT NULL CHECK(role IN ('learner','operator')),
                version INTEGER NOT NULL DEFAULT 1 CHECK(version > 0),
                PRIMARY KEY(product_id,user_id)
            );
            INSERT INTO product_memberships(product_id,user_id,role)
                SELECT 'brioche',id,role FROM users;
            CREATE TABLE product_membership_audit (
                id BIGSERIAL PRIMARY KEY,
                product_id TEXT NOT NULL CHECK(product_id IN ('brioche','hargow')),
                actor_id BIGINT NOT NULL REFERENCES users(id),
                target_id BIGINT NOT NULL REFERENCES users(id),
                old_role TEXT, new_role TEXT NOT NULL CHECK(new_role IN ('learner','operator')),
                old_version INTEGER NOT NULL, new_version INTEGER NOT NULL,
                reason TEXT NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
        "#,
            )
            .await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
            DO $$ BEGIN
                IF EXISTS(SELECT 1 FROM product_memberships WHERE product_id <> 'brioche')
                    OR EXISTS(SELECT 1 FROM product_membership_audit)
                    OR EXISTS(SELECT 1 FROM product_memberships m JOIN users u ON u.id=m.user_id WHERE m.role<>u.role)
                THEN RAISE EXCEPTION 'Product membership rollback requires preserving scoped grants and audit'; END IF;
            END $$;
            DROP TABLE product_membership_audit;
            DROP TABLE product_memberships;
        "#).await?;
        Ok(())
    }
}
