//! The dev-only `test-util` seams stay out of production (plan "Test
//! seams", RBQA-F103).

mod support;

#[path = "../../../tests/support/deadline.rs"]
mod deadline;

use std::path::Path;
use std::process::Command;

use deadline::spawn_with_deadline;
use sc_agent_types::ScAgentErrorResponse;
use support::bin_path::bin_path;

#[expect(
    clippy::disallowed_methods,
    reason = "process spawn: runs cargo tree (a graph query, no build) and the built sc-ai binary"
)]
fn command(program: &Path) -> Command {
    Command::new(program)
}

/// `cargo tree -e normal,features --prefix none --no-dedupe <extra>`.
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
        ])
        .args(extra);
    let output = spawn_with_deadline(&mut cargo, None);
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

/// The workspace libraries that define a `test-util` feature. `cargo tree`
/// prints a package's own feature nodes only in the inverted view, so each
/// is queried with `-i`, which lists every feature node on a path to it.
const TEST_UTIL_PACKAGES: &[&str] = &[
    "sc-agent-types",
    "sc-agent-http",
    "sc-jev",
    "sc-jev-backends",
    "sc-jev-check",
];

/// `cargo tree --workspace -e normal,features` is a graph query: no test
/// build and no dev edges, so a `test-util` feature appears only if a normal
/// edge enables it.
#[test]
fn test_test_util_absent_from_normal_graph() {
    // Control: the detector sees the feature when it is switched on.
    let control = tree(&[
        "-p",
        "sc-jev-check",
        "--features",
        "test-util",
        "-i",
        "sc-agent-types",
    ]);
    assert!(
        !test_util_lines(&control).is_empty(),
        "the detector must see test-util when it is enabled: {control}"
    );
    let workspace = tree(&["--workspace"]);
    assert!(
        workspace.contains("sc-ai v"),
        "the tree must list the workspace crates: {workspace}"
    );
    for package in TEST_UTIL_PACKAGES {
        let inverted = tree(&["--workspace", "-i", package]);
        assert!(
            inverted.contains(&format!("{package} v")),
            "{package} is in the graph: {inverted}"
        );
        let offenders = test_util_lines(&inverted);
        assert!(
            offenders.is_empty(),
            "test-util is enabled on a normal edge of {package}: {offenders:?}"
        );
    }
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
