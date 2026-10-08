use crate::{AppState, development_fixture, router};
use anyhow::{Context, Result, bail};
use sea_orm::{ConnectOptions, ConnectionTrait, Database};
use sea_orm_migration::MigratorTrait;

pub async fn run() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "chef_engine=info,tower_http=info".into()),
        )
        .init();
    let command = std::env::args().nth(1).unwrap_or_else(|| "serve".into());
    let product = crate::product::ProductId::configured("CHEF_PRODUCT")?;
    product.validate_command(&command)?;
    if command == "speech-plan" {
        let args: Vec<String> = std::env::args().skip(2).collect();
        if args.len() != 2 {
            bail!("usage: brioche-server speech-plan <lesson.json> <voice-plan.json>");
        }
        let document = crate::author_json::Document::load(&args[0])?;
        let lesson = crate::author_source::check_lesson(&document)?;
        let config: crate::speech_plan::Config = crate::author_json::load(&args[1])?;
        let plan = crate::speech_plan::compile(&lesson, &document.value, &config)?;
        println!("{}", serde_json::to_string_pretty(&plan)?);
        return Ok(());
    }
    if matches!(command.as_str(), "assets-check" | "audio-bundle-check") {
        let args: Vec<String> = std::env::args().skip(2).collect();
        if args.len() != 2 {
            bail!("usage: brioche-server {command} <bundle.json> <source-directory>");
        }
        let visual = command == "assets-check";
        tokio::task::spawn_blocking(move || -> Result<()> {
            let document = crate::author_json::Document::load(&args[0])?;
            let root = std::path::Path::new(&args[1]);
            if visual {
                let bundle = crate::author_json::from_value(document.value.clone(), "")
                    .map_err(|error| document.semantic(error))?;
                crate::media::check_bundle(&bundle, root)
                    .map_err(|error| document.semantic(error))?;
            } else {
                let bundle = crate::author_json::from_value(document.value.clone(), "")
                    .map_err(|error| document.semantic(error))?;
                crate::recording::check_bundle(&bundle, root)
                    .map_err(|error| document.semantic(error))?;
            }
            Ok(())
        })
        .await??;
        println!(
            "Local metadata and file checks passed; not registered or published. Registry references and publication review still require import and release validation."
        );
        return Ok(());
    }
    if command == "asset-check" {
        let args: Vec<String> = std::env::args().skip(2).collect();
        if args.len() != 2 {
            bail!(
                "usage: brioche-server asset-check <image-file> <image/svg+xml|image/png|image/jpeg|image/webp>"
            );
        }
        let info = tokio::task::spawn_blocking(move || {
            crate::media::inspect_file(std::path::Path::new(&args[0]), &args[1])
                .with_context(|| format!("{}: invalid visual asset", args[0]))
        })
        .await??;
        println!("{}", serde_json::to_string(&info)?);
        return Ok(());
    }
    if command == "audio-check" {
        let args: Vec<String> = std::env::args().skip(2).collect();
        if args.len() != 2 {
            bail!("usage: brioche-server audio-check <recording-file> <audio/mpeg|audio/wav>");
        }
        let (bytes, info) = tokio::task::spawn_blocking(move || {
            crate::audio::inspect_file(std::path::Path::new(&args[0]), &args[1])
                .with_context(|| format!("{}: invalid recording", args[0]))
        })
        .await??;
        use sha2::{Digest, Sha256};
        println!(
            "{}",
            serde_json::json!({
                "sha256": format!("{:x}", Sha256::digest(&bytes)),
                "byteLength": bytes.len(),
                "durationMs": info.duration_ms,
                "sampleRate": info.sample_rate,
                "channels": info.channels,
            })
        );
        return Ok(());
    }
    if matches!(command.as_str(), "check" | "check-release") {
        let args: Vec<String> = std::env::args().skip(2).collect();
        let with_sources = command == "check-release" && args.len() >= 3 && args[1] == "--sources";
        if args.len() != 1 && !with_sources {
            bail!(
                "usage: brioche-server {command} <file.json> (check-release optionally accepts --sources <file-or-directory> [<file-or-directory> ...])"
            );
        }
        let path = &args[0];
        if command == "check-release" {
            let document = crate::author_json::Document::load(path)?;
            let manifest: crate::content::ReleaseManifest =
                crate::author_json::from_value(document.value.clone(), "")
                    .map_err(|error| document.semantic(error))?;
            manifest
                .validate_author()
                .map_err(|error| document.semantic(error))?;
            if with_sources {
                let roots = args[2..]
                    .iter()
                    .map(std::path::PathBuf::from)
                    .collect::<Vec<_>>();
                let count =
                    crate::author_source::check_release_sources(&document, &manifest, &roots)?;
                println!("Checked {count} referenced lesson sources.");
            }
        } else {
            let document = crate::author_json::Document::load(path)?;
            crate::author_source::check_any_lesson(&document)?;
        }
        println!(
            "Structural checks passed. Publication still requires registered media, editorial review and release-stage validation."
        );
        return Ok(());
    }
    // Preserve the original author text before hydration or database operations.
    let import_document = if command == "import" {
        let args: Vec<String> = std::env::args().skip(2).collect();
        if args.iter().any(|arg| arg == "--publish") {
            bail!("use release-stage and release-activate to publish an atomic directory");
        }
        if args.len() != 1 {
            bail!("usage: brioche-server import <lesson.json>");
        }
        let document = crate::author_json::Document::load(&args[0])?;
        crate::media::source_asset_refs(&document.value)
            .map_err(|error| document.semantic(error))?;
        crate::recording::source_audio_refs(&document.value)
            .map_err(|error| document.semantic(error))?;
        crate::author_source::editorial(&document.value)
            .map_err(|error| document.semantic(error))?;
        crate::grading::Grader::validate_author_schema(&document.value)
            .map_err(|error| document.semantic(error))?;
        crate::validate_source_schema(document.value.clone())
            .map_err(|error| document.semantic(error))?;
        Some(document)
    } else {
        None
    };
    let release_source = if command == "release-stage" {
        let args: Vec<String> = std::env::args().skip(2).collect();
        if args.len() != 3 {
            bail!("usage: release-stage <manifest.json> <actor> <reason>");
        }
        let document = crate::author_json::Document::load(&args[0])?;
        let manifest: crate::content::ReleaseManifest =
            crate::author_json::from_value(document.value.clone(), "")
                .map_err(|error| document.semantic(error))?;
        manifest
            .validate_author()
            .map_err(|error| document.semantic(error))?;
        Some((document, manifest))
    } else {
        None
    };
    let media_document = if matches!(command.as_str(), "assets-import" | "audio-import") {
        let args: Vec<String> = std::env::args().skip(2).collect();
        if args.len() != 3 {
            bail!("usage: {command} <bundle.json> <source-directory> <actor>");
        }
        let document = crate::author_json::Document::load(&args[0])?;
        if command == "assets-import" {
            let bundle: crate::media::AssetBundle =
                crate::author_json::from_value(document.value.clone(), "")
                    .map_err(|error| document.semantic(error))?;
            bundle
                .validate_author(&args[2])
                .map_err(|error| document.semantic(error))?;
        } else {
            let bundle: crate::recording::AudioBundle =
                crate::author_json::from_value(document.value.clone(), "")
                    .map_err(|error| document.semantic(error))?;
            bundle
                .validate_author(&args[2])
                .map_err(|error| document.semantic(error))?;
        }
        Some(document)
    } else {
        None
    };
    let fixture = std::env::var("CONTENT_MODE").unwrap_or_else(|_| "database".into()) == "fixture";
    let production =
        std::env::var("APP_ENV").unwrap_or_else(|_| "production".into()) != "development";
    if fixture && production {
        bail!("fixture content is only permitted with APP_ENV=development");
    }
    if product == crate::product::ProductId::Hargow && command == "serve" {
        // Keep the startup boundary closed until local keys and language contracts
        // are verified. Never initialize the legacy Brioche backend or fixture.
        bail!("Product learning data migration incomplete");
    }
    let db = if fixture && command == "serve" {
        None
    } else {
        let mut options = ConnectOptions::new(
            std::env::var("DATABASE_URL")
                .map_err(|_| anyhow::anyhow!("DATABASE_URL is required for database mode"))?,
        );
        options
            .sqlx_logging(false)
            .max_connections(10)
            .connect_timeout(std::time::Duration::from_secs(5))
            .acquire_timeout(std::time::Duration::from_secs(5));
        crate::database_scope::apply(
            &mut options,
            std::env::var("DATABASE_SCHEMA").ok().as_deref(),
        )?;
        Some(
            Database::connect(options)
                .await
                .map_err(|_| anyhow::anyhow!("database connection failed"))?,
        )
    };
    let author_command = matches!(
        command.as_str(),
        "import"
            | "release-stage"
            | "release-activate"
            | "content-withdraw"
            | "release-status"
            | "assets-import"
            | "audio-import"
            | "speech-plan-export"
            | "speech-plan-export-direct"
            | "speech-package-automatic"
            | "speech-plan-preview"
            | "speech-plan-save"
            | "speech-clip-generate"
            | "voice-audition-generate"
            | "voice-audition-review"
            | "speech-clip-review"
            | "character-voice-import"
            | "speech-alignment-import"
            | "speech-package-import"
            | "lesson-direct-publication"
    );
    let author_product = if author_command {
        crate::schema_split::author_scope(db.as_ref().unwrap(), product).await
            .map_err(|_| anyhow::anyhow!("Author command layout not ready; verify the recorded learning schema and complete migration history"))?
    } else {
        if !matches!(
            command.as_str(),
            "serve" | "migrate" | "split-identity-schema" | "migrate-layout"
        ) {
            crate::schema_split::require_combined(db.as_ref().unwrap()).await?;
        }
        None
    };
    if command == "speech-package-import" && author_product.is_none() {
        bail!("Speech package maintenance requires the complete split layout");
    }
    if let Some(scope) = author_product
        && matches!(
            command.as_str(),
            "speech-alignment-import" | "speech-package-import"
        )
    {
        let args: Vec<String> = std::env::args().skip(2).collect();
        anyhow::ensure!(
            args.len() == 3,
            "usage: {command} <id> <operator-email> <request.json>"
        );
        let operator = crate::maintenance_auth::operator(scope, &args[1]).await?;
        let document = crate::author_json::Document::load(&args[2])?;
        if command == "speech-alignment-import" {
            let request: brioche_course_contract::AdminAlignmentImport =
                crate::author_json::from_value(document.value, "")?;
            anyhow::ensure!(
                request.id == args[0],
                "Alignment id must match the fixed request"
            );
            let result = crate::speech_alignments::import_author(
                db.as_ref().unwrap(),
                scope,
                &operator,
                crate::media::media_root(),
                request,
            )
            .await
            .map_err(|_| {
                anyhow::anyhow!("Scoped alignment not confirmed; inspect the fixed report")
            })?;
            println!(
                "{}",
                serde_json::json!({"id":result.id,"reportHash":result.report_hash,"reviewRequired":true,"published":false})
            );
        } else {
            let result = crate::speech_package::import_author(
                db.as_ref().unwrap(),
                scope,
                &operator,
                args[0].clone(),
                crate::media::media_root(),
                crate::author_json::from_value(document.value, "")?,
            )
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "Scoped package import not confirmed; inspect fixed sources and receipt"
                )
            })?;
            println!(
                "{}",
                serde_json::json!({"id":result.id,"lessonId":result.lesson_id,"revision":result.revision,"recordingCount":result.recording_count,"published":false})
            );
        }
        return Ok(());
    }
    if let Some(scope) = author_product
        && command == "lesson-direct-publication"
    {
        let args: Vec<String> = std::env::args().skip(2).collect();
        anyhow::ensure!(
            args.len() == 4,
            "usage: lesson-direct-publication <lesson-id> <revision> <operator-email> <authorization.json>"
        );
        let revision: u32 = args[1]
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid lesson revision"))?;
        let operator = crate::maintenance_auth::operator(scope, &args[2]).await?;
        let document = crate::author_json::Document::load(&args[3])?;
        let result = crate::lesson_audio_reviews::authorize_author(
            db.as_ref().unwrap(),
            scope,
            &operator,
            &args[0],
            revision,
            &crate::media::media_root(),
            crate::author_json::from_value(document.value, "")?,
        )
        .await
        .map_err(|_| {
            anyhow::anyhow!("Scoped publication authorization not confirmed; inspect fixed sources")
        })?;
        println!("{}", serde_json::to_string(&result)?);
        return Ok(());
    }
    if let Some(scope) = author_product
        && matches!(
            command.as_str(),
            "voice-audition-review" | "speech-clip-review"
        )
    {
        let args: Vec<String> = std::env::args().skip(2).collect();
        anyhow::ensure!(
            args.len() == 3,
            "usage: {command} <id> <operator-email> <review.json>"
        );
        let operator = crate::maintenance_auth::operator(scope, &args[1]).await?;
        let document = crate::author_json::Document::load(&args[2])?;
        let accepted = if command == "voice-audition-review" {
            crate::voice_auditions::review_author(
                db.as_ref().unwrap(),
                scope,
                &operator,
                args[0].clone(),
                crate::author_json::from_value(document.value, "")?,
            )
            .await
            .map_err(|_| {
                anyhow::anyhow!("Scoped audition review not confirmed; inspect the fixed record")
            })?
            .accepted
        } else {
            crate::speech_clips::review_author(
                db.as_ref().unwrap(),
                scope,
                &operator,
                args[0].clone(),
                crate::author_json::from_value(document.value, "")?,
            )
            .await
            .map_err(|_| {
                anyhow::anyhow!("Scoped clip review not confirmed; inspect the fixed record")
            })?
            .accepted
        };
        println!(
            "{}",
            serde_json::json!({"id":args[0],"accepted":accepted,"published":false})
        );
        return Ok(());
    }
    if let Some(scope) = author_product
        && command == "character-voice-import"
    {
        let args: Vec<String> = std::env::args().skip(2).collect();
        anyhow::ensure!(
            args.len() == 3,
            "usage: character-voice-import <request.json> <operator-email> <reason>"
        );
        let operator = crate::maintenance_auth::operator(scope, &args[1]).await?;
        let document = crate::author_json::Document::load(&args[0])?;
        let mut request: brioche_course_contract::AdminCharacterVoiceRequest =
            crate::author_json::from_value(document.value, "")?;
        request.reason = args[2].clone();
        let result =
            crate::character_voices::append_author(db.as_ref().unwrap(), scope, &operator, request)
                .await
                .map_err(|_| {
                    anyhow::anyhow!(
                        "Scoped character voice not confirmed; inspect the fixed revision"
                    )
                })?;
        println!(
            "Character voice profile registered at revision {}. No audio generated or published.",
            result.voice_revision
        );
        return Ok(());
    }
    if let Some(scope) = author_product
        && matches!(
            command.as_str(),
            "speech-clip-generate" | "voice-audition-generate"
        )
    {
        let args: Vec<String> = std::env::args().skip(2).collect();
        anyhow::ensure!(
            args.len() == 2,
            "usage: {command} <request.json> <operator-email>"
        );
        let operator = crate::maintenance_auth::operator(scope, &args[1]).await?;
        let document = crate::author_json::Document::load(&args[0])?;
        let service = crate::qwen::Service::from_env()
            .map_err(|_| anyhow::anyhow!("Invalid TTS configuration"))?;
        let summary = if command == "speech-clip-generate" {
            let result = crate::speech_clips::submit_author(
                db.as_ref().unwrap(),
                scope,
                &operator,
                service,
                crate::media::media_root(),
                crate::author_json::from_value(document.value, "")?,
            )
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "Scoped clip not confirmed; inspect the fixed attempt before retrying"
                )
            })?;
            serde_json::json!({"id":result.id,"status":result.status,"durationMs":result.duration_ms,"reviewRequired":result.accepted != Some(true)})
        } else {
            let result = crate::voice_auditions::submit_author(
                db.as_ref().unwrap(),
                scope,
                &operator,
                service,
                crate::media::media_root(),
                crate::author_json::from_value(document.value, "")?,
            )
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "Scoped audition not confirmed; inspect the fixed attempt before retrying"
                )
            })?;
            serde_json::json!({"id":result.id,"status":result.status,"durationMs":result.duration_ms,"reviewRequired":true})
        };
        println!("{summary}");
        return Ok(());
    }
    if let Some(scope) = author_product
        && matches!(command.as_str(), "speech-plan-preview" | "speech-plan-save")
    {
        let args: Vec<String> = std::env::args().skip(2).collect();
        let preview = command == "speech-plan-preview";
        anyhow::ensure!(
            args.len() == if preview { 3 } else { 2 },
            "usage: speech-plan-preview <request.json> <operator-email> <new-private-output.json>; speech-plan-save <request.json> <operator-email>"
        );
        let operator = crate::maintenance_auth::operator(scope, &args[1]).await?;
        let document = crate::author_json::Document::load(&args[0])?;
        let result = if preview {
            let request = crate::author_json::from_value(document.value, "")?;
            let plan = crate::admin_speech_plans::preview_author(
                db.as_ref().unwrap(),
                scope,
                &operator,
                &request,
            )
            .await
            .map_err(|_| anyhow::anyhow!("Scoped speech plan preview failed"))?;
            crate::maintenance_auth::save_private_archive(
                &args[2],
                &serde_json::to_vec_pretty(&plan)?,
            )?;
            plan
        } else {
            let request = crate::author_json::from_value(document.value, "")?;
            crate::admin_speech_plans::save_author(db.as_ref().unwrap(), scope, &operator, request)
                .await.map_err(|_| anyhow::anyhow!("Scoped speech plan not confirmed; inspect the fixed attempt before retrying"))?
        };
        println!(
            "{}",
            serde_json::json!({"id":result.id,"planHash":result.plan_hash,"requestCount":result.request_count,"totalRequestCharacters":result.total_request_characters,"audioGenerated":false})
        );
        return Ok(());
    }
    if let Some(scope) = author_product
        && matches!(
            command.as_str(),
            "speech-plan-export" | "speech-plan-export-direct"
        )
    {
        let args: Vec<String> = std::env::args().skip(2).collect();
        anyhow::ensure!(
            args.len() == 3,
            "usage: {command} <plan-id> <operator-email> <new-private-output.tar>"
        );
        let operator = crate::maintenance_auth::operator(scope, &args[1]).await?;
        let reviewed = command == "speech-plan-export";
        let bytes = crate::speech_export::export_author(
            db.as_ref().unwrap(),
            scope,
            &operator,
            args[0].clone(),
            crate::media::media_root(),
            reviewed,
        )
        .await
        .map_err(|_| anyhow::anyhow!("Private scoped export failed; output is not confirmed"))?;
        crate::maintenance_auth::save_private_archive(&args[2], &bytes)?;
        println!(
            "{}",
            serde_json::json!({"planId":args[0],"byteLength":bytes.len(),"reviewRequired":reviewed,"published":false})
        );
        return Ok(());
    }
    if let Some(scope) = author_product
        && command == "speech-package-automatic"
    {
        let args: Vec<String> = std::env::args().skip(2).collect();
        anyhow::ensure!(
            args.len() == 4,
            "usage: speech-package-automatic <report.json> <package-request.json> <operator-email> <new-private-output.tar>"
        );
        let operator = crate::maintenance_auth::operator(scope, &args[2]).await?;
        let report = crate::author_json::Document::load(&args[0])?;
        let request = crate::author_json::Document::load(&args[1])?;
        let bytes = crate::speech_automatic::assemble_author(
            db.as_ref().unwrap(),
            scope,
            &operator,
            crate::media::media_root(),
            report.value,
            crate::author_json::from_value(request.value, "")?,
        )
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "Automatic scoped package not confirmed; inspect fixed report and sources"
            )
        })?;
        crate::maintenance_auth::save_private_archive(&args[3], &bytes)?;
        println!(
            "{}",
            serde_json::json!({"byteLength":bytes.len(),"humanListeningAsserted":false,"published":false})
        );
        return Ok(());
    }
    match command.as_str() {
        "speech-package-automatic" => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            if args.len() != 4 {
                bail!(
                    "usage: speech-package-automatic <report.json> <package-request.json> <operator-email> <new-private-output.tar>"
                );
            }
            let database = db.as_ref().unwrap();
            let row = database
                .query_one_raw(sea_orm::Statement::from_sql_and_values(
                    sea_orm::DbBackend::Postgres,
                    "SELECT id FROM users WHERE email=$1 AND role='operator'",
                    vec![args[2].clone().into()],
                ))
                .await?
                .ok_or_else(|| anyhow::anyhow!("operator not found"))?;
            let backend = crate::identity::Backend::new(database.clone()).await?;
            let report = crate::author_json::Document::load(&args[0])?;
            let request = crate::author_json::Document::load(&args[1])?;
            let bytes = crate::speech_automatic::assemble_for_actor(
                &backend,
                row.try_get("", "id")?,
                crate::media::media_root(),
                report.value,
                crate::author_json::from_value(request.value, "")?,
            )
            .await
            .map_err(|_| {
                anyhow::anyhow!("automatic package not confirmed; inspect fixed report and sources")
            })?;
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            use std::io::Write;
            let mut file = options.open(&args[3])?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            println!(
                "{}",
                serde_json::json!({"byteLength":bytes.len(),"humanListeningAsserted":false,"published":false})
            );
            return Ok(());
        }
        "lesson-direct-publication" => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            if args.len() != 4 {
                bail!(
                    "usage: lesson-direct-publication <lesson-id> <revision> <operator-email> <authorization.json>"
                );
            }
            let database = db.as_ref().unwrap();
            let row = database
                .query_one_raw(sea_orm::Statement::from_sql_and_values(
                    sea_orm::DbBackend::Postgres,
                    "SELECT id FROM users WHERE email=$1 AND role='operator'",
                    vec![args[2].clone().into()],
                ))
                .await
                .map_err(|_| anyhow::anyhow!("operator lookup failed"))?
                .ok_or_else(|| anyhow::anyhow!("operator not found"))?;
            let actor: i64 = row.try_get("", "id")?;
            let revision: u32 = args[1].parse()?;
            let document = crate::author_json::Document::load(&args[3])?;
            let backend = crate::identity::Backend::new(database.clone()).await?;
            let result = crate::lesson_audio_reviews::authorize_local(
                &backend,
                actor,
                &args[0],
                revision,
                &crate::media::media_root(),
                crate::author_json::from_value(document.value, "")?,
            )
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "direct publication authorization not confirmed; inspect the fixed request"
                )
            })?;
            println!("{}", serde_json::to_string(&result)?);
            return Ok(());
        }
        "voice-audition-review"
        | "speech-clip-review"
        | "speech-plan-export"
        | "speech-plan-export-direct"
        | "speech-alignment-import" => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            if args.len() != 3 {
                bail!(
                    "usage: {command} <id> <operator-email> <review.json|new-private-output.tar>"
                );
            }
            let database = db.as_ref().unwrap();
            let row = database
                .query_one_raw(sea_orm::Statement::from_sql_and_values(
                    sea_orm::DbBackend::Postgres,
                    "SELECT id FROM users WHERE email=$1 AND role='operator'",
                    vec![args[1].clone().into()],
                ))
                .await
                .map_err(|_| anyhow::anyhow!("operator lookup failed"))?
                .ok_or_else(|| anyhow::anyhow!("operator not found"))?;
            let actor: i64 = row.try_get("", "id")?;
            let backend = crate::identity::Backend::new(database.clone()).await?;
            if command == "speech-plan-export" || command == "speech-plan-export-direct" {
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let mut output = options
                    .open(&args[2])
                    .map_err(|_| anyhow::anyhow!("private output must be a new writable file"))?;
                let bytes = if command == "speech-plan-export-direct" {
                    crate::speech_export::export_direct_for_actor(
                        &backend,
                        actor,
                        args[0].clone(),
                        crate::media::media_root(),
                    )
                    .await
                } else {
                    crate::speech_export::export_for_actor(
                        &backend,
                        actor,
                        args[0].clone(),
                        crate::media::media_root(),
                    )
                    .await
                }
                .map_err(|_| anyhow::anyhow!("private export failed; output is not confirmed"))?;
                use std::io::Write;
                output.write_all(&bytes)?;
                output.sync_all()?;
                println!(
                    "{}",
                    serde_json::json!({"planId":args[0],"byteLength":bytes.len(),"reviewRequired":command == "speech-plan-export","published":false})
                );
                return Ok(());
            }
            let document = crate::author_json::Document::load(&args[2])?;
            if command == "speech-alignment-import" {
                let request: brioche_course_contract::AdminAlignmentImport =
                    crate::author_json::from_value(document.value, "")?;
                if request.id != args[0] {
                    bail!("alignment id must match the fixed request");
                }
                let result = crate::speech_alignments::import_local(
                    &backend,
                    actor,
                    crate::media::media_root(),
                    request,
                )
                .await
                .map_err(|_| {
                    anyhow::anyhow!("alignment import not confirmed; inspect the fixed request")
                })?;
                println!(
                    "{}",
                    serde_json::json!({"id":result.id,"planId":result.plan_id,"reviewRequired":true,"published":false})
                );
                return Ok(());
            }
            let accepted = if command == "voice-audition-review" {
                crate::voice_auditions::review_local(
                    &backend,
                    actor,
                    args[0].clone(),
                    crate::author_json::from_value(document.value, "")?,
                )
                .await
                .map_err(|_| {
                    anyhow::anyhow!("audition review not confirmed; inspect the fixed record")
                })?
                .accepted
            } else {
                crate::speech_clips::review_local(
                    &backend,
                    actor,
                    args[0].clone(),
                    crate::author_json::from_value(document.value, "")?,
                )
                .await
                .map_err(|_| {
                    anyhow::anyhow!("clip review not confirmed; inspect the fixed record")
                })?
                .accepted
            };
            println!(
                "{}",
                serde_json::json!({"id":args[0],"accepted":accepted,"published":false})
            );
            return Ok(());
        }
        "speech-plan-preview" | "speech-plan-save" => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            let preview = command == "speech-plan-preview";
            if args.len() != if preview { 3 } else { 2 } {
                bail!(
                    "usage: speech-plan-preview <request.json> <operator-email> <private-output.json>; speech-plan-save <request.json> <operator-email>"
                );
            }
            let document = crate::author_json::Document::load(&args[0])?;
            let database = db.as_ref().unwrap();
            let row = database
                .query_one_raw(sea_orm::Statement::from_sql_and_values(
                    sea_orm::DbBackend::Postgres,
                    "SELECT id FROM users WHERE email=$1 AND role='operator'",
                    vec![args[1].clone().into()],
                ))
                .await
                .map_err(|_| anyhow::anyhow!("operator lookup failed"))?
                .ok_or_else(|| anyhow::anyhow!("operator not found"))?;
            let backend = crate::identity::Backend::new(database.clone()).await?;
            let actor: i64 = row.try_get("", "id")?;
            let result = if preview {
                let request = crate::author_json::from_value(document.value, "")?;
                let plan = crate::admin_speech_plans::preview_for_actor(&backend, actor, &request)
                    .await
                    .map_err(|_| anyhow::anyhow!("fixed speech plan preview failed"))?;
                // Private plans contain full voice parameters. Never print them or overwrite files.
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let mut output = options
                    .open(&args[2])
                    .map_err(|_| anyhow::anyhow!("private output must be a new writable file"))?;
                use std::io::Write;
                output.write_all(&serde_json::to_vec_pretty(&plan)?)?;
                output.sync_all()?;
                plan
            } else {
                let request = crate::author_json::from_value(document.value, "")?;
                crate::admin_speech_plans::save_local(&backend, actor, request)
                    .await
                    .map_err(|_| {
                        anyhow::anyhow!(
                            "speech plan not confirmed; inspect the fixed attempt before retrying"
                        )
                    })?
            };
            println!(
                "{}",
                serde_json::json!({"id":result.id,"planHash":result.plan_hash,"requestCount":result.request_count,"totalRequestCharacters":result.total_request_characters,"audioGenerated":false})
            );
            return Ok(());
        }
        "voice-audition-generate" | "speech-clip-generate" => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            if args.len() != 2 {
                bail!("usage: {command} <request.json> <operator-email>");
            }
            let document = crate::author_json::Document::load(&args[0])?;
            let database = db.as_ref().unwrap();
            let row = database
                .query_one_raw(sea_orm::Statement::from_sql_and_values(
                    sea_orm::DbBackend::Postgres,
                    "SELECT id FROM users WHERE email=$1 AND role='operator'",
                    vec![args[1].clone().into()],
                ))
                .await?
                .ok_or_else(|| anyhow::anyhow!("operator not found"))?;
            let backend = crate::identity::Backend::new(database.clone()).await?;
            let service = crate::qwen::Service::from_env()
                .map_err(|_| anyhow::anyhow!("invalid TTS configuration"))?;
            if command == "speech-clip-generate" {
                let request = crate::author_json::from_value(document.value, "")?;
                let result = crate::speech_clips::submit_local(
                    backend,
                    row.try_get("", "id")?,
                    service,
                    crate::media::media_root(),
                    request,
                )
                .await
                .map_err(|_| {
                    anyhow::anyhow!("clip not confirmed; inspect the fixed attempt before retrying")
                })?;
                println!(
                    "{}",
                    serde_json::json!({"id":result.id,"status":result.status,"durationMs":result.duration_ms,"reviewRequired":result.accepted != Some(true)})
                );
                return Ok(());
            }
            let request = crate::author_json::from_value(document.value, "")?;
            let result = crate::voice_auditions::submit_local(
                backend,
                row.try_get("", "id")?,
                service,
                crate::media::media_root(),
                request,
            )
            .await
            .map_err(|_| {
                anyhow::anyhow!("audition not confirmed; inspect the fixed attempt before retrying")
            })?;
            println!(
                "{}",
                serde_json::json!({"id":result.id,"status":result.status,"durationMs":result.duration_ms,"reviewRequired":true})
            );
            return Ok(());
        }
        "character-voice-import" => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            if args.len() != 3 {
                bail!("usage: character-voice-import <request.json> <operator-email> <reason>");
            }
            let document = crate::author_json::Document::load(&args[0])?;
            let mut request: brioche_course_contract::AdminCharacterVoiceRequest =
                crate::author_json::from_value(document.value, "")?;
            request.reason = args[2].clone();
            let database = db.as_ref().unwrap();
            let row = database
                .query_one_raw(sea_orm::Statement::from_sql_and_values(
                    sea_orm::DbBackend::Postgres,
                    "SELECT id FROM users WHERE email=$1 AND role='operator'",
                    vec![args[1].clone().into()],
                ))
                .await?
                .ok_or_else(|| anyhow::anyhow!("operator not found"))?;
            let actor: i64 = row.try_get("", "id")?;
            let result = crate::character_voices::append_profile(database, actor, request).await?;
            println!(
                "Character voice profile registered at revision {}. No audio generated or published.",
                result.voice_revision
            );
            return Ok(());
        }
        "audio-import" => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            if args.len() != 3 {
                bail!("usage: audio-import <bundle.json> <source-directory> <actor>");
            }
            let document = media_document.as_ref().unwrap();
            let bundle = crate::author_json::from_value(document.value.clone(), "")
                .map_err(|error| document.semantic(error))?;
            crate::recording::import_author_bundle(
                db.as_ref().unwrap(),
                author_product,
                bundle,
                std::path::Path::new(&args[1]),
                &crate::media::media_root(),
                &args[2],
            )
            .await
            .map_err(|error| document.semantic(error))?;
            tracing::info!("immutable recording revisions imported");
            return Ok(());
        }
        "assets-import" => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            if args.len() != 3 {
                bail!("usage: assets-import <bundle.json> <source-directory> <actor>");
            }
            let document = media_document.as_ref().unwrap();
            let bundle = crate::author_json::from_value(document.value.clone(), "")
                .map_err(|error| document.semantic(error))?;
            crate::media::import_author_bundle(
                db.as_ref().unwrap(),
                author_product,
                bundle,
                std::path::Path::new(&args[1]),
                &crate::media::media_root(),
                &args[2],
            )
            .await
            .map_err(|error| document.semantic(error))?;
            tracing::info!("asset and character revisions imported");
            return Ok(());
        }
        "release-stage" => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            if args.len() != 3 {
                bail!("usage: release-stage <manifest.json> <actor> <reason>");
            }
            let (document, manifest) = release_source.as_ref().unwrap();
            crate::content::stage_author_product(
                db.as_ref().unwrap(),
                author_product,
                manifest,
                &args[1],
                &args[2],
                &crate::media::media_root(),
            )
            .await
            .map_err(|error| document.semantic(error))?;
            tracing::info!("immutable directory release staged");
            return Ok(());
        }
        "release-activate" | "content-withdraw" => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            let generation = if command == "release-activate" {
                if args.len() != 4 {
                    bail!(
                        "usage: release-activate <release-id> <expected-generation> <actor> <reason>"
                    );
                }
                crate::content::activate_author_product(
                    db.as_ref().unwrap(),
                    author_product,
                    &args[0],
                    args[1].parse()?,
                    &args[2],
                    &args[3],
                    &crate::media::media_root(),
                )
                .await?
            } else {
                if args.len() != 5 {
                    bail!(
                        "usage: content-withdraw <lesson-id> <revision> <expected-generation> <actor> <reason>"
                    );
                }
                crate::content::withdraw_author_product(
                    db.as_ref().unwrap(),
                    author_product,
                    &args[0],
                    args[1].parse()?,
                    args[2].parse()?,
                    &args[3],
                    &args[4],
                )
                .await?
            };
            println!("content generation: {generation}");
            return Ok(());
        }
        "release-status" => {
            let row = db
                .as_ref()
                .unwrap()
                .query_one_raw(sea_orm::Statement::from_string(
                    sea_orm::DbBackend::Postgres,
                    format!(
                        "SELECT active_release,generation FROM content_state WHERE {}",
                        author_product.map_or_else(
                            || "singleton".to_owned(),
                            |p| format!("product_id='{}'", p.as_str())
                        )
                    ),
                ))
                .await?
                .context("content state missing")?;
            let release: Option<String> = row.try_get("", "active_release")?;
            let generation: i64 = row.try_get("", "generation")?;
            println!(
                "{}",
                serde_json::json!({"activeRelease":release,"generation":generation})
            );
            return Ok(());
        }
        "migrate" => {
            crate::schema_split::require_combined(db.as_ref().unwrap()).await?;
            brioche_migration::Migrator::up(db.as_ref().unwrap(), None).await?;
            tracing::info!("migrations complete");
            return Ok(());
        }
        "split-identity-schema" => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            if args.len() != 2 {
                bail!("usage: split-identity-schema <learning-schema> <new-identity-schema>");
            }
            crate::schema_split::relocate(db.as_ref().unwrap(), &args[0], &args[1])
                .await.map_err(|_| anyhow::anyhow!("Identity schema split not confirmed; inspect the migration layout with the owner connection"))?;
            tracing::info!(
                "identity schema split complete; runtime grants and independent service configuration required"
            );
            return Ok(());
        }
        "migrate-layout" => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            if args.len() != 1 {
                bail!("usage: migrate-layout <learning-schema>");
            }
            crate::schema_split::migrate_layout(db.as_ref().unwrap(),&args[0]).await
                .map_err(|_|anyhow::anyhow!("Layout migration not confirmed; inspect owner, schema layout and migration history"))?;
            tracing::info!("layout migrations complete");
            return Ok(());
        }
        "import" => {
            let document = import_document.as_ref().unwrap();
            crate::author_import::import_author_product(
                db.as_ref().unwrap(),
                author_product,
                document.value.clone(),
                "local-author-cli",
                "local author import",
            )
            .await
            .map_err(|error| document.semantic(error))?;
            tracing::info!("immutable lesson revision imported");
            return Ok(());
        }
        "invite" | "reset-password" => {
            let email = std::env::args().nth(2).context("usage: brioche-server invite|reset-password <email> <private-output-file> [--operator]")?;
            let output = std::env::args()
                .nth(3)
                .context("private output file is required")?;
            let base = std::env::var("PUBLIC_APP_URL").context("PUBLIC_APP_URL is required")?;
            crate::csrf::CsrfPolicy::new([base.clone()])?;
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options
                .open(&output)
                .context("cannot create private output file")?;
            let backend = crate::identity::Backend::new(db.as_ref().unwrap().clone()).await?;
            let reset = command == "reset-password";
            let token = backend
                .issue_token(&email, reset, std::env::args().any(|a| a == "--operator"))
                .await?;
            let mut link = url::Url::parse(&base)?;
            link.set_path(if reset { "/reset-password" } else { "/invite" });
            let fragment = url::form_urlencoded::Serializer::new(String::new())
                .append_pair("token", &token)
                .append_pair("email", &email)
                .finish();
            link.set_fragment(Some(&fragment));
            use std::io::Write;
            writeln!(file, "{link}")?;
            tracing::info!("one-time link written to the requested private file");
            return Ok(());
        }
        "serve" => {}
        _ => bail!("unknown command"),
    }
    let remote_origin = match std::env::var("IDENTITY_INTERNAL_URL") {
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(_) => bail!("Invalid identity endpoint configuration"),
    };
    let remote_identity = remote_origin.is_some();
    let content_url = match std::env::var("CONTENT_DATABASE_URL") {
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(_) => bail!("Invalid content database configuration"),
    };
    if content_url.is_some() && !remote_identity {
        bail!("Independent content database requires remote identity mode");
    }
    if remote_identity && db.is_none() {
        bail!("Remote identity requires database mode");
    }
    let auth_router = if let Some(db) = &db {
        let public_url = std::env::var("PUBLIC_APP_URL")
            .context("PUBLIC_APP_URL is required in database mode")?;
        let mut origins = vec![public_url.clone()];
        if let Ok(additional) = std::env::var("ADDITIONAL_APP_ORIGINS") {
            origins.extend(
                additional
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned),
            );
        }
        let policy = crate::csrf::CsrfPolicy::new(origins)?;
        let secure = url::Url::parse(&public_url)?.scheme() == "https";
        Some(if let Some(origin) = remote_origin {
            let key = std::env::var("IDENTITY_INTERNAL_KEY")
                .map_err(|_| anyhow::anyhow!("Identity service credential required"))?;
            let client = crate::learning_identity::Client::new(&origin, &key, product, secure)?;
            let learning = crate::learning_identity::router(db.clone(), client.clone())?;
            if let Some(url) = content_url {
                let mut options = ConnectOptions::new(url);
                options
                    .sqlx_logging(false)
                    .max_connections(4)
                    .connect_timeout(std::time::Duration::from_secs(5))
                    .acquire_timeout(std::time::Duration::from_secs(5));
                crate::database_scope::apply(
                    &mut options,
                    Some(
                        &std::env::var("CONTENT_DATABASE_SCHEMA")
                            .map_err(|_| anyhow::anyhow!("CONTENT_DATABASE_SCHEMA is required"))?,
                    ),
                )?;
                let content_db = Database::connect(options)
                    .await
                    .map_err(|_| anyhow::anyhow!("Content database connection unavailable"))?;
                learning.merge(crate::admin::independent_router(
                    content_db,
                    client,
                    crate::media::media_root(),
                )?)
            } else {
                learning
            }
        } else {
            crate::schema_split::require_combined(db).await?;
            let backend = crate::identity::Backend::new(db.clone()).await?;
            crate::identity::router(backend, policy, secure)
        })
    } else {
        None
    };
    let cleanup = db
        .clone()
        .filter(|_| !remote_identity)
        .map(crate::identity_cleanup::spawn);
    let qwen = crate::qwen::Service::from_env()
        .map_err(|_| anyhow::anyhow!("Invalid private Qwen configuration"))?;
    let listener = tokio::net::TcpListener::bind(
        std::env::var("API_BIND").unwrap_or_else(|_| "0.0.0.0:3001".into()),
    )
    .await?;
    tracing::info!(address=%listener.local_addr()?,"API listening");
    let media_db = db.clone();
    let state = AppState {
        db,
        fixture: if fixture {
            Some(development_fixture()?)
        } else {
            None
        },
    };
    let mut app = if remote_identity {
        crate::independent_product_router(state, product)
    } else {
        router(state)
    };
    if let Some(auth) = auth_router {
        app = app.merge(auth);
    }
    if let Some(db) = media_db {
        if remote_identity {
            app = app.merge(crate::recording::product_router(
                db.clone(),
                crate::media::media_root(),
                product,
            ));
            app = app.merge(crate::media::product_router(
                db,
                crate::media::media_root(),
                product,
            ));
        } else {
            app = app.merge(crate::recording::router(
                db.clone(),
                crate::media::media_root(),
            ));
            app = app.merge(crate::media::router(db, crate::media::media_root()));
        }
    }
    if let Some(qwen) = qwen {
        app = app.layer(axum::Extension(qwen));
    }
    axum::serve(listener, crate::observability::observe(app))
        .with_graceful_shutdown(shutdown())
        .await?;
    if let Some(cleanup) = cleanup {
        cleanup.abort();
    }
    Ok(())
}
pub async fn shutdown() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("SIGTERM listener");
        tokio::select! {_=tokio::signal::ctrl_c()=>{},_=terminate.recv()=>{}}
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
