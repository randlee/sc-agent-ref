//! Extraction for the four `check.toml` input kinds.
//!
//! [`extract_input`] matches `file`, `table-column`, `subjects`, and
//! `command` directly; there is no extractor trait. A new kind is a new
//! [`InputSpec`] variant plus an ADR (Rust API patterns, r-2 amendment).
//! No grammar crate is linked. `subjects` and `command` consume a
//! `sc/subjects/1` document that some other tool already wrote.

#![warn(missing_docs)]

mod command;
mod file;
mod read;
mod sanitize;
mod subjects;
mod table;

use crate::config::InputSpec;
use sc_agent_types::observability::v2::{GeneralEvent, GeneralLog};
use sc_agent_types::schema::subjects::{Subject, SubjectsDocument};
use sc_agent_types::{JevError, RepoRoot, Rev};

/// Subjects produced by one input, plus the source document when there is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extracted {
    /// Mapped subject id and text values.
    pub subjects: Vec<Subject>,
    /// Present for `subjects` and `command` inputs.
    pub document: Option<SubjectsDocument>,
}

pub(crate) use read::{read_repo_bytes, read_repo_text};

/// Runs the extraction selected by the input kind.
///
/// A `command` input's `{path}` placeholder takes the input's own `path`
/// field (REQ-SAG-0312); `{path}` without one is `CONFIG_INVALID` when the
/// [`CommandSpec`](crate::config::CommandSpec) is built, not here.
///
/// # Errors
///
/// Returns the [`JevError`] of the selected extraction: `INPUT_*` for an
/// unreadable path, `EXTRACTION_*` for a bad document or command result, or
/// the producer's own code when a command reports an error response on
/// stderr (REQ-SAG-0313).
#[cfg(test)]
pub fn extract_input(repo: &RepoRoot, rev: &Rev, input: &InputSpec) -> Result<Extracted, JevError> {
    extract_input_logged(
        repo,
        rev,
        input,
        &sc_agent_types::observability::v2::NoopGeneralLog,
    )
}

/// Logging target of this module.
const TARGET: &str = "check.extract";

/// The input kind as logged: the `check.toml` `kind` spelling.
fn kind_of(input: &InputSpec) -> &'static str {
    match input {
        InputSpec::File { .. } => "file",
        InputSpec::TableColumn { .. } => "table-column",
        InputSpec::Subjects { .. } => "subjects",
        InputSpec::Command(_) => "command",
    }
}

/// [`extract_input`] that reports the extraction to `log`: input kind,
/// whether it reads the working tree or a git revision, the subject count
/// and, on failure, the typed error id. Subject text, command arguments and
/// producer output are never logged.
///
/// # Errors
///
/// As [`extract_input`].
pub fn extract_input_logged(
    repo: &RepoRoot,
    rev: &Rev,
    input: &InputSpec,
    log: &dyn GeneralLog,
) -> Result<Extracted, JevError> {
    let kind = kind_of(input);
    GeneralEvent::debug(TARGET, "extract.start", "extraction started")
        .with_field("kind", kind)
        .with_field("git_rev", !rev.is_worktree())
        .emit(log);
    let result = extract_dispatch(repo, rev, input);
    match &result {
        Ok(extracted) => GeneralEvent::debug(TARGET, "extract.done", "extraction finished")
            .with_field("kind", kind)
            .with_field("subjects", extracted.subjects.len())
            .emit(log),
        Err(error) => GeneralEvent::warn(TARGET, "extract.failed", "extraction failed")
            .with_field("kind", kind)
            .with_error_code(error.code().as_str())
            .emit(log),
    }
    result
}

fn extract_dispatch(repo: &RepoRoot, rev: &Rev, input: &InputSpec) -> Result<Extracted, JevError> {
    match input {
        InputSpec::File { path } => file::extract_file(repo, rev, path),
        InputSpec::TableColumn { path, column } => table::extract_table(repo, rev, path, column),
        InputSpec::Subjects { path } => subjects::extract_subjects(repo, rev, path),
        InputSpec::Command(spec) => command::extract_command(repo, rev, spec),
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use crate::deadline::spawn_with_deadline;

    pub use tempfile::TempDir;

    /// A repo-relative path for tests.
    pub fn rel(text: &str) -> sc_agent_types::RepoRelativePath {
        sc_agent_types::RepoRelativePath::try_from(text).expect("repo-relative path")
    }

    /// A parsed revision for tests.
    pub fn rev(text: &str) -> sc_agent_types::Rev {
        sc_agent_types::Rev::parse(text).expect("revision")
    }

    pub fn temp_dir(label: &str) -> TempDir {
        tempfile::Builder::new()
            .prefix(&format!("sc-jev-check-{label}-"))
            .tempdir()
            .expect("temp dir")
    }

    pub fn fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/jev/git-repo")
    }

    pub fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    pub fn copy_fixture() -> TempDir {
        let dir = temp_dir("fixture");
        copy_tree(&fixture_root(), dir.path()).expect("copy fixture");
        dir
    }

    pub fn git_fixture() -> (TempDir, String) {
        let dir = copy_fixture();
        git(dir.path(), crate::git_env::SHA1_INIT);
        commit_all(dir.path(), "init");
        let sha = git_stdout(dir.path(), &["rev-parse", "HEAD"]);
        (dir, sha.trim().to_owned())
    }

    pub fn python() -> &'static str {
        if cfg!(windows) { "python" } else { "python3" }
    }

    /// Git with no global or system config, no terminal prompt, and a fixed
    /// identity.
    pub fn git_command(cwd: &Path) -> Command {
        crate::git_env::git_command(cwd)
    }

    /// Stages everything under `cwd` and commits it.
    pub fn commit_all(cwd: &Path, message: &str) {
        git(cwd, &["add", "-A"]);
        git(cwd, &["commit", "-q", "-m", message]);
    }

    pub fn git(cwd: &Path, args: &[&str]) {
        let output = spawn_with_deadline(git_command(cwd).args(args), None);
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn git_stdout(cwd: &Path, args: &[&str]) -> String {
        let output = spawn_with_deadline(git_command(cwd).args(args), None);
        assert!(output.status.success(), "git {args:?} failed");
        String::from_utf8(output.stdout).expect("utf-8")
    }

    fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let to = dst.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                copy_tree(&entry.path(), &to)?;
            } else {
                std::fs::copy(entry.path(), to)?;
            }
        }
        Ok(())
    }
}
