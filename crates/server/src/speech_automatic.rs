//! Owner-authorized automatic assembly. No human hearing or timing decisions are inserted.
use crate::{
    AppError,
    identity::Backend,
    learning::{exec, field, hash, one},
};
use axum::{
    Extension, Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, State},
    http::HeaderValue,
    response::Response,
    routing::post,
};
use brioche_course_contract::{AdminAlignmentWord, AdminSpeechPackageRequest};
use sea_orm::{ConnectionTrait, IsolationLevel, TransactionTrait};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::PathBuf};
use unicode_normalization::UnicodeNormalization;

#[derive(Clone)]
struct Store {
    db: sea_orm::DatabaseConnection,
}
pub(crate) fn router<S: Clone + Send + Sync + 'static>(
    db: sea_orm::DatabaseConnection,
) -> Router<S> {
    Router::new()
        .route(
            "/api/v1/operator/speech-packages/automatic",
            post(assemble_http).layer(DefaultBodyLimit::max(5 * 1024 * 1024)),
        )
        .with_state(Store { db })
}
async fn assemble_http(
    auth: crate::admin_auth::AdminAuth,
    State(store): State<Store>,
    Extension(root): Extension<PathBuf>,
    Extension(slots): Extension<std::sync::Arc<tokio::sync::Semaphore>>,
    Json(request): Json<brioche_course_contract::AdminAutomaticSpeechPackageRequest>,
) -> Result<Response, AppError> {
    let operator = auth.require_operator().await?;
    if request.report_json.len() > 4 * 1024 * 1024 {
        return Err(AppError::InvalidInput);
    }
    let report = crate::author_json::parse_document(request.report_json.as_bytes())
        .map_err(|_| AppError::InvalidInput)?;
    let _slot = slots
        .try_acquire_owned()
        .map_err(|_| AppError::RateLimited)?;
    let bytes = assemble_authorized(&store, &operator, root, report, request.package).await?;
    let mut response = Response::new(Body::from(bytes));
    response.headers_mut().insert(
        "content-type",
        HeaderValue::from_static("application/x-tar"),
    );
    response.headers_mut().insert(
        "cache-control",
        HeaderValue::from_static("private, no-store"),
    );
    response.headers_mut().insert(
        "content-disposition",
        HeaderValue::from_static("attachment; filename=\"automatic-speech.tar\""),
    );
    Ok(response)
}

const LEGACY_TRANSCRIPT: &str = "NFC source word units, apostrophes normalized; original scalar ranges retained; raw timestamp classes without interpolation";

fn alias_policy() -> Result<Value, AppError> {
    serde_json::from_str(include_str!(
        "../../../scripts/alignment/transcript-aliases.json"
    ))
    .map_err(|_| AppError::Unavailable)
}

fn model_token(source: &str) -> String {
    source
        .nfc()
        .map(|c| match c {
            '’' | 'ʼ' | '‘' => '\'',
            _ => c,
        })
        .filter(|c| *c == '\'' || c.is_alphabetic() || c.is_numeric())
        .collect()
}

// The native model emits integer 80 ms classes. Check their exact class value,
// never interpolate, shift or silently replace the timestamps for an alias.
fn raw_class_ms(value: &Value) -> Result<u32, AppError> {
    let seconds = value.as_f64().ok_or(AppError::InvalidInput)?;
    let class = seconds * 1000.0 / 80.0;
    if !class.is_finite() || !(0.0..5000.0).contains(&class) || (class - class.round()).abs() > 1e-9
    {
        return Err(AppError::InvalidInput);
    }
    Ok(class.round() as u32 * 80)
}

fn check_aliases(
    report: &Value,
    clip: &Value,
    expected: &[AdminAlignmentWord],
    words: &[AdminAlignmentWord],
) -> Result<(), AppError> {
    let Some(aliases) = clip.get("transcriptAliases") else {
        return Ok(());
    };
    let policy = alias_policy()?;
    let aliases = aliases
        .as_array()
        .filter(|a| !a.is_empty())
        .ok_or(AppError::InvalidInput)?;
    if report["engine"]["transcript"] != policy["policy"] {
        return Err(AppError::InvalidInput);
    }
    let mut tokens: Vec<_> = expected.iter().map(|w| model_token(&w.text)).collect();
    let mut previous = None;
    for alias in aliases {
        let index = alias["wordIndex"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or(AppError::InvalidInput)?;
        let word = expected.get(index).ok_or(AppError::InvalidInput)?;
        if alias.as_object().is_none_or(|v| v.len() != 3)
            || previous.is_some_and(|p| p >= index)
            || alias["sourceText"] != word.text
            || policy["cardinals"].get(&word.text).is_none()
            || alias["modelToken"] != policy["cardinals"][&word.text]
        {
            return Err(AppError::InvalidInput);
        }
        tokens[index] = text(alias, "modelToken")?.to_owned();
        previous = Some(index);
    }
    let raw = clip["rawPredictions"]
        .as_array()
        .ok_or(AppError::InvalidInput)?;
    if raw.len() != expected.len() || words.len() != expected.len() {
        return Err(AppError::InvalidInput);
    }
    for ((prediction, token), word) in raw.iter().zip(tokens).zip(words) {
        if prediction.as_object().is_none_or(|v| v.len() != 3)
            || prediction["text"] != token
            || Some(raw_class_ms(&prediction["startSeconds"])?) != word.start_ms
            || Some(raw_class_ms(&prediction["endSeconds"])?) != word.end_ms
        {
            return Err(AppError::InvalidInput);
        }
    }
    Ok(())
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, AppError> {
    value[key].as_str().ok_or(AppError::InvalidInput)
}
fn check_report(report: &Value, request: &AdminSpeechPackageRequest) -> Result<(), AppError> {
    crate::speech_package::settings(request)?;
    let model: Value = serde_json::from_str(include_str!("../../../scripts/alignment/model.json"))
        .map_err(|_| AppError::Unavailable)?;
    let versions: Value =
        serde_json::from_str(include_str!("../../../scripts/alignment/runtime.json"))
            .map_err(|_| AppError::Unavailable)?;
    let engine = &report["engine"];
    let aliases = alias_policy()?;
    if serde_json::to_vec(report)
        .map_err(|_| AppError::InvalidInput)?
        .len()
        > 4 * 1024 * 1024
        || report["schemaVersion"] != "1.0"
        || report["kind"] != "brioche-automatic-alignment-v1"
        || report["reviewRequired"] != false
        || report["humanListeningAsserted"] != false
        || hash(report)? != request.expected_report_hash
        || !crate::voice_references::hex(text(report, "planId")?, 32)
        || !crate::voice_references::hex(text(report, "sourceArchiveSha256")?, 64)
        || !crate::voice_references::hex(text(report, "originalPredictionReportSha256")?, 64)
        || engine.as_object().is_none_or(|v| v.len() != 8)
        || engine["repository"] != model["repository"]
        || engine["revision"] != model["revision"]
        || engine["files"] != model["files"]
        || engine["versions"] != versions
        || engine["device"] != "cpu"
        || engine["dtype"] != "float32"
        || engine["attention"] != "eager"
        || (engine["transcript"] != LEGACY_TRANSCRIPT && engine["transcript"] != aliases["policy"])
    {
        return Err(AppError::InvalidInput);
    }
    Ok(())
}
async fn snapshot(
    db: &impl ConnectionTrait,
    report: &Value,
    request: &AdminSpeechPackageRequest,
) -> Result<Value, AppError> {
    let input = crate::speech_export::snapshot_direct(db, text(report, "planId")?).await?;
    let plan = &input["plan"];
    if plan["planHash"] != report["planHash"] {
        return Err(AppError::Conflict);
    }
    let lesson = text(plan, "lessonId")?;
    let revision = plan["lessonRevision"]
        .as_u64()
        .and_then(|n| i32::try_from(n).ok())
        .ok_or(AppError::InvalidInput)?;
    let row = one(
        db,
        "SELECT server_document FROM lesson_revisions WHERE lesson_id=$1 AND revision=$2",
        vec![lesson.into(), revision.into()],
    )
    .await?
    .ok_or(AppError::NotFound)?;
    let source: Value = field(&row, "server_document")?;
    if hash(&source)? != plan["sourceHash"] {
        return Err(AppError::Conflict);
    }
    let latest = one(
        db,
        "SELECT MAX(revision) AS revision FROM lesson_revisions WHERE lesson_id=$1",
        vec![lesson.into()],
    )
    .await?
    .ok_or(AppError::Unavailable)?;
    if i64::from(request.lesson_revision) <= i64::from(field::<i32>(&latest, "revision")?) {
        return Err(AppError::Conflict);
    }
    Ok(json!({"input":input,"source":source}))
}
fn manifest(snapshot: &Value, report: &Value) -> Result<Value, AppError> {
    let input = &snapshot["input"];
    let originals = input["clips"].as_array().ok_or(AppError::Unavailable)?;
    let predictions = report["clips"].as_array().ok_or(AppError::InvalidInput)?;
    if predictions.len() != originals.len() || predictions.is_empty() || predictions.len() > 1000 {
        return Err(AppError::InvalidInput);
    }
    let mut seen = BTreeSet::new();
    let mut clips = Vec::new();
    for prediction in predictions {
        let key = text(prediction, "generationKey")?;
        if !seen.insert(key) || prediction["issues"] != json!([]) {
            return Err(AppError::InvalidInput);
        }
        let original = originals
            .iter()
            .find(|c| c["generationKey"] == key)
            .ok_or(AppError::Conflict)?;
        if original["id"] != prediction["clipId"]
            || original["result"]["sha256"] != prediction["sha256"]
            || original["result"]["durationMs"] != prediction["durationMs"]
        {
            return Err(AppError::Conflict);
        }
        let expected: Vec<AdminAlignmentWord> =
            serde_json::from_value(original["words"].clone()).map_err(|_| AppError::Unavailable)?;
        let words: Vec<AdminAlignmentWord> = serde_json::from_value(prediction["words"].clone())
            .map_err(|_| AppError::InvalidInput)?;
        let duration = prediction["durationMs"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or(AppError::InvalidInput)?;
        crate::speech_alignments::validate_words(&words, &expected, duration, true)?;
        check_aliases(report, prediction, &expected, &words)?;
        let targets = prediction["targets"]
            .as_array()
            .ok_or(AppError::InvalidInput)?;
        let expected_targets: Vec<_> = input["plan"]["targets"]
            .as_array()
            .ok_or(AppError::Unavailable)?
            .iter()
            .filter(|t| t["generationKey"] == key)
            .collect();
        if targets.len() != expected_targets.len() {
            return Err(AppError::InvalidInput);
        }
        let mut pointers = BTreeSet::new();
        for target in targets {
            let pointer = text(target, "pointer")?;
            if !pointers.insert(pointer) || target["issues"] != json!([]) {
                return Err(AppError::InvalidInput);
            }
            let expected = expected_targets
                .iter()
                .find(|t| t["pointer"] == pointer)
                .ok_or(AppError::InvalidInput)?;
            if expected["blockId"] != target["blockId"] || expected["entryId"] != target["entryId"]
            {
                return Err(AppError::InvalidInput);
            }
            let mut mapped = Vec::new();
            for unit in expected["words"].as_array().ok_or(AppError::Unavailable)? {
                let word = words
                    .iter()
                    .find(|w| {
                        json!(w.start) == unit["entryStart"]
                            && json!(w.end) == unit["entryEnd"]
                            && json!(w.text) == unit["text"]
                    })
                    .ok_or(AppError::InvalidInput)?;
                let mut value = unit.clone();
                value["startMs"] = json!(word.start_ms);
                value["endMs"] = json!(word.end_ms);
                mapped.push(value);
            }
            if json!(mapped) != target["words"] {
                return Err(AppError::InvalidInput);
            }
        }
        clips.push(json!({"id":original["id"],"generationKey":key,"result":original["result"],"words":words,"review":null,"speechReview":null}));
    }
    let report_hash = hash(report)?;
    Ok(
        json!({"schemaVersion":"1.0","kind":"brioche-automatic-speech-package","alignmentId":format!("automatic-{report_hash}"),"reportHash":report_hash,"predictionEngine":report["engine"],"planId":input["planId"],"plan":input["plan"],"source":snapshot["source"],"clips":clips,"automaticAlignment":report,"humanListeningAsserted":false}),
    )
}
/// Private archive only. Registration, version import and directory activation remain explicit.
pub async fn assemble_for_actor(
    b: &Backend,
    actor: i64,
    root: PathBuf,
    report: Value,
    request: AdminSpeechPackageRequest,
) -> Result<Vec<u8>, AppError> {
    let operator = crate::product_memberships::require_operator(
        &b.db,
        crate::product::ProductId::Brioche,
        actor,
    )
    .await?;
    assemble_authorized(
        &Store { db: b.db.clone() },
        &operator,
        root,
        report,
        request,
    )
    .await
}
async fn assemble_authorized(
    b: &Store,
    operator: &crate::product_memberships::Operator,
    root: PathBuf,
    report: Value,
    request: AdminSpeechPackageRequest,
) -> Result<Vec<u8>, AppError> {
    let actor = operator.actor;
    check_report(&report, &request)?;
    let tx =
        b.db.begin_with_config(Some(IsolationLevel::RepeatableRead), None)
            .await
            .map_err(|_| AppError::Unavailable)?;
    operator.lock_content(&tx).await?;
    let original = snapshot(&tx, &report, &request).await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    let expected = original.clone();
    let assembled_manifest = manifest(&original, &report)?;
    let config = request.clone();
    let archive_sha = text(&report, "sourceArchiveSha256")?.to_owned();
    let bytes = tokio::task::spawn_blocking(move || {
        let input = crate::speech_export::pack(&root, original["input"].clone())?;
        if crate::media::digest(&input) != archive_sha {
            return Err(AppError::Conflict);
        }
        crate::speech_package::pack_automatic(&root, assembled_manifest, &config, actor)
    })
    .await
    .map_err(|_| AppError::Unavailable)??;
    let tx = b.db.begin().await.map_err(|_| AppError::Unavailable)?;
    operator.lock_content(&tx).await?;
    exec(
        &tx,
        "SELECT singleton FROM content_state WHERE singleton FOR UPDATE",
        vec![],
    )
    .await?;
    if snapshot(&tx, &report, &request).await? != expected {
        return Err(AppError::Conflict);
    }
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(bytes)
}

#[cfg(test)]
mod alias_tests {
    use super::*;

    #[test]
    fn cardinal_alias_keeps_exact_native_classes_and_source_binding() {
        let expected = vec![AdminAlignmentWord {
            text: "20".into(),
            start: 3,
            end: 5,
            start_ms: None,
            end_ms: None,
        }];
        let words = vec![AdminAlignmentWord {
            text: "20".into(),
            start: 3,
            end: 5,
            start_ms: Some(80),
            end_ms: Some(240),
        }];
        let report = json!({"engine":{"transcript":alias_policy().unwrap()["policy"]}});
        let clip = json!({"transcriptAliases":[{"wordIndex":0,"sourceText":"20","modelToken":"vingt"}],"rawPredictions":[{"text":"vingt","startSeconds":0.08,"endSeconds":0.24}]});
        check_aliases(&report, &clip, &expected, &words).unwrap();
        for case in 0..7 {
            let mut bad = clip.clone();
            match case {
                0 => bad["transcriptAliases"][0]["modelToken"] = json!("trente"),
                1 => bad["transcriptAliases"][0]["sourceText"] = json!("30"),
                2 => bad["transcriptAliases"][0]["wordIndex"] = json!(1),
                3 => bad["rawPredictions"][0]["text"] = json!("20"),
                4 => bad["rawPredictions"][0]["endSeconds"] = json!(0.32),
                5 => bad["rawPredictions"][0]["startSeconds"] = json!(0.081),
                _ => bad["transcriptAliases"]
                    .as_array_mut()
                    .unwrap()
                    .push(clip["transcriptAliases"][0].clone()),
            }
            assert!(
                check_aliases(&report, &bad, &expected, &words).is_err(),
                "case {case}"
            );
        }
        assert!(
            check_aliases(
                &json!({"engine":{"transcript":LEGACY_TRANSCRIPT}}),
                &clip,
                &expected,
                &words
            )
            .is_err()
        );
    }
}
