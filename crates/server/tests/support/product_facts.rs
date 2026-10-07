//! Synthetic non-authenticating owner and nine populated facts for migration preservation.
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement, TransactionTrait};
const TABLES: [&str; 9] = [
    "learning_sessions",
    "lesson_progress",
    "step_progress",
    "exercise_hints",
    "exercise_attempts",
    "learning_operations",
    "review_cards",
    "review_attempts",
    "saved_items",
];
pub async fn seed(db: &DatabaseConnection, lesson: &str, revision: i32) {
    let row = db.query_one_raw(Statement::from_string(DbBackend::Postgres,"INSERT INTO users(email,password_hash,display_name) VALUES('facts@example.test','not-a-password-hash','Synthetic facts') RETURNING id")).await.unwrap().unwrap();
    let user = row.try_get::<i64>("", "id").unwrap();
    for sql in [
        "INSERT INTO learning_sessions(id,user_id,lesson_id,revision,schema_version,completed_at) VALUES('fact-session',$1,$2,$3,'1.0',CURRENT_TIMESTAMP)",
        "INSERT INTO lesson_progress(user_id,lesson_id,last_session_id,first_completed_at,latest_completed_revision) VALUES($1,$2,'fact-session',CURRENT_TIMESTAMP,$3)",
        "INSERT INTO review_cards(id,user_id,knowledge_id,source_lesson_id,source_revision,snapshot) VALUES('fact-card',$1,'fact-knowledge',$2,$3,'{}')",
        "INSERT INTO saved_items(id,user_id,knowledge_id,source_lesson_id,source_revision,snapshot) VALUES('fact-saved',$1,'fact-knowledge',$2,$3,'{}')",
    ] {
        db.execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            [user.into(), lesson.into(), revision.into()],
        ))
        .await
        .unwrap();
    }
    db.execute_unprepared("INSERT INTO step_progress(session_id,step_id) VALUES('fact-session','read'); INSERT INTO exercise_hints(session_id,exercise_id) VALUES('fact-session','exercise');").await.unwrap();
    for sql in [
        "INSERT INTO exercise_attempts(id,session_id,user_id,exercise_id,attempt_index,answer,result,hint_used) VALUES('fact-attempt','fact-session',$1,'exercise',1,'{}','{}',true)",
        "INSERT INTO learning_operations(user_id,scope,idempotency_key,request_hash,result) VALUES($1,'start','original-idempotency','original-hash','{}')",
        "INSERT INTO review_attempts(id,card_id,user_id,rating,old_stage,new_stage,old_version,new_version,due_at,reviewed_at,time_zone) VALUES('fact-review','fact-card',$1,'again',-1,0,1,2,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP,'Asia/Shanghai')",
    ] {
        db.execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            [user.into()],
        ))
        .await
        .unwrap();
    }
}
pub async fn snapshot(db: &DatabaseConnection) -> Vec<(i64, String)> {
    let mut values = Vec::new();
    for table in TABLES {
        let row=db.query_one_raw(Statement::from_string(DbBackend::Postgres,format!("SELECT count(*)::bigint AS n,md5(coalesce(jsonb_agg(to_jsonb(t)-'product_id' ORDER BY (to_jsonb(t)-'product_id')::text)::text,'')) AS hash FROM {table} t"))).await.unwrap().unwrap();
        values.push((
            row.try_get("", "n").unwrap(),
            row.try_get("", "hash").unwrap(),
        ));
    }
    values
}
pub async fn verify(db: &DatabaseConnection) {
    for table in TABLES {
        let row = db
            .query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                format!("SELECT count(*)::bigint AS n FROM {table} WHERE product_id <> 'brioche'"),
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.try_get::<i64>("", "n").unwrap(), 0);
        assert!(
            db.execute_unprepared(&format!("UPDATE {table} SET product_id='hargow'"))
                .await
                .is_err(),
            "{table} must not be reassigned"
        );
        assert!(
            db.execute_unprepared(&format!("UPDATE {table} SET product_id='unknown'"))
                .await
                .is_err()
        );
    }
    for sql in [
        "INSERT INTO step_progress(product_id,session_id,step_id) VALUES('hargow','fact-session','cross-step')",
        "INSERT INTO exercise_hints(product_id,session_id,exercise_id) VALUES('hargow','fact-session','cross-hint')",
        "INSERT INTO exercise_attempts(product_id,id,session_id,user_id,exercise_id,attempt_index,answer,result,hint_used) SELECT 'hargow','cross-attempt',id,user_id,'cross',1,'{}','{}',false FROM learning_sessions WHERE id='fact-session'",
        "INSERT INTO review_attempts(product_id,id,card_id,user_id,rating,old_stage,new_stage,old_version,new_version,due_at,reviewed_at,time_zone) SELECT 'hargow','cross-review',id,user_id,'again',-1,0,2,3,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP,'Asia/Shanghai' FROM review_cards WHERE id='fact-card'",
    ] {
        assert!(db.execute_unprepared(sql).await.is_err());
    }
    let tx = db.begin().await.unwrap();
    tx.execute_unprepared("DELETE FROM lesson_progress WHERE last_session_id='fact-session'")
        .await
        .unwrap();
    let error=tx.execute_unprepared("INSERT INTO lesson_progress(product_id,user_id,lesson_id,last_session_id) SELECT 'hargow',user_id,lesson_id,id FROM learning_sessions WHERE id='fact-session'").await.unwrap_err();
    assert!(error.to_string().contains("chef_progress_product_session"));
    tx.rollback().await.unwrap();
    // Positive same-product references without persisting Hargow facts while runtime is still blocked.
    let tx = db.begin().await.unwrap();
    tx.execute_unprepared("INSERT INTO learning_sessions(product_id,id,user_id,lesson_id,revision,schema_version,completed_at) SELECT 'hargow','hargow-fact-session',user_id,lesson_id,revision,schema_version,CURRENT_TIMESTAMP FROM learning_sessions WHERE id='fact-session'; INSERT INTO step_progress(product_id,session_id,step_id) VALUES('hargow','hargow-fact-session','read'); INSERT INTO exercise_hints(product_id,session_id,exercise_id) VALUES('hargow','hargow-fact-session','exercise'); INSERT INTO exercise_attempts(product_id,id,session_id,user_id,exercise_id,attempt_index,answer,result,hint_used) SELECT 'hargow','hargow-fact-attempt',id,user_id,'exercise',1,'{}','{}',false FROM learning_sessions WHERE id='hargow-fact-session'").await.unwrap();
    // Product progress keys allow both facts to coexist.
    tx.execute_unprepared("INSERT INTO lesson_progress(product_id,user_id,lesson_id,last_session_id) SELECT 'hargow',user_id,lesson_id,id FROM learning_sessions WHERE id='hargow-fact-session'; INSERT INTO review_cards(product_id,id,user_id,knowledge_id,source_lesson_id,source_revision,snapshot) SELECT 'hargow','hargow-fact-card',user_id,'hargow-knowledge',source_lesson_id,source_revision,snapshot FROM review_cards WHERE id='fact-card'; INSERT INTO saved_items(product_id,id,user_id,knowledge_id,source_lesson_id,source_revision,snapshot) SELECT 'hargow','hargow-fact-saved',user_id,'hargow-knowledge',source_lesson_id,source_revision,snapshot FROM saved_items WHERE id='fact-saved'; INSERT INTO learning_operations(product_id,user_id,scope,idempotency_key,request_hash,result) SELECT 'hargow',user_id,'hargow-start','hargow-key',request_hash,result FROM learning_operations WHERE idempotency_key='original-idempotency'; INSERT INTO review_attempts(product_id,id,card_id,user_id,rating,old_stage,new_stage,old_version,new_version,due_at,reviewed_at,time_zone) SELECT 'hargow','hargow-fact-review',id,user_id,'again',-1,0,1,2,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP,'Asia/Shanghai' FROM review_cards WHERE id='hargow-fact-card'").await.unwrap();
    for table in TABLES {
        let row = tx
            .query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                format!("SELECT count(*)::bigint AS n FROM {table} WHERE product_id='hargow'"),
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.try_get::<i64>("", "n").unwrap(), 1, "{table}");
    }
    tx.rollback().await.unwrap();
}
