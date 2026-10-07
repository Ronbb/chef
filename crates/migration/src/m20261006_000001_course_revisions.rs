use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("lesson_revisions"))
                    .col(ColumnDef::new(Alias::new("lesson_id")).string().not_null())
                    .col(ColumnDef::new(Alias::new("revision")).integer().not_null())
                    .col(
                        ColumnDef::new(Alias::new("published"))
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Alias::new("public_document"))
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("server_document"))
                            .json_binary()
                            .not_null(),
                    )
                    .primary_key(
                        Index::create()
                            .col(Alias::new("lesson_id"))
                            .col(Alias::new("revision")),
                    )
                    .to_owned(),
            )
            .await
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("lesson_revisions"))
                    .to_owned(),
            )
            .await
    }
}
