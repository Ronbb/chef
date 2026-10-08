//! Offline author assembly. Local IDs are never inserted as authenticated paid tasks.
use anyhow::{Result, ensure};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::Path,
};

const MAX_ARCHIVE: usize = 128 * 1024 * 1024;

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= limit, "Local input exceeds size limit");
    Ok(bytes)
}

pub(crate) fn run(args: &[String]) -> Result<()> {
    ensure!(
        args.len() == 6,
        "usage: speech-package-local <lesson.json> <voice-plan.json> <inputs.tar> <predictions.json> <package-request.json> <new-private-output.tar>"
    );
    let document = crate::author_json::Document::load(&args[0])?;
    let lesson = crate::author_source::check_any_lesson(&document)?;
    let voices: crate::speech_plan::Config = crate::author_json::load(&args[1])?;
    let plan = serde_json::to_value(crate::speech_plan::compile_checked(
        &lesson,
        &document.value,
        &voices,
    )?)?;
    // This command consumes authored scalar units. Legacy database tasks retain their original flow.
    ensure!(
        plan["compilerVersion"] == crate::speech_plan::NEUTRAL_VERSION,
        "Expected native author plan"
    );
    let archive = read_bounded(Path::new(&args[2]), MAX_ARCHIVE)?;
    let mut members = BTreeMap::new();
    for member in tar::Archive::new(archive.as_slice()).entries()? {
        let mut member = member?;
        ensure!(
            member.header().entry_type().is_file(),
            "Expected regular archive files"
        );
        let name = member
            .path()?
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid member name"))?
            .to_owned();
        let media = name
            .strip_prefix("media/")
            .and_then(|n| n.strip_suffix(".wav"));
        ensure!(
            name == "manifest.json" || media.is_some_and(|s| crate::voice_references::hex(s, 64)),
            "Invalid archive member"
        );
        ensure!(
            members.len() < 2001 && !members.contains_key(&name),
            "Duplicate or excessive archive members"
        );
        let limit = if name == "manifest.json" {
            4 * 1024 * 1024
        } else {
            16 * 1024 * 1024
        };
        ensure!(
            member.size() > 0 && member.size() <= limit,
            "Archive member exceeds bounds"
        );
        let mut bytes = Vec::new();
        member.read_to_end(&mut bytes)?;
        members.insert(name, bytes);
    }
    let input = crate::author_json::parse_document_bounded(
        members
            .get("manifest.json")
            .ok_or_else(|| anyhow::anyhow!("Missing manifest"))?,
        4 * 1024 * 1024,
    )?;
    ensure!(
        input["schemaVersion"] == "1.0"
            && input["kind"] == "brioche-speech-inputs"
            && input["publicationPolicy"] == "owner-direct-publish"
            && input["humanListeningAsserted"] == false
            && input["plan"] == plan,
        "Input differs from recompiled author plan"
    );
    let report_bytes = read_bounded(Path::new(&args[3]), 4 * 1024 * 1024)?;
    let mut report = crate::author_json::parse_document_bounded(&report_bytes, 4 * 1024 * 1024)?;
    ensure!(
        report["kind"] == "brioche-automatic-alignment-predictions"
            && report["reviewRequired"] == false
            && report["planId"] == input["planId"]
            && report["planHash"] == plan["planHash"]
            && report["sourceArchiveSha256"] == crate::media::digest(&archive),
        "Prediction source mismatch"
    );
    report["kind"] = json!("brioche-automatic-alignment-v1");
    report["humanListeningAsserted"] = json!(false);
    report["originalPredictionReportSha256"] = json!(crate::media::digest(&report_bytes));
    let request: brioche_course_contract::AdminSpeechPackageRequest =
        crate::author_json::load(&args[4])?;
    ensure!(
        request.expected_report_hash == crate::learning::hash(&report)?,
        "Fixed report hash mismatch"
    );
    crate::speech_automatic::check_report(&report, &request)?;
    let mut used = BTreeSet::from(["manifest.json".to_owned()]);
    let mut keys = BTreeSet::new();
    let mut ids = BTreeSet::new();
    let clips = input["clips"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Missing clips"))?;
    let requests = plan["requests"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Missing requests"))?;
    ensure!(clips.len() == requests.len(), "Incomplete clip coverage");
    let output = Path::new(&args[5]);
    let parent = output
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Output parent missing"))?
        .canonicalize()?;
    let workspace = std::env::var_os("CHEF_WORKSPACE_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    ensure!(
        parent.starts_with(workspace.join(".local/private").canonicalize()?) && !output.exists(),
        "Expected new private output"
    );
    // Created only after source/plan/report checks. Never delete or reuse another attempt's directory.
    let media_root = parent.join(format!(
        "assembly-media-{}",
        crate::media::digest(&report_bytes)
    ));
    std::fs::create_dir(&media_root)?;
    for clip in clips {
        let key = clip["generationKey"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing generation key"))?;
        let id = clip["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing clip ID"))?;
        ensure!(
            requests.contains_key(key)
                && keys.insert(key)
                && crate::voice_references::hex(id, 32)
                && ids.insert(id)
                && clip["review"].is_null(),
            "Invalid local clip identity"
        );
        let expected = serde_json::to_value(crate::speech_alignments::request_words(&plan, key)?)?;
        let units: Vec<_> = expected
            .as_array()
            .unwrap()
            .iter()
            .map(|w| json!({"text":w["text"],"start":w["start"],"end":w["end"]}))
            .collect();
        ensure!(
            clip["words"] == json!(units),
            "Clip differs from authored units"
        );
        for (file, sha_field) in [("file", "sha256"), ("providerFile", "providerSha256")] {
            let sha = clip["result"][sha_field]
                .as_str()
                .filter(|s| crate::voice_references::hex(s, 64))
                .ok_or_else(|| anyhow::anyhow!("Invalid audio hash"))?;
            let name = format!("media/{sha}.wav");
            ensure!(clip[file] == name, "Audio path mismatch");
            let bytes = members
                .get(&name)
                .ok_or_else(|| anyhow::anyhow!("Missing audio member"))?;
            ensure!(crate::media::digest(bytes) == sha, "Audio hash mismatch");
            used.insert(name);
            crate::media::store_file(&media_root, bytes, sha, "wav")?;
        }
        crate::speech_media::read(&media_root, &clip["result"])?;
    }
    ensure!(
        used == members.keys().cloned().collect(),
        "Unreferenced archive members"
    );
    let snapshot = json!({"input":input,"source":document.value});
    let mut manifest = crate::speech_automatic::manifest(&snapshot, &report)?;
    manifest["inputIdentity"] =
        json!("local-author-files; IDs are not database task registrations");
    let bytes = crate::speech_package::pack_local(&media_root, manifest, &request)?;
    crate::maintenance_auth::save_private_archive(&args[5], &bytes)?;
    Ok(())
}
