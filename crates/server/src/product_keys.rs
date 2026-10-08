//! Capability checks for trusted product-local identifiers; caller data never selects a table.
use crate::{
    AppError,
    learning::{field, one},
};
use sea_orm::ConnectionTrait;
#[derive(Clone, Copy)]
pub(crate) enum LocalIdTable {
    SpeechPlan,
    SpeechClip,
}
pub(crate) async fn supports_local_ids(
    db: &impl ConnectionTrait,
    table: LocalIdTable,
) -> Result<bool, AppError> {
    let name = match table {
        LocalIdTable::SpeechPlan => "course_speech_plans",
        LocalIdTable::SpeechClip => "course_speech_clips",
    };
    let row=one(db,r#"SELECT EXISTS(SELECT 1 FROM pg_catalog.pg_constraint c WHERE c.contype='p' AND c.conrelid=$1::text::regclass
        AND (SELECT array_agg(a.attname::text ORDER BY k.position) FROM unnest(c.conkey) WITH ORDINALITY k(column_number,position) JOIN pg_catalog.pg_attribute a ON a.attrelid=c.conrelid AND a.attnum=k.column_number)=ARRAY['product_id','id']::text[]) AS ready"#,vec![name.into()]).await?.ok_or(AppError::Unavailable)?;
    field(&row, "ready")
}
