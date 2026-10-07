//! Only isolated test schemas: deliberately publishes unreviewed synthetic fixtures for protocol tests.
use chef_engine::content::{ReleaseLevel, ReleaseManifest, ReleaseUnit, RevisionRef};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement, TransactionTrait};
use std::collections::BTreeMap;
pub async fn fixture_release(db: &DatabaseConnection) {
    let tx = db.begin().await.unwrap();
    let rows=tx.query_all_raw(Statement::from_string(DbBackend::Postgres,"SELECT DISTINCT ON(lesson_id) lesson_id,revision,public_document FROM lesson_revisions WHERE published ORDER BY lesson_id,revision DESC")).await.unwrap();
    let mut levels: BTreeMap<String, BTreeMap<String, Vec<RevisionRef>>> = BTreeMap::new();
    for row in rows {
        let document: serde_json::Value = row.try_get("", "public_document").unwrap();
        levels
            .entry(document["levelId"].as_str().unwrap().into())
            .or_default()
            .entry(document["unitId"].as_str().unwrap().into())
            .or_default()
            .push(RevisionRef {
                lesson_id: row.try_get("", "lesson_id").unwrap(),
                revision: row.try_get::<i32>("", "revision").unwrap() as u32,
            });
    }
    let manifest = ReleaseManifest {
        id: format!(
            "fixture-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ),
        schema_version: "1.0".into(),
        levels: levels
            .into_iter()
            .map(|(id, units)| ReleaseLevel {
                label: id.to_uppercase(),
                id,
                units: units
                    .into_iter()
                    .map(|(id, lessons)| ReleaseUnit {
                        title_zh: id.clone(),
                        id,
                        lessons,
                    })
                    .collect(),
            })
            .collect(),
    };
    tx.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO content_releases(id,manifest,content_hash) VALUES($1,$2,$3)",
        [
            manifest.id.clone().into(),
            serde_json::to_value(&manifest).unwrap().into(),
            "0".repeat(64).into(),
        ],
    ))
    .await
    .unwrap();
    let mut position = 0i32;
    for level in &manifest.levels {
        for unit in &level.units {
            for entry in &unit.lessons {
                tx.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO release_entries(release_id,lesson_id,revision,position) VALUES($1,$2,$3,$4)",[manifest.id.clone().into(),entry.lesson_id.clone().into(),(entry.revision as i32).into(),position.into()])).await.unwrap();
                position += 1;
            }
        }
    }
    tx.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "UPDATE content_state SET active_release=$1 WHERE singleton",
        [manifest.id.into()],
    ))
    .await
    .unwrap();
    tx.commit().await.unwrap();
}
