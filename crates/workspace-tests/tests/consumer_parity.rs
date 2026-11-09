//! One golden pins both consumers (REQ-SAG-0604, ARCHP-014).
//!
//! The built `sc-ai` binary (release, isolated target dir, no `test-util`)
//! dry-runs the committed `git-repo` fixture; its `data` must equal
//! `tests/fixtures/jev/run-response/dry_run_cli.json`. The pytest
//! `test_run_data_matches_golden` asserts the Python result equals the same
//! file (minus `readback`).

#[path = "../../../tests/support/deadline.rs"]
mod deadline;
mod support;

use std::path::{Path, PathBuf};
use std::process::Command;

use deadline::spawn_with_deadline;
use serde_json::Value;
use support::bin_path::bin_path;

fn workspace_root() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .canonicalize()
        .expect("workspace root")
}

#[expect(
    clippy::disallowed_methods,
    reason = "process spawn: runs the built sc-ai binary, the unit under test"
)]
fn sc_ai(binary: &Path) -> Command {
    Command::new(binary)
}

#[test]
fn test_cli_and_python_data_match_golden() {
    let root = workspace_root();
    let binary = bin_path("sc-ai", "sc-ai");
    let output = spawn_with_deadline(
        sc_ai(&binary)
            .args(["jev", "dry-run", "--repo-root"])
            .arg(root.join("tests/fixtures/jev/git-repo"))
            .env_remove("SC_LOG_ROOT"),
        None,
    );
    assert!(
        output.status.success(),
        "sc-ai failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut envelope: Value = serde_json::from_slice(&output.stdout).expect("envelope");
    assert_eq!(envelope["operation"], "jev.dry-run");
    let data = envelope["data"].as_object_mut().expect("data");
    // `readback` is the one consumer-added key and holds local paths.
    assert!(data.remove("readback").is_some(), "sc-ai adds readback");
    let golden: Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("tests/fixtures/jev/run-response/dry_run_cli.json"))
            .expect("golden"),
    )
    .expect("golden json");
    assert_eq!(Value::Object(data.clone()), golden);
}

/// `--request-json` and Python's `request_json=` read the same document and
/// give the same digest; `tests/fixtures/jev-python/tests/test_request_json.py`
/// pins the same literal (REQ-QA-002).
#[test]
fn test_cli_request_json_matches_the_python_pin() {
    let binary = bin_path("sc-ai", "sc-ai");
    let dir = tempfile::tempdir().expect("temp");
    let request = dir.path().join("request.json");
    std::fs::write(
        &request,
        r#"{"questions":[{"id":"relevant","prompt":"Does it apply?","choices":["no","yes"]}],"state":[]}"#,
    )
    .expect("request");
    let output = spawn_with_deadline(
        sc_ai(&binary)
            .args(["jev", "dry-run", "--repo-root"])
            .arg(dir.path())
            .arg("--request-json")
            .arg(&request)
            .env_remove("SC_LOG_ROOT"),
        None,
    );
    assert!(
        output.status.success(),
        "sc-ai failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: Value = serde_json::from_slice(&output.stdout).expect("envelope");
    assert_eq!(
        envelope["data"]["payload_sha256"],
        "79b58710e937c53180dc2f263c9e09fce6afc2168b4a3f019b4f600aac30e0a3"
    );
}
