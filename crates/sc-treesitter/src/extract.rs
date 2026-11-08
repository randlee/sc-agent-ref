//! Matches-mode extraction into `sc/subjects/1`.

use sc_agent_types::schema::subjects::{
    GrammarRef, Span, SubjectsBuilder, SubjectsDocument, SubjectsProducer, SubjectsSource,
};
use sc_agent_types::{BlobSha, JevError, RepoRelativePath, Rev, Sha256Digest};
use tree_sitter::{Node, Parser, Point, Query, QueryCursor, StreamingIterator};

use crate::grammar;
use crate::{PRODUCER_MODE, PRODUCER_NAME};

/// Source and query for one extraction.
///
/// The caller supplies the bytes. The library does not read git or the
/// filesystem. [`ExtractInput::new`] takes already-validated
/// [`RepoRelativePath`], [`Rev`] and [`BlobSha`] values, so a malformed path,
/// revision or blob id fails where the caller builds it and an input can never
/// lack one. [`ExtractInput::with_captures`] sets the optional allow-list.
///
/// # Examples
///
/// ```
/// use sc_agent_types::{BlobSha, RepoRelativePath, Rev};
///
/// let input = sc_treesitter::ExtractInput::new(
///     "python",
///     "(identifier) @name\n",
///     b"x = 1\n",
///     RepoRelativePath::try_from("sample.py")?,
///     Rev::parse("HEAD")?,
///     BlobSha::try_from("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")?,
/// );
/// assert_eq!(input.path().as_str(), "sample.py");
/// assert!(input.captures().is_none());
/// # Ok::<(), sc_agent_types::JevError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ExtractInput<'a> {
    language: &'a str,
    query: &'a str,
    captures: Option<&'a [&'a str]>,
    source: &'a [u8],
    path: RepoRelativePath,
    rev: Rev,
    blob_sha: BlobSha,
}

impl<'a> ExtractInput<'a> {
    /// Starts an input for `language`, `query`, and `source`.
    ///
    /// `path`, `rev`, and `blob_sha` are recorded in `source` of the document.
    /// `captures` starts as `None`.
    #[must_use]
    pub fn new(
        language: &'a str,
        query: &'a str,
        source: &'a [u8],
        path: RepoRelativePath,
        rev: Rev,
        blob_sha: BlobSha,
    ) -> Self {
        Self {
            language,
            query,
            captures: None,
            source,
            path,
            rev,
            blob_sha,
        }
    }

    /// Sets the capture allow-list.
    #[must_use]
    pub fn with_captures(mut self, captures: Option<&'a [&'a str]>) -> Self {
        self.captures = captures;
        self
    }

    /// Grammar id, such as `python` or `rust`.
    #[must_use]
    pub fn language(&self) -> &str {
        self.language
    }

    /// Query text.
    ///
    /// `producer.query_sha256` is the SHA-256 of this text after CRLF and bare
    /// CR are normalized to LF (`Sha256Digest::of_text`), so it equals the
    /// SHA-256 of the bytes only for a query without CR.
    #[must_use]
    pub fn query(&self) -> &str {
        self.query
    }

    /// Capture allow-list.
    ///
    /// `None` emits every capture whose name does not start with `_`.
    /// `Some` emits only listed names. Helper captures (`@_…`, stored without
    /// the `@`) are never subjects.
    #[must_use]
    pub fn captures(&self) -> Option<&[&str]> {
        self.captures
    }

    /// UTF-8 source the query runs on.
    #[must_use]
    pub fn source(&self) -> &[u8] {
        self.source
    }

    /// Repo-relative path used in `source.path` and subject ids.
    #[must_use]
    pub fn path(&self) -> &RepoRelativePath {
        &self.path
    }

    /// Revision stored in `source.rev`, such as `HEAD`.
    #[must_use]
    pub fn rev(&self) -> &Rev {
        &self.rev
    }

    /// Git blob id of [`Self::source`].
    #[must_use]
    pub fn blob_sha(&self) -> &BlobSha {
        &self.blob_sha
    }
}

struct RawCapture<'a> {
    name: &'a str,
    start_byte: usize,
    end_byte: usize,
    start: Point,
    end: Point,
}

/// Runs `input.query` in matches mode and builds a subjects document.
///
/// Subjects are ordered by `(start_byte, capture)`. The first subject on a
/// line is `{path}:{line}`. Later subjects on that line are `{path}:{line}:1`,
/// `{path}:{line}:2`, and so on. `producer.mode` is always [`PRODUCER_MODE`].
///
/// # Errors
///
/// Returns [`JevError::LanguageUnavailable`] for an unknown grammar,
/// [`JevError::QueryInvalid`] when the query does not compile,
/// [`JevError::ExtractionParseError`] when an `ERROR` or `MISSING` node sits
/// inside a subject span, [`JevError::ExtractionEmpty`] when no subject
/// remains, and [`JevError::InputInvalid`] when the source is not UTF-8 or a
/// span does not fit `u32`.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "python")] {
/// use sc_agent_types::{BlobSha, RepoRelativePath, Rev};
///
/// let input = sc_treesitter::ExtractInput::new(
///     "python",
///     "(identifier) @name\n",
///     b"value = 1\n",
///     RepoRelativePath::try_from("sample.py")?,
///     Rev::parse("HEAD")?,
///     BlobSha::try_from("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")?,
/// )
/// .with_captures(Some(&["name"]));
/// let document = sc_treesitter::extract(&input)?;
/// assert_eq!(document.subjects()[0].capture(), "name");
/// assert_eq!(document.subjects()[0].text(), "value");
/// # }
/// # Ok::<(), sc_agent_types::JevError>(())
/// ```
pub fn extract(input: &ExtractInput<'_>) -> Result<SubjectsDocument, JevError> {
    let path = &input.path;
    let rev = input.rev.clone();
    let blob_sha = &input.blob_sha;
    if input.query.is_empty() {
        return Err(JevError::query_invalid(
            PRODUCER_NAME,
            "query text is empty",
        ));
    }
    let Ok(source) = std::str::from_utf8(input.source) else {
        return Err(JevError::input_invalid("source is not UTF-8"));
    };

    let grammar = grammar::lookup(input.language)?;
    let info = &grammar.info;
    let language = (grammar.load)();
    let mut parser = Parser::new();
    parser.set_language(&language).map_err(|error| {
        JevError::extraction_failed(format!("grammar for {} failed to load: {error}", info.id))
    })?;
    let query = Query::new(&language, input.query)
        .map_err(|error| JevError::query_invalid(PRODUCER_NAME, error.to_string()))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| JevError::extraction_failed("parser returned no tree"))?;

    let capture_names = query.capture_names();
    let mut found = Vec::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
    while let Some(query_match) = matches.next() {
        for capture in query_match.captures() {
            let index = capture.index as usize;
            debug_assert!(
                index < capture_names.len(),
                "tree-sitter capture index is always inside the query"
            );
            let name = capture_names[index];
            if !capture_allowed(name, input.captures) {
                continue;
            }
            found.push(RawCapture {
                name,
                start_byte: capture.node.start_byte(),
                end_byte: capture.node.end_byte(),
                start: capture.node.start_position(),
                end: capture.node.end_position(),
            });
        }
    }
    drop(matches);

    if found.is_empty() {
        return Err(JevError::ExtractionEmpty);
    }

    found.sort_by(|left, right| {
        left.start_byte
            .cmp(&right.start_byte)
            .then_with(|| left.name.cmp(right.name))
            .then_with(|| left.end_byte.cmp(&right.end_byte))
    });

    let root = tree.root_node();
    if root.has_error() {
        let parse_error_line = found
            .iter()
            .filter_map(|capture| error_line_inside(root, capture.start_byte, capture.end_byte))
            .min();
        if let Some(line) = parse_error_line {
            return Err(JevError::extraction_parse_error(
                path.as_str().to_owned(),
                line,
            ));
        }
    }

    let mut builder = SubjectsBuilder::new(
        SubjectsSource::new(path.clone(), rev, blob_sha.clone()),
        SubjectsProducer::new(
            PRODUCER_NAME,
            env!("CARGO_PKG_VERSION"),
            GrammarRef {
                language: info.id.to_owned(),
                grammar: info.grammar.to_owned(),
                grammar_version: info.grammar_version.to_owned(),
            },
            Sha256Digest::of_text(input.query),
        )
        .with_mode(PRODUCER_MODE),
    );
    for capture in found {
        debug_assert!(
            capture.start_byte <= capture.end_byte && capture.end_byte <= source.len(),
            "tree-sitter node spans always lie inside the parsed source"
        );
        let text = source
            .get(capture.start_byte..capture.end_byte)
            .ok_or_else(|| {
                JevError::extraction_invalid(format!(
                    "subject bytes {}..{} are not on UTF-8 character boundaries",
                    capture.start_byte, capture.end_byte
                ))
            })?;
        let (start_line, end_line) = inclusive_lines(capture.start, capture.end)?;
        let span = Span::new(
            start_line,
            end_line,
            u32_from_usize("start byte", capture.start_byte)?,
            u32_from_usize("end byte", capture.end_byte)?,
        );
        builder = builder.push_subject(span, text, capture.name);
    }
    builder.build()
}

fn capture_allowed(name: &str, captures: Option<&[&str]>) -> bool {
    if name.starts_with('_') {
        return false;
    }
    match captures {
        Some(allow) => allow.contains(&name),
        None => true,
    }
}

fn inclusive_lines(start: Point, end: Point) -> Result<(u32, u32), JevError> {
    let start_line = u32_from_usize("start line", start.row.saturating_add(1))?;
    let end_row = if end.column == 0 && end.row > start.row {
        end.row
    } else {
        end.row.saturating_add(1)
    };
    let end_line = u32_from_usize("end line", end_row.max(start_line as usize))?;
    Ok((start_line, end_line))
}

/// Narrows a span field to the `u32` the subjects schema stores.
///
/// A source large enough to overflow is an input problem, not a byte-cap hit,
/// so the error is `INPUT_INVALID` naming the field and value.
fn u32_from_usize(field: &str, value: usize) -> Result<u32, JevError> {
    u32::try_from(value).map_err(|_| {
        JevError::input_invalid(format!(
            "subject {field} {value} does not fit the u32 span fields of sc/subjects/1"
        ))
    })
}

/// Returns the first line of an `ERROR` or `MISSING` node inside `start..end`.
///
/// A `MISSING` node is zero-width; one at the span end still counts.
fn error_line_inside(root: Node<'_>, start: usize, end: usize) -> Option<u32> {
    let mut found = None;
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        let node_start = node.start_byte();
        let node_end = node.end_byte();
        if node_end < start || node_start > end || !node.has_error() {
            continue;
        }
        let inside = if node.is_missing() {
            node_start >= start && node_end <= end
        } else {
            node.is_error() && node_start >= start && node_start < end && node_end <= end
        };
        if inside {
            let line =
                u32::try_from(node.start_position().row.saturating_add(1)).unwrap_or(u32::MAX);
            found = Some(found.map_or(line, |previous: u32| previous.min(line)));
        }
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                stack.push(cursor.node());
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(any(feature = "python", feature = "rust"))]
    use crate::query_source;
    #[cfg(feature = "python")]
    use crate::subjects_to_json_bytes;
    #[cfg(feature = "python")]
    use std::collections::BTreeSet;
    #[cfg(feature = "python")]
    use std::path::PathBuf;

    const BLOB: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn run<'a>(
        language: &'a str,
        query: &'a str,
        source: &'a [u8],
        path: &'a str,
        captures: Option<&'a [&'a str]>,
    ) -> Result<SubjectsDocument, JevError> {
        extract(
            &ExtractInput::new(
                language,
                query,
                source,
                RepoRelativePath::try_from(path)?,
                Rev::parse("HEAD")?,
                BlobSha::try_from(BLOB)?,
            )
            .with_captures(captures),
        )
    }

    /// `query_sha256` hashes the LF-normalized query text, so a CRLF query and
    /// its LF form carry one digest (other implementations must do the same).
    #[cfg(feature = "python")]
    #[test]
    fn query_sha256_is_the_digest_of_the_lf_normalized_query() {
        let lf = "(identifier) @name\n(identifier) @name\n";
        let crlf = lf.replace('\n', "\r\n");
        let cr = lf.replace('\n', "\r");
        let digest = |query: &str| {
            run("python", query, b"x = 1\n", "a.py", None)
                .expect("extract")
                .producer()
                .query_sha256()
                .clone()
        };
        assert_eq!(digest(&crlf), Sha256Digest::of_text(lf));
        assert_eq!(digest(&cr), Sha256Digest::of_text(lf));
        assert_eq!(digest(lf), Sha256Digest::of_text(lf));
        // SHA-256 of the LF bytes; the raw CRLF bytes hash to a different value.
        assert_eq!(
            digest(&crlf).as_str(),
            "6882063e0fbeac9c629dcacb8f8d965aa5c3a0e4d811f3a57c5d1c2f4e6ba7dc"
        );
        assert_ne!(
            digest(&crlf).as_str(),
            "61fddca9b6019c10daa92f1d38b39d97c02f4bc7e811a2b06352328f03e5f382"
        );
    }

    #[cfg(feature = "python")]
    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/cross-impl/branches")
            .join(name)
    }

    #[cfg(feature = "python")]
    #[test]
    fn test_tree_sitter_python_branches_lists_arms_and_refusals() {
        let source = std::fs::read(fixture("hook.py")).expect("hook.py");
        let query = query_source("python", "branches").expect("branches query");
        let document = run(
            "python",
            query,
            &source,
            "tests/fixtures/cross-impl/branches/hook.py",
            None,
        )
        .expect("extract branches");
        assert_eq!(
            document.schema(),
            sc_agent_types::schema::subjects::SUBJECTS_SCHEMA_V1
        );
        assert_eq!(document.producer().mode(), PRODUCER_MODE);
        assert_eq!(document.producer().name(), PRODUCER_NAME);
        assert_eq!(document.producer().grammar(), "tree-sitter-python");
        assert_eq!(document.producer().grammar_version(), "0.25.0");
        let arms: Vec<&str> = document
            .subjects()
            .iter()
            .filter(|subject| subject.capture() == "arm")
            .map(|subject| subject.text())
            .collect();
        let refusals: Vec<&str> = document
            .subjects()
            .iter()
            .filter(|subject| subject.capture() == "refusal")
            .map(|subject| subject.text())
            .collect();
        assert_eq!(
            arms,
            vec![
                "Transition.GIVE_BACK",
                "Transition.KEEP",
                "Transition.STOP",
                "_"
            ]
        );
        assert_eq!(
            refusals,
            vec!["Refusal(\"give-back\")", "Refusal(\"stop\")"]
        );
        let only_arms = run(
            "python",
            query,
            &source,
            "tests/fixtures/cross-impl/branches/hook.py",
            Some(&["arm"]),
        )
        .expect("arm allow-list");
        assert!(
            only_arms
                .subjects()
                .iter()
                .all(|subject| subject.capture() == "arm")
        );
        assert_eq!(only_arms.producer().mode(), PRODUCER_MODE);
    }

    #[cfg(feature = "rust")]
    #[test]
    fn test_tree_sitter_rust_match_arms() {
        let source = r#"
fn classify(value: Transition) -> &'static str {
    match value {
        Transition::GiveBack => "give-back",
        Transition::Keep => "keep",
    }
}
"#;
        let query = query_source("rust", "match-arms").expect("match-arms");
        let document =
            run("rust", query, source.as_bytes(), "src/classify.rs", None).expect("rust");
        let arms: Vec<&str> = document
            .subjects()
            .iter()
            .map(|subject| subject.text())
            .collect();
        assert_eq!(arms, vec!["Transition::GiveBack", "Transition::Keep"]);
        assert!(
            document
                .subjects()
                .iter()
                .all(|subject| subject.capture() == "arm")
        );
        assert_eq!(document.producer().mode(), PRODUCER_MODE);
        assert_eq!(document.producer().grammar(), "tree-sitter-rust");
        assert_eq!(document.producer().grammar_version(), "0.24.2");
    }

    #[cfg(feature = "python")]
    #[test]
    fn test_tree_sitter_helper_capture_is_not_a_subject() {
        let source = std::fs::read(fixture("hook.py")).expect("hook.py");
        let query = query_source("python", "branches").expect("branches query");
        assert!(query.contains("@_fn"), "fixture query must name a helper");
        let document = run("python", query, &source, "hook.py", None).expect("extract");
        assert!(
            document
                .subjects()
                .iter()
                .all(|subject| !subject.capture().starts_with('_'))
        );
        assert!(
            document
                .subjects()
                .iter()
                .all(|subject| subject.capture() != "_fn" && subject.text() != "Refusal")
        );
        assert!(
            document
                .subjects()
                .iter()
                .any(|subject| subject.capture() == "refusal" && subject.text().contains("Refusal"))
        );
    }

    #[cfg(feature = "python")]
    #[test]
    fn test_tree_sitter_same_line_ids_are_unique_and_stable() {
        let source = b"return Refusal(\"give-back\")\n";
        let query = "(return_statement (call function: (identifier) @arm arguments: (argument_list (string) @refusal)))\n";
        let once = run("python", query, source, "same.py", None).expect("once");
        let twice = run("python", query, source, "same.py", None).expect("twice");
        assert_eq!(once, twice);
        assert_eq!(once.subjects().len(), 2);
        assert_eq!(
            once.subjects()[0].span().start_line(),
            once.subjects()[1].span().start_line()
        );
        assert_eq!(once.subjects()[0].id(), "same.py:1");
        assert_eq!(once.subjects()[1].id(), "same.py:1:1");
        assert!(once.subjects()[0].span().start_byte() < once.subjects()[1].span().start_byte());
        let ids: BTreeSet<&str> = once.subjects().iter().map(|subject| subject.id()).collect();
        assert_eq!(ids.len(), once.subjects().len());
    }

    #[cfg(feature = "python")]
    #[test]
    fn test_tree_sitter_parse_error_is_typed() {
        let error = run("python", "(ERROR) @broken\n", b"def (\n", "broken.py", None)
            .expect_err("syntax error must fail");
        assert_eq!(error.code(), "EXTRACTION_PARSE_ERROR");
        match error {
            JevError::ExtractionParseError { path, line, .. } => {
                assert_eq!(path, "broken.py");
                assert!(line >= 1, "line {line}");
            }
            other => panic!("unexpected {}", other.code()),
        }
    }

    #[test]
    fn test_tree_sitter_unknown_language_is_typed() {
        let error = run("cobol", "(identifier) @name\n", b"x\n", "a.py", None).expect_err("cobol");
        assert_eq!(error.code(), "LANGUAGE_UNAVAILABLE");
        match error {
            JevError::LanguageUnavailable { language, .. } => assert_eq!(language, "cobol"),
            other => panic!("unexpected {}", other.code()),
        }
    }

    #[cfg(feature = "python")]
    #[test]
    fn test_matches_mode_duplicate_captures() {
        let query = "(binary_operator left: (identifier) @name right: (identifier) @name)\n";
        let document =
            run("python", query, b"value = left + right\n", "dup.py", None).expect("dup");
        assert_eq!(document.producer().mode().as_str(), "matches");
        let names: Vec<&str> = document
            .subjects()
            .iter()
            .filter(|subject| subject.capture() == "name")
            .map(|subject| subject.text())
            .collect();
        assert_eq!(names, vec!["left", "right"]);
    }

    #[cfg(feature = "python")]
    #[test]
    fn test_subjects_schema_roundtrip() {
        let query = query_source("python", "branches").expect("query");
        let source = std::fs::read(fixture("hook.py")).expect("hook.py");
        let document = run(
            "python",
            query,
            &source,
            "tests/fixtures/cross-impl/branches/hook.py",
            None,
        )
        .expect("extract");
        let bytes = subjects_to_json_bytes(&document).expect("json");
        let parsed: SubjectsDocument = serde_json::from_slice(&bytes).expect("parse");
        assert_eq!(parsed, document);
        assert_eq!(
            parsed.schema(),
            sc_agent_types::schema::subjects::SUBJECTS_SCHEMA_V1
        );
        assert_eq!(subjects_to_json_bytes(&parsed).expect("again"), bytes);
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("value");
        assert_eq!(value["schema"], "sc/subjects/1");
        assert_eq!(value["producer"]["mode"], "matches");
    }

    #[cfg(feature = "python")]
    #[test]
    fn empty_extraction_is_typed() {
        let error = run(
            "python",
            "(class_definition) @name\n",
            b"pass\n",
            "empty.py",
            None,
        )
        .expect_err("no class");
        assert_eq!(error.code(), "EXTRACTION_EMPTY");
    }

    #[cfg(feature = "python")]
    #[test]
    fn invalid_query_is_typed() {
        let error = run("python", "((( \n", b"pass\n", "bad.py", None).expect_err("query");
        assert_eq!(error.code(), "QUERY_INVALID");
    }

    #[test]
    fn pinned_grammar_versions_match_cargo_toml() {
        let manifest = include_str!("../Cargo.toml");
        for language in crate::languages() {
            let needle = format!(
                "{} = {{ version = \"={}\"",
                language.grammar, language.grammar_version
            );
            assert!(manifest.contains(&needle), "missing {needle}");
        }
        assert!(manifest.contains("tree-sitter = { version = \"=0.27.1\""));
    }

    #[test]
    fn crate_manifest_does_not_depend_on_jev() {
        let manifest = include_str!("../Cargo.toml");
        assert!(!manifest.contains("sc-jev"));
    }

    #[cfg(feature = "python")]
    #[test]
    fn missing_node_inside_subject_is_a_parse_error() {
        // tree-sitter-python recovers `def f(:` with a zero-width MISSING ")"
        // and no ERROR node, so only the is_missing rule catches it.
        let source = b"def f(:\n  pass\n";
        let error = run(
            "python",
            "(function_definition) @def\n",
            source,
            "missing.py",
            None,
        )
        .expect_err("MISSING node must fail");
        assert_eq!(error, JevError::extraction_parse_error("missing.py", 1));
    }

    #[cfg(feature = "python")]
    #[test]
    fn clean_source_skips_the_error_walk_and_succeeds() {
        let document =
            run("python", "(identifier) @name\n", b"a = b\n", "ok.py", None).expect("clean source");
        assert_eq!(document.subjects().len(), 2);
    }

    #[cfg(feature = "python")]
    #[test]
    fn malformed_rev_path_and_blob_fail_at_the_caller() {
        // Validation happens when the typed value is built, not in `extract`.
        assert_eq!(
            Rev::parse("--output=x").expect_err("leading dash").code(),
            "INPUT_INVALID"
        );
        assert_eq!(
            RepoRelativePath::try_from("../a.py")
                .expect_err("escape")
                .code(),
            "INPUT_INVALID"
        );
        assert_eq!(
            BlobSha::try_from("xyz").expect_err("not hex").code(),
            "INPUT_INVALID"
        );
    }

    #[test]
    fn span_overflow_is_input_invalid_with_detail() {
        let error = u32_from_usize("end byte", usize::MAX).expect_err("overflow");
        assert_eq!(error.code(), "INPUT_INVALID");
        assert!(error.to_string().contains("end byte"), "{error}");
        let lines = inclusive_lines(Point::new(usize::MAX - 1, 0), Point::new(usize::MAX - 1, 3))
            .expect_err("line overflow");
        assert_eq!(lines.code(), "INPUT_INVALID");
        assert!(lines.to_string().contains("start line"), "{lines}");
    }
}
