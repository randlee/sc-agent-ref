//! `check.toml` and LLM prompt paths.
//!
//! Prompt text is not sent from this module; the run facade injects it into
//! `JudgeRequest`. Production prose lives in `assets/llm-evaluator-system.md`
//! and is compiled in as [`PACKAGED_SYSTEM_PROMPT`]. That constant is the
//! only source of the packaged prompt. Packaged `assets/jev.toml` carries no
//! settings: the request byte cap belongs to each backend's descriptor
//! (REQ-SAG-0314), so this crate keeps no second copy.
//!
//! Every path in `check.toml` is a [`RepoRelativePath`], validated when the
//! document is parsed.

#![warn(missing_docs)]

mod command;

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use sc_agent_types::{BackendId, BackendSet, JevError, ModelAliases, ModelId, RepoRelativePath};
use serde::{Deserialize, Serialize};

pub use command::CommandSpec;

/// Check input stored under `[inputs.<name>]` in `check.toml`.
///
/// `<name>` is the table key, not a field. Keys match
/// `docs/sc-jev-check/extraction.md`: `kind`, `path`, `column`, and for
/// `command` the argv array `command`, an optional `path` substituted for
/// `{path}`, an optional `producer` name, and optional `timeout_seconds`
/// (default 30). Only these four
/// kinds exist; a new kind is a new variant plus an ADR.
///
/// The enum and the struct variants are `#[non_exhaustive]`, so outside this
/// crate a spec is built with [`InputSpec::file`], [`InputSpec::table_column`],
/// [`InputSpec::subjects`] or [`InputSpec::command`], and a new field or kind
/// is not a breaking change. A `command` input is a [`CommandSpec`], which
/// owns the setters that apply only to commands.
///
/// # Examples
///
/// ```
/// use sc_agent_types::RepoRelativePath;
/// use sc_jev_check::config::{CommandSpec, InputSpec};
///
/// let command = CommandSpec::new(
///     vec!["tool".to_owned(), "{path}".to_owned()],
///     Some(RepoRelativePath::try_from("src/lib.rs")?),
/// )?
/// .with_timeout_seconds(5)?;
/// let spec = InputSpec::command(command);
/// assert!(matches!(&spec, InputSpec::Command(c) if c.timeout_seconds().get() == 5));
/// # Ok::<(), sc_agent_types::JevError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", try_from = "RawInputSpec")]
#[non_exhaustive]
pub enum InputSpec {
    /// Read a file at the caller-supplied revision.
    #[non_exhaustive]
    File {
        /// Repo-relative path.
        path: RepoRelativePath,
    },
    /// Read one markdown table column.
    #[non_exhaustive]
    TableColumn {
        /// Repo-relative path.
        path: RepoRelativePath,
        /// Column header to extract.
        column: String,
    },
    /// Read a `sc/subjects/1` document from `path`.
    #[non_exhaustive]
    Subjects {
        /// Repo-relative path of the artifact.
        path: RepoRelativePath,
    },
    /// Run an argv array. No shell.
    Command(CommandSpec),
}

/// The `check.toml` shape of an input, before `command` is validated.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum RawInputSpec {
    File {
        path: RepoRelativePath,
    },
    TableColumn {
        path: RepoRelativePath,
        column: String,
    },
    Subjects {
        path: RepoRelativePath,
    },
    Command {
        command: Vec<String>,
        #[serde(default = "default_command_timeout_seconds")]
        timeout_seconds: u64,
        #[serde(default)]
        path: Option<RepoRelativePath>,
        #[serde(default)]
        producer: Option<String>,
    },
}

fn default_command_timeout_seconds() -> u64 {
    command::DEFAULT_TIMEOUT_SECONDS
}

impl TryFrom<RawInputSpec> for InputSpec {
    type Error = JevError;

    fn try_from(raw: RawInputSpec) -> Result<Self, Self::Error> {
        Ok(match raw {
            RawInputSpec::File { path } => Self::File { path },
            RawInputSpec::TableColumn { path, column } => Self::TableColumn { path, column },
            RawInputSpec::Subjects { path } => Self::Subjects { path },
            RawInputSpec::Command {
                command,
                timeout_seconds,
                path,
                producer,
            } => Self::Command(CommandSpec::from_parts(
                command,
                timeout_seconds,
                path,
                producer,
            )?),
        })
    }
}

impl InputSpec {
    /// A `file` input reading `path`.
    #[must_use]
    pub fn file(path: RepoRelativePath) -> Self {
        Self::File { path }
    }

    /// A `table-column` input reading `column` of the markdown table at `path`.
    #[must_use]
    pub fn table_column(path: RepoRelativePath, column: impl Into<String>) -> Self {
        Self::TableColumn {
            path,
            column: column.into(),
        }
    }

    /// A `subjects` input reading the `sc/subjects/1` document at `path`.
    #[must_use]
    pub fn subjects(path: RepoRelativePath) -> Self {
        Self::Subjects { path }
    }

    /// A `command` input running `command`.
    #[must_use]
    pub fn command(command: CommandSpec) -> Self {
        Self::Command(command)
    }
}

/// The name of a check input: the `<name>` of `[inputs.<name>]`, matching
/// `[A-Za-z0-9_-]+`.
///
/// One validation path serves `check.toml` parsing and hand-built specs, so
/// an input name that would break a `{name}` placeholder cannot exist.
///
/// # Examples
///
/// ```
/// use sc_jev_check::config::InputName;
///
/// assert_eq!(InputName::try_from("rule-1")?.as_str(), "rule-1");
/// assert_eq!(InputName::try_from("bad name").unwrap_err().code(), "CONFIG_INVALID");
/// assert!(InputName::try_from("").is_err());
/// # Ok::<(), sc_agent_types::JevError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InputName(String);

impl InputName {
    /// The name as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for InputName {
    type Error = JevError;

    /// # Errors
    ///
    /// `CONFIG_INVALID` unless `name` is one or more of `A-Z a-z 0-9 _ -`.
    fn try_from(name: &str) -> Result<Self, Self::Error> {
        if is_input_name(name) {
            Ok(Self(name.to_owned()))
        } else {
            Err(JevError::config_invalid(format!(
                "check.toml input name {name} must be [A-Za-z0-9_-]+"
            )))
        }
    }
}

impl TryFrom<String> for InputName {
    type Error = JevError;

    /// # Errors
    ///
    /// `CONFIG_INVALID` as for `TryFrom<&str>`.
    fn try_from(name: String) -> Result<Self, Self::Error> {
        Self::try_from(name.as_str())
    }
}

impl FromStr for InputName {
    type Err = JevError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::try_from(name)
    }
}

impl fmt::Display for InputName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::borrow::Borrow<str> for InputName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for InputName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Packaged evaluator instructions (`include_str` of the asset file).
pub const PACKAGED_SYSTEM_PROMPT: &str = include_str!("../../assets/llm-evaluator-system.md");

/// Display name of the packaged system prompt in errors.
pub(crate) const PACKAGED_SYSTEM_PROMPT_NAME: &str = "llm-evaluator-system.md";

/// Prompt path chosen for a check. The file is not read here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResolvedPromptPath {
    /// `[llm_judge] system_prompt_file` in `check.toml`.
    Repo(RepoRelativePath),
    /// The packaged asset compiled into this crate.
    Packaged,
}

/// A parsed `check.toml`.
///
/// Input names are `[inputs.<name>]` table keys, held as validated
/// [`InputName`]s. `sc-jev-check` is the only crate that runs those inputs.
/// The fields are private: read them through the accessors and build a spec
/// with [`CheckSpec::new`] and the by-value `with_*` setters.
///
/// # Examples
///
/// ```
/// use sc_agent_types::RepoRelativePath;
/// use sc_jev_check::config::{CheckSpec, InputName, InputSpec};
///
/// let spec = CheckSpec::new(RepoRelativePath::try_from("questions.json")?)
///     .with_input(
///         InputName::try_from("rule")?,
///         InputSpec::file(RepoRelativePath::try_from("rule.md")?),
///     );
/// assert_eq!(spec.inputs().len(), 1);
/// assert!(spec.state_path().is_none());
/// # Ok::<(), sc_agent_types::JevError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckSpec {
    questions_path: RepoRelativePath,
    state_path: Option<RepoRelativePath>,
    system_prompt_file: Option<RepoRelativePath>,
    inputs: BTreeMap<InputName, InputSpec>,
    backends: Option<BackendSet>,
    models: ModelAliases,
}

impl CheckSpec {
    /// A check with only a questions document: no state, inputs, prompt
    /// override, or backends.
    #[must_use]
    pub fn new(questions_path: RepoRelativePath) -> Self {
        Self {
            questions_path,
            state_path: None,
            system_prompt_file: None,
            inputs: BTreeMap::new(),
            backends: None,
            models: ModelAliases::builtin(),
        }
    }

    /// Sets the state document; absent means an empty state.
    #[must_use]
    pub fn with_state_path(mut self, path: RepoRelativePath) -> Self {
        self.state_path = Some(path);
        self
    }

    /// Overrides the packaged system-prompt path.
    #[must_use]
    pub fn with_system_prompt_file(mut self, path: RepoRelativePath) -> Self {
        self.system_prompt_file = Some(path);
        self
    }

    /// Adds the extractor `name`; a later call with the same name replaces it.
    #[must_use]
    pub fn with_input(mut self, name: InputName, input: InputSpec) -> Self {
        self.inputs.insert(name, input);
        self
    }

    /// Sets the top-level `backends` list. `RunConfig::backends` wins over it.
    #[must_use]
    pub fn with_backends(mut self, backends: BackendSet) -> Self {
        self.backends = Some(backends);
        self
    }

    /// Sets the model aliases applied over the catalog (REQ-SAG-0148).
    #[must_use]
    pub fn with_models(mut self, models: ModelAliases) -> Self {
        self.models = models;
        self
    }

    /// Repo-relative `plan-anchoring/questions/1` document.
    #[must_use]
    pub fn questions_path(&self) -> &RepoRelativePath {
        &self.questions_path
    }

    /// Repo-relative state document. Absent means an empty state.
    #[must_use]
    pub fn state_path(&self) -> Option<&RepoRelativePath> {
        self.state_path.as_ref()
    }

    /// Optional override of the packaged system-prompt path.
    #[must_use]
    pub fn system_prompt_file(&self) -> Option<&RepoRelativePath> {
        self.system_prompt_file.as_ref()
    }

    /// Named extractors, in table-key order.
    #[must_use]
    pub fn inputs(&self) -> &BTreeMap<InputName, InputSpec> {
        &self.inputs
    }

    /// Top-level `backends` list. `RunConfig::backends` wins over it, and
    /// `[jev]` applies when both are absent (ADR-SAG-0024).
    #[must_use]
    pub fn backends(&self) -> Option<&BackendSet> {
        self.backends.as_ref()
    }

    /// The `[models]` table applied over the catalog (`alias = model
    /// string`); the catalog's own aliases when the table is absent.
    /// `RunConfig::models` wins over it (REQ-SAG-0148).
    #[must_use]
    pub fn models(&self) -> &ModelAliases {
        &self.models
    }

    /// Selects the system-prompt path without reading the file.
    ///
    /// `check.toml` wins when `system_prompt_file` is set. Otherwise the
    /// packaged asset ([`PACKAGED_SYSTEM_PROMPT_NAME`]) is used.
    #[must_use]
    pub(crate) fn resolve_prompt_path(&self) -> ResolvedPromptPath {
        match &self.system_prompt_file {
            Some(path) => ResolvedPromptPath::Repo(path.clone()),
            None => ResolvedPromptPath::Packaged,
        }
    }
}

/// Parses `check.toml` text.
///
/// # Errors
///
/// Returns [`JevError::ConfigInvalid`] when the document does not match the
/// check shape: unknown keys, a path that is not repo-relative, an unknown
/// backend id, an empty or repeated `backends` list, or a `[models]` entry
/// that names an unknown alias or an invalid model string.
pub(crate) fn load_check_str(text: &str) -> Result<CheckSpec, JevError> {
    let raw: RawCheck = toml::from_str(text)
        .map_err(|error| JevError::config_invalid(format!("check.toml: {error}")))?;
    let mut inputs = BTreeMap::new();
    for (name, input) in raw.inputs {
        inputs.insert(InputName::try_from(name)?, InputSpec::try_from(input)?);
    }
    let backends = match raw.backends {
        Some(ids) => Some(BackendSet::try_from(ids)?),
        None => None,
    };
    let models = models_from(&raw.models)?;
    Ok(CheckSpec {
        questions_path: raw.questions.path,
        state_path: raw.state.map(|state| state.path),
        system_prompt_file: raw.llm_judge.map(|llm| llm.system_prompt_file),
        inputs,
        backends,
        models,
    })
}

/// Applies the `[models]` table over the catalog aliases.
///
/// An invalid model string is `INPUT_INVALID` from `ModelId::from_str`; it
/// is `CONFIG_INVALID` here, with its message and fix hint kept and the
/// `check.toml` key (`[models].<alias>`) added (REQ-SAG-0301).
fn models_from(table: &BTreeMap<String, String>) -> Result<ModelAliases, JevError> {
    let mut aliases = ModelAliases::builtin();
    for (alias, text) in table {
        let key = format!("check.toml [models].{alias}");
        let model: ModelId = text
            .parse()
            .map_err(|error: JevError| JevError::config_invalid(format!("{key}: {error}")))?;
        aliases = aliases
            .with_override(alias, model)
            .map_err(|error| error.map_detail(|detail| format!("{key}: {detail}")))?;
    }
    Ok(aliases)
}

/// Reads `check.toml` at `relative` and `rev` through the repo-root resolver.
///
/// # Errors
///
/// Returns [`JevError`] when the path cannot be read or the document is invalid.
pub fn load_check_at(
    repo: &sc_agent_types::RepoRoot,
    rev: &sc_agent_types::Rev,
    relative: &RepoRelativePath,
) -> Result<CheckSpec, JevError> {
    let bytes = crate::extract::read_repo_bytes(repo, rev, relative)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| JevError::config_invalid(format!("{relative} is not UTF-8")))?;
    load_check_str(text)
}

fn is_input_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCheck {
    questions: RawPath,
    #[serde(default)]
    state: Option<RawPath>,
    #[serde(default)]
    llm_judge: Option<RawLlmJudge>,
    #[serde(default)]
    inputs: BTreeMap<String, RawInputSpec>,
    #[serde(default)]
    backends: Option<Vec<BackendId>>,
    #[serde(default)]
    models: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPath {
    path: RepoRelativePath,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLlmJudge {
    system_prompt_file: RepoRelativePath,
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::config::InputSpec;
    use crate::extract::test_support::rel;

    #[test]
    fn payload_packaged_system_prompt_matches_asset() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/llm-evaluator-system.md");
        let disk = std::fs::read_to_string(path).expect("asset");
        assert_eq!(disk, PACKAGED_SYSTEM_PROMPT);
        assert_eq!(
            PACKAGED_SYSTEM_PROMPT,
            include_str!("../../assets/llm-evaluator-system.md")
        );
        assert!(
            !PACKAGED_SYSTEM_PROMPT.contains("TOKEN"),
            "the shipped prompt carries no test sentinel"
        );
        assert!(PACKAGED_SYSTEM_PROMPT.contains("canonical judge payload"));
    }

    /// REQ-SAG-0314: the engine keeps no request byte cap; packaged
    /// `jev.toml` is an empty document and no code reads `max_input_bytes`.
    #[test]
    fn packaged_jev_toml_has_no_byte_cap() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/jev.toml");
        let text = std::fs::read_to_string(path).expect("jev.toml");
        let table: toml::Table = toml::from_str(&text).expect("toml");
        assert!(table.is_empty(), "{table:?}");
        assert!(!text.contains("max_input_bytes ="), "{text}");
    }

    #[test]
    fn check_paths_are_repo_relative_and_backends_are_a_set() {
        for bad in [
            "[questions]\npath = \"../q.json\"\n",
            "[questions]\npath = \"/etc/q.json\"\n",
            "[questions]\npath = \"\"\n",
            "[questions]\npath = \"q.json\"\n[llm_judge]\nsystem_prompt_file = \"\"\n",
            "[questions]\npath = \"q.json\"\n[inputs.a]\nkind = \"file\"\npath = \"../a\"\n",
            "backends = []\n[questions]\npath = \"q.json\"\n",
            "backends = [\"jev\", \"jev\"]\n[questions]\npath = \"q.json\"\n",
            "backends = [\"nope\"]\n[questions]\npath = \"q.json\"\n",
        ] {
            let error = load_check_str(bad).expect_err(bad);
            assert_eq!(error.code(), "CONFIG_INVALID", "{bad}");
        }
        let spec =
            load_check_str("backends = [\"haiku\", \"jev\"]\n[questions]\npath = \"q.json\"\n")
                .expect("backends");
        assert_eq!(
            spec.backends().expect("set").as_slice(),
            [BackendId::Haiku, BackendId::Jev]
        );
    }

    #[test]
    fn input_name_is_validated_on_construction_and_on_parse() {
        for good in ["a", "rule-1", "A_b-9"] {
            assert_eq!(InputName::try_from(good).expect(good).as_str(), good);
        }
        for bad in ["", "has space", "a.b", "{a}", "caf\u{e9}", "a/b"] {
            let error = InputName::try_from(bad).expect_err(bad);
            assert_eq!(error.code(), "CONFIG_INVALID", "{bad:?}");
            assert_eq!(
                error.to_string(),
                load_check_str(&format!(
                    "[questions]\npath = \"q.json\"\n[inputs.\"{bad}\"]\nkind = \"file\"\npath = \"a\"\n"
                ))
                .expect_err(bad)
                .to_string(),
                "{bad:?}: one validation path, one message"
            );
        }
        // A hand-built spec cannot carry a bad name: it fails before the spec.
        assert!(InputName::try_from("bad name").is_err());
    }

    #[test]
    fn command_extractor_toml_omitted_timeout_is_30() {
        let spec = load_check_str(
            r#"
            [questions]
            path = "questions.json"

            [inputs.live]
            kind = "command"
            command = ["python3", "-c", "pass"]

            [inputs.with_path]
            kind = "command"
            command = ["sc-treesitter", "{path}"]
            path = "src/hook.py"
            "#,
        )
        .expect("check");
        match &spec.inputs()["live"] {
            InputSpec::Command(command) => {
                assert_eq!(command.timeout_seconds().get(), 30);
                assert!(command.path().is_none());
            }
            other => panic!("expected command, got {other:?}"),
        }
        match &spec.inputs()["with_path"] {
            InputSpec::Command(command) => {
                assert_eq!(
                    command.path().map(RepoRelativePath::as_str),
                    Some("src/hook.py")
                );
            }
            other => panic!("expected command, got {other:?}"),
        }
    }

    const ZERO_TIMEOUT_TEXT: &str = "command timeout_seconds must be greater than zero";
    const NO_PATH_TEXT: &str = "command argv contains {path}, but the input has no path field; set path = \"<repo-relative file>\" on the command input";

    fn command_toml(fields: &str) -> String {
        format!("[questions]\npath = \"q.json\"\n[inputs.run]\nkind = \"command\"\n{fields}\n")
    }

    #[test]
    fn command_zero_timeout_is_refused_once_when_built_and_when_parsed() {
        let argv = || vec!["tool".to_owned()];
        let built = CommandSpec::new(argv(), None)
            .expect("spec")
            .with_timeout_seconds(0)
            .expect_err("zero");
        assert_eq!(built.code(), "CONFIG_INVALID");
        assert_eq!(
            built.to_string(),
            format!("config is invalid: {ZERO_TIMEOUT_TEXT}")
        );

        let parsed = load_check_str(&command_toml("command = [\"tool\"]\ntimeout_seconds = 0"))
            .expect_err("zero");
        assert_eq!(parsed.code(), "CONFIG_INVALID");
        assert_eq!(parsed.to_string(), built.to_string());
        let nonzero = load_check_str(&command_toml("command = [\"tool\"]\ntimeout_seconds = 1"))
            .expect("one second");
        assert!(matches!(
            &nonzero.inputs()["run"],
            InputSpec::Command(command) if command.timeout_seconds().get() == 1
        ));
    }

    #[test]
    fn command_path_placeholder_without_a_path_is_refused_when_built_and_when_parsed() {
        let argv = || vec!["tool".to_owned(), "--file={path}".to_owned()];
        let built = CommandSpec::new(argv(), None).expect_err("no path");
        assert_eq!(built.code(), "CONFIG_INVALID");
        assert_eq!(
            built.to_string(),
            format!("config is invalid: {NO_PATH_TEXT}")
        );
        assert!(
            built.message().contains("path field"),
            "{}",
            built.message()
        );

        let parsed = load_check_str(&command_toml("command = [\"tool\", \"--file={path}\"]"))
            .expect_err("no path");
        assert_eq!(parsed.code(), "CONFIG_INVALID");
        assert_eq!(parsed.to_string(), built.to_string());

        CommandSpec::new(argv(), Some(rel("a.py"))).expect("path given");
        load_check_str(&command_toml(
            "command = [\"tool\", \"{path}\"]\npath = \"a.py\"",
        ))
        .expect("path given in toml");
        // A zero timeout still wins over the missing path, as it did at run time.
        let both = load_check_str(&command_toml(
            "command = [\"tool\", \"{path}\"]\ntimeout_seconds = 0",
        ))
        .expect_err("both");
        assert_eq!(
            both.to_string(),
            format!("config is invalid: {ZERO_TIMEOUT_TEXT}")
        );
    }

    #[test]
    fn check_prompt_path_overrides_the_packaged_asset() {
        let spec = load_check_str(
            r#"
            [questions]
            path = "questions.json"

            [llm_judge]
            system_prompt_file = "prompt.md"
            "#,
        )
        .expect("check");
        let resolved = spec.resolve_prompt_path();
        assert_eq!(resolved, ResolvedPromptPath::Repo(rel("prompt.md")));

        let bare = load_check_str(
            r#"
            [questions]
            path = "questions.json"
            "#,
        )
        .expect("bare");
        assert_eq!(bare.resolve_prompt_path(), ResolvedPromptPath::Packaged);
    }

    #[test]
    fn input_spec_matches_extraction_check_toml_keys() {
        let file: InputSpec = serde_json::from_str(
            r#"{"kind":"file","path":"skills/atm-bd-lite/scripts/atm-bd-task-hook.py"}"#,
        )
        .expect("file");
        assert_eq!(
            file,
            InputSpec::File {
                path: rel("skills/atm-bd-lite/scripts/atm-bd-task-hook.py"),
            }
        );

        let subjects: InputSpec = serde_json::from_str(
            r#"{"kind":"subjects","path":"tests/fixtures/cross-impl/branches/subjects.json"}"#,
        )
        .expect("subjects");
        assert!(matches!(subjects, InputSpec::Subjects { .. }));

        let column: InputSpec = serde_json::from_str(
            r#"{"kind":"table-column","path":"docs/decisions.md","column":"Choice"}"#,
        )
        .expect("table-column");
        assert_eq!(
            column,
            InputSpec::TableColumn {
                path: rel("docs/decisions.md"),
                column: "Choice".into(),
            }
        );

        let command: InputSpec = serde_json::from_str(
            r#"{"kind":"command","command":["sc-treesitter","extract","--language","python","--query","branches","{path}"],"path":"a.py"}"#,
        )
        .expect("command");
        let expected = CommandSpec::new(
            [
                "sc-treesitter",
                "extract",
                "--language",
                "python",
                "--query",
                "branches",
                "{path}",
            ]
            .map(String::from)
            .to_vec(),
            Some(rel("a.py")),
        )
        .expect("spec");
        assert_eq!(command, InputSpec::command(expected));
        assert_eq!(
            serde_json::to_string(&command).expect("json"),
            r#"{"kind":"command","command":["sc-treesitter","extract","--language","python","--query","branches","{path}"],"timeout_seconds":30,"path":"a.py"}"#
        );

        let with_name =
            serde_json::from_str::<InputSpec>(r#"{"kind":"file","name":"hook","path":"a.py"}"#);
        assert!(
            with_name.is_err(),
            "input name is the table key, not a field"
        );
        let with_argv = serde_json::from_str::<InputSpec>(r#"{"kind":"command","argv":["echo"]}"#);
        assert!(with_argv.is_err(), "argv is the check.toml key command");
    }
}
