use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement, TransactionTrait};
const TABLES: [&str; 5] = [
    "voice_clone_jobs",
    "voice_clone_events",
    "voice_auditions",
    "voice_audition_events",
    "voice_audition_reviews",
];
pub async fn seed(db: &DatabaseConnection) {
    db.execute_unprepared("INSERT INTO voice_clone_jobs(id,grant_id,prefix,actor_id,reason) SELECT repeat('f',32),id,'layout01',actor_id,'Layout fixture' FROM voice_reference_grants WHERE id=repeat('c',32)").await.unwrap();
    db.execute_unprepared("INSERT INTO voice_clone_events(job_id,version,status,voice_id,actor_id,reason) SELECT id,1,'ready','fixture-voice',actor_id,'Layout fixture' FROM voice_clone_jobs WHERE id=repeat('f',32)").await.unwrap();
    db.execute_unprepared("INSERT INTO voice_auditions(id,clone_job_id,clone_version,profile,parameters,actor_id,reason) SELECT repeat('e',32),repeat('f',32),1,profile,'{\"input\":{\"text\":\"Bonjour.\"},\"sceneEmotion\":\"Friendly.\"}',actor_id,'Layout fixture' FROM character_voice_profiles WHERE character_id='layout-voice-character' AND character_revision=1 AND revision=1").await.unwrap();
    db.execute_unprepared("INSERT INTO voice_auditions(id,character_id,character_revision,base_voice_revision,profile,parameters,actor_id,reason) SELECT repeat('d',32),character_id,character_revision,0,jsonb_set(profile,'{referenceAudio}','null'),'{\"input\":{\"text\":\"Bonjour.\"},\"sceneEmotion\":\"Friendly.\"}',actor_id,'Layout fixture' FROM character_voice_profiles WHERE character_id='layout-voice-character' AND character_revision=1 AND revision=1").await.unwrap();
    db.execute_unprepared("INSERT INTO voice_audition_events(audition_id,version,status) SELECT id,1,'submitted' FROM voice_auditions WHERE id IN (repeat('e',32),repeat('d',32))").await.unwrap();
    db.execute_unprepared("INSERT INTO voice_audition_reviews(audition_id,accepted,character_id,character_revision,voice_revision,actor_id,reason) SELECT repeat('e',32),false,character_id,character_revision,NULL,actor_id,'Layout fixture' FROM character_voice_profiles WHERE character_id='layout-voice-character' AND character_revision=1 AND revision=1").await.unwrap();
}
pub async fn snapshot(db: &DatabaseConnection) -> Vec<(i64, String)> {
    let mut out = Vec::new();
    for table in TABLES {
        let row=db.query_one_raw(Statement::from_string(DbBackend::Postgres,format!("SELECT count(*)::bigint AS n,md5(coalesce(jsonb_agg(to_jsonb(t)-ARRAY['product_id','base_profile_revision','reference_asset_id','reference_asset_revision'] ORDER BY to_jsonb(t)-ARRAY['product_id','base_profile_revision','reference_asset_id','reference_asset_revision'])::text,'')) AS hash FROM {table} t"))).await.unwrap().unwrap();
        out.push((
            row.try_get("", "n").unwrap(),
            row.try_get("", "hash").unwrap(),
        ));
    }
    out
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
    crate::product_voices::seed_hargow(&tx).await;
    tx.execute_unprepared("INSERT INTO voice_clone_jobs(product_id,id,grant_id,prefix,actor_id,reason) SELECT 'hargow',repeat('1',32),id,'hfixture',actor_id,reason FROM voice_reference_grants WHERE product_id='hargow'").await.unwrap();
    tx.execute_unprepared("INSERT INTO voice_clone_events(product_id,job_id,version,status,voice_id,actor_id,reason) SELECT 'hargow',id,1,'ready','fixture-voice',actor_id,reason FROM voice_clone_jobs WHERE product_id='hargow'").await.unwrap();
    tx.execute_unprepared("INSERT INTO voice_auditions(product_id,id,clone_job_id,clone_version,profile,parameters,actor_id,reason) SELECT 'hargow',repeat('2',32),repeat('1',32),1,profile,'{\"input\":{\"text\":\"Bonjour.\"},\"sceneEmotion\":\"Friendly.\"}',actor_id,reason FROM character_voice_profiles WHERE product_id='hargow'").await.unwrap();
    tx.execute_unprepared("INSERT INTO voice_auditions(product_id,id,character_id,character_revision,base_voice_revision,profile,parameters,actor_id,reason) SELECT 'hargow',repeat('3',32),character_id,character_revision,0,jsonb_set(profile,'{referenceAudio}','null'),'{\"input\":{\"text\":\"Bonjour.\"},\"sceneEmotion\":\"Friendly.\"}',actor_id,reason FROM character_voice_profiles WHERE product_id='hargow'").await.unwrap();
    tx.execute_unprepared("INSERT INTO voice_audition_events(product_id,audition_id,version,status) SELECT 'hargow',id,1,'submitted' FROM voice_auditions WHERE product_id='hargow'").await.unwrap();
    tx.execute_unprepared("INSERT INTO voice_audition_reviews(product_id,audition_id,accepted,character_id,character_revision,voice_revision,actor_id,reason) SELECT 'hargow',repeat('2',32),true,character_id,character_revision,revision,actor_id,reason FROM character_voice_profiles WHERE product_id='hargow'").await.unwrap();
    for table in TABLES {
        let row = tx
            .query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                format!("SELECT count(*)::bigint AS n FROM {table} WHERE product_id='hargow'"),
            ))
            .await
            .unwrap()
            .unwrap();
        let expected = if ["voice_auditions", "voice_audition_events"].contains(&table) {
            2
        } else {
            1
        };
        assert_eq!(row.try_get::<i64>("", "n").unwrap(), expected);
    }
    tx.execute_unprepared("INSERT INTO voice_reference_grants(product_id,id,token_hash,character_id,character_revision,voice_revision,asset_id,asset_revision,descriptor,reference,actor_id,reason,model,single_speaker_confirmed,expires_at) SELECT product_id,repeat('b',32),repeat('b',64),character_id,character_revision,voice_revision,asset_id,asset_revision,descriptor,reference,actor_id,reason,model,single_speaker_confirmed,expires_at FROM voice_reference_grants WHERE product_id='brioche'").await.unwrap();
    for (sql, constraint) in [
        (
            "INSERT INTO voice_clone_jobs(product_id,id,grant_id,prefix,actor_id,reason) SELECT 'hargow',repeat('9',32),id,'foreign',actor_id,reason FROM voice_reference_grants WHERE id=repeat('b',32)",
            "chef_clone_product_grant",
        ),
        (
            "INSERT INTO voice_clone_events(product_id,job_id,version,status,reason) VALUES('brioche',repeat('1',32),2,'submitted','Foreign job')",
            "chef_clone_event_product_job",
        ),
        (
            "INSERT INTO voice_clone_events(product_id,job_id,version,status,reason) VALUES('hargow',repeat('f',32),2,'submitted','Foreign job')",
            "chef_clone_event_product_job",
        ),
        (
            "INSERT INTO voice_auditions(product_id,id,clone_job_id,clone_version,profile,parameters,actor_id,reason) SELECT 'brioche',repeat('9',32),repeat('1',32),1,profile,'{}',actor_id,reason FROM character_voice_profiles WHERE product_id='brioche'",
            "chef_audition_product_clone",
        ),
        (
            "INSERT INTO voice_auditions(product_id,id,clone_job_id,clone_version,profile,parameters,actor_id,reason) SELECT 'brioche',repeat('9',32),repeat('f',32),99,profile,'{}',actor_id,reason FROM character_voice_profiles WHERE product_id='brioche'",
            "chef_audition_product_clone",
        ),
        (
            "INSERT INTO voice_auditions(product_id,id,character_id,character_revision,base_voice_revision,profile,parameters,actor_id,reason) SELECT 'brioche',repeat('9',32),'voice-h-character',1,0,profile,'{}',actor_id,reason FROM character_voice_profiles WHERE product_id='brioche'",
            "chef_audition_product_character",
        ),
        (
            "INSERT INTO voice_auditions(product_id,id,character_id,character_revision,base_voice_revision,profile,parameters,actor_id,reason) SELECT 'brioche',repeat('9',32),character_id,character_revision,99,profile,'{}',actor_id,reason FROM character_voice_profiles WHERE product_id='brioche'",
            "chef_audition_product_base_voice",
        ),
        (
            "INSERT INTO voice_auditions(product_id,id,character_id,character_revision,base_voice_revision,profile,parameters,actor_id,reason) SELECT 'brioche',repeat('9',32),character_id,character_revision,1,jsonb_set(profile,'{referenceAudio,assetId}','\"voice-h-audio\"'),'{}',actor_id,reason FROM character_voice_profiles WHERE product_id='brioche'",
            "chef_audition_product_reference",
        ),
        (
            "INSERT INTO voice_audition_events(product_id,audition_id,version,status) VALUES('brioche',repeat('2',32),2,'submitted')",
            "chef_audition_event_product_audition",
        ),
        (
            "INSERT INTO voice_audition_reviews(product_id,audition_id,accepted,character_id,character_revision,voice_revision,actor_id,reason) SELECT 'brioche',repeat('3',32),false,character_id,character_revision,NULL,actor_id,reason FROM character_voice_profiles WHERE product_id='brioche'",
            "chef_audition_review_product_audition",
        ),
        (
            "INSERT INTO voice_audition_reviews(product_id,audition_id,accepted,character_id,character_revision,voice_revision,actor_id,reason) SELECT 'hargow',repeat('3',32),false,character_id,character_revision,NULL,actor_id,reason FROM character_voice_profiles WHERE product_id='brioche'",
            "chef_audition_review_product_character",
        ),
    ] {
        tx.execute_unprepared("SAVEPOINT voice_work_edge")
            .await
            .unwrap();
        let error = tx.execute_unprepared(sql).await.unwrap_err();
        assert!(
            error.to_string().contains(constraint),
            "{constraint}: {error}"
        );
        tx.execute_unprepared("ROLLBACK TO SAVEPOINT voice_work_edge")
            .await
            .unwrap();
    }
    // Isolate the voice edge from the independent character edge, only within
    // this owned disposable savepoint; rollback restores the original constraint.
    tx.execute_unprepared("SAVEPOINT review_voice_edge")
        .await
        .unwrap();
    tx.execute_unprepared(
        "ALTER TABLE voice_audition_reviews DROP CONSTRAINT chef_audition_review_product_character",
    )
    .await
    .unwrap();
    let error=tx.execute_unprepared("INSERT INTO voice_audition_reviews(product_id,audition_id,accepted,character_id,character_revision,voice_revision,actor_id,reason) SELECT 'hargow',repeat('3',32),true,character_id,character_revision,revision,actor_id,reason FROM character_voice_profiles WHERE product_id='brioche'").await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("chef_audition_review_product_voice"),
        "{error}"
    );
    tx.execute_unprepared("ROLLBACK TO SAVEPOINT review_voice_edge")
        .await
        .unwrap();
    tx.rollback().await.unwrap();
}

// Disposable graph only: identical local IDs with separate product parents and versioned events.
pub async fn verify_local_keys(db: &DatabaseConnection) {
    let tx = db.begin().await.unwrap();
    super::product_voices::seed_hargow(&tx).await;
    tx.execute_unprepared("INSERT INTO voice_reference_grants(product_id,id,token_hash,character_id,character_revision,voice_revision,asset_id,asset_revision,descriptor,reference,actor_id,reason,model,single_speaker_confirmed,expires_at) SELECT 'brioche',repeat('d',32),md5('local-b-token')||md5('local-b-token'),character_id,character_revision,voice_revision,asset_id,asset_revision,descriptor,reference,actor_id,reason,model,single_speaker_confirmed,CURRENT_TIMESTAMP+interval '5 minutes' FROM voice_reference_grants WHERE product_id='brioche' AND id=repeat('c',32)").await.unwrap();
    tx.execute_unprepared("INSERT INTO voice_clone_jobs(product_id,id,grant_id,prefix,actor_id,reason) SELECT product_id,repeat('1',32),id,CASE product_id WHEN 'brioche' THEN 'localb' ELSE 'localh' END,actor_id,reason FROM voice_reference_grants WHERE id=repeat('d',32); INSERT INTO voice_clone_events(product_id,job_id,version,status,voice_id,actor_id,reason) SELECT product_id,id,1,'ready','synthetic',actor_id,reason FROM voice_clone_jobs WHERE id=repeat('1',32); INSERT INTO voice_reference_revocations(product_id,grant_id,actor_id,reason) SELECT product_id,id,actor_id,reason FROM voice_reference_grants WHERE product_id='brioche' AND id=repeat('d',32); INSERT INTO voice_reference_reads(product_id,grant_id) SELECT product_id,id FROM voice_reference_grants WHERE product_id='brioche' AND id=repeat('d',32)").await.unwrap();
    tx.execute_unprepared("INSERT INTO voice_auditions(product_id,id,clone_job_id,clone_version,profile,parameters,actor_id,reason) SELECT product_id,repeat('2',32),repeat('1',32),1,profile,'{}',actor_id,reason FROM character_voice_profiles WHERE (product_id='brioche' AND character_id='layout-voice-character' OR product_id='hargow' AND character_id='voice-h-character') AND revision=1; INSERT INTO voice_audition_events(product_id,audition_id,version,status) SELECT product_id,id,1,'submitted' FROM voice_auditions WHERE id=repeat('2',32); INSERT INTO voice_audition_reviews(product_id,audition_id,accepted,character_id,character_revision,voice_revision,actor_id,reason) SELECT product_id,repeat('2',32),false,character_id,character_revision,NULL,actor_id,reason FROM character_voice_profiles WHERE (product_id='brioche' AND character_id='layout-voice-character' OR product_id='hargow' AND character_id='voice-h-character') AND revision=1").await.unwrap();
    for (table, key, id) in [
        ("voice_reference_grants", "id", "d"),
        ("voice_reference_revocations", "grant_id", "d"),
        ("voice_clone_jobs", "id", "1"),
        ("voice_clone_events", "job_id", "1"),
        ("voice_auditions", "id", "2"),
        ("voice_audition_events", "audition_id", "2"),
        ("voice_audition_reviews", "audition_id", "2"),
    ] {
        let row = tx
            .query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                format!("SELECT count(*)::bigint AS n FROM {table} WHERE {key}=repeat('{id}',32)"),
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.try_get::<i64>("", "n").unwrap(), 2, "{table}");
        tx.execute_unprepared("SAVEPOINT local_duplicate")
            .await
            .unwrap();
        let row=tx.query_one_raw(Statement::from_string(DbBackend::Postgres,format!("SELECT string_agg(quote_ident(attname),',' ORDER BY attnum) AS columns FROM pg_catalog.pg_attribute WHERE attrelid='{table}'::regclass AND attnum>0 AND NOT attisdropped AND attgenerated=''"))).await.unwrap().unwrap();
        let columns = row.try_get::<String>("", "columns").unwrap();
        let error=tx.execute_unprepared(&format!("INSERT INTO {table}({columns}) SELECT {columns} FROM {table} WHERE product_id='hargow' AND {key}=repeat('{id}',32)")).await.unwrap_err();
        assert!(
            error.to_string().contains("duplicate key"),
            "{table}: {error}"
        );
        tx.execute_unprepared("ROLLBACK TO SAVEPOINT local_duplicate")
            .await
            .unwrap();
    }
    let error=tx.execute_unprepared("INSERT INTO voice_clone_jobs(product_id,id,grant_id,prefix,actor_id,reason) SELECT product_id,repeat('3',32),grant_id,'newlocal',actor_id,reason FROM voice_clone_jobs WHERE product_id='brioche' AND id=repeat('1',32)").await.unwrap_err();
    assert!(
        error.to_string().contains("chef_local_clone_grant"),
        "{error}"
    );
    tx.rollback().await.unwrap();
}
