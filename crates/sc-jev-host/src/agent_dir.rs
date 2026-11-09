//! The repo-local `.sc-ai/` directory: owner-only, and ignored by git.
//!
//! The `sc-ai` default log root and session directory sit under
//! `<repo-root>/.sc-ai`. Session files hold prompts and the logs hold answers
//! and repo paths, so a `git add -A` must not commit them. When sc-agent
//! creates that directory it writes `.gitignore` with a single `*` (which
//! also ignores the `.gitignore` itself), so the directory never shows in
//! `git status`.
//!
//! The rule is narrow on purpose:
//!
//! - only a directory sc-agent creates in this call (an existing `.sc-ai/` is
//!   the user's and is left alone);
//! - only a policy default under the repo root (`DefaultRoot::UnderRepoRoot`,
//!   `SessionDefault::UnderRepoRoot`): the Python policy defaults to
//!   `~/.sc-agent/log`, outside any repository;
//! - only when the resolved log root or session directory is inside it;
//! - an existing `.gitignore` is never overwritten.

use std::io::Write;
use std::path::{Component, Path, PathBuf};

use sc_agent_types::RepoRoot;

use crate::compose::ResolvedPaths;
use crate::obs::create_private_dirs;
use crate::paths::{DefaultRoot, PathPolicy, SessionDefault};

/// The ignore file's content: everything in the directory, itself included.
const GITIGNORE: &str = "*\n";

/// The repo-local directories `policy` defaults to, as `<repo-root>/<first
/// component>` (so `.sc-ai/sessions` yields `.sc-ai`).
fn repo_local_dirs(policy: &PathPolicy, repo: &RepoRoot) -> Vec<PathBuf> {
    let relative = [
        match &policy.log_root_default {
            DefaultRoot::UnderRepoRoot(path) => Some(path),
            _ => None,
        },
        match &policy.session_dir_default {
            SessionDefault::UnderRepoRoot(path) => Some(path),
            _ => None,
        },
    ];
    let mut dirs: Vec<PathBuf> = Vec::new();
    for path in relative.into_iter().flatten() {
        let first = path.components().find_map(|part| match part {
            Component::Normal(name) => Some(name),
            _ => None,
        });
        if let Some(first) = first {
            let dir = repo.as_path().join(first);
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
    }
    dirs
}

/// Creates the repo-local directory the run is about to write into, owner-only
/// and ignored by git, when it does not exist yet. Best effort: a failure
/// leaves the directory to be created later, without the ignore file.
pub(crate) fn prepare(policy: &PathPolicy, resolved: &ResolvedPaths) {
    for dir in repo_local_dirs(policy, &resolved.repo_root) {
        let used = resolved.log_root.starts_with(&dir) || resolved.sessions_dir.starts_with(&dir);
        if used && !dir.exists() {
            create_ignored(&dir);
        }
    }
}

/// Creates `dir` (owner-only) and its `.gitignore`, never overwriting one.
fn create_ignored(dir: &Path) {
    if create_private_dirs(dir).is_err() {
        return;
    }
    let created = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(".gitignore"));
    if let Ok(mut file) = created {
        let _ = file.write_all(GITIGNORE.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(repo: &Path, log_root: PathBuf, sessions_dir: PathBuf) -> ResolvedPaths {
        ResolvedPaths {
            repo_root: RepoRoot::new(repo),
            log_root,
            sessions_dir,
        }
    }

    #[test]
    fn a_new_default_directory_gets_a_star_gitignore() {
        let repo = tempfile::tempdir().expect("temp");
        let dir = repo.path().join(".sc-ai");
        prepare(
            &PathPolicy::cli(),
            &resolved(repo.path(), dir.clone(), dir.join("sessions")),
        );
        assert_eq!(
            std::fs::read_to_string(dir.join(".gitignore")).expect("gitignore"),
            "*\n"
        );
    }

    #[test]
    fn an_existing_directory_and_its_gitignore_are_left_alone() {
        let repo = tempfile::tempdir().expect("temp");
        let dir = repo.path().join(".sc-ai");
        std::fs::create_dir(&dir).expect("dir");
        let paths = resolved(repo.path(), dir.clone(), dir.join("sessions"));
        prepare(&PathPolicy::cli(), &paths);
        assert!(!dir.join(".gitignore").exists(), "not created by us");

        // A race that leaves a user's file in place is never overwritten.
        std::fs::write(dir.join(".gitignore"), "keep\n").expect("write");
        create_ignored(&dir);
        assert_eq!(
            std::fs::read_to_string(dir.join(".gitignore")).expect("read"),
            "keep\n"
        );
    }

    #[test]
    fn a_run_elsewhere_creates_nothing_in_the_repo() {
        let repo = tempfile::tempdir().expect("temp");
        let elsewhere = tempfile::tempdir().expect("temp");
        prepare(
            &PathPolicy::cli(),
            &resolved(
                repo.path(),
                elsewhere.path().join("log"),
                elsewhere.path().join("sessions"),
            ),
        );
        assert!(!repo.path().join(".sc-ai").exists());
    }

    #[test]
    fn sessions_under_the_repo_directory_alone_still_ignore_it() {
        let repo = tempfile::tempdir().expect("temp");
        let elsewhere = tempfile::tempdir().expect("temp");
        let dir = repo.path().join(".sc-ai");
        prepare(
            &PathPolicy::cli(),
            &resolved(
                repo.path(),
                elsewhere.path().join("log"),
                dir.join("sessions"),
            ),
        );
        assert!(dir.join(".gitignore").exists());
    }

    #[test]
    fn the_python_policy_has_no_repo_local_directory() {
        let repo = tempfile::tempdir().expect("temp");
        let dir = repo.path().join("log");
        prepare(
            &PathPolicy::python(),
            &resolved(repo.path(), dir.clone(), dir.join("sessions")),
        );
        assert!(!dir.exists());
    }

    #[cfg(unix)]
    #[test]
    fn the_created_directory_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let repo = tempfile::tempdir().expect("temp");
        let dir = repo.path().join(".sc-ai");
        prepare(
            &PathPolicy::cli(),
            &resolved(repo.path(), dir.clone(), dir.join("sessions")),
        );
        let mode = std::fs::metadata(&dir).expect("meta").permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }
}
