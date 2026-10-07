use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement, TransactionTrait};
pub const TABLES: [&str; 10] = [
    "lesson_revisions",
    "content_releases",
    "release_entries",
    "content_state",
    "content_withdrawals",
    "content_audit",
    "lesson_import_audit",
    "editorial_reviews",
    "lesson_audio_reviews",
    "lesson_direct_publications",
];
pub async fn snapshot(db: &DatabaseConnection) -> Vec<(i64, String)> {
    let mut result = Vec::new();
    for table in TABLES {
        let row=db.query_one_raw(Statement::from_string(DbBackend::Postgres,format!("SELECT count(*)::bigint AS n,md5(coalesce(jsonb_agg(to_jsonb(t)-'product_id' ORDER BY to_jsonb(t)-'product_id')::text,'')) AS hash FROM {table} t"))).await.unwrap().unwrap();
        result.push((
            row.try_get("", "n").unwrap(),
            row.try_get("", "hash").unwrap(),
        ));
    }
    result
}
pub async fn verify(db: &DatabaseConnection, lesson: &str, revision: i32) {
    for table in TABLES {
        let row = db
            .query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                format!("SELECT count(*)::bigint AS n FROM {table} WHERE product_id<>'brioche'"),
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.try_get::<i64>("", "n").unwrap(), 0, "{table}");
    }
    let tx = db.begin().await.unwrap();
    let error = tx
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE lesson_revisions SET product_id='hargow' WHERE lesson_id=$1 AND revision=$2",
            [lesson.into(), revision.into()],
        ))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("product"), "{error}");
    tx.rollback().await.unwrap();
    let tx = db.begin().await.unwrap();
    tx.execute_unprepared("INSERT INTO content_releases(product_id,id,manifest,content_hash) VALUES('hargow','hargow-product-fixture','{}',repeat('a',64))").await.unwrap();
    let error=tx.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO release_entries(product_id,release_id,lesson_id,revision,position) VALUES('hargow','hargow-product-fixture',$1,$2,0)",[lesson.into(),revision.into()])).await.unwrap_err();
    assert!(
        error.to_string().contains("chef_entry_product_lesson"),
        "{error}"
    );
    tx.rollback().await.unwrap();
    let tx = db.begin().await.unwrap();
    tx.execute_unprepared("INSERT INTO content_releases(product_id,id,manifest,content_hash) VALUES('hargow','hargow-product-fixture','{}',repeat('a',64))").await.unwrap();
    tx.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO lesson_revisions(product_id,lesson_id,revision,published,public_document,server_document) SELECT 'hargow','hargow-product-lesson',revision,false,public_document,server_document FROM lesson_revisions WHERE lesson_id=$1 AND revision=$2",[lesson.into(),revision.into()])).await.unwrap();
    tx.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO release_entries(product_id,release_id,lesson_id,revision,position) VALUES('hargow','hargow-product-fixture','hargow-product-lesson',$1,0)",[revision.into()])).await.unwrap();
    tx.rollback().await.unwrap();
}
