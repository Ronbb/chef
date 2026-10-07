use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("browser_sessions"))
                    .col(
                        ColumnDef::new(Alias::new("id_hash"))
                            .string_len(64)
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Alias::new("data")).json_binary().not_null())
                    .col(
                        ColumnDef::new(Alias::new("expires_at"))
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("browser_sessions_expiry")
                    .table(Alias::new("browser_sessions"))
                    .col(Alias::new("expires_at"))
                    .to_owned(),
            )
            .await
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("browser_sessions"))
                    .to_owned(),
            )
            .await
    }
}
