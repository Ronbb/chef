//! Synthetic protocol data for a named loopback-only disposable database. Never production content.
use anyhow::{Result, ensure};
use axum::{body::Body, http::Request};
use chef_engine::{
    csrf::CsrfPolicy,
    development_source,
    identity::{self, Backend},
    project_source,
};
use http_body_util::BodyExt;
use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};
use sea_orm_migration::MigratorTrait;
use tower::ServiceExt;
#[path = "../tests/support/mod.rs"]
mod support;

#[tokio::main]
async fn main() -> Result<()> {
    let connection = std::env::var("TEST_DATABASE_URL")?;
    let url = url::Url::parse(&connection)?;
    ensure!(
        url.scheme() == "postgres"
            && url.host_str() == Some("127.0.0.1")
            && url.path() == "/brioche_browser_qa"
            && url.query().is_none(),
        "only named loopback disposable database is permitted"
    );
    let db = Database::connect(connection).await?;
    brioche_migration::Migrator::up(&db, None).await?;
    let mut source = development_source()?;
    if let Some(file) = std::env::var_os("BROWSER_QA_RECORDING") {
        use brioche_course_contract::{AudioAsset, AudioCue, AudioTrack, AudioWordRange, Block};
        use chef_engine::{
            audio,
            recording::{AudioBundle, AudioSpec},
        };
        use sha2::{Digest, Sha256};
        let file = std::path::PathBuf::from(file).canonicalize()?;
        let (bytes, info) = audio::inspect_file(&file, "audio/wav")?;
        let sha = format!("{:x}", Sha256::digest(&bytes));
        recording_import(
            &db,
            AudioBundle {
                schema_version: "1.0".into(),
                assets: vec![AudioSpec {
                    asset_id: "audio-browser-qa".into(),
                    revision: 1,
                    sha256: sha.clone(),
                    mime_type: "audio/wav".into(),
                    duration_ms: info.duration_ms,
                    credit_zh: "合成协议测试音，非教学配音".into(),
                    file: file.file_name().unwrap().to_string_lossy().into_owned(),
                    status: "ready".into(),
                    source: "local FFmpeg sine generator".into(),
                    license: "original protocol fixture; no third-party recording".into(),
                    creator: "Brioche browser protocol tests".into(),
                    rights_confirmed: true,
                }],
            },
            file.parent().unwrap(),
        )
        .await?;
        let mut lesson = project_source(source.clone())?;
        lesson.audio = vec![AudioAsset {
            asset_id: "audio-browser-qa".into(),
            revision: 1,
            sha256: sha.clone(),
            mime_type: "audio/wav".into(),
            duration_ms: info.duration_ms,
            credit_zh: "合成协议测试音，非教学配音".into(),
            url: format!("/api/audio/{sha}.wav"),
        }];
        for block in &lesson.blocks {
            let (id, entries) = match block {
                Block::Dialogue { id, turns, .. } => (
                    id,
                    turns
                        .iter()
                        .map(|entry| (&entry.id, &entry.segments))
                        .collect::<Vec<_>>(),
                ),
                Block::Article { id, paragraphs, .. } => (
                    id,
                    paragraphs
                        .iter()
                        .map(|entry| (&entry.id, &entry.segments))
                        .collect::<Vec<_>>(),
                ),
                _ => continue,
            };
            let mut cues: Vec<_> = entries
                .iter()
                .enumerate()
                .map(|(i, (entry, _))| AudioCue {
                    entry_id: (*entry).clone(),
                    segment_id: None,
                    word_range: None,
                    start_ms: i as u32 * 1000,
                    end_ms: (i as u32 + 1) * 1000,
                })
                .collect();
            if let Some((entry, segments)) = entries.first()
                && segments[0].text.starts_with("Bonjour")
            {
                cues.push(AudioCue {
                    entry_id: (*entry).clone(),
                    segment_id: Some(segments[0].id.clone()),
                    word_range: None,
                    start_ms: 50,
                    end_ms: 900,
                });
                cues.push(AudioCue {
                    entry_id: (*entry).clone(),
                    segment_id: Some(segments[0].id.clone()),
                    word_range: Some(AudioWordRange { start: 0, end: 7 }),
                    start_ms: 100,
                    end_ms: 600,
                });
            }
            lesson.audio_tracks.push(AudioTrack {
                block_id: id.clone(),
                asset_id: "audio-browser-qa".into(),
                cues,
            });
        }
        lesson.validate().map_err(anyhow::Error::msg)?;
        source["audio"] = serde_json::to_value(lesson.audio)?;
        source["audioTracks"] = serde_json::to_value(lesson.audio_tracks)?;
    }
    let lesson = project_source(source.clone())?;
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"INSERT INTO lesson_revisions(lesson_id,revision,published,public_document,server_document) VALUES($1,$2,true,$3,$4)",[lesson.id.clone().into(),(lesson.revision as i32).into(),serde_json::to_value(lesson)?.into(),source.into()])).await?;
    support::fixture_release(&db).await;
    let backend = Backend::new(db.clone()).await?;
    let token = backend
        .issue_token("browser-qa@example.test", false, false)
        .await?;
    let app = identity::router(
        backend,
        CsrfPolicy::new(["http://127.0.0.1:5175".into()])?,
        false,
    );
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/csrf")
                .body(Body::empty())?,
        )
        .await?;
    let cookie = response.headers()["set-cookie"]
        .to_str()?
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let csrf: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await?.to_bytes())?;
    let body = serde_json::json!({"email":"browser-qa@example.test","token":token,"password":"Browser protocol test only passphrase","displayName":"Browser QA"});
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/accept-invite")
                .header("cookie", cookie)
                .header("origin", "http://127.0.0.1:5175")
                .header("x-csrf-token", csrf["csrfToken"].as_str().unwrap())
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body)?))?,
        )
        .await?;
    ensure!(
        response.status().is_success(),
        "synthetic account creation failed"
    );
    println!("Disposable browser fixture ready; unreviewed synthetic course, test account only.");
    Ok(())
}
async fn recording_import(
    db: &sea_orm::DatabaseConnection,
    bundle: chef_engine::recording::AudioBundle,
    root: &std::path::Path,
) -> Result<()> {
    chef_engine::recording::import_bundle(
        db,
        bundle,
        root,
        &chef_engine::media::media_root(),
        "browser-protocol-test",
    )
    .await
}
