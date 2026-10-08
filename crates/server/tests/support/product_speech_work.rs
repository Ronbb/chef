use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement, TransactionTrait};
const TABLES: [&str; 7] = [
    "course_speech_plans",
    "course_speech_clips",
    "course_speech_clip_events",
    "course_speech_clip_reviews",
    "speech_alignments",
    "speech_alignment_reviews",
    "speech_package_imports",
];
async fn graph(
    db: &impl ConnectionTrait,
    product: Option<&str>,
    lesson: &str,
    id: &str,
    reuse: &str,
    actor: i64,
) {
    let column = if product.is_some() { "product_id," } else { "" };
    let value = product.map(|p| format!("'{p}',")).unwrap_or_default();
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,format!("INSERT INTO lesson_revisions({column}lesson_id,revision,published,public_document,server_document) SELECT {value}$1,1,false,public_document,server_document FROM lesson_revisions ORDER BY lesson_id,revision LIMIT 1"),[lesson.into()])).await.unwrap();
    let summary = serde_json::json!({"id":null,"lessonId":lesson,"lessonRevision":1,"sourceHash":"0".repeat(64),"planHash":"0".repeat(64),"requestCount":0,"totalRequestCharacters":0,"selection":{"voices":[],"knowledgeNarrator":{"characterId":"layout-voice-character","characterRevision":1,"voiceRevision":1},"emotions":{}},"voices":[],"targets":[],"createdAt":null});
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,format!("INSERT INTO course_speech_plans({column}id,lesson_id,lesson_revision,request,plan,summary,actor_id,reason) VALUES({value}$1,$2,1,'{{}}','{{}}',$3,$4,'Layout speech fixture')"),[id.into(),lesson.into(),summary.into(),actor.into()])).await.unwrap();
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,format!("INSERT INTO course_speech_clips({column}id,plan_id,generation_key,request,actor_id,reason) VALUES({value}$1,$1,repeat('f',64),'{{}}',$2,'Layout speech fixture')"),[id.into(),actor.into()])).await.unwrap();
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,format!("INSERT INTO course_speech_clips({column}id,plan_id,generation_key,request,actor_id,reason,reused_from) VALUES({value}$1,$2,repeat('f',64),'{{}}',$3,'Layout reuse fixture',$2)"),[reuse.into(),id.into(),actor.into()])).await.unwrap();
    for clip in [id, reuse] {
        db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,format!("INSERT INTO course_speech_clip_events({column}clip_id,version,status,result) VALUES({value}$1,1,'ready','{{}}')"),[clip.into()])).await.unwrap();
    }
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,format!("INSERT INTO course_speech_clip_reviews({column}clip_id,accepted,actor_id,reason) VALUES({value}$1,false,$2,'Layout speech fixture')"),[id.into(),actor.into()])).await.unwrap();
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,format!("INSERT INTO speech_alignments({column}id,plan_id,request,report,report_hash,actor_id,reason) VALUES({value}$1,$1,'{{}}','{{}}',repeat('f',64),$2,'Layout speech fixture')"),[id.into(),actor.into()])).await.unwrap();
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,format!("INSERT INTO speech_alignment_reviews({column}alignment_id,clip_id,accepted,words,request,actor_id,reason) VALUES({value}$1,$1,false,'[]','{{}}',$2,'Layout speech fixture')"),[id.into(),actor.into()])).await.unwrap();
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,format!("INSERT INTO speech_package_imports({column}id,alignment_id,actor_id,request,result,manifest,reason,lesson_id,revision) VALUES({value}$1,$1,$2,'{{}}','{{}}','{{}}','Layout speech fixture',$3,1)"),[id.into(),actor.into(),lesson.into()])).await.unwrap();
}
pub async fn seed(db: &DatabaseConnection, actor: i64) {
    graph(
        db,
        None,
        "layout-speech-lesson",
        &"7".repeat(32),
        &"6".repeat(32),
        actor,
    )
    .await;
}
pub async fn seed_foreign(db: &DatabaseConnection, actor: i64) {
    graph(
        db,
        Some("hargow"),
        "layout-h-speech-lesson",
        &"4".repeat(32),
        &"5".repeat(32),
        actor,
    )
    .await;
}
pub async fn snapshot(db: &DatabaseConnection) -> Vec<(i64, String)> {
    let mut out = Vec::new();
    for table in TABLES {
        let row=db.query_one_raw(Statement::from_string(DbBackend::Postgres,format!("SELECT count(*)::bigint AS n,md5(coalesce(jsonb_agg(to_jsonb(t)-'product_id' ORDER BY to_jsonb(t)-'product_id')::text,'')) AS hash FROM {table} t"))).await.unwrap().unwrap();
        out.push((
            row.try_get("", "n").unwrap(),
            row.try_get("", "hash").unwrap(),
        ));
    }
    out
}
pub async fn verify(db: &DatabaseConnection, actor: i64) {
    for table in TABLES {
        let row = db
            .query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                format!("SELECT count(*)::bigint AS n FROM {table} WHERE product_id<>'brioche'"),
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.try_get::<i64>("", "n").unwrap(), 0);
        let tx = db.begin().await.unwrap();
        let error = tx
            .execute_unprepared(&format!("UPDATE {table} SET product_id='hargow'"))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("product"), "{error}");
        tx.rollback().await.unwrap();
    }
    let tx = db.begin().await.unwrap();
    graph(
        &tx,
        Some("hargow"),
        "layout-h-speech-lesson",
        &"4".repeat(32),
        &"5".repeat(32),
        actor,
    )
    .await;
    for table in TABLES {
        let row = tx
            .query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                format!("SELECT count(*)::bigint AS n FROM {table} WHERE product_id='hargow'"),
            ))
            .await
            .unwrap()
            .unwrap();
        let expected = if ["course_speech_clips", "course_speech_clip_events"].contains(&table) {
            2
        } else {
            1
        };
        assert_eq!(row.try_get::<i64>("", "n").unwrap(), expected);
    }
    tx.execute_unprepared("INSERT INTO lesson_revisions(product_id,lesson_id,revision,published,public_document,server_document) SELECT product_id,lesson_id,2,false,public_document,server_document FROM lesson_revisions WHERE lesson_id IN ('layout-speech-lesson','layout-h-speech-lesson') AND revision=1").await.unwrap();
    // Existing global parents are present: these fail on the new product edges, not old missing-ID checks.
    for (sql, constraint) in [
        (
            "INSERT INTO course_speech_plans(product_id,id,lesson_id,lesson_revision,request,plan,summary,actor_id,reason) SELECT 'hargow',repeat('9',32),lesson_id,lesson_revision,request,plan,summary,actor_id,reason FROM course_speech_plans WHERE id=repeat('7',32)",
            "chef_speech_plan_product_lesson",
        ),
        (
            "INSERT INTO course_speech_plans(product_id,id,lesson_id,lesson_revision,request,plan,summary,actor_id,reason) SELECT 'brioche',repeat('9',32),lesson_id,lesson_revision,request,plan,summary,actor_id,reason FROM course_speech_plans WHERE id=repeat('4',32)",
            "chef_speech_plan_product_lesson",
        ),
        (
            "INSERT INTO course_speech_clips(product_id,id,plan_id,generation_key,request,actor_id,reason) SELECT 'hargow',repeat('9',32),plan_id,generation_key,request,actor_id,reason FROM course_speech_clips WHERE id=repeat('7',32)",
            "chef_speech_clip_product_plan",
        ),
        (
            "INSERT INTO course_speech_clips(product_id,id,plan_id,generation_key,request,actor_id,reason,reused_from) SELECT 'brioche',repeat('9',32),plan_id,generation_key,request,actor_id,reason,repeat('4',32) FROM course_speech_clips WHERE id=repeat('7',32)",
            "chef_speech_clip_product_reuse",
        ),
        (
            "INSERT INTO course_speech_clips(product_id,id,plan_id,generation_key,request,actor_id,reason,reused_from) SELECT 'hargow',repeat('9',32),plan_id,generation_key,request,actor_id,reason,repeat('7',32) FROM course_speech_clips WHERE id=repeat('4',32)",
            "chef_speech_clip_product_reuse",
        ),
        (
            "INSERT INTO course_speech_clip_events(product_id,clip_id,version,status,result) VALUES('brioche',repeat('4',32),2,'ready','{}')",
            "chef_speech_event_product_clip",
        ),
        (
            "INSERT INTO course_speech_clip_events(product_id,clip_id,version,status,result) VALUES('hargow',repeat('7',32),2,'ready','{}')",
            "chef_speech_event_product_clip",
        ),
        (
            "INSERT INTO course_speech_clip_reviews(product_id,clip_id,accepted,actor_id,reason) SELECT 'hargow',repeat('6',32),false,actor_id,reason FROM course_speech_plans WHERE id=repeat('7',32)",
            "chef_speech_review_product_clip",
        ),
        (
            "INSERT INTO speech_alignments(product_id,id,plan_id,request,report,report_hash,actor_id,reason) SELECT 'hargow',repeat('9',32),plan_id,request,report,report_hash,actor_id,reason FROM speech_alignments WHERE id=repeat('7',32)",
            "chef_alignment_product_plan",
        ),
        (
            "INSERT INTO speech_alignment_reviews(product_id,alignment_id,clip_id,accepted,words,request,actor_id,reason) SELECT 'hargow',repeat('7',32),repeat('5',32),false,'[]',request,actor_id,reason FROM speech_alignments WHERE id=repeat('4',32)",
            "chef_alignment_review_product_alignment",
        ),
        (
            "INSERT INTO speech_alignment_reviews(product_id,alignment_id,clip_id,accepted,words,request,actor_id,reason) SELECT 'hargow',repeat('4',32),repeat('7',32),false,'[]',request,actor_id,reason FROM speech_alignments WHERE id=repeat('4',32)",
            "chef_alignment_review_product_clip",
        ),
        (
            "INSERT INTO speech_package_imports(product_id,id,alignment_id,actor_id,request,result,manifest,reason,lesson_id,revision) SELECT 'hargow',repeat('9',32),repeat('7',32),actor_id,request,result,manifest,reason,'layout-h-speech-lesson',2 FROM speech_package_imports WHERE id=repeat('4',32)",
            "chef_package_product_alignment",
        ),
        (
            "INSERT INTO speech_package_imports(product_id,id,alignment_id,actor_id,request,result,manifest,reason,lesson_id,revision) SELECT 'hargow',repeat('9',32),alignment_id,actor_id,request,result,manifest,reason,'layout-speech-lesson',2 FROM speech_package_imports WHERE id=repeat('4',32)",
            "chef_package_product_lesson",
        ),
        (
            "INSERT INTO course_speech_plans(product_id,id,lesson_id,lesson_revision,request,plan,summary,actor_id,reason) SELECT 'unknown',repeat('9',32),lesson_id,lesson_revision,request,plan,summary,actor_id,reason FROM course_speech_plans WHERE id=repeat('7',32)",
            "product_id_check",
        ),
    ] {
        tx.execute_unprepared("SAVEPOINT rejected_edge")
            .await
            .unwrap();
        let error = tx.execute_unprepared(sql).await.unwrap_err();
        assert!(
            error.to_string().contains(constraint),
            "{constraint}: {error}"
        );
        tx.execute_unprepared("ROLLBACK TO SAVEPOINT rejected_edge")
            .await
            .unwrap();
    }
    tx.rollback().await.unwrap();
}

pub async fn verify_local_keys(db: &DatabaseConnection, actor: i64) {
    let tx = db.begin().await.unwrap();
    for product in ["brioche", "hargow"] {
        graph(
            &tx,
            Some(product),
            "local-speech-lesson",
            &"1".repeat(32),
            &"2".repeat(32),
            actor,
        )
        .await;
    }
    for (table, key) in [
        ("course_speech_plans", "id"),
        ("course_speech_clips", "id"),
        ("course_speech_clip_events", "clip_id"),
        ("course_speech_clip_reviews", "clip_id"),
        ("speech_alignments", "id"),
        ("speech_alignment_reviews", "alignment_id"),
        ("speech_package_imports", "id"),
    ] {
        let row = tx
            .query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                format!("SELECT count(*)::bigint AS n FROM {table} WHERE {key}=repeat('1',32)"),
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.try_get::<i64>("", "n").unwrap(), 2, "{table}");
        tx.execute_unprepared("SAVEPOINT local_speech_duplicate")
            .await
            .unwrap();
        let error=tx.execute_unprepared(&format!("INSERT INTO {table} SELECT * FROM {table} WHERE product_id='hargow' AND {key}=repeat('1',32)")).await.unwrap_err();
        assert!(
            error.to_string().contains("duplicate key"),
            "{table}: {error}"
        );
        tx.execute_unprepared("ROLLBACK TO SAVEPOINT local_speech_duplicate")
            .await
            .unwrap();
    }
    let error=tx.execute_unprepared("INSERT INTO speech_package_imports(product_id,id,alignment_id,actor_id,request,result,manifest,reason,lesson_id,revision) SELECT product_id,repeat('3',32),alignment_id,actor_id,request,result,manifest,reason,lesson_id,revision FROM speech_package_imports WHERE product_id='brioche' AND id=repeat('1',32)").await.unwrap_err();
    assert!(
        error.to_string().contains("chef_local_package_lesson"),
        "{error}"
    );
    tx.rollback().await.unwrap();
}
