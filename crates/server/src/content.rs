//! Immutable directory releases; operations are local operator CLI transactions.
use crate::{
    AppError,
    grading::Grader,
    learning::{exec, field, hash, one, product_filter},
    project_source,
};
use brioche_course_contract::{Catalog, LessonSummary, Level, PublicLesson, Unit};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement, TransactionTrait};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

fn search_text(value: &str) -> String {
    value
        .nfkd()
        .filter(|c| !is_combining_mark(*c))
        .collect::<String>()
        .replace('’', "'")
        .to_lowercase()
}
pub fn search_terms(query: &str) -> Result<Vec<String>, AppError> {
    if query.chars().count() > 120 || query.chars().any(|c| c.is_control() && !c.is_whitespace()) {
        return Err(AppError::InvalidInput);
    }
    Ok(search_text(query)
        .split_whitespace()
        .map(str::to_owned)
        .collect())
}
pub fn search_catalog(catalog: Catalog, terms: &[String]) -> Catalog {
    search_catalog_with_vocabulary(catalog, terms, &BTreeMap::new())
}
pub fn vocabulary_search_text(lesson: &PublicLesson) -> String {
    lesson
        .knowledge
        .vocabulary
        .iter()
        .map(|word| format!("{} {}", word.lemma, word.meaning_zh))
        .collect::<Vec<_>>()
        .join(" ")
}
pub fn search_catalog_with_vocabulary(
    mut catalog: Catalog,
    terms: &[String],
    vocabulary: &BTreeMap<String, String>,
) -> Catalog {
    if terms.is_empty() {
        return catalog;
    }
    for level in &mut catalog.levels {
        for unit in &mut level.units {
            unit.lessons.retain(|lesson| {
                let text = search_text(&format!(
                    "{} {} {} {} {} {}",
                    level.label,
                    unit.title_zh,
                    lesson.title.zh,
                    lesson.title.fr,
                    lesson.summary_zh,
                    vocabulary.get(&lesson.id).map(String::as_str).unwrap_or("")
                ));
                terms.iter().all(|term| text.contains(term))
            });
        }
        level.units.retain(|unit| !unit.lessons.is_empty());
    }
    catalog.levels.retain(|level| !level.units.is_empty());
    catalog
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseManifest {
    pub id: String,
    pub schema_version: String,
    pub levels: Vec<ReleaseLevel>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseLevel {
    pub id: String,
    pub label: String,
    pub units: Vec<ReleaseUnit>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseUnit {
    pub id: String,
    pub title_zh: String,
    pub lessons: Vec<RevisionRef>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevisionRef {
    pub lesson_id: String,
    pub revision: u32,
}
fn identifier(value: &str) -> bool {
    brioche_course_contract::valid_content_id(value)
}
fn text(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 1000 && !value.chars().any(char::is_control)
}

impl ReleaseManifest {
    pub fn deserialize_file(path: &str) -> anyhow::Result<Self> {
        crate::author_json::load(path)
    }
    pub fn validate(&self) -> Result<(), AppError> {
        self.validate_author().map_err(|_| AppError::InvalidInput)
    }
    /// Local author diagnostics; public callers retain the opaque AppError.
    pub fn validate_author(&self) -> anyhow::Result<()> {
        use anyhow::ensure;
        ensure!(identifier(&self.id), "/id: invalid release ID");
        ensure!(
            self.schema_version == "1.0",
            "/schemaVersion: unsupported schema version"
        );
        ensure!(
            self.levels.len() <= 20,
            "/levels: at most 20 levels are allowed"
        );
        let (mut levels, mut units, mut lessons) =
            (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
        for (li, level) in self.levels.iter().enumerate() {
            let level_path = format!("/levels/{li}");
            ensure!(
                identifier(&level.id) && levels.insert(&level.id),
                "{level_path}/id: invalid or duplicate level ID"
            );
            ensure!(
                text(&level.label),
                "{level_path}/label: expected nonempty label without control characters, at most 1000 bytes"
            );
            ensure!(
                !level.units.is_empty(),
                "{level_path}/units: level must contain a unit"
            );
            for (ui, unit) in level.units.iter().enumerate() {
                let unit_path = format!("{level_path}/units/{ui}");
                ensure!(
                    identifier(&unit.id) && units.insert(&unit.id),
                    "{unit_path}/id: invalid or duplicate unit ID"
                );
                ensure!(
                    text(&unit.title_zh),
                    "{unit_path}/titleZh: expected nonempty title without control characters, at most 1000 bytes"
                );
                ensure!(
                    !unit.lessons.is_empty(),
                    "{unit_path}/lessons: unit must contain a lesson"
                );
                for (ri, lesson) in unit.lessons.iter().enumerate() {
                    let lesson_path = format!("{unit_path}/lessons/{ri}");
                    ensure!(
                        identifier(&lesson.lesson_id) && lessons.insert(&lesson.lesson_id),
                        "{lesson_path}/lessonId: invalid or duplicate lesson reference"
                    );
                    ensure!(
                        brioche_course_contract::valid_content_revision(lesson.revision),
                        "{lesson_path}/revision: expected positive database-compatible revision"
                    );
                    ensure!(
                        lessons.len() <= 5000,
                        "{unit_path}/lessons: at most 5000 lesson references are allowed"
                    );
                }
            }
        }
        Ok(())
    }
}
enum ContentActor<'a> {
    Local(&'a str),
    Operator(&'a crate::product_memberships::Operator),
}
impl ContentActor<'_> {
    fn audit_actor(&self) -> String {
        match self {
            Self::Local(actor) => (*actor).to_owned(),
            Self::Operator(operator) => operator.audit_actor(),
        }
    }
    async fn lock(&self, tx: &sea_orm::DatabaseTransaction) -> Result<(), AppError> {
        match self {
            Self::Local(_) => Ok(()),
            Self::Operator(operator) => operator.lock_content(tx).await,
        }
    }
}
pub(crate) async fn stage_operator(
    db: &DatabaseConnection,
    manifest: &ReleaseManifest,
    operator: &crate::product_memberships::Operator,
    reason: &str,
    root: &std::path::Path,
) -> Result<(), AppError> {
    stage_impl(db, manifest, ContentActor::Operator(operator), reason, root)
        .await
        .map_err(|error| error.runtime)
}
pub(crate) async fn activate_operator(
    db: &DatabaseConnection,
    product: Option<crate::product::ProductId>,
    id: &str,
    expected: i64,
    operator: &crate::product_memberships::Operator,
    reason: &str,
    root: &std::path::Path,
) -> Result<i64, AppError> {
    if product.is_some_and(|product| product != operator.product) {
        return Err(AppError::Forbidden);
    }
    activate_impl(
        db,
        product,
        id,
        expected,
        ContentActor::Operator(operator),
        reason,
        root,
    )
    .await
    .map_err(|error| error.runtime)
}
pub(crate) async fn withdraw_operator(
    db: &DatabaseConnection,
    product: Option<crate::product::ProductId>,
    id: &str,
    revision: u32,
    expected: i64,
    operator: &crate::product_memberships::Operator,
    reason: &str,
) -> Result<i64, AppError> {
    if product.is_some_and(|product| product != operator.product) {
        return Err(AppError::Forbidden);
    }
    withdraw_impl(
        db,
        product,
        id,
        revision,
        expected,
        ContentActor::Operator(operator),
        reason,
    )
    .await
    .map_err(|error| error.runtime)
}
pub async fn stage(
    db: &DatabaseConnection,
    manifest: &ReleaseManifest,
    actor: &str,
    reason: &str,
    media_root: &std::path::Path,
) -> Result<(), AppError> {
    stage_impl(db, manifest, ContentActor::Local(actor), reason, media_root)
        .await
        .map_err(|error| error.runtime)
}

/// Local CLI diagnostics use the same transaction and publication gates as stage.
pub async fn stage_author(
    db: &DatabaseConnection,
    manifest: &ReleaseManifest,
    actor: &str,
    reason: &str,
    media_root: &std::path::Path,
) -> anyhow::Result<()> {
    stage_impl(db, manifest, ContentActor::Local(actor), reason, media_root)
        .await
        .map_err(|error| {
            anyhow::anyhow!(error.diagnostic.unwrap_or_else(|| {
                "/: staging database operation failed; verify release status before retrying".into()
            }))
        })
}

struct ReleaseFailure {
    runtime: AppError,
    diagnostic: Option<String>,
    pointer: Option<String>,
}
impl From<AppError> for ReleaseFailure {
    fn from(runtime: AppError) -> Self {
        Self {
            runtime,
            diagnostic: None,
            pointer: None,
        }
    }
}
impl ReleaseFailure {
    fn at(runtime: AppError, pointer: &str, message: &str) -> Self {
        Self {
            runtime,
            diagnostic: Some(format!("{pointer}: {message}")),
            pointer: Some(pointer.to_owned()),
        }
    }
}
// The same publication gates run in both staging and the read-only upload check.
// PostgreSQL read-only transactions cannot acquire row locks; staging retains
// its singleton lock and revision FOR SHARE locks until the atomic commit.
async fn checked_entries<'a>(
    db: &impl ConnectionTrait,
    manifest: &'a ReleaseManifest,
    media_root: &std::path::Path,
    lock_revisions: bool,
) -> Result<(Vec<&'a RevisionRef>, Vec<String>), ReleaseFailure> {
    let mut revision_query = "SELECT public_document,server_document,EXISTS(SELECT 1 FROM content_withdrawals w WHERE (w.lesson_id,w.revision)=(r.lesson_id,r.revision)) AS withdrawn FROM lesson_revisions r WHERE lesson_id=$1 AND revision=$2".to_owned();
    if lock_revisions {
        revision_query.push_str(" FOR SHARE");
    }
    if one(
        db,
        "SELECT id FROM content_releases WHERE id=$1",
        vec![manifest.id.clone().into()],
    )
    .await?
    .is_some()
    {
        return Err(ReleaseFailure::at(
            AppError::Conflict,
            "/id",
            "release ID already exists; releases are immutable",
        ));
    }
    let mut entries = Vec::new();
    let mut source_hashes = Vec::new();
    for (li, level) in manifest.levels.iter().enumerate() {
        for (ui, unit) in level.units.iter().enumerate() {
            for (ri, entry) in unit.lessons.iter().enumerate() {
                let path = format!("/levels/{li}/units/{ui}/lessons/{ri}");
                let revision_path = format!("{path}/revision");
                let row = one(
                    db,
                    &revision_query,
                    vec![
                        entry.lesson_id.clone().into(),
                        (entry.revision as i32).into(),
                    ],
                )
                .await?
                .ok_or_else(|| {
                    ReleaseFailure::at(
                        AppError::NotFound,
                        &revision_path,
                        "referenced lesson revision has not been imported",
                    )
                })?;
                if field::<bool>(&row, "withdrawn")? {
                    return Err(ReleaseFailure::at(
                        AppError::Gone,
                        &revision_path,
                        "referenced lesson revision was withdrawn",
                    ));
                }
                let source: serde_json::Value = field(&row, "server_document")?;
                if !crate::admin::approved(db, &entry.lesson_id, entry.revision, &source).await? {
                    return Err(ReleaseFailure::at(
                        AppError::InvalidInput,
                        &path,
                        "imported lesson requires reviewed editorial status",
                    ));
                }
                let lesson = project_source(source.clone()).map_err(|error| {
                    ReleaseFailure::at(
                        AppError::InvalidInput,
                        &path,
                        &format!("imported lesson validation failed: {error}"),
                    )
                })?;
                if lesson.level_id != level.id
                    || lesson.unit_id != unit.id
                    || lesson.id != entry.lesson_id
                    || lesson.revision != entry.revision
                    || serde_json::to_value(&lesson).map_err(|_| AppError::Unavailable)?
                        != field::<serde_json::Value>(&row, "public_document")?
                {
                    return Err(ReleaseFailure::at(
                        AppError::InvalidInput,
                        &path,
                        "lesson level/unit/revision or stored public projection does not match this directory entry",
                    ));
                }
                Grader::from_author_source(&lesson, &source).map_err(|error| {
                    ReleaseFailure::at(
                        AppError::InvalidInput,
                        &path,
                        &format!("imported lesson grading validation failed: {error}"),
                    )
                })?;
                crate::media::validate_lesson_detailed(db, &lesson, media_root)
                    .await
                    .map_err(|error| ReleaseFailure::at(error.runtime, &path, &error.diagnostic))?;
                source_hashes.push(hash(&source)?);
                entries.push(entry);
            }
        }
    }
    Ok((entries, source_hashes))
}

pub(crate) async fn check_registered_release(
    db: &impl ConnectionTrait,
    document: &crate::author_json::Document,
    media_root: &std::path::Path,
) -> Result<brioche_course_contract::AdminDocumentCheck, AppError> {
    let manifest: ReleaseManifest =
        serde_json::from_value(document.value.clone()).map_err(|_| AppError::InvalidInput)?;
    match checked_entries(db, &manifest, media_root, false).await {
        Ok(_) => Ok(brioche_course_contract::AdminDocumentCheck {
            valid: true,
            issue: None,
        }),
        Err(error) => {
            let message = match error.runtime {
                AppError::NotFound => "该课程版本尚未导入。",
                AppError::Gone => "该课程版本已撤回，请选择可发布的版本。",
                AppError::Conflict => "发布目录编号已存在，请使用新的编号。",
                AppError::InvalidInput => {
                    "该课程未满足发布条件，请核对发布授权、等级与单元、题目和登记素材。"
                }
                runtime => return Err(runtime),
            };
            Ok(document.uploaded_issue(error.pointer.as_deref().unwrap_or("/"), message))
        }
    }
}

async fn stage_impl(
    db: &DatabaseConnection,
    manifest: &ReleaseManifest,
    caller: ContentActor<'_>,
    reason: &str,
    media_root: &std::path::Path,
) -> Result<(), ReleaseFailure> {
    let audit_actor = caller.audit_actor();
    let actor = audit_actor.as_str();
    manifest.validate_author().map_err(|error| ReleaseFailure {
        runtime: AppError::InvalidInput,
        diagnostic: Some(error.to_string()),
        pointer: None,
    })?;
    if !text(actor) || !text(reason) {
        return Err(ReleaseFailure::at(
            AppError::InvalidInput,
            "/",
            "actor and reason must be nonempty, without control characters, at most 1000 bytes",
        ));
    }
    let tx = db.begin().await.map_err(|_| AppError::Unavailable)?;
    caller.lock(&tx).await?;
    // All content mutations lock the singleton before revision rows: no activation/withdrawal deadlock.
    let state = one(
        &tx,
        "SELECT generation FROM content_state WHERE singleton FOR UPDATE",
        vec![],
    )
    .await?
    .ok_or(AppError::Unavailable)?;
    let (entries, source_hashes) = checked_entries(&tx, manifest, media_root, true).await?;
    exec(
        &tx,
        "INSERT INTO content_releases(id,manifest,content_hash) VALUES($1,$2,$3)",
        vec![
            manifest.id.clone().into(),
            serde_json::to_value(manifest)
                .map_err(|_| AppError::Unavailable)?
                .into(),
            hash(&serde_json::json!({"manifest":manifest,"sources":source_hashes}))?.into(),
        ],
    )
    .await?;
    for (position, entry) in entries.into_iter().enumerate() {
        exec(&tx,"INSERT INTO release_entries(release_id,lesson_id,revision,position) VALUES($1,$2,$3,$4)",vec![manifest.id.clone().into(),entry.lesson_id.clone().into(),(entry.revision as i32).into(),(position as i32).into()]).await?;
    }
    exec(&tx,"INSERT INTO content_audit(action,actor,reason,release_id,generation) VALUES('stage',$1,$2,$3,$4)",vec![actor.into(),reason.into(),manifest.id.clone().into(),field::<i64>(&state,"generation")?.into()]).await?;
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(())
}
pub async fn activate(
    db: &DatabaseConnection,
    id: &str,
    expected: i64,
    actor: &str,
    reason: &str,
    media_root: &std::path::Path,
) -> Result<i64, AppError> {
    activate_impl(
        db,
        None,
        id,
        expected,
        ContentActor::Local(actor),
        reason,
        media_root,
    )
    .await
    .map_err(|error| error.runtime)
}
/// Local diagnostics never change runtime status codes or publication gates.
pub async fn activate_author(
    db: &DatabaseConnection,
    id: &str,
    expected: i64,
    actor: &str,
    reason: &str,
    media_root: &std::path::Path,
) -> anyhow::Result<i64> {
    activate_impl(
        db,
        None,
        id,
        expected,
        ContentActor::Local(actor),
        reason,
        media_root,
    )
    .await
    .map_err(|error| {
        anyhow::anyhow!(
            "release {id}: {}",
            error.diagnostic.unwrap_or_else(|| {
                "activation database operation failed; verify release status before retrying".into()
            })
        )
    })
}
async fn activate_impl(
    db: &DatabaseConnection,
    product: Option<crate::product::ProductId>,
    id: &str,
    expected: i64,
    caller: ContentActor<'_>,
    reason: &str,
    media_root: &std::path::Path,
) -> Result<i64, ReleaseFailure> {
    let audit_actor = caller.audit_actor();
    let actor = audit_actor.as_str();
    if !identifier(id) {
        return Err(ReleaseFailure::at(
            AppError::InvalidInput,
            "release-id",
            "invalid release identifier",
        ));
    }
    if expected < 0 {
        return Err(ReleaseFailure::at(
            AppError::InvalidInput,
            "expected-generation",
            "must be nonnegative",
        ));
    }
    if !text(actor) || !text(reason) {
        return Err(ReleaseFailure::at(
            AppError::InvalidInput,
            "actor/reason",
            "must be nonempty, without control characters, at most 1000 bytes",
        ));
    }
    let tx = db.begin().await.map_err(|_| AppError::Unavailable)?;
    caller.lock(&tx).await?;
    let state_selector = product.map_or_else(
        || "singleton".to_owned(),
        |p| format!("product_id='{}'", p.as_str()),
    );
    let state = one(
        &tx,
        &format!("SELECT generation FROM content_state WHERE {state_selector} FOR UPDATE"),
        vec![],
    )
    .await?
    .ok_or(AppError::Unavailable)?;
    let generation = field::<i64>(&state, "generation")?;
    if expected != generation {
        return Err(ReleaseFailure::at(
            AppError::Conflict,
            "expected-generation",
            &format!("content generation changed: expected {expected}, current {generation}"),
        ));
    }
    if one(
        &tx,
        &format!(
            "SELECT id FROM content_releases WHERE id=$1{}",
            product_filter(product, "product_id")
        ),
        vec![id.into()],
    )
    .await?
    .is_none()
    {
        return Err(ReleaseFailure::at(
            AppError::NotFound,
            "release-id",
            "release is not staged",
        ));
    }
    if let Some(row) = one(&tx,&format!("SELECT e.lesson_id,e.revision FROM release_entries e JOIN content_withdrawals w USING(lesson_id,revision) WHERE e.release_id=$1{}{} ORDER BY e.position LIMIT 1",product_filter(product,"e.product_id"),product_filter(product,"w.product_id")),vec![id.into()]).await? {
        return Err(ReleaseFailure::at(AppError::Gone, "release-id", &format!("release contains a withdrawn lesson revision: {}@{}", field::<String>(&row,"lesson_id")?,field::<i32>(&row,"revision")?)));
    }
    let next = generation.checked_add(1).ok_or(AppError::Unavailable)?;
    let rows=tx.query_all_raw(Statement::from_sql_and_values(DbBackend::Postgres,format!("SELECT r.public_document,r.server_document FROM release_entries e JOIN lesson_revisions r USING(lesson_id,revision) WHERE e.release_id=$1{}{} ORDER BY e.position",product_filter(product,"e.product_id"),product_filter(product,"r.product_id")),[id.into()])).await.map_err(|_|AppError::Unavailable)?;
    for row in rows {
        let lesson: PublicLesson = serde_json::from_value(field(&row, "public_document")?)
            .map_err(|_| AppError::Unavailable)?;
        if !crate::admin::approved(
            &tx,
            &lesson.id,
            lesson.revision,
            &field(&row, "server_document")?,
        )
        .await?
        {
            return Err(ReleaseFailure::at(
                AppError::Conflict,
                "release-id",
                "release contains a lesson that is no longer approved",
            ));
        }
        crate::media::validate_lesson_detailed(&tx, &lesson, media_root)
            .await
            .map_err(|error| ReleaseFailure {
                runtime: error.runtime,
                diagnostic: Some(format!(
                    "lesson {}@{}: {}",
                    lesson.id, lesson.revision, error.diagnostic
                )),
                pointer: None,
            })?;
    }
    exec(&tx,&format!("UPDATE lesson_revisions r SET published=true FROM release_entries e WHERE e.release_id=$1 AND (r.lesson_id,r.revision)=(e.lesson_id,e.revision){}{}",product_filter(product,"e.product_id"),product_filter(product,"r.product_id")),vec![id.into()]).await?;
    exec(
        &tx,
        &format!("UPDATE content_state SET active_release=$1,generation=$2 WHERE {state_selector}"),
        vec![id.into(), next.into()],
    )
    .await?;
    if let Some(product) = product {
        exec(&tx,"INSERT INTO content_audit(action,actor,reason,release_id,generation,product_id) VALUES('activate',$1,$2,$3,$4,$5)",vec![actor.into(),reason.into(),id.into(),next.into(),product.as_str().into()]).await?;
    } else {
        exec(&tx,"INSERT INTO content_audit(action,actor,reason,release_id,generation) VALUES('activate',$1,$2,$3,$4)",vec![actor.into(),reason.into(),id.into(),next.into()]).await?;
    }
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(next)
}
pub async fn withdraw(
    db: &DatabaseConnection,
    id: &str,
    revision: u32,
    expected: i64,
    actor: &str,
    reason: &str,
) -> Result<i64, AppError> {
    withdraw_impl(
        db,
        None,
        id,
        revision,
        expected,
        ContentActor::Local(actor),
        reason,
    )
    .await
    .map_err(|error| error.runtime)
}
/// Local CLI diagnostics share the runtime transaction and irreversible withdrawal checks.
pub async fn withdraw_author(
    db: &DatabaseConnection,
    id: &str,
    revision: u32,
    expected: i64,
    actor: &str,
    reason: &str,
) -> anyhow::Result<i64> {
    withdraw_impl(
        db,
        None,
        id,
        revision,
        expected,
        ContentActor::Local(actor),
        reason,
    )
    .await
    .map_err(|error| {
        anyhow::anyhow!(
            "lesson {id}@{revision}: {}",
            error.diagnostic.unwrap_or_else(|| {
                "withdrawal database operation failed; verify content status before retrying".into()
            })
        )
    })
}
async fn withdraw_impl(
    db: &DatabaseConnection,
    product: Option<crate::product::ProductId>,
    id: &str,
    revision: u32,
    expected: i64,
    caller: ContentActor<'_>,
    reason: &str,
) -> Result<i64, ReleaseFailure> {
    let audit_actor = caller.audit_actor();
    let actor = audit_actor.as_str();
    if !identifier(id) {
        return Err(ReleaseFailure::at(
            AppError::InvalidInput,
            "lesson-id",
            "invalid lesson identifier",
        ));
    }
    if !brioche_course_contract::valid_content_revision(revision) {
        return Err(ReleaseFailure::at(
            AppError::InvalidInput,
            "revision",
            "expected revision in 1..2147483647",
        ));
    }
    if expected < 0 {
        return Err(ReleaseFailure::at(
            AppError::InvalidInput,
            "expected-generation",
            "must be nonnegative",
        ));
    }
    if !text(actor) || !text(reason) {
        return Err(ReleaseFailure::at(
            AppError::InvalidInput,
            "actor/reason",
            "must be nonempty, without control characters, at most 1000 bytes",
        ));
    }
    let tx = db.begin().await.map_err(|_| AppError::Unavailable)?;
    caller.lock(&tx).await?;
    let state_selector = product.map_or_else(
        || "singleton".to_owned(),
        |p| format!("product_id='{}'", p.as_str()),
    );
    let state = one(
        &tx,
        &format!("SELECT generation FROM content_state WHERE {state_selector} FOR UPDATE"),
        vec![],
    )
    .await?
    .ok_or(AppError::Unavailable)?;
    let generation = field::<i64>(&state, "generation")?;
    if generation != expected {
        return Err(ReleaseFailure::at(
            AppError::Conflict,
            "expected-generation",
            &format!("content generation changed: expected {expected}, current {generation}"),
        ));
    }
    if exec(
        &tx,
        &format!(
            "UPDATE lesson_revisions SET published=false WHERE lesson_id=$1 AND revision=$2{}",
            product_filter(product, "product_id")
        ),
        vec![id.into(), (revision as i32).into()],
    )
    .await?
        != 1
    {
        return Err(ReleaseFailure::at(
            AppError::NotFound,
            "lesson-id/revision",
            "lesson revision does not exist",
        ));
    }
    let inserted = if let Some(product) = product {
        exec(&tx,"INSERT INTO content_withdrawals(product_id,lesson_id,revision) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",vec![product.as_str().into(),id.into(),(revision as i32).into()]).await?
    } else {
        exec(&tx,"INSERT INTO content_withdrawals(lesson_id,revision) VALUES($1,$2) ON CONFLICT DO NOTHING",vec![id.into(),(revision as i32).into()]).await?
    };
    if inserted != 1 {
        return Err(ReleaseFailure::at(
            AppError::Gone,
            "lesson-id/revision",
            "lesson revision was already withdrawn",
        ));
    }
    let next = expected.checked_add(1).ok_or(AppError::Unavailable)?;
    exec(
        &tx,
        &format!("UPDATE content_state SET generation=$1 WHERE {state_selector}"),
        vec![next.into()],
    )
    .await?;
    if let Some(product) = product {
        exec(&tx,"INSERT INTO content_audit(action,actor,reason,lesson_id,revision,generation,product_id) VALUES('withdraw',$1,$2,$3,$4,$5,$6)",vec![actor.into(),reason.into(),id.into(),(revision as i32).into(),next.into(),product.as_str().into()]).await?;
    } else {
        exec(&tx,"INSERT INTO content_audit(action,actor,reason,lesson_id,revision,generation) VALUES('withdraw',$1,$2,$3,$4,$5)",vec![actor.into(),reason.into(),id.into(),(revision as i32).into(),next.into()]).await?;
    }
    tx.commit().await.map_err(|_| AppError::Unavailable)?;
    Ok(next)
}
pub async fn catalog<C: ConnectionTrait>(db: &C) -> Result<Catalog, AppError> {
    catalog_matching(db, &[]).await
}
pub async fn catalog_matching<C: ConnectionTrait>(
    db: &C,
    terms: &[String],
) -> Result<Catalog, AppError> {
    catalog_matching_for_product(db, None, terms).await
}
pub async fn catalog_matching_for_product<C: ConnectionTrait>(
    db: &C,
    product: Option<crate::product::ProductId>,
    terms: &[String],
) -> Result<Catalog, AppError> {
    // One statement observes the pointer, immutable manifest and availability together.
    // Import/publication validate full immutable documents; catalog reads need only
    // the public summary, not every dialogue, answer-free exercise and audio timeline.
    // jsonb_to_record detoasts each immutable document once rather than once per
    // summary key. The full body and private grading document are never returned.
    let scoped = product.is_some();
    let rows = db.query_all_raw(Statement::from_sql_and_values(DbBackend::Postgres, format!(r#"
        SELECT cr.manifest, COALESCE(jsonb_agg(jsonb_build_object(
            'summary', jsonb_build_object(
                'id',p.id,'revision',p.revision,'levelId',p."levelId",'unitId',p."unitId",
                'title',p.title,'summaryZh',p."summaryZh",'estimatedMinutes',p."estimatedMinutes"),
            'searchText', CASE WHEN $1 THEN COALESCE((
                SELECT string_agg(concat_ws(' ',v->>'lemma',v->>'meaningZh'),' ')
                FROM jsonb_array_elements(COALESCE(p.knowledge->'vocabulary','[]'::jsonb)) v
            ),'') ELSE '' END
        ) ORDER BY e.position) FILTER(WHERE r.lesson_id IS NOT NULL),'[]'::jsonb) AS summaries
        FROM content_state s JOIN content_releases cr ON cr.id=s.active_release{}
        LEFT JOIN release_entries e ON e.release_id=cr.id{}
        LEFT JOIN lesson_revisions r ON (r.lesson_id,r.revision)=(e.lesson_id,e.revision){}
            AND r.published AND NOT EXISTS(
                SELECT 1 FROM content_withdrawals w WHERE (w.lesson_id,w.revision)=(r.lesson_id,r.revision){})
        LEFT JOIN LATERAL jsonb_to_record(r.public_document) AS p(
            id text,revision integer,"levelId" text,"unitId" text,title jsonb,
            "summaryZh" text,"estimatedMinutes" integer,knowledge jsonb) ON true
        WHERE {} GROUP BY cr.id
    "#,
        if scoped{" AND cr.product_id=s.product_id"}else{""},
        if scoped{" AND e.product_id=cr.product_id"}else{""},
        if scoped{" AND r.product_id=e.product_id"}else{""},
        if scoped{" AND w.product_id=r.product_id"}else{""},
        product.map_or_else(||"s.singleton".to_owned(),|p|format!("s.product_id='{}'",p.as_str())),
    ),[(!terms.is_empty()).into()])).await.map_err(|_|AppError::Unavailable)?;
    let Some(first) = rows.first() else {
        return Ok(Catalog {
            levels: vec![],
            development_fixture: false,
        });
    };
    let manifest: ReleaseManifest =
        serde_json::from_value(field(first, "manifest")?).map_err(|_| AppError::Unavailable)?;
    let mut lessons = BTreeMap::new();
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct SearchableSummary {
        summary: LessonSummary,
        search_text: String,
    }
    let mut vocabulary = BTreeMap::new();
    let summaries: Vec<SearchableSummary> =
        serde_json::from_value(field(first, "summaries")?).map_err(|_| AppError::Unavailable)?;
    for row in summaries {
        vocabulary.insert(row.summary.id.clone(), row.search_text);
        lessons.insert(row.summary.id.clone(), row.summary);
    }
    let catalog = Catalog {
        development_fixture: false,
        levels: manifest
            .levels
            .into_iter()
            .filter_map(|level| {
                let units: Vec<_> = level
                    .units
                    .into_iter()
                    .filter_map(|unit| {
                        let entries: Vec<_> = unit
                            .lessons
                            .into_iter()
                            .filter_map(|entry| lessons.remove(&entry.lesson_id))
                            .collect();
                        (!entries.is_empty()).then_some(Unit {
                            id: unit.id,
                            title_zh: unit.title_zh,
                            lessons: entries,
                        })
                    })
                    .collect();
                (!units.is_empty()).then_some(Level {
                    id: level.id,
                    label: level.label,
                    units,
                })
            })
            .collect(),
    };
    Ok(search_catalog_with_vocabulary(catalog, terms, &vocabulary))
}

#[cfg(test)]
mod author_tests {
    use super::*;
    fn fixture() -> serde_json::Value {
        serde_json::from_str(include_str!("../../../docs/examples/catalog.release.json")).unwrap()
    }
    #[test]
    fn locates_release_fields_and_preserves_opaque_public_errors() {
        for (pointer, value) in [
            ("/id", serde_json::json!("bad release")),
            ("/schemaVersion", serde_json::json!("2.0")),
            ("/levels/0/id", serde_json::json!("bad level")),
            ("/levels/0/label", serde_json::json!(" ")),
            ("/levels/0/units", serde_json::json!([])),
            ("/levels/0/units/0/id", serde_json::json!("bad unit")),
            ("/levels/0/units/0/titleZh", serde_json::json!("\n")),
            ("/levels/0/units/0/lessons", serde_json::json!([])),
            (
                "/levels/0/units/0/lessons/0/lessonId",
                serde_json::json!("bad lesson"),
            ),
            ("/levels/0/units/0/lessons/0/revision", serde_json::json!(0)),
            (
                "/levels/0/units/0/lessons/0/revision",
                serde_json::json!(2147483648u32),
            ),
        ] {
            let mut source = fixture();
            *source.pointer_mut(pointer).unwrap() = value;
            let manifest: ReleaseManifest = serde_json::from_value(source).unwrap();
            assert!(
                manifest
                    .validate_author()
                    .unwrap_err()
                    .to_string()
                    .starts_with(&format!("{pointer}:"))
            );
            assert!(matches!(manifest.validate(), Err(AppError::InvalidInput)));
        }
        let manifest: ReleaseManifest = serde_json::from_value(fixture()).unwrap();
        assert!(manifest.validate().is_ok());
        let empty: ReleaseManifest = serde_json::from_value(
            serde_json::json!({"id":"empty","schemaVersion":"1.0","levels":[]}),
        )
        .unwrap();
        assert!(empty.validate().is_ok());
    }
    #[test]
    fn identifies_second_duplicate_and_bounds_catalog_size() {
        let mut source = fixture();
        let reference = source["levels"][0]["units"][0]["lessons"][0].clone();
        source["levels"][0]["units"][0]["lessons"]
            .as_array_mut()
            .unwrap()
            .push(reference);
        let manifest: ReleaseManifest = serde_json::from_value(source).unwrap();
        assert!(
            manifest
                .validate_author()
                .unwrap_err()
                .to_string()
                .starts_with("/levels/0/units/0/lessons/1/lessonId:")
        );
        let mut source = fixture();
        let unit = source["levels"][0]["units"][0].clone();
        source["levels"][0]["units"]
            .as_array_mut()
            .unwrap()
            .push(unit);
        let manifest: ReleaseManifest = serde_json::from_value(source).unwrap();
        assert!(
            manifest
                .validate_author()
                .unwrap_err()
                .to_string()
                .starts_with("/levels/0/units/1/id:")
        );
        let mut source = fixture();
        source["levels"][0]["units"][0]["lessons"] = serde_json::json!(
            (0..5001)
                .map(|i| serde_json::json!({"lessonId":format!("lesson-{i}"),"revision":1}))
                .collect::<Vec<_>>()
        );
        let manifest: ReleaseManifest = serde_json::from_value(source).unwrap();
        assert!(
            manifest
                .validate_author()
                .unwrap_err()
                .to_string()
                .starts_with("/levels/0/units/0/lessons:")
        );
        let mut source = fixture();
        source["levels"] = serde_json::json!(
            (0..21)
                .map(|i| serde_json::json!({"id":format!("level-{i}"),"label":"A1","units":[]}))
                .collect::<Vec<_>>()
        );
        let manifest: ReleaseManifest = serde_json::from_value(source).unwrap();
        assert!(
            manifest
                .validate_author()
                .unwrap_err()
                .to_string()
                .starts_with("/levels:")
        );
    }
}
