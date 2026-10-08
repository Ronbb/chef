use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement, TransactionTrait};
const TABLES: [&str; 2] = ["audio_assets", "audio_import_audit"];
pub async fn seed(db: &DatabaseConnection, root: &std::path::Path) {
    use chef_engine::recording::{AudioBundle, AudioSpec};
    use sha2::{Digest, Sha256};
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/audio");
    let (bytes, info) =
        chef_engine::audio::inspect_file(&source.join("synthetic.mp3"), "audio/mpeg").unwrap();
    chef_engine::recording::import_bundle(
        db,
        AudioBundle {
            schema_version: "1.0".into(),
            assets: vec![AudioSpec {
                asset_id: "layout-recording-fixture".into(),
                revision: 1,
                sha256: format!("{:x}", Sha256::digest(bytes)),
                mime_type: "audio/mpeg".into(),
                duration_ms: info.duration_ms,
                credit_zh: "Synthetic fixture".into(),
                file: "synthetic.mp3".into(),
                status: "ready".into(),
                source: "test:synthetic".into(),
                license: "LicenseRef-TestOnly".into(),
                creator: "Protocol fixture".into(),
                rights_confirmed: true,
            }],
        },
        &source,
        root,
        "protocol-test",
    )
    .await
    .unwrap();
}
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
        assert!(error.to_string().contains("product"), "{error}");
        tx.rollback().await.unwrap();
    }
    let tx = db.begin().await.unwrap();
    let error=tx.execute_unprepared("INSERT INTO audio_assets(product_id,asset_id,revision,descriptor,provenance,sha256,extension,byte_size,duration_ms,sample_rate,channels) SELECT 'unknown','bad-product',1,descriptor,provenance,sha256,extension,byte_size,duration_ms,sample_rate,channels FROM audio_assets LIMIT 1").await.unwrap_err();
    assert!(
        error.to_string().contains("audio_assets_product_id_check"),
        "{error}"
    );
    tx.rollback().await.unwrap();
    let tx = db.begin().await.unwrap();
    tx.execute_unprepared("INSERT INTO audio_assets(product_id,asset_id,revision,descriptor,provenance,sha256,extension,byte_size,duration_ms,sample_rate,channels) SELECT 'hargow','synthetic-h-recording',1,descriptor,provenance,sha256,extension,byte_size,duration_ms,sample_rate,channels FROM audio_assets LIMIT 1").await.unwrap();
    tx.execute_unprepared("INSERT INTO audio_import_audit(product_id,actor,bundle_hash,asset_count) VALUES('hargow','synthetic',repeat('a',64),1)").await.unwrap();
    let row=tx.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT count(*) FROM audio_assets WHERE product_id='hargow')+(SELECT count(*) FROM audio_import_audit WHERE product_id='hargow') AS n")).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 2);
    tx.rollback().await.unwrap();
}
