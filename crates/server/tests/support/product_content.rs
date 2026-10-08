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
    tx.execute_unprepared("INSERT INTO content_state(product_id,singleton,active_release) VALUES('hargow',false,'hargow-product-fixture')").await.unwrap();
    let before = tx
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT md5(to_jsonb(s)::text) AS hash FROM content_state s WHERE product_id='brioche'",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "hash")
        .unwrap();
    tx.execute_unprepared("UPDATE content_state SET generation=17 WHERE product_id='hargow'")
        .await
        .unwrap();
    let after=tx.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT md5(to_jsonb(s)::text) AS hash,(SELECT generation FROM content_state WHERE product_id='hargow') AS h_generation FROM content_state s WHERE product_id='brioche'")).await.unwrap().unwrap();
    assert_eq!(after.try_get::<String>("", "hash").unwrap(), before);
    assert_eq!(after.try_get::<i64>("", "h_generation").unwrap(), 17);
    let row=tx.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT chef_lock_product_release_state('hargow') AS h,chef_lock_product_release_state('brioche') IS NOT DISTINCT FROM chef_lock_release_state() AS same,(SELECT count(*) FROM content_state WHERE singleton)::bigint AS legacy_count")).await.unwrap().unwrap();
    assert_eq!(
        row.try_get::<String>("", "h").unwrap(),
        "hargow-product-fixture"
    );
    assert!(row.try_get::<bool>("", "same").unwrap());
    assert_eq!(row.try_get::<i64>("", "legacy_count").unwrap(), 1);
    let error=tx.execute_unprepared("UPDATE content_state SET active_release='hargow-product-fixture' WHERE product_id='brioche'").await.unwrap_err();
    assert!(
        error.to_string().contains("chef_state_product_release"),
        "{error}"
    );
    tx.rollback().await.unwrap();
}

// Schema proof only: public/runtime duplicate-ID imports remain a separate migration.
pub async fn verify_local_lesson_keys(db: &DatabaseConnection, lesson: &str, revision: i32) {
    let row=db.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT (SELECT array_agg(a.attname::text ORDER BY k.position) FROM pg_catalog.pg_constraint c CROSS JOIN LATERAL unnest(c.conkey) WITH ORDINALITY k(column_number,position) JOIN pg_catalog.pg_attribute a ON a.attrelid=c.conrelid AND a.attnum=k.column_number WHERE c.conrelid='lesson_revisions'::regclass AND c.contype='p')=ARRAY['product_id','lesson_id','revision']::text[] AND NOT EXISTS(SELECT 1 FROM pg_catalog.pg_constraint WHERE confrelid='lesson_revisions'::regclass AND contype='f' AND array_length(confkey,1)=2) AS correct")).await.unwrap().unwrap();
    assert!(row.try_get::<bool>("", "correct").unwrap());
    let tx = db.begin().await.unwrap();
    tx.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO lesson_revisions(product_id,lesson_id,revision,published,public_document,server_document) SELECT 'hargow',lesson_id,revision,false,public_document,server_document FROM lesson_revisions WHERE product_id='brioche' AND lesson_id=$1 AND revision=$2",[lesson.into(),revision.into()])).await.unwrap();
    let row = tx
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT count(*)::bigint AS n FROM lesson_revisions WHERE lesson_id=$1 AND revision=$2",
            [lesson.into(), revision.into()],
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 2);
    tx.execute_unprepared("INSERT INTO content_releases(product_id,id,manifest,content_hash) VALUES('hargow','local-lesson-fixture','{}',repeat('f',64))").await.unwrap();
    tx.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO release_entries(product_id,release_id,lesson_id,revision,position) VALUES('hargow','local-lesson-fixture',$1,$2,0)",[lesson.into(),revision.into()])).await.unwrap();
    // Same-product duplicates still fail; dropping the legacy global key is not dropping uniqueness.
    let error=tx.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO lesson_revisions(product_id,lesson_id,revision,published,public_document,server_document) SELECT product_id,lesson_id,revision,published,public_document,server_document FROM lesson_revisions WHERE product_id='hargow' AND lesson_id=$1 AND revision=$2",[lesson.into(),revision.into()])).await.unwrap_err();
    assert!(error.to_string().contains("duplicate key"), "{error}");
    tx.rollback().await.unwrap();
}

// Synthetic structural records: not language content, rights or real human listening evidence.
pub async fn verify_local_lesson_records(
    db: &DatabaseConnection,
    lesson: &str,
    revision: i32,
    actor: i64,
) {
    let tx = db.begin().await.unwrap();
    tx.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO lesson_revisions(product_id,lesson_id,revision,published,public_document,server_document) SELECT p.id,'local-record-fixture',1,false,l.public_document,l.server_document FROM lesson_revisions l CROSS JOIN (VALUES('brioche'),('hargow')) p(id) WHERE l.product_id='brioche' AND l.lesson_id=$1 AND l.revision=$2",[lesson.into(),revision.into()])).await.unwrap();
    tx.execute_unprepared("INSERT INTO content_withdrawals(product_id,lesson_id,revision) VALUES('hargow','local-record-fixture',1); UPDATE lesson_revisions SET published=true WHERE product_id='brioche' AND lesson_id='local-record-fixture'").await.unwrap();
    tx.execute_unprepared("SAVEPOINT local_withdrawal")
        .await
        .unwrap();
    let error=tx.execute_unprepared("UPDATE lesson_revisions SET published=true WHERE product_id='hargow' AND lesson_id='local-record-fixture'").await.unwrap_err();
    assert!(error.to_string().contains("withdrawn revision"), "{error}");
    tx.execute_unprepared("ROLLBACK TO SAVEPOINT local_withdrawal")
        .await
        .unwrap();
    tx.execute_unprepared("INSERT INTO content_withdrawals(product_id,lesson_id,revision) VALUES('brioche','local-record-fixture',1); INSERT INTO lesson_import_audit(product_id,lesson_id,revision,actor,reason) SELECT product_id,lesson_id,revision,'synthetic-author','Isolated structural fixture' FROM lesson_revisions WHERE lesson_id='local-record-fixture'").await.unwrap();
    tx.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO editorial_reviews(product_id,lesson_id,revision,version,approved,actor_id,reason) SELECT product_id,lesson_id,revision,1,false,$1,'Isolated structural fixture' FROM lesson_revisions WHERE lesson_id='local-record-fixture'",[actor.into()])).await.unwrap();
    tx.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO lesson_audio_reviews(product_id,lesson_id,revision,version,lesson_hash,accepted,heard,actor_id,reason) SELECT product_id,lesson_id,revision,1,repeat('e',64),false,false,$1,'Isolated structural fixture' FROM lesson_revisions WHERE lesson_id='local-record-fixture'",[actor.into()])).await.unwrap();
    tx.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO lesson_direct_publications(product_id,lesson_id,revision,lesson_hash,actor_id,reason,request,review_version) SELECT product_id,lesson_id,revision,repeat('e',64),$1,'Isolated structural fixture','{}',0 FROM lesson_revisions WHERE lesson_id='local-record-fixture'",[actor.into()])).await.unwrap();
    for table in [
        "content_withdrawals",
        "lesson_import_audit",
        "editorial_reviews",
        "lesson_audio_reviews",
        "lesson_direct_publications",
    ] {
        let row=tx.query_one_raw(Statement::from_string(DbBackend::Postgres,format!("SELECT count(*)::bigint AS n FROM {table} WHERE lesson_id='local-record-fixture'"))).await.unwrap().unwrap();
        assert_eq!(row.try_get::<i64>("", "n").unwrap(), 2, "{table}");
        tx.execute_unprepared("SAVEPOINT local_duplicate")
            .await
            .unwrap();
        let error=tx.execute_unprepared(&format!("INSERT INTO {table} SELECT * FROM {table} WHERE product_id='hargow' AND lesson_id='local-record-fixture'")).await.unwrap_err();
        assert!(
            error.to_string().contains("duplicate key"),
            "{table}: {error}"
        );
        tx.execute_unprepared("ROLLBACK TO SAVEPOINT local_duplicate")
            .await
            .unwrap();
    }
    tx.execute_unprepared("SAVEPOINT local_restore")
        .await
        .unwrap();
    let error=tx.execute_unprepared("UPDATE lesson_revisions SET published=true WHERE product_id='brioche' AND lesson_id='local-record-fixture'").await.unwrap_err();
    assert!(error.to_string().contains("withdrawn revision"), "{error}");
    tx.execute_unprepared("ROLLBACK TO SAVEPOINT local_restore")
        .await
        .unwrap();
    // Retain both immutable row guards after replacing uniqueness.
    let error=tx.execute_unprepared("UPDATE lesson_import_audit SET reason='changed' WHERE lesson_id='local-record-fixture'").await.unwrap_err();
    assert!(error.to_string().contains("immutable"), "{error}");
    tx.rollback().await.unwrap();
}

pub async fn verify_local_release_keys(db: &DatabaseConnection, lesson: &str, revision: i32) {
    let tx = db.begin().await.unwrap();
    tx.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO lesson_revisions(product_id,lesson_id,revision,published,public_document,server_document) SELECT p.id,'local-release-course',1,false,l.public_document,l.server_document FROM lesson_revisions l CROSS JOIN (VALUES('brioche'),('hargow')) p(id) WHERE l.product_id='brioche' AND l.lesson_id=$1 AND l.revision=$2",[lesson.into(),revision.into()])).await.unwrap();
    tx.execute_unprepared("INSERT INTO content_releases(product_id,id,manifest,content_hash) VALUES('brioche','local-release-fixture','{}',repeat('e',64)),('hargow','local-release-fixture','{}',repeat('e',64)); INSERT INTO release_entries(product_id,release_id,lesson_id,revision,position) VALUES('brioche','local-release-fixture','local-release-course',1,0),('hargow','local-release-fixture','local-release-course',1,0); INSERT INTO content_audit(product_id,action,actor,reason,release_id,generation) VALUES('brioche','stage','synthetic','Isolated structural fixture','local-release-fixture',0)").await.unwrap();
    tx.execute_unprepared("INSERT INTO lesson_revisions(product_id,lesson_id,revision,published,public_document,server_document) SELECT 'hargow','local-release-position',revision,published,public_document,server_document FROM lesson_revisions WHERE product_id='hargow' AND lesson_id='local-release-course'; SAVEPOINT release_position").await.unwrap();
    let error=tx.execute_unprepared("INSERT INTO release_entries(product_id,release_id,lesson_id,revision,position) VALUES('hargow','local-release-fixture','local-release-position',1,0)").await.unwrap_err();
    assert!(
        error.to_string().contains("chef_local_release_position"),
        "{error}"
    );
    tx.execute_unprepared("ROLLBACK TO SAVEPOINT release_position")
        .await
        .unwrap();
    tx.execute_unprepared("INSERT INTO lesson_revisions(product_id,lesson_id,revision,published,public_document,server_document) SELECT 'hargow','local-release-next',revision,published,public_document,server_document FROM lesson_revisions WHERE product_id='hargow' AND lesson_id='local-release-course'; INSERT INTO release_entries(product_id,release_id,lesson_id,revision,position) VALUES('hargow','local-release-fixture','local-release-next',1,1); INSERT INTO content_audit(product_id,action,actor,reason,release_id,generation) VALUES('hargow','stage','synthetic','Isolated structural fixture','local-release-fixture',0); INSERT INTO content_state(product_id,singleton,active_release,generation) VALUES('hargow',false,'local-release-fixture',8); UPDATE content_state SET active_release='local-release-fixture',generation=3 WHERE product_id='brioche'").await.unwrap();
    for product in ["brioche", "hargow"] {
        tx.execute_unprepared("SAVEPOINT release_immutable")
            .await
            .unwrap();
        let error=tx.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO release_entries(product_id,release_id,lesson_id,revision,position) VALUES($1,'local-release-fixture','missing-course',1,9)",[product.into()])).await.unwrap_err();
        assert!(
            error.to_string().contains("staged directory is immutable"),
            "{error}"
        );
        tx.execute_unprepared("ROLLBACK TO SAVEPOINT release_immutable")
            .await
            .unwrap();
    }
    tx.execute_unprepared("SAVEPOINT release_duplicate")
        .await
        .unwrap();
    let error=tx.execute_unprepared("INSERT INTO content_releases(product_id,id,manifest,content_hash) VALUES('hargow','local-release-fixture','{}',repeat('e',64))").await.unwrap_err();
    assert!(error.to_string().contains("duplicate key"), "{error}");
    tx.execute_unprepared("ROLLBACK TO SAVEPOINT release_duplicate; INSERT INTO content_releases(product_id,id,manifest,content_hash) VALUES('brioche','local-release-foreign','{}',repeat('e',64)); SAVEPOINT release_parent").await.unwrap();
    let error=tx.execute_unprepared("UPDATE content_state SET active_release='local-release-foreign' WHERE product_id='hargow'").await.unwrap_err();
    assert!(
        error.to_string().contains("chef_state_product_release"),
        "{error}"
    );
    tx.execute_unprepared("ROLLBACK TO SAVEPOINT release_parent")
        .await
        .unwrap();
    let rows = tx
        .query_all_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT product_id,generation FROM content_state ORDER BY product_id",
        ))
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].try_get::<i64>("", "generation").unwrap(), 3);
    assert_eq!(rows[1].try_get::<i64>("", "generation").unwrap(), 8);
    tx.rollback().await.unwrap();
}
