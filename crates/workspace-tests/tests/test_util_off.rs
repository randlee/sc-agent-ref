//! The dev-only `test-util` seams stay out of production (plan "Test
//! seams", RBQA-F103).

mod support;

#[path = "../../../tests/support/deadline.rs"]
mod deadline;

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use deadline::{spawn_with_deadline, spawn_with_limit};
use sc_agent_types::ScAgentErrorResponse;
use support::bin_path::bin_path;

#[expect(
    clippy::disallowed_methods,
    reason = "process spawn: runs cargo tree (a graph query, no build) and the built sc-ai binary"
)]
fn command(program: &Path) -> Command {
    Command::new(program)
}

/// Longest one `cargo tree` may take: a graph query, so the 60 s runner
/// default. Calls are serialized by [`TREE_LOCK`] so they do not queue behind
/// each other on Cargo's file locks (FTQ-001, FTQ-004).
const CARGO_TREE_DEADLINE: Duration = Duration::from_secs(60);

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
        spawn_with_limit(command, None, CARGO_TREE_DEADLINE)
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

/// `cargo tree -e normal,features --prefix none --no-dedupe --locked --offline <extra>`.
fn tree(extra: &[&str]) -> String {
    let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let mut cargo = command(Path::new(env!("CARGO")));
    cargo
        .current_dir(root)
        .args([
            "tree",
            "-e",
            "normal,features",
            "--prefix",
            "none",
            "--no-dedupe",
            "--locked",
            "--offline",
        ])
        .args(extra);
    let output = run_tree(&mut cargo);
    assert!(
        output.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf8 tree")
}

fn test_util_lines(tree: &str) -> Vec<&str> {
    tree.lines()
        .filter(|line| line.contains("feature \"test-util\""))
        .collect()
}

/// The workspace libraries that define a `test-util` feature; each must be in
/// the graph the test inspects, so an empty result cannot pass on a dead tree.
const TEST_UTIL_PACKAGES: &[&str] = &[
    "sc-agent-types",
    "sc-decision-schema",
    "sc-agent-http",
    "sc-jev",
    "sc-jev-backends",
    "sc-jev-check",
];

/// `cargo tree --workspace -e normal,features` is a graph query: no test
/// build and no dev edges, so a `test-util` feature appears only if a normal
/// edge enables it. `cargo tree` prints a package's own feature nodes only in
/// the inverted view, so one invocation inverts every package that defines the
/// feature (`-i` repeats) and the output is parsed once. A second query with
/// the feature switched on is the detector's control.
#[test]
fn test_test_util_absent_from_normal_graph() {
    let control = tree(&[
        "-p",
        "sc-jev-check",
        "--features",
        "test-util",
        "-i",
        "sc-decision-schema",
    ]);
    assert!(
        !test_util_lines(&control).is_empty(),
        "the detector must see test-util when it is enabled: {control}"
    );
    let mut args = vec!["--workspace"];
    for package in TEST_UTIL_PACKAGES {
        args.extend(["-i", package]);
    }
    let workspace = tree(&args);
    assert!(
        workspace.contains("sc-ai v"),
        "the tree must list the workspace crates: {workspace}"
    );
    for package in TEST_UTIL_PACKAGES {
        assert!(
            workspace.contains(&format!("{package} v")),
            "{package} is in the graph: {workspace}"
        );
    }
    let offenders = test_util_lines(&workspace);
    assert!(
        offenders.is_empty(),
        "test-util is enabled on a normal edge: {offenders:?}"
    );
}

/// RBQA-F103 (a): the published `sc-ai` binary has no `--scripted-answers`.
#[test]
fn test_release_binary_rejects_scripted_answers() {
    let binary = bin_path("sc-ai", "sc-ai");
    let output = spawn_with_deadline(
        command(&binary)
            .args(["jev", "run", "--scripted-answers", "x"])
            .env_remove("SC_LOG_ROOT"),
        None,
    );
    assert_eq!(output.status.code(), Some(2), "{:?}", output.status);
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(lines.len(), 1, "one ScAgentErrorResponse line: {stderr}");
    let error: ScAgentErrorResponse = serde_json::from_str(lines[0]).expect("typed error line");
    assert_eq!(error.schema_version(), 1);
    let value: serde_json::Value = serde_json::from_str(lines[0]).expect("json");
    assert_eq!(value["error"], "INPUT_INVALID");
    assert!(
        value["message"]
            .as_str()
            .expect("message")
            .contains("--scripted-answers"),
        "{value}"
    );
}
