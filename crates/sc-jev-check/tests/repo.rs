//! `discover_main_repo_root` (REQ-SAG-0614, ADR-SAG-0027 git-enabler rule).
//!
//! One temp repository per test. Fixture git runs ignore the host's global
//! and system config and go through the workspace's bounded runner.

use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "../../../tests/support/deadline.rs"]
mod deadline;

use sc_jev_check::run::{discover_main_repo_root, discover_repo_root};

fn null_device() -> &'static str {
    if cfg!(windows) { "NUL" } else { "/dev/null" }
}

#[expect(
    clippy::disallowed_methods,
    reason = "process spawn: builds fixture repositories with isolated `git`"
)]
fn git(dir: &Path, args: &[&str]) {
    let hooks = format!("core.hooksPath={}", null_device());
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.email=t@example.com", "-c", "user.name=t"])
        .args(["-c", "commit.gpgsign=false", "-c", &hooks])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", null_device())
        .env("GIT_CONFIG_SYSTEM", null_device())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0");
    for var in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_COMMON_DIR",
        "GIT_PREFIX",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG_COUNT",
        "GIT_DEFAULT_HASH",
    ] {
        command.env_remove(var);
    }
    let output = deadline::spawn_with_deadline(&mut command, None);
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).expect("canonical")
}

fn commit_one(dir: &Path) {
    std::fs::write(dir.join("a.txt"), "a\n").expect("file");
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "init"]);
}

/// A linked worktree resolves to the repository it was added from; the
/// toplevel rule still gives the worktree itself.
#[test]
fn test_main_repo_root_resolves_worktree() {
    let main = tempfile::tempdir().expect("temp");
    git(main.path(), &["init", "-q"]);
    commit_one(main.path());
    let linked = main.path().join("linked");
    git(
        main.path(),
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "side",
            linked.to_str().expect("utf8"),
        ],
    );
    std::fs::create_dir(linked.join("sub")).expect("sub");

    for start in [
        main.path().to_path_buf(),
        linked.clone(),
        linked.join("sub"),
    ] {
        let root = discover_main_repo_root(&start).expect("main root");
        assert_eq!(
            canonical(root.as_path()),
            canonical(main.path()),
            "{start:?}"
        );
    }
    let toplevel = discover_repo_root(&linked.join("sub")).expect("toplevel");
    assert_eq!(canonical(toplevel.as_path()), canonical(&linked));
}

/// Outside git the root is the start folder made absolute.
#[test]
fn test_main_repo_root_outside_git_is_cwd() {
    let outside = tempfile::tempdir().expect("temp");
    let root = discover_main_repo_root(outside.path()).expect("root");
    assert_eq!(root.as_path(), outside.path());
    assert!(root.as_path().is_absolute());
}

/// A separate git dir is not a `.git` directory inside a work tree, so the
/// current toplevel is used.
#[test]
fn test_main_repo_root_separate_git_dir_falls_back_to_toplevel() {
    let work = tempfile::tempdir().expect("work");
    let gitdir = tempfile::tempdir().expect("gitdir");
    let separate = gitdir.path().join("store");
    git(
        work.path(),
        &[
            "init",
            "-q",
            "--separate-git-dir",
            separate.to_str().expect("utf8"),
        ],
    );
    commit_one(work.path());
    let root = discover_main_repo_root(work.path()).expect("root");
    assert_eq!(canonical(root.as_path()), canonical(work.path()));
}
