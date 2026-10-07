//! Account-owned product grants. Global account role never grants another product.
use crate::{
    AppError,
    identity::{AuthSession, Backend},
    product::ProductId,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, patch},
};
use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait};
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Membership {
    pub role: String,
    pub version: u32,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Change {
    pub role: String,
    pub expected_version: u32,
    pub reason: String,
}
pub async fn read<C: ConnectionTrait>(
    db: &C,
    product: ProductId,
    user: i64,
) -> Result<Membership, AppError> {
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT role,version FROM product_memberships WHERE product_id=$1 AND user_id=$2",
            [product.as_str().into(), user.into()],
        ))
        .await
        .map_err(|_| AppError::Unavailable)?;
    match row {
        Some(row) => Ok(Membership {
            role: row.try_get("", "role").map_err(|_| AppError::Unavailable)?,
            version: u32::try_from(
                row.try_get::<i32>("", "version")
                    .map_err(|_| AppError::Unavailable)?,
            )
            .map_err(|_| AppError::Unavailable)?,
        }),
        None => Ok(Membership {
            role: "learner".into(),
            version: 0,
        }),
    }
}
pub async fn change(
    backend: &Backend,
    product: ProductId,
    actor: i64,
    target: i64,
    request: Change,
) -> Result<Membership, AppError> {
    let reason = request.reason.trim();
    if !matches!(request.role.as_str(), "learner" | "operator")
        || reason.is_empty()
        || reason.chars().count() > 500
        || reason.chars().any(char::is_control)
    {
        return Err(AppError::InvalidInput);
    }
    let tx = backend
        .db
        .begin()
        .await
        .map_err(|_| AppError::Unavailable)?;
    tx.execute_unprepared("SELECT pg_advisory_xact_lock(hashtextextended('account-admin',0))")
        .await
        .map_err(|_| AppError::Unavailable)?;
    // Recheck after locking, so an in-flight request cannot retain a revoked grant.
    if read(&tx, product, actor).await?.role != "operator" {
        return Err(AppError::Forbidden);
    }
    tx.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT id FROM users WHERE id=$1 FOR UPDATE",
        [target.into()],
    ))
    .await
    .map_err(|_| AppError::Unavailable)?
    .ok_or(AppError::NotFound)?;
    let previous = read(&tx, product, target).await?;
    if previous.version != request.expected_version || previous.version >= 2147483647 {
        return Err(AppError::Conflict);
    }
    if previous.role == "operator" && request.role != "operator" {
        let row=tx.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,
            "SELECT count(*)::bigint AS count FROM product_memberships WHERE product_id=$1 AND role='operator'",[product.as_str().into()])).await.map_err(|_|AppError::Unavailable)?.ok_or(AppError::Unavailable)?;
        if row
            .try_get::<i64>("", "count")
            .map_err(|_| AppError::Unavailable)?
            <= 1
        {
            return Err(AppError::Conflict);
        }
    }
    let next = previous.version + 1;
    tx.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,
        "INSERT INTO product_memberships(product_id,user_id,role,version) VALUES($1,$2,$3,$4) ON CONFLICT(product_id,user_id) DO UPDATE SET role=EXCLUDED.role,version=EXCLUDED.version",
        [product.as_str().into(),target.into(),request.role.clone().into(),i32::try_from(next).map_err(|_|AppError::Conflict)?.into()])).await.map_err(|_|AppError::Unavailable)?;
    tx.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,
        "INSERT INTO product_membership_audit(product_id,actor_id,target_id,old_role,new_role,old_version,new_version,reason) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
        vec![product.as_str().into(),actor.into(),target.into(),if previous.version==0 {None::<String>} else {Some(previous.role)}.into(),request.role.clone().into(),i32::try_from(previous.version).map_err(|_|AppError::Conflict)?.into(),i32::try_from(next).map_err(|_|AppError::Conflict)?.into(),reason.into()])).await.map_err(|_|AppError::Unavailable)?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Membership {
        role: request.role,
        version: next,
    })
}
async fn current(
    auth: AuthSession,
    State((backend, product)): State<(Backend, ProductId)>,
) -> Result<Json<Membership>, AppError> {
    let actor = crate::learning::owner(&auth)?;
    Ok(Json(read(&backend.db, product, actor).await?))
}
async fn update(
    auth: AuthSession,
    State((backend, product)): State<(Backend, ProductId)>,
    Path(target): Path<i64>,
    Json(request): Json<Change>,
) -> Result<Json<Membership>, AppError> {
    let actor = crate::learning::owner(&auth)?;
    Ok(Json(
        change(&backend, product, actor, target, request).await?,
    ))
}
pub(crate) fn router(backend: Backend, product: ProductId) -> Router<Backend> {
    Router::new()
        .route("/api/v1/account/membership", get(current))
        .route("/api/v1/account-admin/members/{id}", patch(update))
        .with_state((backend, product))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{ConnectOptions, Database};
    use sea_orm_migration::MigratorTrait;
    #[tokio::test]
    #[ignore = "set TEST_DATABASE_URL to a dedicated PostgreSQL database"]
    async fn scoped_grants_recheck_actor_cas_last_operator_and_audit() {
        let url = std::env::var("TEST_DATABASE_URL").unwrap();
        let admin = Database::connect(&url).await.unwrap();
        let schema = format!(
            "memberships_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        admin
            .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
            .await
            .unwrap();
        let mut options = ConnectOptions::new(url);
        options.set_schema_search_path(&schema).sqlx_logging(false);
        let db = Database::connect(options).await.unwrap();
        brioche_migration::Migrator::up(&db, Some(28))
            .await
            .unwrap();
        db.execute_unprepared("INSERT INTO users(id,email,password_hash,display_name,role) VALUES(101,'operator-one@example.test','test','One','operator'),(102,'operator-two@example.test','test','Two','operator'),(103,'learner@example.test','test','Three','learner')").await.unwrap();
        brioche_migration::Migrator::up(&db, None).await.unwrap();
        let backend = Backend::new(db.clone()).await.unwrap();
        let request = |role: &str, expected_version| Change {
            role: role.into(),
            expected_version,
            reason: "Synthetic scoped grant test".into(),
        };
        assert_eq!(
            read(&db, ProductId::Brioche, 101).await.unwrap().role,
            "operator"
        );
        assert_eq!(read(&db, ProductId::Hargow, 101).await.unwrap().version, 0);
        assert!(matches!(
            change(
                &backend,
                ProductId::Hargow,
                101,
                103,
                request("operator", 0)
            )
            .await,
            Err(AppError::Forbidden)
        ));
        assert_eq!(
            change(
                &backend,
                ProductId::Brioche,
                101,
                103,
                request("operator", 1)
            )
            .await
            .unwrap()
            .version,
            2
        );
        assert_eq!(
            read(&db, ProductId::Hargow, 103).await.unwrap().role,
            "learner"
        );
        let (a, b) = tokio::join!(
            change(
                &backend,
                ProductId::Brioche,
                101,
                103,
                request("learner", 2)
            ),
            change(
                &backend,
                ProductId::Brioche,
                102,
                103,
                request("operator", 2)
            )
        );
        assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
        assert!(matches!(a, Ok(_) | Err(AppError::Conflict)));
        assert!(matches!(b, Ok(_) | Err(AppError::Conflict)));
        change(
            &backend,
            ProductId::Brioche,
            102,
            103,
            request("learner", 3),
        )
        .await
        .unwrap();
        change(
            &backend,
            ProductId::Brioche,
            102,
            101,
            request("learner", 1),
        )
        .await
        .unwrap();
        assert!(matches!(
            change(
                &backend,
                ProductId::Brioche,
                101,
                103,
                request("operator", 4)
            )
            .await,
            Err(AppError::Forbidden)
        ));
        assert!(matches!(
            change(
                &backend,
                ProductId::Brioche,
                102,
                102,
                request("learner", 1)
            )
            .await,
            Err(AppError::Conflict)
        ));
        // Explicit initial Hargow grant is synthetic fixture setup, never inherited from global role.
        db.execute_unprepared("INSERT INTO product_memberships(product_id,user_id,role) VALUES('hargow',101,'operator')").await.unwrap();
        assert_eq!(
            change(
                &backend,
                ProductId::Hargow,
                101,
                103,
                request("operator", 0)
            )
            .await
            .unwrap()
            .version,
            1
        );
        assert_eq!(
            read(&db, ProductId::Brioche, 103).await.unwrap().role,
            "learner"
        );
        assert!(matches!(
            change(
                &backend,
                ProductId::Brioche,
                102,
                999,
                request("operator", 0)
            )
            .await,
            Err(AppError::NotFound)
        ));
        let rows=db.query_all_raw(Statement::from_string(DbBackend::Postgres,"SELECT product_id,actor_id,target_id,reason FROM product_membership_audit ORDER BY id")).await.unwrap();
        assert_eq!(rows.len(), 5);
        assert_eq!(
            rows.last()
                .unwrap()
                .try_get::<String>("", "product_id")
                .unwrap(),
            "hargow"
        );
        let account = db
            .query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT role FROM users WHERE id=101",
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(account.try_get::<String>("", "role").unwrap(), "operator");
        assert!(
            brioche_migration::Migrator::down(&db, Some(1))
                .await
                .is_err()
        );
        assert_eq!(
            read(&db, ProductId::Hargow, 103).await.unwrap().role,
            "operator"
        );
        // Disposable test cleanup after proving rollback refusal; not a production rollback recipe.
        db.execute_unprepared("DELETE FROM product_membership_audit; DELETE FROM product_memberships WHERE product_id='hargow'; UPDATE product_memberships m SET role=u.role FROM users u WHERE u.id=m.user_id").await.unwrap();
        brioche_migration::Migrator::down(&db, None).await.unwrap();
        admin
            .execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
            .await
            .unwrap();
    }
}
