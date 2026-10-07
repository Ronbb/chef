//! Learning persistence only: no password service, authentication backend or session store.
use sea_orm::DatabaseConnection;

#[derive(Clone)]
pub struct LearningStore {
    pub(crate) db: DatabaseConnection,
    pub(crate) product: Option<crate::product::ProductId>,
}
impl LearningStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db, product: None }
    }
    pub(crate) fn for_product(db: DatabaseConnection, product: crate::product::ProductId) -> Self {
        Self {
            db,
            product: Some(product),
        }
    }
}
pub(crate) async fn lock_lesson<C: sea_orm::ConnectionTrait>(
    db: &C,
    product: Option<crate::product::ProductId>,
    lesson: &str,
    revision: i32,
) -> Result<(), crate::AppError> {
    if let Some(product) = product {
        let row = crate::learning::one(
            db,
            "SELECT chef_lock_product_lesson($1,$2,$3) AS found",
            vec![product.as_str().into(), lesson.into(), revision.into()],
        )
        .await?
        .ok_or(crate::AppError::Unavailable)?;
        return if crate::learning::field::<bool>(&row, "found")? {
            Ok(())
        } else {
            Err(crate::AppError::NotFound)
        };
    }
    crate::learning::one(
        db,
        "SELECT chef_lock_lesson($1,$2)",
        vec![lesson.into(), revision.into()],
    )
    .await?;
    Ok(())
}
pub(crate) fn routes() -> axum::Router<LearningStore> {
    axum::Router::new()
        .merge(crate::learning::router())
        .merge(crate::reviews::router())
        .merge(crate::library::router())
        .merge(crate::dashboard::router())
}
