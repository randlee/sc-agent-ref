//! Module seams are cargo features (REQ-SAG-0107, ADR-SAG-0027).
//!
//! `cargo tree -p sc-agent-types --no-default-features -e normal` is pinned
//! against `tests/fixtures/no-default-features-deps.txt` (crate names only,
//! so a patch bump does not churn it; `--target all`, so the list does not
//! depend on the host platform). `cargo tree` runs `--offline` through the
//! workspace's bounded runner, so it needs a warm registry cache (any prior
//! `cargo build` or `cargo test` of the workspace provides one). `sha2` is a core dependency because
//! `Sha256Digest::of_text` is in core `ids`.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

#[path = "../../../tests/support/deadline.rs"]
#[expect(
    dead_code,
    reason = "the shared runner is included whole; this target uses only spawn_with_limit"
)]
mod deadline;

/// Longest one `cargo tree` may take: a graph query, so the 60 s runner
/// default (FTQ-001, FTQ-004).
const CARGO_TREE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(60);

/// Serializes `cargo tree` calls in this test binary: tests run on parallel
/// threads and concurrent cargo invocations queue on Cargo's file locks.
static TREE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Runs a `cargo tree` command under [`CARGO_TREE_DEADLINE`], with a failure
/// message that names Cargo lock contention, the usual cause of a stall.
fn run_tree(command: &mut Command) -> std::process::Output {
    let _guard = TREE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        deadline::spawn_with_limit(command, None, CARGO_TREE_DEADLINE)
    }))
    .unwrap_or_else(|panic| {
        let detail = panic
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| panic.downcast_ref::<&str>().map(|text| (*text).to_owned()))
            .unwrap_or_default();
        panic!(
            "cargo tree did not finish within {CARGO_TREE_DEADLINE:?}; another cargo process holding a Cargo file lock (\"Blocking waiting for file lock\") is the usual cause: {detail}"
        )
    })
}

#[expect(
    clippy::disallowed_methods,
    reason = "process spawn: runs `cargo tree`, a graph query"
)]
fn tree(args: &[&str]) -> BTreeSet<String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let mut command = Command::new(cargo);
    command
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "tree",
            "-p",
            "sc-agent-types",
            "-e",
            "normal",
            "--prefix",
            "none",
        ])
        .args(["--locked", "--offline", "--target", "all"])
        .args(args);
    let output = run_tree(&mut command);
    assert!(
        output.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("utf8")
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_owned)
        .collect()
}

#[test]
fn test_no_default_features_dep_set() {
    let pinned: BTreeSet<String> = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/no-default-features-deps.txt"),
    )
    .expect("pinned list")
    .lines()
    .map(str::to_owned)
    .collect();
    let actual = tree(&["--no-default-features"]);
    assert_eq!(actual, pinned);
    assert!(actual.contains("sc-sha"));
    assert!(actual.contains("ulid"));
    assert!(!actual.contains("uuid"));
}

#[test]
fn test_no_forbidden_crate_in_any_normal_tree() {
    for args in [
        &[][..],
        &["--no-default-features"][..],
        &["--all-features"][..],
    ] {
        let names = tree(args);
        for forbidden in [
            "sc-observability",
            "sc-observability-types",
            "ureq",
            "pyo3",
            "tree-sitter",
        ] {
            assert!(!names.contains(forbidden), "{forbidden} with {args:?}");
        }
    }
}
