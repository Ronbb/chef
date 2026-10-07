//! Learning preferences are keyed by a trusted product and authenticated account.
use crate::{AppError, product::ProductId};
use brioche_course_contract::UserSettings;
use sea_orm::{ConnectionTrait, DbBackend, Statement};

pub struct Preferences {
    pub settings: UserSettings,
    pub version: u32,
}
pub(crate) async fn ensure<C: ConnectionTrait>(
    db: &C,
    product: ProductId,
    user: i64,
) -> Result<(), AppError> {
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,
        "INSERT INTO product_user_settings(product_id,user_id) VALUES($1,$2) ON CONFLICT DO NOTHING",
        [product.as_str().into(), user.into()])).await.map_err(|_| AppError::Unavailable)?;
    Ok(())
}
pub async fn read<C: ConnectionTrait>(
    db: &C,
    product: ProductId,
    user: i64,
) -> Result<Preferences, AppError> {
    let row = db.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,
        "SELECT s.settings,s.version FROM users u LEFT JOIN product_user_settings s ON s.user_id=u.id AND s.product_id=$2 WHERE u.id=$1",
        [user.into(), product.as_str().into()])).await.map_err(|_| AppError::Unavailable)?.ok_or(AppError::Unauthorized)?;
    let settings: Option<serde_json::Value> = row
        .try_get("", "settings")
        .map_err(|_| AppError::Unavailable)?;
    let version: Option<i32> = row
        .try_get("", "version")
        .map_err(|_| AppError::Unavailable)?;
    Ok(Preferences {
        settings: settings
            .map(serde_json::from_value)
            .transpose()
            .map_err(|_| AppError::Unavailable)?
            .unwrap_or_default(),
        version: u32::try_from(version.unwrap_or(1)).map_err(|_| AppError::Unavailable)?,
    })
}
/// Caller uses its transaction; optimistic concurrency is independent per product.
pub(crate) async fn save<C: ConnectionTrait>(
    db: &C,
    product: ProductId,
    user: i64,
    expected: u32,
    settings: &UserSettings,
) -> Result<u32, AppError> {
    ensure(db, product, user).await?;
    let row = db.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,
        "UPDATE product_user_settings SET settings=$3,version=version+1 WHERE product_id=$1 AND user_id=$2 AND version=$4 AND version<2147483647 RETURNING version",
        [product.as_str().into(), user.into(), serde_json::to_value(settings).map_err(|_| AppError::Unavailable)?.into(), i32::try_from(expected).map_err(|_| AppError::Conflict)?.into()]))
        .await.map_err(|_| AppError::Unavailable)?.ok_or(AppError::Conflict)?;
    u32::try_from(
        row.try_get::<i32>("", "version")
            .map_err(|_| AppError::Unavailable)?,
    )
    .map_err(|_| AppError::Unavailable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{ConnectOptions, Database, TransactionTrait};
    use sea_orm_migration::MigratorTrait;
    #[tokio::test]
    #[ignore = "set TEST_DATABASE_URL to a dedicated PostgreSQL database"]
    async fn migration_preserves_brioche_and_settings_revisions_are_product_local() {
        let url = std::env::var("TEST_DATABASE_URL").expect("dedicated test database required");
        let admin = Database::connect(&url).await.unwrap();
        let schema = format!(
            "product_settings_{}",
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
        brioche_migration::Migrator::up(&db, Some(27))
            .await
            .unwrap();
        db.execute_unprepared(r#"INSERT INTO users(id,email,password_hash,display_name,role,settings,profile_version)
            VALUES(101,'shared-settings@example.test','test-hash-preserved','Shared learner','learner',
                '{"timeZone":"Europe/Paris","weeklyDays":3,"dailyMinutes":15,"showTranslation":true,"speechRate":0.75}',7)"#).await.unwrap();
        brioche_migration::Migrator::up(&db, None).await.unwrap();
        let before = read(&db, ProductId::Brioche, 101).await.unwrap();
        assert_eq!(before.version, 7);
        assert_eq!(before.settings.time_zone, "Europe/Paris");
        assert!(before.settings.show_translation);
        assert_eq!(before.settings.speech_rate, 0.75);
        let other = read(&db, ProductId::Hargow, 101).await.unwrap();
        assert_eq!(other.version, 1);
        assert_eq!(other.settings.time_zone, "Asia/Shanghai");
        assert!(!other.settings.show_translation);
        assert!(matches!(
            read(&db, ProductId::Hargow, 999).await,
            Err(AppError::Unauthorized)
        ));
        let tx = db.begin().await.unwrap();
        let mut french = before.settings;
        french.time_zone = "Pacific/Honolulu".into();
        assert_eq!(
            save(&tx, ProductId::Brioche, 101, 7, &french)
                .await
                .unwrap(),
            8
        );
        tx.commit().await.unwrap();
        let tx = db.begin().await.unwrap();
        let mut cantonese = other.settings;
        cantonese.speech_rate = 1.25;
        assert_eq!(
            save(&tx, ProductId::Hargow, 101, 1, &cantonese)
                .await
                .unwrap(),
            2
        );
        tx.commit().await.unwrap();
        assert_eq!(
            read(&db, ProductId::Brioche, 101)
                .await
                .unwrap()
                .settings
                .speech_rate,
            0.75
        );
        assert_eq!(
            read(&db, ProductId::Hargow, 101)
                .await
                .unwrap()
                .settings
                .time_zone,
            "Asia/Shanghai"
        );
        let tx = db.begin().await.unwrap();
        assert!(matches!(
            save(&tx, ProductId::Brioche, 101, 7, &cantonese).await,
            Err(AppError::Conflict)
        ));
        tx.rollback().await.unwrap();
        // Concurrent writers for one product cannot both consume the same revision.
        let write = || async {
            let tx = db.begin().await.unwrap();
            let result = save(&tx, ProductId::Hargow, 101, 2, &cantonese).await;
            match result {
                Ok(_) => tx.commit().await.unwrap(),
                Err(_) => tx.rollback().await.unwrap(),
            }
            result
        };
        let (a, b) = tokio::join!(write(), write());
        assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
        assert!(matches!(a, Ok(3) | Err(AppError::Conflict)));
        assert!(matches!(b, Ok(3) | Err(AppError::Conflict)));
        let account = db
            .query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT id,password_hash,profile_version FROM users WHERE id=101",
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            account.try_get::<String>("", "password_hash").unwrap(),
            "test-hash-preserved"
        );
        assert_eq!(account.try_get::<i32>("", "profile_version").unwrap(), 7);
        assert!(
            brioche_migration::Migrator::down(&db, Some(1))
                .await
                .is_err()
        );
        assert_eq!(read(&db, ProductId::Hargow, 101).await.unwrap().version, 3);
        // Explicit deletion below is only disposable test data, never a rollback procedure.
        db.execute_unprepared("DELETE FROM product_user_settings WHERE product_id='hargow'")
            .await
            .unwrap();
        brioche_migration::Migrator::down(&db, Some(1))
            .await
            .unwrap();
        brioche_migration::Migrator::up(&db, None).await.unwrap();
        let restored = read(&db, ProductId::Brioche, 101).await.unwrap();
        assert_eq!(restored.version, 8);
        assert_eq!(restored.settings.time_zone, "Pacific/Honolulu");
        assert_eq!(restored.settings.daily_minutes, 15);
        brioche_migration::Migrator::down(&db, None).await.unwrap();
        admin
            .execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
            .await
            .unwrap();
    }
}
