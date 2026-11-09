//! The published `sc-ai` binary has no `--scripted-answers` (plan "Test
//! seams", RBQA-F103 (a)). The graph half is `test_util_off`; this half builds
//! a release binary, so it lives in its own test binary and never runs a
//! `cargo build` in parallel with that binary's `cargo tree` calls.

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
    reason = "process spawn: runs the built sc-ai binary"
)]
fn command(program: &Path) -> Command {
    Command::new(program)
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

/// ERR-QA-18: the loopback endpoint override is a test seam too: the published
/// binary rejects `--loopback-port`, so nothing can aim it at another host.
#[test]
fn test_release_binary_rejects_loopback_port() {
    let binary = bin_path("sc-ai", "sc-ai");
    let output = spawn_with_deadline(
        command(&binary)
            .args(["jev", "run", "--loopback-port", "1"])
            .env_remove("SC_LOG_ROOT"),
        None,
    );
    assert_eq!(output.status.code(), Some(2), "{:?}", output.status);
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    let value: serde_json::Value =
        serde_json::from_str(stderr.lines().next().expect("a line")).expect("json");
    assert_eq!(value["error"], "INPUT_INVALID");
    assert_eq!(value["field"], "--loopback-port");
}
