//! The `command` input of `check.toml` (REQ-SAG-0305, REQ-SAG-0312).

use std::num::NonZeroU64;

use sc_agent_types::{JevError, RepoRelativePath};
use serde::Serialize;

/// Default process time limit of a `command` input, in seconds.
pub(crate) const DEFAULT_TIMEOUT_SECONDS: u64 = 30;

/// A `command` input: an argv array run without a shell.
///
/// The two rules that can make a command unrunnable are checked once, where
/// the value is built, whether by [`CommandSpec::new`] and
/// [`CommandSpec::with_timeout_seconds`] or by parsing `check.toml`, so the
/// extractor never meets them:
///
/// - the time limit is greater than zero, and
/// - `{path}` appears in the argv only when a `path` file is given.
///
/// Both failures are `CONFIG_INVALID`.
///
/// # Examples
///
/// ```
/// use sc_agent_types::RepoRelativePath;
/// use sc_jev_check::config::CommandSpec;
///
/// let spec = CommandSpec::new(
///     vec!["tool".to_owned(), "{path}".to_owned()],
///     Some(RepoRelativePath::try_from("src/lib.rs")?),
/// )?
/// .with_timeout_seconds(5)?;
/// assert_eq!(spec.timeout_seconds().get(), 5);
///
/// let error = CommandSpec::new(vec!["tool".to_owned(), "{path}".to_owned()], None).unwrap_err();
/// assert_eq!(error.code(), "CONFIG_INVALID");
/// # Ok::<(), sc_agent_types::JevError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandSpec {
    command: Vec<String>,
    timeout_seconds: NonZeroU64,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<RepoRelativePath>,
    #[serde(skip_serializing_if = "Option::is_none")]
    producer: Option<String>,
}

impl CommandSpec {
    /// A command running the argv array `command` with the default 30 second
    /// limit and no producer name.
    ///
    /// `path` is the repo-relative file substituted for `{path}`. It is
    /// validated like a `file` input path before the command runs
    /// (REQ-SAG-0312).
    ///
    /// # Errors
    ///
    /// `CONFIG_INVALID` when an argument contains `{path}` and `path` is
    /// `None`.
    pub fn new(command: Vec<String>, path: Option<RepoRelativePath>) -> Result<Self, JevError> {
        if path.is_none() && command.iter().any(|arg| arg.contains("{path}")) {
            return Err(JevError::config_invalid(
                "command argv contains {path}, but the input has no path field; set path = \"<repo-relative file>\" on the command input",
            ));
        }
        Ok(Self {
            command,
            timeout_seconds: NonZeroU64::MIN.saturating_add(DEFAULT_TIMEOUT_SECONDS - 1),
            path,
            producer: None,
        })
    }

    /// Sets the process time limit.
    ///
    /// # Errors
    ///
    /// `CONFIG_INVALID` when `seconds` is zero.
    pub fn with_timeout_seconds(mut self, seconds: u64) -> Result<Self, JevError> {
        self.timeout_seconds = timeout_from_seconds(seconds)?;
        Ok(self)
    }

    /// Sets the producer name used when the command exits with an error
    /// response on stderr (REQ-SAG-0313).
    #[must_use]
    pub fn with_producer(mut self, name: impl Into<String>) -> Self {
        self.producer = Some(name.into());
        self
    }

    /// Builds a command from parsed `check.toml` fields. The time limit is
    /// checked first, as the extractor did when it was the only check.
    pub(crate) fn from_parts(
        command: Vec<String>,
        timeout_seconds: u64,
        path: Option<RepoRelativePath>,
        producer: Option<String>,
    ) -> Result<Self, JevError> {
        let timeout_seconds = timeout_from_seconds(timeout_seconds)?;
        let mut spec = Self::new(command, path)?;
        spec.timeout_seconds = timeout_seconds;
        spec.producer = producer;
        Ok(spec)
    }

    /// Argument vector. The check.toml key is `command`.
    #[must_use]
    pub fn command(&self) -> &[String] {
        &self.command
    }

    /// Process time limit in seconds; 30 unless set.
    #[must_use]
    pub fn timeout_seconds(&self) -> NonZeroU64 {
        self.timeout_seconds
    }

    /// Repo-relative file substituted for `{path}`.
    #[must_use]
    pub fn path(&self) -> Option<&RepoRelativePath> {
        self.path.as_ref()
    }

    /// Producer name for an error response; `None` uses the file stem of
    /// `command[0]`.
    #[must_use]
    pub fn producer(&self) -> Option<&str> {
        self.producer.as_deref()
    }
}

/// The one zero-timeout check, shared by construction and parsing.
fn timeout_from_seconds(seconds: u64) -> Result<NonZeroU64, JevError> {
    NonZeroU64::new(seconds).ok_or_else(|| {
        JevError::config_invalid("command timeout_seconds must be greater than zero")
    })
}
