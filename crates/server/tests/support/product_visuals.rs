use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement, TransactionTrait};
const TABLES: [&str; 3] = ["media_assets", "character_revisions", "asset_import_audit"];
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
pub async fn verify(db: &DatabaseConnection) {
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
        let tx = db.begin().await.unwrap();
        let error = tx
            .execute_unprepared(&format!("UPDATE {table} SET product_id='hargow'"))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("product"), "{table}: {error}");
        tx.rollback().await.unwrap();
    }
    // Database-only synthetic registrations; not a course or real Hargow character.
    let tx = db.begin().await.unwrap();
    let error=tx.execute_unprepared("INSERT INTO character_revisions(product_id,character_id,revision,snapshot,avatar_id,avatar_revision) SELECT 'hargow','foreign-avatar-test',1,snapshot,avatar_id,avatar_revision FROM character_revisions LIMIT 1").await.unwrap_err();
    assert!(
        error.to_string().contains("chef_character_product_avatar"),
        "{error}"
    );
    tx.rollback().await.unwrap();
    let tx = db.begin().await.unwrap();
    tx.execute_unprepared("INSERT INTO media_assets(product_id,asset_id,revision,descriptor,provenance,sha256,extension,byte_size) SELECT 'hargow','synthetic-h-avatar',1,descriptor,provenance,sha256,extension,byte_size FROM media_assets LIMIT 1").await.unwrap();
    tx.execute_unprepared("INSERT INTO character_revisions(product_id,character_id,revision,snapshot,avatar_id,avatar_revision) SELECT 'hargow','synthetic-h-character',1,snapshot,'synthetic-h-avatar',1 FROM character_revisions LIMIT 1").await.unwrap();
    tx.execute_unprepared("INSERT INTO asset_import_audit(product_id,actor,bundle_hash,asset_count,character_count) VALUES('hargow','synthetic',repeat('a',64),1,1)").await.unwrap();
    let row=tx.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM media_assets WHERE product_id='hargow')+(SELECT count(*) FROM character_revisions WHERE product_id='hargow')+(SELECT count(*) FROM asset_import_audit WHERE product_id='hargow') AS n")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 3);
    let error=tx.execute_unprepared("INSERT INTO character_revisions(product_id,character_id,revision,snapshot,avatar_id,avatar_revision) SELECT 'brioche','reverse-foreign-avatar',1,snapshot,'synthetic-h-avatar',1 FROM character_revisions LIMIT 1").await.unwrap_err();
    assert!(
        error.to_string().contains("chef_character_product_avatar"),
        "{error}"
    );
    tx.rollback().await.unwrap();
}
