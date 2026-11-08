//! Shared git isolation for test fixtures (FTQ-001, FTQ-002).
//!
//! Include it with `#[path = "../../../tests/support/git_env.rs"] mod git_env;`
//! (or the matching relative path). [`isolate_git`] is the one scrubbing point
//! for a test's git spawns: it removes the variables a git hook, alias or
//! worktree script exports (which would point `git -C <tmp>` at the caller's
//! repository, its index, or extra config) and replaces global and system
//! config with the null device. The list is the r-3c form used by
//! `crates/sc-agent-types/tests/git.rs`, including `GIT_DEFAULT_HASH`, so a
//! fixture `git init` also needs [`SHA1_INIT`] to pin the object format.

use std::process::Command;

/// Variables removed from every fixture git command.
pub const INHERITED_GIT_VARS: &[&str] = &[
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
    "GIT_CEILING_DIRECTORIES",
];

/// Arguments for a fixture `git init`: quiet, SHA-1 objects.
pub const SHA1_INIT: &[&str] = &["init", "-q", "--object-format=sha1"];

/// Platform null device, used for `GIT_CONFIG_GLOBAL` / `GIT_CONFIG_SYSTEM`.
pub fn null_device() -> &'static str {
    if cfg!(windows) { "NUL" } else { "/dev/null" }
}

/// Isolates `command` from the caller's repository, config and prompts.
pub fn isolate_git(command: &mut Command) -> &mut Command {
    for name in INHERITED_GIT_VARS {
        command.env_remove(name);
    }
    command
        .env("GIT_CONFIG_GLOBAL", null_device())
        .env("GIT_CONFIG_SYSTEM", null_device())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
}

/// A `git -C <dir>` command with isolation, a fixed identity, no signing and
/// no hooks. Callers add the subcommand and run it with a deadline.
#[expect(
    clippy::disallowed_methods,
    reason = "process spawn: builds the isolated `git` fixture command"
)]
pub fn git_command(dir: &std::path::Path) -> Command {
    let hooks = format!("core.hooksPath={}", null_device());
    let mut command = Command::new("git");
    command.arg("-C").arg(dir).args([
        "-c",
        "user.email=fixture@example.com",
        "-c",
        "user.name=fixture",
        "-c",
        "commit.gpgsign=false",
        "-c",
        &hooks,
    ]);
    isolate_git(&mut command);
    command
}
