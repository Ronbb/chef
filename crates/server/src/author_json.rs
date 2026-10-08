//! Bounded author files; duplicate object members must never silently overwrite content.
use anyhow::{Context, Result, bail};
use serde::{
    Deserializer,
    de::{self, DeserializeOwned, DeserializeSeed, MapAccess, SeqAccess, Visitor},
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fmt,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

const LIMIT: usize = 2 * 1024 * 1024;

/// HTTP author imports use the same duplicate-member and size checks as local files.
pub fn parse_document(bytes: &[u8]) -> Result<Value> {
    parse_document_bounded(bytes, LIMIT)
}
pub(crate) fn parse_document_bounded(bytes: &[u8], limit: usize) -> Result<Value> {
    if bytes.len() > limit {
        bail!("JSON exceeds {limit} bytes");
    }
    Ok(parse(bytes)?)
}

fn read(path: &Path) -> Result<Vec<u8>> {
    let file =
        File::open(path).with_context(|| format!("{}: cannot open author file", path.display()))?;
    let mut bytes = Vec::new();
    file.take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .with_context(|| format!("{}: cannot read author file", path.display()))?;
    if bytes.len() > LIMIT {
        bail!("{}: author JSON exceeds 2 MiB", path.display());
    }
    parse(&bytes).with_context(|| format!("{}: invalid author JSON", path.display()))?;
    Ok(bytes)
}

pub fn load<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<T> {
    let path = path.as_ref();
    let bytes = read(path)?;
    let mut deserializer = serde_json::Deserializer::from_slice(&bytes);
    serde_path_to_error::deserialize(&mut deserializer)
        .with_context(|| format!("{}: invalid author document", path.display()))
}

pub fn from_value<T: DeserializeOwned>(value: Value, prefix: &str) -> Result<T> {
    serde_path_to_error::deserialize(value).map_err(|error| {
        use serde_path_to_error::Segment;
        let mut pointer = prefix.to_owned();
        for segment in error.path().iter() {
            let token = match segment {
                Segment::Seq { index } => index.to_string(),
                Segment::Map { key } => key.clone(),
                Segment::Enum { .. } | Segment::Unknown => continue,
            };
            pointer.push('/');
            pointer.push_str(&token.replace('~', "~0").replace('/', "~1"));
        }
        anyhow::anyhow!(
            "{}: {}",
            if pointer.is_empty() { "/" } else { &pointer },
            error.inner()
        )
    })
}

/// Original source positions, retained before any author-to-public projection.
pub struct Document {
    pub value: Value,
    path: PathBuf,
    text: String,
    offsets: BTreeMap<String, usize>,
}
impl Document {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let bytes = read(path)?;
        Self::from_bytes(path, &bytes)
    }
    pub(crate) fn from_bytes(path: &Path, bytes: &[u8]) -> Result<Self> {
        if bytes.len() > LIMIT {
            bail!("author JSON exceeds 2 MiB");
        }
        let value = parse(bytes)?;
        let mut index = SourceIndex {
            bytes,
            position: 0,
            offsets: BTreeMap::new(),
        };
        index
            .value(String::new())
            .with_context(|| format!("{}: cannot index author source", path.display()))?;
        Ok(Self {
            value,
            path: path.to_owned(),
            offsets: index.offsets,
            text: String::from_utf8(bytes.to_vec()).expect("validated JSON is UTF-8"),
        })
    }
    pub fn diagnostic(&self, pointer: &str, message: &str) -> anyhow::Error {
        let (line, column) = self.location(pointer);
        anyhow::anyhow!(
            "{}:{line}:{column}: {}: {message}",
            self.path.display(),
            if pointer.is_empty() { "/" } else { pointer }
        )
    }
    fn location(&self, pointer: &str) -> (usize, usize) {
        let mut found = if pointer == "/" { "" } else { pointer };
        while !self.offsets.contains_key(found) {
            found = found.rsplit_once('/').map_or("", |(parent, _)| parent);
        }
        let before = &self.text[..self.offsets[found]];
        let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
        let column = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
        (line, column)
    }
    pub fn semantic(&self, error: anyhow::Error) -> anyhow::Error {
        // Existing domain validators expose JSON pointers. Locate their first
        // diagnostic in the unmodified source rather than serialized DTO text.
        for cause in error.chain() {
            let message = cause.to_string();
            if let Some((pointer, reason)) = message.split_once(": ")
                && pointer.starts_with('/')
            {
                return self.diagnostic(pointer, reason);
            }
        }
        self.diagnostic("", &format!("{error:#}"))
    }
}

/// Authenticated Web preflight returns positions only, never author values,
/// private answers, filesystem paths or raw parser/provider/database errors.
#[cfg(test)]
pub(crate) fn check_uploaded(
    bytes: &[u8],
    release: bool,
) -> brioche_course_contract::AdminDocumentCheck {
    match prepare_uploaded(bytes, release) {
        Ok(_) => brioche_course_contract::AdminDocumentCheck {
            valid: true,
            issue: None,
        },
        Err(report) => report,
    }
}
pub(crate) fn prepare_uploaded(
    bytes: &[u8],
    release: bool,
) -> std::result::Result<Document, brioche_course_contract::AdminDocumentCheck> {
    use brioche_course_contract::{AdminDocumentCheck, AdminDocumentIssue};
    let document = match Document::from_bytes(Path::new("uploaded.json"), bytes) {
        Ok(document) => document,
        Err(error) => {
            let position = error
                .chain()
                .find_map(|cause| cause.downcast_ref::<serde_json::Error>());
            return Err(AdminDocumentCheck {
                valid: false,
                issue: Some(AdminDocumentIssue {
                    pointer: "/".into(),
                    line: position.map_or(1, |error| error.line().max(1)) as u32,
                    column: position.map_or(1, |error| error.column().max(1)) as u32,
                    message_zh: "JSON 格式不正确、字段重复或文件超过限制。".into(),
                }),
            });
        }
    };
    let validation = if release {
        from_value::<crate::content::ReleaseManifest>(document.value.clone(), "")
            .and_then(|manifest| manifest.validate_author())
    } else {
        crate::media::source_asset_refs(&document.value)
            .and_then(|_| crate::recording::source_audio_refs(&document.value))
            .and_then(|_| crate::author_source::validate_any_source_schema(document.value.clone()))
    };
    match validation {
        Ok(()) => Ok(document),
        Err(error) => {
            let pointer = error
                .chain()
                .find_map(|cause| {
                    let message = cause.to_string();
                    let (pointer, _) = message.split_once(": ")?;
                    (pointer.starts_with('/')
                        && pointer.len() <= 1024
                        && !pointer.chars().any(char::is_control))
                    .then(|| pointer.to_owned())
                })
                .unwrap_or_else(|| "/".into());
            Err(document.uploaded_issue(&pointer, "字段类型、课程结构或引用规则不符合要求。"))
        }
    }
}
impl Document {
    pub(crate) fn uploaded_issue(
        &self,
        pointer: &str,
        message: &str,
    ) -> brioche_course_contract::AdminDocumentCheck {
        let mut pointer = if pointer.starts_with('/')
            && pointer.len() <= 1024
            && !pointer.chars().any(char::is_control)
        {
            pointer.to_owned()
        } else {
            "/".into()
        };
        // Hydrated registry descriptors are not author fields. Point back to the
        // actual reference object in the uploaded source, not an invented field.
        for (hydrated, authored) in [("media", "assetRefs"), ("audio", "audioRefs")] {
            if self.value.get(authored).is_some()
                && let Some(rest) = pointer.strip_prefix(&format!("/{hydrated}/"))
            {
                let index = rest.split('/').next().unwrap_or("");
                if let Ok(index) = index.parse::<usize>()
                    && self.value[authored].get(index).is_some()
                {
                    pointer = format!("/{authored}/{index}");
                }
            }
        }
        let (line, column) = self.location(&pointer);
        brioche_course_contract::AdminDocumentCheck {
            valid: false,
            issue: Some(brioche_course_contract::AdminDocumentIssue {
                pointer,
                line: line as u32,
                column: column as u32,
                message_zh: message.into(),
            }),
        }
    }
}

// This only indexes JSON already accepted by the strict parser above; it does
// not replace syntax, duplicate-member, UTF-8 or depth validation.
struct SourceIndex<'a> {
    bytes: &'a [u8],
    position: usize,
    offsets: BTreeMap<String, usize>,
}
impl SourceIndex<'_> {
    fn whitespace(&mut self) {
        while self
            .bytes
            .get(self.position)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.position += 1;
        }
    }
    fn string(&mut self) -> &[u8] {
        let start = self.position;
        self.position += 1;
        loop {
            match self.bytes[self.position] {
                b'\\' => self.position += 2,
                b'"' => {
                    self.position += 1;
                    break;
                }
                _ => self.position += 1,
            }
        }
        &self.bytes[start..self.position]
    }
    fn value(&mut self, pointer: String) -> Result<()> {
        self.whitespace();
        if self.offsets.len() >= 100_000 {
            bail!("author JSON exceeds 100000 source locations");
        }
        self.offsets.insert(pointer.clone(), self.position);
        match self.bytes[self.position] {
            b'{' => {
                self.position += 1;
                self.whitespace();
                if self.bytes[self.position] != b'}' {
                    loop {
                        let key: String = serde_json::from_slice(self.string())?;
                        self.whitespace();
                        self.position += 1;
                        self.value(format!(
                            "{}/{}",
                            pointer,
                            key.replace('~', "~0").replace('/', "~1")
                        ))?;
                        self.whitespace();
                        if self.bytes[self.position] == b'}' {
                            break;
                        }
                        self.position += 1;
                        self.whitespace();
                    }
                }
                self.position += 1;
            }
            b'[' => {
                self.position += 1;
                self.whitespace();
                if self.bytes[self.position] != b']' {
                    let mut item = 0;
                    loop {
                        self.value(format!("{pointer}/{item}"))?;
                        item += 1;
                        self.whitespace();
                        if self.bytes[self.position] == b']' {
                            break;
                        }
                        self.position += 1;
                    }
                }
                self.position += 1;
            }
            b'"' => {
                self.string();
            }
            _ => {
                while self
                    .bytes
                    .get(self.position)
                    .is_some_and(|b| !b.is_ascii_whitespace() && !matches!(b, b',' | b']' | b'}'))
                {
                    self.position += 1;
                }
            }
        }
        Ok(())
    }
}

fn parse(bytes: &[u8]) -> Result<Value, serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = Node(String::new()).deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(value)
}

struct Node(String);
impl<'de> DeserializeSeed<'de> for Node {
    type Value = Value;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Node {
    type Value = Value;
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a JSON value")
    }
    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> {
        Ok(value.into())
    }
    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Value, E> {
        Ok(value.into())
    }
    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Value, E> {
        Ok(value.into())
    }
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Value, E> {
        serde_json::Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("non-finite number"))
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> {
        Ok(value.into())
    }
    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) =
            seq.next_element_seed(Node(format!("{}/{}", self.0, values.len())))?
        {
            values.push(value);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut values = serde_json::Map::new();
        while let Some(key) = map.next_key::<String>()? {
            let pointer = format!("{}/{}", self.0, key.replace('~', "~0").replace('/', "~1"));
            if values.contains_key(&key) {
                return Err(de::Error::custom(format!(
                    "duplicate JSON field at {pointer}"
                )));
            }
            values.insert(key, map.next_value_seed(Node(pointer))?);
        }
        Ok(Value::Object(values))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_duplicate_members_with_pointer_and_location() {
        let error = parse(br#"{"steps":[{"a/b~c":1,"a/b~c":2}]}"#).unwrap_err();
        assert!(error.to_string().contains("/steps/0/a~1b~0c"));
        assert_eq!(error.line(), 1);
        assert!(error.column() > 1);
        assert!(parse(br#"{"id":1,"\u0069d":2}"#).is_err());
        assert!(parse(br#"{"a":{"id":1},"b":{"id":2}}"#).is_ok());
    }
    #[test]
    fn rejects_trailing_invalid_and_deep_documents() {
        for bytes in [b"{} {}".as_slice(), b"{", &[0xff]] {
            assert!(parse(bytes).is_err());
        }
        let deep = format!("{}0{}", "[".repeat(150), "]".repeat(150));
        assert!(parse(deep.as_bytes()).is_err());
        assert_eq!(
            parse(br#"[null,true,-1,18446744073709551615,1.25,"bonjour"]"#).unwrap(),
            serde_json::from_slice::<Value>(
                br#"[null,true,-1,18446744073709551615,1.25,"bonjour"]"#
            )
            .unwrap()
        );
    }
    #[test]
    fn bounds_actual_file_reads_and_names_the_file() {
        let path = std::env::temp_dir().join(format!(
            "brioche-author-{}.json",
            crate::learning::random_id().unwrap()
        ));
        std::fs::write(&path, vec![b' '; LIMIT + 1]).unwrap();
        let error = load::<Value>(&path).unwrap_err().to_string();
        assert!(error.contains("2 MiB"));
        assert!(error.contains(path.file_name().unwrap().to_str().unwrap()));
        std::fs::write(&path, br#"{"id":1,"id":2}"#).unwrap();
        assert!(
            format!("{:#}", load::<Value>(&path).unwrap_err())
                .contains("duplicate JSON field at /id")
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn typed_errors_preserve_nested_field_and_original_line() {
        let path = std::env::temp_dir().join(format!(
            "brioche-author-{}.json",
            crate::learning::random_id().unwrap()
        ));
        std::fs::write(
            &path,
            br#"{
          "id":"test", "schemaVersion":"1.0",
          "levels":[{"id":"a1", "label":"A1", "units":[{
            "id":"unit", "titleZh":"Breakfast",
            "lessons":[{"lessonId":"lesson", "revision":"bad"}]
          }]}]
        }"#,
        )
        .unwrap();
        let error = format!(
            "{:#}",
            load::<crate::content::ReleaseManifest>(&path).unwrap_err()
        );
        std::fs::remove_file(path).unwrap();
        assert!(
            error.contains("levels[0].units[0].lessons[0].revision"),
            "{error}"
        );
        assert!(error.contains("line 5 column"), "{error}");
    }

    #[test]
    fn current_author_examples_remain_readable() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples");
        load::<Value>(root.join("a1-bakery.lesson.json")).unwrap();
        load::<crate::content::ReleaseManifest>(root.join("catalog.release.json")).unwrap();
        load::<crate::media::AssetBundle>(root.join("asset-bundle.json")).unwrap();
    }

    #[test]
    fn source_index_preserves_unicode_escaped_keys_arrays_and_nearest_parent() {
        let text = "{\r\n  \"汉字\": \"\\\"[]{}\",\r\n  \"a\\u002fb~c\": [true, {\"bad\": 7}],\r\n  \"empty\": []\r\n}";
        let bytes = text.as_bytes();
        parse(bytes).unwrap();
        let mut index = SourceIndex {
            bytes,
            position: 0,
            offsets: BTreeMap::new(),
        };
        index.value(String::new()).unwrap();
        let document = Document {
            value: parse(bytes).unwrap(),
            path: "original.json".into(),
            text: text.into(),
            offsets: index.offsets,
        };
        assert_eq!(
            document.value.pointer("/a~1b~0c/1/bad"),
            Some(&serde_json::json!(7))
        );
        assert!(
            document
                .diagnostic("/a~1b~0c/1/bad", "bad reference")
                .to_string()
                .contains("original.json:3:32:")
        );
        let expected = text.find('7').unwrap();
        assert_eq!(document.offsets["/a~1b~0c/1/bad"], expected);
        assert!(
            document
                .diagnostic("/汉字", "bad text")
                .to_string()
                .contains("original.json:2:9:")
        );
        assert!(
            document
                .diagnostic("/empty/missing", "missing")
                .to_string()
                .contains("original.json:4:12:")
        );
        let typed = from_value::<Vec<u32>>(serde_json::json!(["bad"]), "/items").unwrap_err();
        assert!(typed.to_string().starts_with("/items/0:"));
    }

    #[test]
    fn indexes_all_values_without_confusing_primitive_or_string_delimiters() {
        for text in [
            "null",
            "42",
            "-1.25e+2",
            "false",
            "\"é \\\" /\"",
            "{}",
            "[]",
            "[{},[],null,\"abc\",true,1]",
        ] {
            parse(text.as_bytes()).unwrap();
            let mut index = SourceIndex {
                bytes: text.as_bytes(),
                position: 0,
                offsets: BTreeMap::new(),
            };
            index.value(String::new()).unwrap();
            assert_eq!(index.position, text.len());
            for (pointer, offset) in index.offsets {
                let expected = parse(text.as_bytes())
                    .unwrap()
                    .pointer(&pointer)
                    .unwrap()
                    .clone();
                let actual = serde_json::Deserializer::from_slice(&text.as_bytes()[offset..])
                    .into_iter::<Value>()
                    .next()
                    .unwrap()
                    .unwrap();
                assert_eq!(actual, expected);
            }
        }
    }

    #[test]
    fn bounds_number_of_indexed_values() {
        let text = format!("[{}]", vec!["0"; 100_000].join(","));
        parse(text.as_bytes()).unwrap();
        let mut index = SourceIndex {
            bytes: text.as_bytes(),
            position: 0,
            offsets: BTreeMap::new(),
        };
        assert!(
            index
                .value(String::new())
                .unwrap_err()
                .to_string()
                .contains("100000 source locations")
        );
        assert_eq!(index.offsets.len(), 100_000);
    }
}
#[test]
fn uploaded_preflight_locates_fields_without_returning_private_values() {
    let mut source = crate::development_source().unwrap();
    let valid = serde_json::to_vec(&source).unwrap();
    assert!(check_uploaded(&valid, false).valid);
    let mut referenced = source.clone();
    referenced["assetRefs"] = serde_json::json!([{"assetId":"art-bakery-morning","revision":1}]);
    referenced["media"] = serde_json::json!("registered placeholder");
    referenced["audioRefs"] = serde_json::json!([]);
    referenced["audio"] = serde_json::json!("registered placeholder");
    let text = serde_json::to_vec(&referenced).unwrap();
    let prepared = prepare_uploaded(&text, false).unwrap();
    assert_eq!(
        prepared
            .uploaded_issue("/media/0/sha256", "safe message")
            .issue
            .unwrap()
            .pointer,
        "/assetRefs/0"
    );
    source["serverOnly"]["grading"]["exercise-intention"]["correctOptionId"] =
        serde_json::json!("private-answer-marker");
    let text = serde_json::to_string_pretty(&source)
        .unwrap()
        .replace('\n', "\r\n");
    let report = check_uploaded(text.as_bytes(), false);
    let issue = report.issue.as_ref().unwrap();
    assert!(!report.valid);
    assert_eq!(
        issue.pointer,
        "/serverOnly/grading/exercise-intention/correctOptionId"
    );
    let offset = text.find("\"private-answer-marker\"").unwrap();
    let before = &text[..offset];
    assert_eq!(
        issue.line as usize,
        before.bytes().filter(|b| *b == b'\n').count() + 1
    );
    assert_eq!(
        issue.column as usize,
        before.rsplit('\n').next().unwrap().chars().count() + 1
    );
    let serialized = serde_json::to_string(&report).unwrap();
    assert!(!serialized.contains("private-answer-marker"));
    assert!(!serialized.contains("uploaded.json"));
    for bytes in [
        b"{\"secret\":1,\"secret\":2}".as_slice(),
        b"{bad}",
        &[b' '; LIMIT + 1],
    ] {
        let report = check_uploaded(bytes, false);
        assert!(!report.valid);
        assert_eq!(report.issue.unwrap().pointer, "/");
    }
    let release = include_bytes!("../../../docs/examples/catalog.release.json");
    assert!(check_uploaded(release, true).valid);
    let mut source: Value = serde_json::from_slice(release).unwrap();
    source["levels"][0]["units"][0]["lessons"][0]["revision"] = serde_json::json!(0);
    let report = check_uploaded(&serde_json::to_vec(&source).unwrap(), true);
    assert_eq!(
        report.issue.unwrap().pointer,
        "/levels/0/units/0/lessons/0/revision"
    );
}
