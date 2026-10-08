//! Local, content-addressed visual assets and immutable character snapshots.
use crate::{
    AppError,
    learning::{exec, field, hash, one, random_id},
};
use anyhow::{Context, Result, bail, ensure};
use brioche_course_contract::{Block, Character, MediaAsset, PublicLesson};
use sea_orm::{ConnectionTrait, DatabaseConnection, TransactionTrait};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
fn supported_character_locale(locale: &str) -> bool {
    use brioche_course_contract::TargetLanguage;
    [TargetLanguage::French, TargetLanguage::Cantonese]
        .iter()
        .any(|language| language.locale() == locale)
}
const MAX_BYTES: u64 = 32 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssetBundle {
    pub schema_version: String,
    pub assets: Vec<AssetSpec>,
    pub characters: Vec<CharacterSpec>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssetSpec {
    pub asset_id: String,
    pub revision: u32,
    pub sha256: String,
    pub mime_type: String,
    pub width: u32,
    pub height: u32,
    pub alt_zh: String,
    pub credit_zh: String,
    pub file: String,
    pub status: String,
    pub source: String,
    pub license: String,
    pub creator: String,
    pub rights_confirmed: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CharacterSpec {
    pub snapshot: Character,
    pub avatar_revision: u32,
}
impl AssetBundle {
    /// Metadata checks run before database access and filesystem mutation.
    pub fn validate_author(&self, actor: &str) -> Result<()> {
        ensure!(self.schema_version == "1.0", "/schemaVersion: expected 1.0");
        ensure!(self.assets.len() <= 500, "/assets: at most 500 assets");
        ensure!(
            self.characters.len() <= 500,
            "/characters: at most 500 characters"
        );
        ensure!(text(actor), "/: invalid import actor");
        let mut ids = BTreeSet::new();
        for (index, asset) in self.assets.iter().enumerate() {
            let p = format!("/assets/{index}");
            ensure!(valid_id(&asset.asset_id), "{p}/assetId: invalid asset ID");
            ensure!(
                brioche_course_contract::valid_content_revision(asset.revision),
                "{p}/revision: outside database range"
            );
            ensure!(
                ids.insert((&asset.asset_id, asset.revision)),
                "{p}/assetId: duplicate asset revision"
            );
            ensure!(asset.status == "ready", "{p}/status: asset must be ready");
            ensure!(
                asset.rights_confirmed,
                "{p}/rightsConfirmed: confirmed rights required"
            );
            for (field, value) in [
                ("source", &asset.source),
                ("license", &asset.license),
                ("creator", &asset.creator),
                ("creditZh", &asset.credit_zh),
                ("altZh", &asset.alt_zh),
            ] {
                ensure!(text(value), "{p}/{field}: expected nonempty text");
            }
            ensure!(
                asset.sha256.len() == 64
                    && asset
                        .sha256
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "{p}/sha256: expected lowercase SHA-256"
            );
            extension(&asset.mime_type)
                .with_context(|| format!("{p}/mimeType: unsupported visual MIME"))?;
            ensure!(
                (1..=8192).contains(&asset.width),
                "{p}/width: outside image dimension range"
            );
            ensure!(
                (1..=8192).contains(&asset.height),
                "{p}/height: outside image dimension range"
            );
            ensure!(
                !asset.file.is_empty()
                    && Path::new(&asset.file)
                        .components()
                        .all(|c| matches!(c, Component::Normal(_))),
                "{p}/file: expected relative path without traversal"
            );
        }
        let mut ids = BTreeSet::new();
        for (index, character) in self.characters.iter().enumerate() {
            let p = format!("/characters/{index}");
            let snapshot = &character.snapshot;
            ensure!(
                valid_id(&snapshot.character_id),
                "{p}/snapshot/characterId: invalid character ID"
            );
            ensure!(
                brioche_course_contract::valid_content_revision(snapshot.revision),
                "{p}/snapshot/revision: outside database range"
            );
            ensure!(
                ids.insert((&snapshot.character_id, snapshot.revision)),
                "{p}/snapshot/characterId: duplicate character revision"
            );
            ensure!(
                text(&snapshot.display_name),
                "{p}/snapshot/displayName: expected nonempty name"
            );
            ensure!(
                supported_character_locale(&snapshot.speech_locale),
                "{p}/snapshot/speechLocale: expected fr-FR or yue-Hant-HK"
            );
            ensure!(
                valid_id(&snapshot.avatar_id),
                "{p}/snapshot/avatarId: invalid avatar ID"
            );
            ensure!(
                brioche_course_contract::valid_content_revision(character.avatar_revision),
                "{p}/avatarRevision: outside database range"
            );
            // Exact bundled revisions can be checked before files or database access.
            // Other revisions may already exist in the registry; import resolves those.
            if let Some(avatar) = self.assets.iter().find(|asset| {
                asset.asset_id == snapshot.avatar_id && asset.revision == character.avatar_revision
            }) {
                ensure!(
                    avatar.width == avatar.height,
                    "{p}/snapshot/avatarId: avatar must be square"
                );
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssetRef {
    pub asset_id: String,
    pub revision: u32,
}
pub(crate) fn valid_id(value: &str) -> bool {
    brioche_course_contract::valid_content_id(value)
}
pub(crate) fn text(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 2000 && !value.chars().any(char::is_control)
}
fn extension(mime: &str) -> Result<&'static str> {
    match mime {
        "image/svg+xml" => Ok("svg"),
        "image/png" => Ok("png"),
        "image/jpeg" => Ok("jpg"),
        "image/webp" => Ok("webp"),
        _ => bail!("unsupported visual MIME type"),
    }
}
pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn read_file(path: &Path) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path).context("asset file unavailable")?;
    ensure!(file.metadata()?.is_file(), "asset must be a regular file");
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        !bytes.is_empty() && bytes.len() as u64 <= MAX_BYTES,
        "asset size must be 1..32 MiB"
    );
    Ok(bytes)
}
fn svg_dimensions(bytes: &[u8]) -> Result<(u32, u32)> {
    use quick_xml::{Reader, XmlVersion, events::Event};
    let mut reader = Reader::from_str(std::str::from_utf8(bytes)?);
    let (mut depth, mut roots, mut dimensions) = (0usize, 0usize, None);
    let mut namespace = false;
    reader.config_mut().check_comments = true;
    loop {
        match reader.read_event()? {
            event @ (Event::Start(_) | Event::Empty(_)) => {
                let empty = matches!(event, Event::Empty(_));
                let element = match event {
                    Event::Start(e) | Event::Empty(e) => e,
                    _ => unreachable!(),
                };
                let name = element.name();
                let name = name.as_ref();
                ensure!(
                    matches!(
                        name,
                        "svg"
                            | "g"
                            | "path"
                            | "rect"
                            | "circle"
                            | "ellipse"
                            | "line"
                            | "polyline"
                            | "polygon"
                            | "defs"
                            | "linearGradient"
                            | "radialGradient"
                            | "stop"
                            | "title"
                            | "desc"
                            | "clipPath"
                            | "mask"
                    ),
                    "SVG contains unsupported or active element"
                );
                if depth == 0 {
                    ensure!(name == "svg" && roots == 0, "SVG requires one root");
                    roots += 1;
                }
                for attribute in element.attributes() {
                    let attribute = attribute?;
                    let key = attribute.key.as_ref();
                    ensure!(
                        matches!(
                            key,
                            "xmlns"
                                | "viewBox"
                                | "width"
                                | "height"
                                | "x"
                                | "y"
                                | "x1"
                                | "x2"
                                | "y1"
                                | "y2"
                                | "cx"
                                | "cy"
                                | "r"
                                | "rx"
                                | "ry"
                                | "d"
                                | "points"
                                | "fill"
                                | "stroke"
                                | "stroke-width"
                                | "stroke-linecap"
                                | "stroke-linejoin"
                                | "stroke-dasharray"
                                | "fill-rule"
                                | "clip-rule"
                                | "opacity"
                                | "fill-opacity"
                                | "stroke-opacity"
                                | "transform"
                                | "id"
                                | "role"
                                | "aria-labelledby"
                                | "aria-hidden"
                                | "offset"
                                | "stop-color"
                                | "stop-opacity"
                                | "gradientUnits"
                                | "gradientTransform"
                                | "clip-path"
                                | "mask"
                                | "preserveAspectRatio"
                        ),
                        "SVG contains unsupported attribute"
                    );
                    let value = attribute.normalized_value(XmlVersion::Implicit1_0)?;
                    if key == "xmlns" {
                        namespace = true;
                        ensure!(
                            depth == 0 && value == "http://www.w3.org/2000/svg",
                            "SVG namespace must be local and standard"
                        );
                    }
                    let lower = value.to_ascii_lowercase();
                    if matches!(key, "fill" | "stroke" | "clip-path" | "mask")
                        && lower.contains("url")
                    {
                        ensure!(
                            value.starts_with("url(#")
                                && value.ends_with(')')
                                && valid_id(&value[5..value.len() - 1]),
                            "SVG external reference rejected"
                        );
                    }
                    if depth == 0 && key == "viewBox" {
                        let parts = value
                            .split(|c: char| c.is_ascii_whitespace() || c == ',')
                            .filter(|v| !v.is_empty())
                            .map(str::parse::<f64>)
                            .collect::<std::result::Result<Vec<_>, _>>()?;
                        ensure!(
                            parts.len() == 4
                                && parts.iter().all(|v| v.is_finite())
                                && parts[2] > 0.0
                                && parts[3] > 0.0
                                && parts[2].fract() == 0.0
                                && parts[3].fract() == 0.0
                                && parts[2] <= 8192.0
                                && parts[3] <= 8192.0,
                            "SVG viewBox dimensions invalid"
                        );
                        dimensions = Some((parts[2] as u32, parts[3] as u32));
                    }
                }
                if !empty {
                    depth += 1;
                    ensure!(depth <= 128, "SVG nesting too deep");
                }
            }
            Event::End(_) => {
                ensure!(depth > 0, "unexpected SVG closing element");
                depth -= 1;
            }
            Event::DocType(_) | Event::PI(_) | Event::CData(_) | Event::GeneralRef(_) => {
                bail!("SVG entities or executable content rejected")
            }
            Event::Eof => break,
            Event::Text(text) if depth == 0 => {
                ensure!(
                    text.xml_content(XmlVersion::Implicit1_0).trim().is_empty(),
                    "text outside SVG root"
                );
            }
            _ => {}
        }
    }
    ensure!(
        roots == 1 && depth == 0 && namespace,
        "incomplete SVG document or missing SVG namespace"
    );
    dimensions.context("SVG requires viewBox")
}
fn dimensions(bytes: &[u8], mime: &str) -> Result<(u32, u32)> {
    if mime == "image/svg+xml" {
        return svg_dimensions(bytes);
    }
    use image::{GenericImageView, ImageFormat, ImageReader, Limits};
    let format = image::guess_format(bytes)?;
    ensure!(
        matches!(
            (format, mime),
            (ImageFormat::Png, "image/png")
                | (ImageFormat::Jpeg, "image/jpeg")
                | (ImageFormat::WebP, "image/webp")
        ),
        "MIME does not match file bytes"
    );
    let mut reader = ImageReader::with_format(std::io::Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    Ok(reader.decode()?.dimensions())
}

/// Author inspection shares the exact format, size and SVG restrictions of imports.
/// It does not store, register or authorize the asset.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualFileInfo {
    pub sha256: String,
    pub byte_length: usize,
    pub mime_type: String,
    pub width: u32,
    pub height: u32,
}

pub fn inspect_file(path: &Path, mime: &str) -> Result<VisualFileInfo> {
    extension(mime)?;
    let bytes = read_file(path)?;
    let (width, height) = dimensions(&bytes, mime)?;
    ensure!(width > 0 && height > 0, "asset dimensions must be positive");
    Ok(VisualFileInfo {
        sha256: digest(&bytes),
        byte_length: bytes.len(),
        mime_type: mime.into(),
        width,
        height,
    })
}
pub fn media_root() -> PathBuf {
    std::env::var_os("MEDIA_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".local/media"))
}
pub(crate) fn store_file(root: &Path, bytes: &[u8], sha: &str, ext: &str) -> Result<()> {
    std::fs::create_dir_all(root)?;
    let final_path = root.join(format!("{sha}.{ext}"));
    if final_path.exists() {
        ensure!(
            digest(&stored_bytes(root, sha, ext)?) == sha,
            "existing media object is corrupt"
        );
        return Ok(());
    }
    let temp = root.join(format!(
        "{sha}-{}.tmp",
        random_id().map_err(anyhow::Error::msg)?
    ));
    let result = (|| -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        match std::fs::hard_link(&temp, &final_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => ensure!(
                digest(&stored_bytes(root, sha, ext)?) == sha,
                "concurrent object corrupt"
            ),
            Err(error) => return Err(error.into()),
        };
        Ok(())
    })();
    let _ = std::fs::remove_file(&temp);
    result
}
/// Read-only local checks. Existing registry references still require import validation.
/// Call from a blocking worker: reads and decodes one bounded file at a time.
pub fn check_bundle(bundle: &AssetBundle, source_root: &Path) -> Result<()> {
    bundle.validate_author("offline-check")?;
    let root = source_root
        .canonicalize()
        .context("/: source directory unavailable")?;
    ensure!(root.is_dir(), "/: expected source directory");
    for (index, asset) in bundle.assets.iter().enumerate() {
        inspect_source_file(&root, asset, index)?;
    }
    Ok(())
}

fn inspect_source_file(root: &Path, asset: &AssetSpec, index: usize) -> Result<Vec<u8>> {
    let p = format!("/assets/{index}");
    let path = root
        .join(&asset.file)
        .canonicalize()
        .with_context(|| format!("{p}/file: visual file unavailable"))?;
    ensure!(
        path.starts_with(root),
        "{p}/file: asset escapes source directory"
    );
    let bytes = read_file(&path).with_context(|| format!("{p}/file: invalid visual file"))?;
    ensure!(
        digest(&bytes) == asset.sha256,
        "{p}/sha256: asset hash mismatch"
    );
    let (width, height) = dimensions(&bytes, &asset.mime_type)
        .with_context(|| format!("{p}/file: invalid visual format"))?;
    ensure!(
        width == asset.width,
        "{p}/width: declared width does not match file"
    );
    ensure!(
        height == asset.height,
        "{p}/height: declared height does not match file"
    );
    Ok(bytes)
}

pub async fn import_bundle(
    db: &DatabaseConnection,
    bundle: AssetBundle,
    source_root: &Path,
    store: &Path,
    actor: &str,
) -> Result<()> {
    import_bundle_impl(db, None, bundle, source_root, store, actor, None).await
}
/// Trusted author CLI; the dispatcher verifies layout and supplies its fixed product.
pub(crate) async fn import_author_bundle(
    db: &DatabaseConnection,
    product: Option<crate::product::ProductId>,
    bundle: AssetBundle,
    source_root: &Path,
    store: &Path,
    actor: &str,
) -> Result<()> {
    import_bundle_impl(db, product, bundle, source_root, store, actor, None).await
}
pub(crate) async fn import_operator_bundle(
    db: &DatabaseConnection,
    product: Option<crate::product::ProductId>,
    bundle: AssetBundle,
    source_root: &Path,
    store: &Path,
    operator: &crate::product_memberships::Operator,
    reason: &str,
) -> Result<()> {
    if product.is_some_and(|product| product != operator.product) {
        return Err(AppError::Forbidden.into());
    }
    crate::admin::reason(reason)?;
    import_bundle_impl(
        db,
        product,
        bundle,
        source_root,
        store,
        &operator.audit_actor(),
        Some(OperatorImport {
            operator,
            reason,
            expected_character: None,
        }),
    )
    .await
}
#[derive(Clone, Copy)]
struct OperatorImport<'a> {
    operator: &'a crate::product_memberships::Operator,
    reason: &'a str,
    expected_character: Option<(&'a str, u32)>,
}
pub(crate) async fn import_operator_character(
    db: &DatabaseConnection,
    product: Option<crate::product::ProductId>,
    character: CharacterSpec,
    root: &Path,
    operator: &crate::product_memberships::Operator,
    expected_revision: u32,
    reason: &str,
) -> Result<()> {
    if product.is_some_and(|product| product != operator.product) {
        return Err(AppError::Forbidden.into());
    }
    crate::admin::reason(reason)?;
    let id = character.snapshot.character_id.clone();
    let bundle = AssetBundle {
        schema_version: "1.0".into(),
        assets: vec![],
        characters: vec![character],
    };
    import_bundle_impl(
        db,
        product,
        bundle,
        root,
        root,
        &operator.audit_actor(),
        Some(OperatorImport {
            operator,
            reason,
            expected_character: Some((&id, expected_revision)),
        }),
    )
    .await
}
async fn import_bundle_impl(
    db: &DatabaseConnection,
    product: Option<crate::product::ProductId>,
    bundle: AssetBundle,
    source_root: &Path,
    store: &Path,
    actor: &str,
    operator: Option<OperatorImport<'_>>,
) -> Result<()> {
    bundle.validate_author(actor)?;
    ensure!(
        bundle.schema_version == "1.0"
            && bundle.assets.len() <= 500
            && bundle.characters.len() <= 500
            && text(actor),
        "invalid asset bundle"
    );
    let bundle_hash = hash(&bundle).map_err(anyhow::Error::msg)?;
    let source_root = source_root.canonicalize()?;
    let store = store.to_path_buf();
    let assets = bundle.assets.clone();
    // Parsing, hashing, image decoding and filesystem writes never run on the async executor.
    let descriptors =
        tokio::task::spawn_blocking(move || -> Result<Vec<(MediaAsset, String, usize)>> {
            let mut ids = BTreeSet::new();
            let mut descriptors = Vec::new();
            for (index, asset) in assets.iter().enumerate() {
                ensure!(
                    valid_id(&asset.asset_id)
                        && brioche_course_contract::valid_content_revision(asset.revision)
                        && ids.insert((&asset.asset_id, asset.revision)),
                    "duplicate or invalid asset identity"
                );
                ensure!(
                    asset.status == "ready"
                        && asset.rights_confirmed
                        && text(&asset.source)
                        && text(&asset.license)
                        && text(&asset.creator)
                        && text(&asset.alt_zh)
                        && text(&asset.credit_zh),
                    "asset needs ready status, rights, credit and alt text"
                );
                ensure!(
                    asset.sha256.len() == 64
                        && asset
                            .sha256
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                    "SHA-256 must be lowercase hex"
                );
                let relative = Path::new(&asset.file);
                ensure!(
                    relative
                        .components()
                        .all(|c| matches!(c, Component::Normal(_))),
                    "asset path must be relative, without traversal"
                );
                let bytes = inspect_source_file(&source_root, asset, index)?;
                let ext = extension(&asset.mime_type)?;
                store_file(&store, &bytes, &asset.sha256, ext)?;
                descriptors.push((
                    MediaAsset {
                        asset_id: asset.asset_id.clone(),
                        revision: asset.revision,
                        sha256: asset.sha256.clone(),
                        mime_type: asset.mime_type.clone(),
                        width: asset.width,
                        height: asset.height,
                        alt_zh: asset.alt_zh.clone(),
                        credit_zh: asset.credit_zh.clone(),
                        url: format!("/api/media/{}.{}", asset.sha256, ext),
                    },
                    ext.to_owned(),
                    bytes.len(),
                ));
            }
            Ok(descriptors)
        })
        .await??;
    let tx = db.begin().await?;
    if let Some(OperatorImport { operator, .. }) = operator {
        operator.lock_content(&tx).await?;
    }
    one(
        &tx,
        &format!(
            "SELECT generation FROM content_state WHERE {} FOR UPDATE",
            product.map_or_else(
                || "singleton".to_owned(),
                |p| format!("product_id='{}'", p.as_str())
            )
        ),
        vec![],
    )
    .await
    .map_err(anyhow::Error::msg)?
    .context("content state missing")?;
    let local_ids = product.is_some() && local_visual_keys(&tx).await?;
    if let Some((id, expected)) = operator.and_then(|o| o.expected_character) {
        if let Some(product) = product.filter(|_| !local_ids) {
            let foreign = one(&tx,"SELECT 1 AS collision FROM character_revisions WHERE character_id=$1 AND product_id<>$2 LIMIT 1",vec![id.into(),product.as_str().into()]).await?;
            if foreign.is_some() {
                return Err(AppError::NotFound.into());
            }
        }
        let row=one(&tx,&format!("SELECT COALESCE(max(revision),0) AS revision FROM character_revisions WHERE character_id=$1{}",crate::learning::product_filter(product,"product_id")),vec![id.into()]).await?.ok_or(AppError::Unavailable)?;
        if field::<i32>(&row, "revision")? as u32 != expected {
            return Err(AppError::Conflict.into());
        }
    }
    // Official imports share this lock, so duplicate diagnostics remain stable under concurrency.
    // Check every revision before registering any member of the batch.
    for (index, spec) in bundle.assets.iter().enumerate() {
        let existing = one(
            &tx,
            &format!(
                "SELECT revision FROM media_assets WHERE asset_id=$1 AND revision=$2{}",
                if local_ids {
                    crate::learning::product_filter(product, "product_id")
                } else {
                    String::new()
                }
            ),
            vec![spec.asset_id.clone().into(), (spec.revision as i32).into()],
        )
        .await
        .map_err(anyhow::Error::msg)?;
        ensure!(existing.is_none() || operator.is_none(), AppError::Conflict);
        ensure!(
            existing.is_none(),
            "/assets/{index}/revision: asset revision already registered"
        );
    }
    for (index, character) in bundle.characters.iter().enumerate() {
        let existing = one(
            &tx,
            &format!(
                "SELECT revision FROM character_revisions WHERE character_id=$1 AND revision=$2{}",
                if local_ids {
                    crate::learning::product_filter(product, "product_id")
                } else {
                    String::new()
                }
            ),
            vec![
                character.snapshot.character_id.clone().into(),
                (character.snapshot.revision as i32).into(),
            ],
        )
        .await
        .map_err(anyhow::Error::msg)?;
        ensure!(
            existing.is_none(),
            "/characters/{index}/snapshot/revision: character revision already registered"
        );
    }
    for (spec, (descriptor, ext, size)) in bundle.assets.iter().zip(&descriptors) {
        let mut values = vec![
            spec.asset_id.clone().into(),
            (spec.revision as i32).into(),
            serde_json::to_value(descriptor)?.into(),
            serde_json::to_value(spec)?.into(),
            spec.sha256.clone().into(),
            ext.clone().into(),
            (*size as i64).into(),
        ];
        let sql = if let Some(product) = product {
            values.push(product.as_str().into());
            "INSERT INTO media_assets(asset_id,revision,descriptor,provenance,sha256,extension,byte_size,product_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8)"
        } else {
            "INSERT INTO media_assets(asset_id,revision,descriptor,provenance,sha256,extension,byte_size) VALUES($1,$2,$3,$4,$5,$6,$7)"
        };
        exec(&tx, sql, values).await.map_err(anyhow::Error::msg)?;
    }
    let mut ids = BTreeSet::new();
    for (index, character) in bundle.characters.iter().enumerate() {
        let p = format!("/characters/{index}");
        let snapshot = &character.snapshot;
        ensure!(
            valid_id(&snapshot.character_id)
                && brioche_course_contract::valid_content_revision(snapshot.revision)
                && text(&snapshot.display_name)
                && supported_character_locale(&snapshot.speech_locale)
                && brioche_course_contract::valid_content_revision(character.avatar_revision)
                && ids.insert((&snapshot.character_id, snapshot.revision)),
            "invalid or duplicate character"
        );
        let asset = one(
            &tx,
            &format!(
                "SELECT descriptor FROM media_assets WHERE asset_id=$1 AND revision=$2{}",
                crate::learning::product_filter(product, "product_id")
            ),
            vec![
                snapshot.avatar_id.clone().into(),
                (character.avatar_revision as i32).into(),
            ],
        )
        .await
        .map_err(anyhow::Error::msg)?
        .with_context(|| format!("{p}/avatarRevision: character avatar revision is missing"))?;
        let descriptor: MediaAsset =
            serde_json::from_value(field(&asset, "descriptor").map_err(anyhow::Error::msg)?)?;
        ensure!(
            descriptor.width == descriptor.height,
            "{p}/snapshot/avatarId: avatar must be square"
        );
        let mut values = vec![
            snapshot.character_id.clone().into(),
            (snapshot.revision as i32).into(),
            serde_json::to_value(snapshot)?.into(),
            snapshot.avatar_id.clone().into(),
            (character.avatar_revision as i32).into(),
        ];
        let sql = if let Some(product) = product {
            values.push(product.as_str().into());
            "INSERT INTO character_revisions(character_id,revision,snapshot,avatar_id,avatar_revision,product_id) VALUES($1,$2,$3,$4,$5,$6)"
        } else {
            "INSERT INTO character_revisions(character_id,revision,snapshot,avatar_id,avatar_revision) VALUES($1,$2,$3,$4,$5)"
        };
        exec(&tx, sql, values).await.map_err(anyhow::Error::msg)?;
    }
    let target = operator.map(|_| {
        bundle
            .assets
            .iter()
            .map(|a| format!("{} v{}", a.asset_id, a.revision))
            .chain(
                bundle
                    .characters
                    .iter()
                    .map(|c| format!("{} v{}", c.snapshot.character_id, c.snapshot.revision)),
            )
            .collect::<Vec<_>>()
            .join(", ")
    });
    let mut values = vec![
        actor.into(),
        bundle_hash.into(),
        (bundle.assets.len() as i32).into(),
        (bundle.characters.len() as i32).into(),
        operator.map(|o| o.operator.actor).into(),
        operator.map(|o| o.reason.to_owned()).into(),
        target.into(),
    ];
    let sql = if let Some(product) = product {
        values.push(product.as_str().into());
        "INSERT INTO asset_import_audit(actor,bundle_hash,asset_count,character_count,actor_id,reason,target,product_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8)"
    } else {
        "INSERT INTO asset_import_audit(actor,bundle_hash,asset_count,character_count,actor_id,reason,target) VALUES($1,$2,$3,$4,$5,$6,$7)"
    };
    exec(&tx, sql, values).await.map_err(anyhow::Error::msg)?;
    tx.commit().await?;
    Ok(())
}
pub fn source_asset_refs(source: &serde_json::Value) -> Result<Vec<AssetRef>> {
    source_refs(source, "assetRefs")
}
pub(crate) fn source_refs(source: &serde_json::Value, key: &str) -> Result<Vec<AssetRef>> {
    let Some(refs) = source.get(key) else {
        return Ok(Vec::new());
    };
    let refs: Vec<AssetRef> = crate::author_json::from_value(refs.clone(), &format!("/{key}"))?;
    ensure!(refs.len() <= 500, "/{key}: too many asset references");
    let mut ids = BTreeSet::new();
    for (index, reference) in refs.iter().enumerate() {
        ensure!(
            valid_id(&reference.asset_id),
            "/{key}/{index}/assetId: invalid asset ID"
        );
        ensure!(
            brioche_course_contract::valid_content_revision(reference.revision),
            "/{key}/{index}/revision: expected revision in database range"
        );
        ensure!(
            ids.insert(reference.asset_id.clone()),
            "/{key}/{index}/assetId: duplicate asset reference"
        );
    }
    Ok(refs)
}

pub async fn hydrate_source<C: ConnectionTrait>(
    db: &C,
    source: serde_json::Value,
) -> Result<serde_json::Value> {
    hydrate_source_for_product(db, None, source).await
}
pub(crate) async fn hydrate_source_for_product<C: ConnectionTrait>(
    db: &C,
    product: Option<crate::product::ProductId>,
    mut source: serde_json::Value,
) -> Result<serde_json::Value> {
    if source.get("assetRefs").is_none() {
        return Ok(source);
    }
    let refs = source_asset_refs(&source)?;
    let mut descriptors = Vec::new();
    for (index, reference) in refs.into_iter().enumerate() {
        let row = one(
            db,
            &format!(
                "SELECT descriptor FROM media_assets WHERE asset_id=$1 AND revision=$2{}",
                crate::learning::product_filter(product, "product_id")
            ),
            vec![
                reference.asset_id.into(),
                (reference.revision as i32).into(),
            ],
        )
        .await
        .map_err(anyhow::Error::msg)?
        .with_context(|| {
            format!("/assetRefs/{index}/revision: registered asset revision missing")
        })?;
        descriptors
            .push(field::<serde_json::Value>(&row, "descriptor").map_err(anyhow::Error::msg)?);
    }
    source["media"] = serde_json::Value::Array(descriptors);
    Ok(source)
}
// Draft imports may embed descriptors rather than assetRefs. Verify their
// product ownership too, without requiring draft publication/file checks.
pub(crate) async fn validate_checked_product_references<C: ConnectionTrait>(
    db: &C,
    product: Option<crate::product::ProductId>,
    lesson: &crate::author_source::CheckedLesson,
) -> Result<()> {
    validate_asset_product_references(db, product, lesson.media(), lesson.cast()).await
}
async fn validate_asset_product_references<C: ConnectionTrait>(
    db: &C,
    product: Option<crate::product::ProductId>,
    assets: &[MediaAsset],
    cast: &[Character],
) -> Result<()> {
    let Some(product) = product else {
        return Ok(());
    };
    for (index, asset) in assets.iter().enumerate() {
        let row = one(db,"SELECT 1 AS registered FROM media_assets WHERE product_id=$1 AND asset_id=$2 AND revision=$3",vec![product.as_str().into(),asset.asset_id.clone().into(),(asset.revision as i32).into()]).await?;
        ensure!(
            row.is_some(),
            "/media/{index}/revision: visual asset revision is not registered for product"
        );
    }
    for (index, character) in cast.iter().enumerate() {
        let row = one(db,"SELECT 1 AS registered FROM character_revisions WHERE product_id=$1 AND character_id=$2 AND revision=$3",vec![product.as_str().into(),character.character_id.clone().into(),(character.revision as i32).into()]).await?;
        ensure!(
            row.is_some(),
            "/cast/{index}/revision: character revision is not registered for product"
        );
    }
    Ok(())
}
/// Publication diagnostics are local-author only; HTTP callers keep AppError.
#[derive(Debug)]
pub(crate) struct PublicationFailure {
    pub runtime: AppError,
    pub diagnostic: String,
}
impl From<AppError> for PublicationFailure {
    fn from(runtime: AppError) -> Self {
        Self {
            runtime,
            diagnostic:
                "publication registry operation failed; verify storage and database availability"
                    .into(),
        }
    }
}
impl PublicationFailure {
    pub(crate) fn at(pointer: &str, message: &str) -> Self {
        Self {
            runtime: AppError::InvalidInput,
            diagnostic: format!("imported lesson {pointer}: {message}"),
        }
    }
}
pub async fn validate_lesson<C: ConnectionTrait>(
    db: &C,
    lesson: &PublicLesson,
    root: &Path,
) -> Result<(), AppError> {
    validate_lesson_detailed(db, None, lesson, root)
        .await
        .map_err(|error| error.runtime)
}
pub(crate) async fn validate_lesson_detailed<C: ConnectionTrait>(
    db: &C,
    product: Option<crate::product::ProductId>,
    lesson: &PublicLesson,
    root: &Path,
) -> Result<(), PublicationFailure> {
    lesson
        .validate()
        .map_err(|_| PublicationFailure::at("/", "public lesson validation failed"))?;
    crate::recording::validate_lesson_detailed(db, product, lesson, root).await?;
    let scenes = lesson
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(i, b)| match b {
            Block::Scene {
                illustration_id, ..
            } => Some((i, illustration_id.as_str())),
            _ => None,
        })
        .collect::<Vec<_>>();
    validate_assets_detailed(db, product, &lesson.media, &lesson.cast, &scenes, root).await
}
pub(crate) async fn validate_checked_lesson_detailed<C: ConnectionTrait>(
    db: &C,
    product: Option<crate::product::ProductId>,
    lesson: &crate::author_source::CheckedLesson,
    root: &Path,
) -> Result<(), PublicationFailure> {
    lesson
        .validate_public()
        .map_err(|_| PublicationFailure::at("/", "public lesson validation failed"))?;
    crate::recording::validate_checked_lesson_detailed(db, product, lesson, root).await?;
    validate_assets_detailed(
        db,
        product,
        lesson.media(),
        lesson.cast(),
        &lesson.scene_references(),
        root,
    )
    .await
}
async fn validate_assets_detailed<C: ConnectionTrait>(
    db: &C,
    product: Option<crate::product::ProductId>,
    assets: &[MediaAsset],
    cast: &[Character],
    scenes: &[(usize, &str)],
    root: &Path,
) -> Result<(), PublicationFailure> {
    let mut ids = BTreeSet::new();
    for (index, asset) in assets.iter().enumerate() {
        let pointer = format!("/media/{index}");
        if !ids.insert(asset.asset_id.as_str()) {
            return Err(PublicationFailure::at(
                &format!("{pointer}/assetId"),
                "duplicate visual asset ID",
            ));
        }
        if !brioche_course_contract::valid_content_revision(asset.revision) {
            return Err(PublicationFailure::at(
                &format!("{pointer}/revision"),
                "visual asset revision outside database range",
            ));
        }
        let row = one(
            db,
            &format!(
                "SELECT descriptor FROM media_assets WHERE asset_id=$1 AND revision=$2{}",
                crate::learning::product_filter(product, "product_id")
            ),
            vec![
                asset.asset_id.clone().into(),
                (asset.revision as i32).into(),
            ],
        )
        .await?
        .ok_or_else(|| {
            PublicationFailure::at(
                &format!("{pointer}/revision"),
                "visual asset revision is not registered",
            )
        })?;
        if field::<serde_json::Value>(&row, "descriptor")?
            != serde_json::to_value(asset).map_err(|_| AppError::Unavailable)?
        {
            return Err(PublicationFailure::at(
                &pointer,
                "visual descriptor does not match registered revision",
            ));
        }
        let root = root.to_path_buf();
        let asset = asset.clone();
        tokio::task::spawn_blocking(move || -> std::result::Result<(), &'static str> {
            let ext = extension(&asset.mime_type).map_err(|_| "unsupported visual format")?;
            let bytes = stored_bytes(&root, &asset.sha256, ext)
                .map_err(|_| "stored visual object is missing or unreadable")?;
            if digest(&bytes) != asset.sha256 {
                return Err("stored visual object hash does not match registered revision");
            }
            Ok(())
        })
        .await
        .map_err(|_| AppError::Unavailable)?
        .map_err(|message| PublicationFailure::at(&format!("{pointer}/sha256"), message))?;
    }
    for (index, illustration_id) in scenes {
        if !ids.contains(*illustration_id) {
            return Err(PublicationFailure::at(
                &format!("/blocks/{index}/illustrationId"),
                "scene illustration is absent from registered lesson media",
            ));
        }
    }
    for (index, character) in cast.iter().enumerate() {
        let pointer = format!("/cast/{index}");
        if !brioche_course_contract::valid_content_revision(character.revision) {
            return Err(PublicationFailure::at(
                &format!("{pointer}/revision"),
                "character revision outside database range",
            ));
        }
        let row=one(db,&format!("SELECT snapshot,avatar_revision FROM character_revisions WHERE character_id=$1 AND revision=$2{}",crate::learning::product_filter(product,"product_id")),vec![character.character_id.clone().into(),(character.revision as i32).into()]).await?
            .ok_or_else(|| PublicationFailure::at(&format!("{pointer}/revision"), "character revision is not registered"))?;
        let avatar_revision = field::<i32>(&row, "avatar_revision")?;
        if field::<serde_json::Value>(&row, "snapshot")?
            != serde_json::to_value(character).map_err(|_| AppError::Unavailable)?
        {
            return Err(PublicationFailure::at(
                &pointer,
                "character snapshot does not match registered revision",
            ));
        }
        if !assets.iter().any(|asset| {
            asset.asset_id == character.avatar_id && asset.revision == avatar_revision as u32
        }) {
            return Err(PublicationFailure::at(
                &format!("{pointer}/avatarId"),
                "registered character avatar revision is absent from lesson media",
            ));
        }
    }
    Ok(())
}
pub(crate) fn stored_bytes(root: &Path, sha: &str, ext: &str) -> Result<Vec<u8>> {
    let root = root.canonicalize()?;
    let path = root.join(format!("{sha}.{ext}")).canonicalize()?;
    ensure!(path.starts_with(&root), "stored object escapes media root");
    read_file(&path)
}
#[derive(Clone)]
struct MediaState {
    product: Option<crate::product::ProductId>,
    db: DatabaseConnection,
    root: PathBuf,
    permits: std::sync::Arc<tokio::sync::Semaphore>,
}
pub fn router(db: DatabaseConnection, root: PathBuf) -> axum::Router {
    build_router(db, root, None)
}
/// Fixed by deployment assembly; clients cannot select another product.
pub fn product_router(
    db: DatabaseConnection,
    root: PathBuf,
    product: crate::product::ProductId,
) -> axum::Router {
    build_router(db, root, Some(product))
}
fn build_router(
    db: DatabaseConnection,
    root: PathBuf,
    product: Option<crate::product::ProductId>,
) -> axum::Router {
    axum::Router::new()
        .route("/api/media/{name}", axum::routing::get(serve))
        .with_state(MediaState {
            product,
            db,
            root,
            permits: std::sync::Arc::new(tokio::sync::Semaphore::new(2)),
        })
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicMediaQuery {}
async fn serve(
    axum::extract::State(state): axum::extract::State<MediaState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    axum::extract::Query(_query): axum::extract::Query<PublicMediaQuery>,
) -> Result<axum::response::Response, AppError> {
    let Some((sha, ext)) = name.split_once('.') else {
        return Err(AppError::NotFound);
    };
    if sha.len() != 64
        || !sha
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || !matches!(ext, "svg" | "png" | "jpg" | "webp")
    {
        return Err(AppError::NotFound);
    }
    // Staged or unreferenced media are never made public by registration alone.
    let row = one(&state.db, &format!("SELECT descriptor FROM media_assets m WHERE sha256=$1 AND extension=$2{} AND EXISTS(SELECT 1 FROM lesson_revisions r WHERE r.published{} AND NOT EXISTS(SELECT 1 FROM content_withdrawals w WHERE (w.lesson_id,w.revision)=(r.lesson_id,r.revision){}) AND r.public_document->'media' @> jsonb_build_array(jsonb_build_object('assetId',m.asset_id,'revision',m.revision))) LIMIT 1",
        crate::learning::product_filter(state.product,"m.product_id"),
        if state.product.is_some() { " AND r.product_id=m.product_id" } else { "" },
        if state.product.is_some() { " AND w.product_id=r.product_id" } else { "" }),
        vec![sha.into(),ext.into()]).await?.ok_or(AppError::NotFound)?;
    let descriptor: MediaAsset =
        serde_json::from_value(field(&row, "descriptor")?).map_err(|_| AppError::Unavailable)?;
    asset_response(state.root, descriptor, state.permits).await
}

pub(crate) async fn asset_response(
    root: PathBuf,
    descriptor: MediaAsset,
    permits: std::sync::Arc<tokio::sync::Semaphore>,
) -> Result<axum::response::Response, AppError> {
    let ext = extension(&descriptor.mime_type)
        .map_err(|_| AppError::Unavailable)?
        .to_owned();
    let sha = descriptor.sha256.clone();
    if sha.len() != 64
        || !sha
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(AppError::Unavailable);
    }
    let permit = crate::media_read::acquire(permits).await?;
    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
        let _permit = permit;
        let bytes = stored_bytes(&root, &sha, &ext)?;
        ensure!(digest(&bytes) == sha, "media object corrupt");
        Ok(bytes)
    })
    .await
    .map_err(|_| AppError::Unavailable)?
    .map_err(|_| AppError::Unavailable)?;
    axum::response::Response::builder()
        .header("content-type", descriptor.mime_type)
        .header("cache-control", "no-store")
        .header("x-content-type-options", "nosniff")
        .header("cross-origin-resource-policy", "same-origin")
        .header("content-security-policy", "default-src 'none'; sandbox")
        .body(axum::body::Body::from(bytes))
        .map_err(|_| AppError::Unavailable)
}
// Scope duplicate/CAS checks only when both registry identities are actually product-local.
async fn local_visual_keys(db: &impl ConnectionTrait) -> Result<bool, AppError> {
    let row=one(db,r#"SELECT count(*)=2 AS ready FROM pg_catalog.pg_constraint c
        WHERE c.contype='p' AND (
            (c.conrelid='media_assets'::regclass AND (SELECT array_agg(a.attname::text ORDER BY k.position) FROM unnest(c.conkey) WITH ORDINALITY k(column_number,position) JOIN pg_catalog.pg_attribute a ON a.attrelid=c.conrelid AND a.attnum=k.column_number)=ARRAY['product_id','asset_id','revision']::text[])
            OR (c.conrelid='character_revisions'::regclass AND (SELECT array_agg(a.attname::text ORDER BY k.position) FROM unnest(c.conkey) WITH ORDINALITY k(column_number,position) JOIN pg_catalog.pg_attribute a ON a.attrelid=c.conrelid AND a.attnum=k.column_number)=ARRAY['product_id','character_id','revision']::text[])
        )"#,vec![]).await?.ok_or(AppError::Unavailable)?;
    field(&row, "ready")
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageFormat;
    #[test]
    fn bundled_avatars_use_exact_revisions_and_defer_registry_references() {
        let mut bundle: AssetBundle =
            serde_json::from_str(include_str!("../../../docs/examples/asset-bundle.json")).unwrap();
        for asset in &mut bundle.assets {
            asset.status = "ready".into();
            asset.rights_confirmed = true;
            asset.license = "LicenseRef-TestOnly".into();
        }
        assert!(bundle.validate_author("protocol-test").is_ok());
        let index = bundle
            .assets
            .iter()
            .position(|asset| asset.asset_id == bundle.characters[0].snapshot.avatar_id)
            .unwrap();
        bundle.assets[index].width -= 1;
        assert!(
            bundle
                .validate_author("protocol-test")
                .unwrap_err()
                .to_string()
                .contains("/characters/0/snapshot/avatarId: avatar must be square")
        );
        bundle.characters[0].avatar_revision = 2;
        // Revision 2 may be in the existing registry even when only revision 1 is bundled.
        assert!(bundle.validate_author("protocol-test").is_ok());
        let mut next = bundle.assets[index].clone();
        next.revision = 2;
        next.width = next.height;
        bundle.assets.push(next);
        assert!(bundle.validate_author("protocol-test").is_ok());
        bundle.characters[0].avatar_revision = 1;
        assert!(bundle.validate_author("protocol-test").is_err());
        bundle.characters[0].avatar_revision = 3;
        assert!(bundle.validate_author("protocol-test").is_ok());
        bundle.assets.clear();
        assert!(bundle.validate_author("protocol-test").is_ok());
    }
    #[test]
    fn raster_validation_decodes_supported_formats_and_checks_mime() {
        for (format, mime) in [
            (ImageFormat::Png, "image/png"),
            (ImageFormat::Jpeg, "image/jpeg"),
            (ImageFormat::WebP, "image/webp"),
        ] {
            let image = image::DynamicImage::ImageRgb8(image::RgbImage::new(3, 2));
            let mut output = std::io::Cursor::new(Vec::new());
            image.write_to(&mut output, format).unwrap();
            assert_eq!(dimensions(output.get_ref(), mime).unwrap(), (3, 2));
            let wrong_mime = if mime == "image/png" {
                "image/jpeg"
            } else {
                "image/png"
            };
            assert!(dimensions(output.get_ref(), wrong_mime).is_err());
            assert!(dimensions(&output.get_ref()[..8], mime).is_err());
        }
    }
    #[test]
    fn graphics_validation_rejects_active_svg_and_bad_dimensions() {
        assert_eq!(
            svg_dimensions(include_bytes!("../../../test-fixtures/visuals/bakery.svg")).unwrap(),
            (640, 470)
        );
        assert_eq!(
            svg_dimensions(include_bytes!(
                "../../../test-fixtures/visuals/avatars/camille.svg"
            ))
            .unwrap(),
            (96, 96)
        );
        for bytes in [
            br#"<svg viewBox="0 0 10 10"><script>alert(1)</script></svg>"#.as_slice(),
            br#"<!DOCTYPE svg SYSTEM "http://evil.test"><svg viewBox="0 0 10 10"/>"#,
            br#"<svg viewBox="0 0 10 10"><path onload="alert(1)"/></svg>"#,
            br#"<svg viewBox="0 0 10 10"><path fill="url(https://evil.test)"/></svg>"#,
            br#"<svg viewBox="0 0 NaN 10"/>"#,
            br#"<svg viewBox="0 0 10 10"><g></svg>"#,
            br#"<svg viewBox="0 0 10 10"/><svg viewBox="0 0 10 10"/>"#,
        ] {
            assert!(svg_dimensions(bytes).is_err());
        }
        assert!(dimensions(b"not a PNG", "image/png").is_err());
    }
}
