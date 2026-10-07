use sea_orm::entity::prelude::*;
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "lesson_revisions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub lesson_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub revision: i32,
    pub published: bool,
    pub public_document: Json,
    pub server_document: Json,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
