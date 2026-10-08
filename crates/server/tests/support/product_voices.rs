use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement, TransactionTrait};
const TABLES: [&str; 4] = [
    "character_voice_profiles",
    "voice_reference_grants",
    "voice_reference_revocations",
    "voice_reference_reads",
];
pub async fn seed(db: &DatabaseConnection, actor: i64) {
    db.execute_unprepared("INSERT INTO character_revisions(character_id,revision,snapshot,avatar_id,avatar_revision) SELECT 'layout-voice-character',1,jsonb_set(snapshot,'{characterId}','\"layout-voice-character\"'),avatar_id,avatar_revision FROM character_revisions ORDER BY character_id,revision LIMIT 1").await.unwrap();
    let row=db.query_one_raw(Statement::from_string(DbBackend::Postgres,"SELECT character_id,revision FROM character_revisions WHERE character_id='layout-voice-character' AND revision=1")).await.unwrap().unwrap();
    let id: String = row.try_get("", "character_id").unwrap();
    let revision: i32 = row.try_get("", "revision").unwrap();
    let profile = serde_json::json!({"personality":"Warm and patient.","speakingStyle":"Natural conversation.","defaultEmotion":"Friendly.","provider":"qwen","model":"qwen-audio-3.1-tts-flash","voiceId":"test-voice","voiceKind":"system","locale":"fr-FR","rate":1.0,"referenceAudio":{"assetId":"layout-recording-fixture","revision":1,"transcript":"Bonjour.","cloningPermission":"Synthetic fixture only"}});
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO character_voice_profiles(character_id,character_revision,revision,profile,actor_id,reason) VALUES($1,$2,1,$3,$4,'Layout fixture')",[id.clone().into(),revision.into(),profile.into(),actor.into()])).await.unwrap();
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO voice_reference_grants(id,token_hash,character_id,character_revision,voice_revision,asset_id,asset_revision,descriptor,reference,actor_id,reason,model,single_speaker_confirmed,expires_at) SELECT repeat('c',32),repeat('c',64),$1,$2,1,asset_id,revision,descriptor,(SELECT profile->'referenceAudio' FROM character_voice_profiles WHERE character_id=$1 AND character_revision=$2 AND revision=1),$3,'Layout fixture','qwen-audio-3.1-tts-flash',true,CURRENT_TIMESTAMP+interval '5 minutes' FROM audio_assets WHERE asset_id='layout-recording-fixture' AND revision=1",[id.into(),revision.into(),actor.into()])).await.unwrap();
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO voice_reference_revocations(grant_id,actor_id,reason) VALUES(repeat('c',32),$1,'Layout fixture')",[actor.into()])).await.unwrap();
    db.execute_unprepared("INSERT INTO voice_reference_reads(grant_id) VALUES(repeat('c',32))")
        .await
        .unwrap();
}
pub async fn snapshot(db: &DatabaseConnection) -> Vec<(i64, String)> {
    let mut result = Vec::new();
    for table in TABLES {
        let row=db.query_one_raw(Statement::from_string(DbBackend::Postgres,format!("SELECT count(*)::bigint AS n,md5(coalesce(jsonb_agg(to_jsonb(t)-'product_id'-'reference_asset_id'-'reference_asset_revision' ORDER BY to_jsonb(t)-'product_id'-'reference_asset_id'-'reference_asset_revision')::text,'')) AS hash FROM {table} t"))).await.unwrap().unwrap();
        result.push((
            row.try_get("", "n").unwrap(),
            row.try_get("", "hash").unwrap(),
        ));
    }
    result
}
async fn rejects(db: &DatabaseConnection, sql: &str, constraint: &str) {
    let tx = db.begin().await.unwrap();
    let error = tx.execute_unprepared(sql).await.unwrap_err();
    assert!(error.to_string().contains(constraint), "{error}");
    tx.rollback().await.unwrap();
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
        rejects(
            db,
            &format!("UPDATE {table} SET product_id='hargow'"),
            "product",
        )
        .await;
    }
    rejects(db,"INSERT INTO character_voice_profiles(product_id,character_id,character_revision,revision,profile,actor_id,reason) SELECT 'hargow',character_id,character_revision,2,profile,actor_id,reason FROM character_voice_profiles LIMIT 1","chef_voice_product_character").await;
    rejects(
        db,
        "INSERT INTO voice_reference_reads(product_id,grant_id) VALUES('hargow',repeat('c',32))",
        "chef_reference_read_product_grant",
    )
    .await;
    let tx = db.begin().await.unwrap();
    tx.execute_unprepared("INSERT INTO voice_reference_grants(product_id,id,token_hash,character_id,character_revision,voice_revision,asset_id,asset_revision,descriptor,reference,actor_id,reason,model,single_speaker_confirmed,expires_at) SELECT product_id,repeat('b',32),repeat('b',64),character_id,character_revision,voice_revision,asset_id,asset_revision,descriptor,reference,actor_id,reason,model,single_speaker_confirmed,expires_at FROM voice_reference_grants LIMIT 1").await.unwrap();
    let error=tx.execute_unprepared("INSERT INTO voice_reference_revocations(product_id,grant_id,actor_id,reason) SELECT 'hargow',repeat('b',32),actor_id,reason FROM voice_reference_grants LIMIT 1").await.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("chef_reference_revoke_product_grant"),
        "{error}"
    );
    tx.rollback().await.unwrap();
    // Real database edges, synthetic data only; rollback all H registrations.
    let tx = db.begin().await.unwrap();
    tx.execute_unprepared("INSERT INTO media_assets(product_id,asset_id,revision,descriptor,provenance,sha256,extension,byte_size) SELECT 'hargow','voice-h-avatar',1,descriptor,provenance,sha256,extension,byte_size FROM media_assets LIMIT 1").await.unwrap();
    tx.execute_unprepared("INSERT INTO character_revisions(product_id,character_id,revision,snapshot,avatar_id,avatar_revision) SELECT 'hargow','voice-h-character',1,snapshot,'voice-h-avatar',1 FROM character_revisions LIMIT 1").await.unwrap();
    tx.execute_unprepared("INSERT INTO audio_assets(product_id,asset_id,revision,descriptor,provenance,sha256,extension,byte_size,duration_ms,sample_rate,channels) SELECT 'hargow','voice-h-audio',1,descriptor,provenance,sha256,extension,byte_size,duration_ms,sample_rate,channels FROM audio_assets LIMIT 1").await.unwrap();
    tx.execute_unprepared("INSERT INTO character_voice_profiles(product_id,character_id,character_revision,revision,profile,actor_id,reason) SELECT 'hargow','voice-h-character',1,1,jsonb_set(profile,'{referenceAudio,assetId}','\"voice-h-audio\"'),actor_id,reason FROM character_voice_profiles LIMIT 1").await.unwrap();
    tx.execute_unprepared("INSERT INTO voice_reference_grants(product_id,id,token_hash,character_id,character_revision,voice_revision,asset_id,asset_revision,descriptor,reference,actor_id,reason,model,single_speaker_confirmed,expires_at) SELECT 'hargow',repeat('d',32),repeat('d',64),'voice-h-character',1,1,'voice-h-audio',1,descriptor,reference,actor_id,reason,model,single_speaker_confirmed,CURRENT_TIMESTAMP+interval '5 minutes' FROM voice_reference_grants LIMIT 1").await.unwrap();
    tx.execute_unprepared("INSERT INTO voice_reference_revocations(product_id,grant_id,actor_id,reason) SELECT 'hargow',repeat('d',32),actor_id,reason FROM voice_reference_revocations LIMIT 1").await.unwrap();
    tx.execute_unprepared(
        "INSERT INTO voice_reference_reads(product_id,grant_id) VALUES('hargow',repeat('d',32))",
    )
    .await
    .unwrap();
    for table in TABLES {
        let row = tx
            .query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                format!("SELECT count(*)::bigint AS n FROM {table} WHERE product_id='hargow'"),
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.try_get::<i64>("", "n").unwrap(), 1);
    }
    for (sql, constraint) in [
        (
            "INSERT INTO character_voice_profiles(product_id,character_id,character_revision,revision,profile,actor_id,reason) SELECT 'brioche',character_id,character_revision,2,profile,actor_id,reason FROM character_voice_profiles WHERE product_id='hargow'",
            "chef_voice_product_character",
        ),
        (
            "INSERT INTO character_voice_profiles(product_id,character_id,character_revision,revision,profile,actor_id,reason) SELECT 'brioche',character_id,character_revision,2,jsonb_set(profile,'{referenceAudio,assetId}','\"voice-h-audio\"'),actor_id,reason FROM character_voice_profiles WHERE product_id='brioche'",
            "chef_voice_product_reference",
        ),
        (
            "INSERT INTO voice_reference_grants(product_id,id,token_hash,character_id,character_revision,voice_revision,asset_id,asset_revision,descriptor,reference,actor_id,reason,model,single_speaker_confirmed,expires_at) SELECT 'hargow',repeat('e',32),repeat('e',64),character_id,character_revision,voice_revision,'layout-recording-fixture',1,descriptor,reference,actor_id,reason,model,single_speaker_confirmed,CURRENT_TIMESTAMP+interval '5 minutes' FROM voice_reference_grants WHERE product_id='hargow'",
            "chef_reference_grant_product_audio",
        ),
        (
            "INSERT INTO voice_reference_grants(product_id,id,token_hash,character_id,character_revision,voice_revision,asset_id,asset_revision,descriptor,reference,actor_id,reason,model,single_speaker_confirmed,expires_at) SELECT 'brioche',repeat('e',32),repeat('e',64),character_id,character_revision,voice_revision,'layout-recording-fixture',1,descriptor,reference,actor_id,reason,model,single_speaker_confirmed,CURRENT_TIMESTAMP+interval '5 minutes' FROM voice_reference_grants WHERE product_id='hargow'",
            "chef_reference_grant_product_voice",
        ),
        (
            "INSERT INTO voice_reference_reads(product_id,grant_id) VALUES('brioche',repeat('d',32))",
            "chef_reference_read_product_grant",
        ),
        (
            "INSERT INTO character_voice_profiles(product_id,character_id,character_revision,revision,profile,actor_id,reason) SELECT 'brioche',character_id,character_revision,2,profile #- '{referenceAudio,revision}',actor_id,reason FROM character_voice_profiles WHERE product_id='brioche'",
            "chef_voice_reference_pair",
        ),
        (
            "INSERT INTO character_voice_profiles(product_id,character_id,character_revision,revision,profile,actor_id,reason) SELECT 'unknown',character_id,character_revision,2,profile,actor_id,reason FROM character_voice_profiles WHERE product_id='brioche'",
            "product_id_check",
        ),
    ] {
        tx.execute_unprepared("SAVEPOINT voice_edge").await.unwrap();
        let error = tx.execute_unprepared(sql).await.unwrap_err();
        assert!(
            error.to_string().contains(constraint),
            "{constraint}: {error}"
        );
        tx.execute_unprepared("ROLLBACK TO SAVEPOINT voice_edge")
            .await
            .unwrap();
    }
    // Optional reference is valid for a system voice; no MATCH FULL with product_id.
    tx.execute_unprepared("INSERT INTO character_voice_profiles(product_id,character_id,character_revision,revision,profile,actor_id,reason) SELECT product_id,character_id,character_revision,3,jsonb_set(profile,'{referenceAudio}','null'),actor_id,reason FROM character_voice_profiles WHERE product_id='hargow'").await.unwrap();
    tx.rollback().await.unwrap();
}
