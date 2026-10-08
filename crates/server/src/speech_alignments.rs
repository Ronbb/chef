//! Private model predictions and explicit human timing decisions. Never audio publication.
use crate::{
    AppError,
    admin_auth::AdminAuth,
    identity::Backend,
    learning::{exec, field, one, product_filter},
    speech_clips,
    voice_references::hex,
};
use axum::{
    Extension, Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    routing::{get, post},
};
use brioche_course_contract::{
    AdminAlignment, AdminAlignmentClip, AdminAlignmentImport, AdminAlignmentReview,
    AdminAlignmentSummary, AdminAlignmentWord, AdminAlignments,
};
use sea_orm::{ConnectionTrait, DbBackend, QueryResult, Statement, TransactionTrait};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::PathBuf, sync::Arc};
use unicode_segmentation::UnicodeSegmentation;

const MAX_REPORT: usize = 4 * 1024 * 1024;
#[derive(Clone)]
struct Store {
    product: Option<crate::product::ProductId>,
    db: sea_orm::DatabaseConnection,
}
pub(crate) fn router<S: Clone + Send + Sync + 'static>(
    db: sea_orm::DatabaseConnection,
    product: Option<crate::product::ProductId>,
) -> Router<S> {
    Router::new()
        .route(
            "/api/v1/operator/speech-alignments",
            post(import).layer(DefaultBodyLimit::max(8 * 1024 * 1024)),
        )
        .route("/api/v1/operator/speech-plans/{id}/alignments", get(list))
        .route("/api/v1/operator/speech-alignments/{id}", get(read))
        .route(
            "/api/v1/operator/speech-alignments/{id}/clips/{clip}/review",
            post(review),
        )
        .with_state(Store { db, product })
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Report {
    schema_version: String,
    kind: String,
    plan_id: String,
    plan_hash: String,
    source_archive_sha256: String,
    engine: Value,
    review_required: bool,
    clips: Vec<Clip>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Clip {
    clip_id: String,
    generation_key: String,
    sha256: String,
    duration_ms: u32,
    raw_predictions: Vec<Prediction>,
    words: Vec<AdminAlignmentWord>,
    issues: Vec<String>,
    targets: Vec<Target>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Prediction {
    text: String,
    start_seconds: Value,
    end_seconds: Value,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Target {
    pointer: String,
    block_id: Option<String>,
    entry_id: String,
    words: Vec<Value>,
    issues: Vec<String>,
}
fn report(value: Value) -> Result<Report, AppError> {
    let report: Report = serde_json::from_value(value).map_err(|_| AppError::InvalidInput)?;
    let model: Value = serde_json::from_str(include_str!("../../../scripts/alignment/model.json"))
        .map_err(|_| AppError::Unavailable)?;
    let runtime: Value =
        serde_json::from_str(include_str!("../../../scripts/alignment/runtime.json"))
            .map_err(|_| AppError::Unavailable)?;
    if report.schema_version != "1.0"
        || report.kind != "brioche-alignment-predictions"
        || !report.review_required
        || !hex(&report.plan_id, 32)
        || !hex(&report.plan_hash, 64)
        || !hex(&report.source_archive_sha256, 64)
        || report.clips.is_empty()
        || report.clips.len() > 1000
        || report.engine.as_object().is_none_or(|v| v.len() != 8)
        || report.engine["repository"] != model["repository"]
        || report.engine["revision"] != model["revision"]
        || report.engine["files"] != model["files"]
        || report.engine["versions"] != runtime
        || report.engine["device"] != "cpu"
        || report.engine["dtype"] != "float32"
        || report.engine["attention"] != "eager"
        || report.engine["transcript"]
            != "NFC source word units, apostrophes normalized; original scalar ranges retained; raw timestamp classes without interpolation"
    {
        return Err(AppError::InvalidInput);
    }
    let mut ids = BTreeSet::new();
    let mut keys = BTreeSet::new();
    for clip in &report.clips {
        if !hex(&clip.clip_id, 32)
            || !hex(&clip.generation_key, 64)
            || !hex(&clip.sha256, 64)
            || !ids.insert(&clip.clip_id)
            || !keys.insert(&clip.generation_key)
            || clip.duration_ms == 0
            || clip.duration_ms > 180000
            || clip.raw_predictions.len() > 600
            || clip.words.len() > 600
            || clip.targets.is_empty()
            || clip.targets.len() > 1000
        {
            return Err(AppError::InvalidInput);
        }
        issue_labels(&clip.issues)?;
        for prediction in &clip.raw_predictions {
            if prediction.text.len() > 2400 || prediction.text.chars().any(char::is_control) {
                return Err(AppError::InvalidInput);
            }
            for time in [&prediction.start_seconds, &prediction.end_seconds] {
                if !time.is_number()
                    && time
                        .as_str()
                        .is_none_or(|s| s.len() > 32 || s.chars().any(char::is_control))
                {
                    return Err(AppError::InvalidInput);
                }
            }
        }
        for target in &clip.targets {
            issue_labels(&target.issues)?;
            if target.words.len() > 600 {
                return Err(AppError::InvalidInput);
            }
        }
    }
    Ok(report)
}
fn issue_labels(labels: &[String]) -> Result<(), AppError> {
    if labels.len() > 20
        || labels
            .iter()
            .any(|s| s.is_empty() || s.len() > 80 || !s.bytes().all(|b| b.is_ascii_alphanumeric()))
    {
        return Err(AppError::InvalidInput);
    }
    Ok(())
}
fn source_words(text: &str) -> Vec<AdminAlignmentWord> {
    text.unicode_word_indices()
        .map(|(byte, word)| {
            let start = text[..byte].chars().count() as u32;
            AdminAlignmentWord {
                text: word.into(),
                start,
                end: start + word.chars().count() as u32,
                start_ms: None,
                end_ms: None,
            }
        })
        .collect()
}
pub(crate) fn validate_words(
    words: &[AdminAlignmentWord],
    expected: &[AdminAlignmentWord],
    duration: u32,
    require_times: bool,
) -> Result<(), AppError> {
    if words.len() != expected.len() {
        return Err(AppError::InvalidInput);
    }
    let mut previous = 0;
    for (w, e) in words.iter().zip(expected) {
        if w.text != e.text || w.start != e.start || w.end != e.end {
            return Err(AppError::InvalidInput);
        }
        match (w.start_ms, w.end_ms) {
            (Some(start), Some(end)) if previous <= start && start < end && end <= duration => {
                previous = end
            }
            (None, None) if !require_times => {}
            _ => return Err(AppError::InvalidInput),
        }
    }
    Ok(())
}
fn request_text<'a>(plan: &'a Value, key: &str) -> Result<&'a str, AppError> {
    plan["requests"][key]["parameters"]["input"]["text"]
        .as_str()
        .ok_or(AppError::Unavailable)
}
async fn sources(
    db: &impl ConnectionTrait,
    product: Option<crate::product::ProductId>,
    report: &Report,
) -> Result<(Value, Vec<Value>), AppError> {
    let plan = speech_clips::plan_for_product(db, product, &report.plan_id).await?;
    if plan["planHash"] != report.plan_hash {
        return Err(AppError::Conflict);
    }
    let requests = plan["requests"].as_object().ok_or(AppError::Unavailable)?;
    if requests.len() != report.clips.len()
        || report
            .clips
            .iter()
            .any(|c| !requests.contains_key(&c.generation_key))
    {
        return Err(AppError::InvalidInput);
    }
    let mut results = Vec::new();
    for clip in &report.clips {
        let row = speech_clips::latest_for_product(db, product, &clip.generation_key)
            .await?
            .ok_or(AppError::Conflict)?;
        let latest = speech_clips::item(&row)?;
        let result: Value = field::<Option<Value>>(&row, "result")?.ok_or(AppError::Conflict)?;
        if latest.id != clip.clip_id
            || latest.status != "ready"
            || latest.accepted != Some(true)
            || result["sha256"] != clip.sha256
            || result["durationMs"] != clip.duration_ms
        {
            return Err(AppError::Conflict);
        }
        let expected = source_words(request_text(&plan, &clip.generation_key)?);
        if !clip.words.is_empty() {
            validate_words(&clip.words, &expected, clip.duration_ms, true)?;
        }
        let original: Vec<_> = plan["targets"]
            .as_array()
            .ok_or(AppError::Unavailable)?
            .iter()
            .filter(|t| t["generationKey"] == clip.generation_key)
            .collect();
        if original.len() != clip.targets.len() {
            return Err(AppError::InvalidInput);
        }
        let mut pointers = BTreeSet::new();
        for t in &clip.targets {
            if !pointers.insert(&t.pointer)
                || !original.iter().any(|o| {
                    o["pointer"] == t.pointer
                        && o["blockId"] == json!(t.block_id)
                        && o["entryId"] == t.entry_id
                })
            {
                return Err(AppError::InvalidInput);
            }
        }
        results.push(result);
    }
    Ok((plan, results))
}
async fn media(root: PathBuf, results: Vec<Value>) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || {
        let mut seen = BTreeSet::new();
        let mut total = 0usize;
        for result in results {
            let (sha, bytes) = crate::speech_media::read(&root, &result)?;
            if seen.insert(sha) {
                total = total
                    .checked_add(bytes.len())
                    .ok_or(AppError::InvalidInput)?;
            }
            if total > 128 * 1024 * 1024 {
                return Err(AppError::InvalidInput);
            }
        }
        Ok(())
    })
    .await
    .map_err(|_| AppError::Unavailable)?
}
const SELECT: &str = "SELECT a.id,a.plan_id,a.report,a.report_hash,to_char(a.created_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS created_at FROM speech_alignments a";
async fn load(
    db: &impl ConnectionTrait,
    product: Option<crate::product::ProductId>,
    id: &str,
) -> Result<QueryResult, AppError> {
    if !hex(id, 32) {
        return Err(AppError::InvalidInput);
    }
    one(
        db,
        &format!(
            "{SELECT} WHERE a.id=$1{}",
            product_filter(product, "a.product_id")
        ),
        vec![id.into()],
    )
    .await?
    .ok_or(AppError::NotFound)
}
pub(crate) async fn package_snapshot_for_product(
    db: &impl ConnectionTrait,
    product: Option<crate::product::ProductId>,
    id: &str,
    expected_hash: &str,
) -> Result<Value, AppError> {
    let row = load(db, product, id).await?;
    if field::<String>(&row, "report_hash")? != expected_hash {
        return Err(AppError::Conflict);
    }
    let report: Report =
        serde_json::from_value(field(&row, "report")?).map_err(|_| AppError::Unavailable)?;
    let (plan, results) = sources(db, product, &report).await?;
    let speech = crate::speech_export::snapshot_for_product(db, product, &report.plan_id).await?;
    let mut clips = Vec::new();
    for (clip, result) in report.clips.iter().zip(results) {
        let review = one(db,
            &format!("SELECT accepted,words,actor_id,reason,request FROM speech_alignment_reviews WHERE alignment_id=$1 AND clip_id=$2{}", product_filter(product, "product_id")),
            vec![id.into(), clip.clip_id.clone().into()]).await?.ok_or(AppError::Conflict)?;
        if !field::<bool>(&review, "accepted")? {
            return Err(AppError::Conflict);
        }
        let words: Vec<AdminAlignmentWord> =
            serde_json::from_value(field(&review, "words")?).map_err(|_| AppError::Unavailable)?;
        validate_words(
            &words,
            &source_words(request_text(&plan, &clip.generation_key)?),
            clip.duration_ms,
            true,
        )?;
        let audio_review = speech["clips"]
            .as_array()
            .ok_or(AppError::Unavailable)?
            .iter()
            .find(|c| c["id"] == clip.clip_id)
            .ok_or(AppError::Conflict)?;
        clips.push(json!({"id":clip.clip_id,"generationKey":clip.generation_key,"result":result,"speechReview":audio_review["review"],
            "words":words,"review":{"actorId":field::<i64>(&review,"actor_id")?,
            "reason":field::<String>(&review,"reason")?,"request":field::<Value>(&review,"request")?}}));
    }
    Ok(
        json!({"schemaVersion":"1.0","kind":"brioche-speech-package","alignmentId":id,
        "reportHash":expected_hash,"predictionEngine":report.engine,"planId":report.plan_id,
        "plan":plan,"clips":clips}),
    )
}
async fn view(
    db: &impl ConnectionTrait,
    product: Option<crate::product::ProductId>,
    row: &QueryResult,
) -> Result<AdminAlignment, AppError> {
    let id: String = field(row, "id")?;
    let report: Report =
        serde_json::from_value(field(row, "report")?).map_err(|_| AppError::Unavailable)?;
    let (plan, _) = sources(db, product, &report).await?;
    let mut clips = Vec::new();
    for c in report.clips {
        let text = request_text(&plan, &c.generation_key)?.to_owned();
        let mut words = if c.words.is_empty() {
            source_words(&text)
        } else {
            c.words
        };
        let mut issues = c.issues;
        issues.extend(c.targets.iter().flat_map(|t| t.issues.clone()));
        if words.iter().any(|w| w.start_ms.is_none()) {
            issues.push("missingWordTimes".into());
        }
        let reviewed=one(db,&format!("SELECT accepted,words FROM speech_alignment_reviews WHERE alignment_id=$1 AND clip_id=$2{}", product_filter(product,"product_id")),vec![id.clone().into(),c.clip_id.clone().into()]).await?;
        let accepted = if let Some(row) = reviewed {
            let accepted: bool = field(&row, "accepted")?;
            if accepted {
                words = serde_json::from_value(field(&row, "words")?)
                    .map_err(|_| AppError::Unavailable)?;
            }
            Some(accepted)
        } else {
            None
        };
        issues.sort();
        issues.dedup();
        clips.push(AdminAlignmentClip {
            clip_id: c.clip_id,
            generation_key: c.generation_key,
            text,
            duration_ms: c.duration_ms,
            issues,
            words,
            accepted,
        });
    }
    Ok(AdminAlignment {
        id,
        plan_id: report.plan_id,
        plan_hash: report.plan_hash,
        report_hash: field(row, "report_hash")?,
        clips,
        created_at: field(row, "created_at")?,
    })
}
async fn import(
    auth: AdminAuth,
    State(b): State<Store>,
    Extension(root): Extension<PathBuf>,
    Extension(permits): Extension<Arc<tokio::sync::Semaphore>>,
    Json(request): Json<AdminAlignmentImport>,
) -> Result<Json<AdminAlignment>, AppError> {
    let operator = auth.require_operator().await?;
    Ok(Json(
        import_for_actor(&b, &operator, root, permits, request).await?,
    ))
}

/// Import real predictions without declaring human timing acceptance.
pub async fn import_local(
    b: &Backend,
    actor: i64,
    root: PathBuf,
    mut request: AdminAlignmentImport,
) -> Result<AdminAlignment, AppError> {
    request.reason = format!("[local-cli] {}", request.reason);
    let operator = crate::product_memberships::require_operator(
        &b.db,
        crate::product::ProductId::Brioche,
        actor,
    )
    .await?;
    import_for_actor(
        &Store {
            db: b.db.clone(),
            product: None,
        },
        &operator,
        root,
        Arc::new(tokio::sync::Semaphore::new(2)),
        request,
    )
    .await
}

async fn import_for_actor(
    b: &Store,
    operator: &crate::product_memberships::Operator,
    root: PathBuf,
    permits: Arc<tokio::sync::Semaphore>,
    request: AdminAlignmentImport,
) -> Result<AdminAlignment, AppError> {
    if b.product.is_some_and(|product| product != operator.product) {
        return Err(AppError::Forbidden);
    }
    let actor = operator.actor;
    crate::admin::reason(&request.reason)?;
    if !hex(&request.id, 32) || !hex(&request.plan_id, 32) || !hex(&request.expected_plan_hash, 64)
    {
        return Err(AppError::InvalidInput);
    }
    let value =
        crate::author_json::parse_document_bounded(request.report_json.as_bytes(), MAX_REPORT)
            .map_err(|_| AppError::InvalidInput)?;
    let report = report(value.clone())?;
    if report.plan_id != request.plan_id || report.plan_hash != request.expected_plan_hash {
        return Err(AppError::Conflict);
    }
    let report_hash =
        crate::media::digest(&serde_json::to_vec(&value).map_err(|_| AppError::InvalidInput)?);
    let request_json = serde_json::to_value(&request).map_err(|_| AppError::InvalidInput)?;
    let tx = b.db.begin().await.map_err(|_| AppError::Unavailable)?;
    operator.lock_content(&tx).await?;
    exec(
        &tx,
        &format!(
            "SELECT generation FROM content_state WHERE {} FOR UPDATE",
            b.product.map_or_else(
                || "singleton".to_owned(),
                |p| format!("product_id='{}'", p.as_str())
            )
        ),
        vec![],
    )
    .await?;
    // Only a completed local key allows equal IDs in separate products.
    let local_ids = b.product.is_some()
        && crate::product_keys::supports_local_ids(
            &tx,
            crate::product_keys::LocalIdTable::Alignment,
        )
        .await?;
    if let Some(product) = b.product.filter(|_| !local_ids)
        && one(
            &tx,
            "SELECT 1 FROM speech_alignments WHERE id=$1 AND product_id<>$2",
            vec![request.id.clone().into(), product.as_str().into()],
        )
        .await?
        .is_some()
    {
        return Err(AppError::NotFound);
    }
    if let Some(row) = one(
        &tx,
        &format!(
            "SELECT actor_id,request FROM speech_alignments WHERE id=$1{}",
            product_filter(b.product, "product_id")
        ),
        vec![request.id.clone().into()],
    )
    .await?
    {
        if field::<i64>(&row, "actor_id")? != actor
            || field::<Value>(&row, "request")? != request_json
        {
            return Err(AppError::Conflict);
        }
        return view(&tx, b.product, &load(&tx, b.product, &request.id).await?).await;
    }
    let _permit = permits
        .try_acquire_owned()
        .map_err(|_| AppError::RateLimited)?;
    sources(&tx, b.product, &report).await?;
    let manifest =
        crate::speech_export::snapshot_for_product(&tx, b.product, &report.plan_id).await?;
    let expected = report.source_archive_sha256.clone();
    tokio::task::spawn_blocking(move || {
        let bytes = crate::speech_export::pack(&root, manifest)?;
        if crate::media::digest(&bytes) != expected {
            return Err(AppError::Conflict);
        }
        Ok(())
    })
    .await
    .map_err(|_| AppError::Unavailable)??;
    let mut values = vec![
        request.id.clone().into(),
        request.plan_id.into(),
        request_json.into(),
        value.into(),
        report_hash.into(),
        actor.into(),
        request.reason.into(),
    ];
    let sql = if let Some(product) = b.product {
        values.push(product.as_str().into());
        "INSERT INTO speech_alignments(id,plan_id,request,report,report_hash,actor_id,reason,product_id)VALUES($1,$2,$3,$4,$5,$6,$7,$8)"
    } else {
        "INSERT INTO speech_alignments(id,plan_id,request,report,report_hash,actor_id,reason)VALUES($1,$2,$3,$4,$5,$6,$7)"
    };
    exec(&tx, sql, values).await?;
    let view = view(&tx, b.product, &load(&tx, b.product, &request.id).await?).await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(view)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemQuery {}
async fn read(
    auth: AdminAuth,
    State(b): State<Store>,
    Path(id): Path<String>,
    Query(_query): Query<ItemQuery>,
) -> Result<Json<AdminAlignment>, AppError> {
    auth.require_operator().await?;
    Ok(Json(
        view(&b.db, b.product, &load(&b.db, b.product, &id).await?).await?,
    ))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Cursor {
    after: Option<String>,
}
async fn list(
    auth: AdminAuth,
    State(b): State<Store>,
    Path(id): Path<String>,
    Query(query): Query<Cursor>,
) -> Result<Json<AdminAlignments>, AppError> {
    auth.require_operator().await?;
    speech_clips::plan_for_product(&b.db, b.product, &id).await?;
    let after = query.after.unwrap_or_default();
    if !after.is_empty() && !hex(&after, 32) {
        return Err(AppError::InvalidInput);
    }
    let rows=b.db.query_all_raw(Statement::from_sql_and_values(DbBackend::Postgres,
        format!("SELECT a.id,a.plan_id,a.report->>'planHash' AS plan_hash,a.report_hash,to_char(a.created_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS created_at,jsonb_array_length(a.report->'clips')::integer AS clip_count,(SELECT count(*)::integer FROM speech_alignment_reviews r WHERE r.alignment_id=a.id AND r.accepted{}) AS accepted_count FROM speech_alignments a WHERE a.plan_id=$1 AND a.id>$2{} ORDER BY a.id LIMIT 21", if b.product.is_some() {" AND r.product_id=a.product_id"} else {""}, product_filter(b.product,"a.product_id")),
        vec![id.into(),after.into()])).await.map_err(|_|AppError::Unavailable)?;
    let mut items = Vec::new();
    for row in rows.iter().take(20) {
        items.push(AdminAlignmentSummary {
            id: field(row, "id")?,
            plan_id: field(row, "plan_id")?,
            plan_hash: field(row, "plan_hash")?,
            report_hash: field(row, "report_hash")?,
            clip_count: field::<i32>(row, "clip_count")? as u32,
            accepted_count: field::<i32>(row, "accepted_count")? as u32,
            created_at: field(row, "created_at")?,
        });
    }
    let next = if rows.len() > 20 {
        items.last().map(|i| i.id.clone())
    } else {
        None
    };
    Ok(Json(AdminAlignments { items, next }))
}
async fn review(
    auth: AdminAuth,
    State(b): State<Store>,
    Path((id, clip_id)): Path<(String, String)>,
    Extension(root): Extension<PathBuf>,
    Extension(permits): Extension<Arc<tokio::sync::Semaphore>>,
    Json(request): Json<AdminAlignmentReview>,
) -> Result<Json<AdminAlignment>, AppError> {
    let operator = auth.require_operator().await?;
    crate::admin::reason(&request.reason)?;
    if !hex(&id, 32)
        || !hex(&clip_id, 32)
        || !hex(&request.expected_report_hash, 64)
        || !request.heard
        || (request.accepted && !request.timings_checked)
        || request.words.len() > 600
    {
        return Err(AppError::InvalidInput);
    }
    if b.product.is_some_and(|product| product != operator.product) {
        return Err(AppError::Forbidden);
    }
    let actor = operator.actor;
    let tx = b.db.begin().await.map_err(|_| AppError::Unavailable)?;
    operator.lock_content(&tx).await?;
    exec(
        &tx,
        &format!(
            "SELECT generation FROM content_state WHERE {} FOR UPDATE",
            b.product.map_or_else(
                || "singleton".to_owned(),
                |p| format!("product_id='{}'", p.as_str())
            )
        ),
        vec![],
    )
    .await?;
    let row = load(&tx, b.product, &id).await?;
    if field::<String>(&row, "report_hash")? != request.expected_report_hash {
        return Err(AppError::Conflict);
    }
    let report: Report =
        serde_json::from_value(field(&row, "report")?).map_err(|_| AppError::Unavailable)?;
    let clip = report
        .clips
        .iter()
        .find(|c| c.clip_id == clip_id)
        .ok_or(AppError::InvalidInput)?;
    let (plan, results) = sources(&tx, b.product, &report).await?;
    let expected = source_words(request_text(&plan, &clip.generation_key)?);
    if request.accepted || !request.words.is_empty() {
        validate_words(
            &request.words,
            &expected,
            clip.duration_ms,
            request.accepted,
        )?;
    }
    if request.accepted {
        // Segment words must map exactly; a whole-token prediction cannot be split by averaging.
        let ranges: BTreeSet<_> = request
            .words
            .iter()
            .map(|w| (w.start, w.end, w.text.as_str()))
            .collect();
        for target in plan["targets"]
            .as_array()
            .ok_or(AppError::Unavailable)?
            .iter()
            .filter(|t| t["generationKey"] == clip.generation_key)
        {
            for word in target["words"].as_array().ok_or(AppError::Unavailable)? {
                let range = (
                    word["entryStart"].as_u64().ok_or(AppError::Unavailable)? as u32,
                    word["entryEnd"].as_u64().ok_or(AppError::Unavailable)? as u32,
                    word["text"].as_str().ok_or(AppError::Unavailable)?,
                );
                if !ranges.contains(&range) {
                    return Err(AppError::Conflict);
                }
            }
        }
    }
    let request_json = serde_json::to_value(&request).map_err(|_| AppError::InvalidInput)?;
    if let Some(existing)=one(&tx,&format!("SELECT actor_id,request FROM speech_alignment_reviews WHERE alignment_id=$1 AND clip_id=$2{}", product_filter(b.product,"product_id")),vec![id.clone().into(),clip_id.clone().into()]).await? {
        if field::<i64>(&existing,"actor_id")?!=actor || field::<Value>(&existing,"request")?!=request_json {return Err(AppError::Conflict);}
        return Ok(Json(view(&tx,b.product,&row).await?));
    }
    let _permit = permits
        .try_acquire_owned()
        .map_err(|_| AppError::RateLimited)?;
    let index = report
        .clips
        .iter()
        .position(|c| c.clip_id == clip_id)
        .ok_or(AppError::InvalidInput)?;
    media(root, vec![results[index].clone()]).await?;
    let mut values = vec![
        id.into(),
        clip_id.into(),
        request.accepted.into(),
        serde_json::to_value(request.words)
            .map_err(|_| AppError::InvalidInput)?
            .into(),
        request_json.into(),
        actor.into(),
        request.reason.into(),
    ];
    let sql = if let Some(product) = b.product {
        values.push(product.as_str().into());
        "INSERT INTO speech_alignment_reviews(alignment_id,clip_id,accepted,words,request,actor_id,reason,product_id)VALUES($1,$2,$3,$4,$5,$6,$7,$8)"
    } else {
        "INSERT INTO speech_alignment_reviews(alignment_id,clip_id,accepted,words,request,actor_id,reason)VALUES($1,$2,$3,$4,$5,$6,$7)"
    };
    exec(&tx, sql, values).await?;
    let result = view(&tx, b.product, &row).await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(Json(result))
}
