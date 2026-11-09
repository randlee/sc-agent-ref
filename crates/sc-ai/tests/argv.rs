//! Argv edge cases of the clap parser (REQ-SAG-0712, port of the a-10
//! hand-rolled-parser tests).
//!
//! Every case runs the built binary. Parse failures must be one
//! `ScAgentErrorResponse` line on stderr with exit 2 and the code the
//! hand-rolled parser used; `--help` and `--version` print to stdout and
//! exit 0. Nothing here needs a backend: every run is a parse failure, help,
//! or a dry-run.

#[path = "../../../tests/support/deadline.rs"]
mod deadline;

use std::path::Path;
use std::process::{Command, Output};

use deadline::spawn_with_deadline;
use serde_json::Value;
use tempfile::TempDir;

#[expect(
    clippy::disallowed_methods,
    reason = "process spawn: runs the built sc-ai binary, the unit under test"
)]
fn sc_ai() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sc-ai"))
}

fn run(cwd: &Path, args: &[&str]) -> Output {
    spawn_with_deadline(sc_ai().args(args).current_dir(cwd), None)
}

fn error_of(output: &Output) -> Value {
    assert_eq!(output.status.code(), Some(2), "status {:?}", output.status);
    assert!(output.stdout.is_empty(), "stdout must be empty on failure");
    let stderr = output.stderr.strip_suffix(b"\n").unwrap_or(&output.stderr);
    assert!(!stderr.contains(&b'\n'), "stderr must be one line");
    let value: Value = serde_json::from_slice(stderr).expect("stderr json");
    assert_eq!(value["schema_version"], 1);
    value
}

fn error_code(output: &Output) -> String {
    error_of(output)["error"]
        .as_str()
        .expect("error code")
        .to_owned()
}

fn stdout_text(output: &Output) -> String {
    assert!(
        output.status.success(),
        "status {:?}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty(), "stderr must be empty on success");
    String::from_utf8(output.stdout.clone()).expect("utf8 stdout")
}

fn repo() -> TempDir {
    let dir = tempfile::Builder::new()
        .prefix("sc-ai-argv-")
        .tempdir()
        .expect("temp dir");
    std::fs::write(
        dir.path().join("questions.json"),
        r#"{"schema":"plan-anchoring/questions/1","version":1,"questions":{"relevant":{"type":"choice","instructions":"Does it apply?","criteria":{"no":"No.","yes":"Yes."}}}}"#,
    )
    .expect("questions");
    std::fs::write(
        dir.path().join("check.toml"),
        "[questions]\npath = \"questions.json\"\n",
    )
    .expect("check");
    dir
}

fn message(output: &Output) -> String {
    error_of(output)["message"]
        .as_str()
        .expect("message")
        .to_owned()
}

#[test]
fn test_version_flag_exits_zero() {
    let dir = TempDir::new().expect("temp");
    let expected = format!("sc-ai {}\n", env!("CARGO_PKG_VERSION"));
    for flag in ["--version", "-V"] {
        assert_eq!(stdout_text(&run(dir.path(), &[flag])), expected, "{flag}");
    }
}

#[test]
fn test_help_prints_usage_and_exits_zero() {
    let dir = TempDir::new().expect("temp");
    for args in [
        &["--help"][..],
        &["-h"],
        &["jev", "--help"],
        &["jev", "run", "--help"],
        &["jev", "dry-run", "-h"],
        &["jev", "ask", "--help"],
    ] {
        let text = stdout_text(&run(dir.path(), args));
        assert!(text.contains("Usage: sc-ai"), "{args:?}: {text}");
    }
    let text = stdout_text(&run(dir.path(), &["jev", "run", "--help"]));
    for needle in [
        "--llm-system-prompt-file",
        "--repo-root",
        "--chain-context",
        "SC_LOG_ROOT",
        ".sc-ai",
    ] {
        assert!(text.contains(needle), "{needle} missing from {text}");
    }
    assert!(
        !text.contains("scripted-answers"),
        "the test seam flag must stay hidden"
    );
}

/// No arguments is an argument error (exit 2, `INPUT_INVALID`), not help.
#[test]
fn test_no_arguments_is_input_invalid() {
    let dir = TempDir::new().expect("temp");
    let output = run(dir.path(), &[]);
    assert_eq!(error_code(&output), "INPUT_INVALID");
    assert!(message(&output).contains("command is required"));
    assert_eq!(
        error_code(&run(dir.path(), &["jev"])),
        "INPUT_INVALID",
        "jev needs run, dry-run or ask"
    );
}

#[test]
fn test_unknown_command_flag_and_positional_are_input_invalid() {
    let dir = TempDir::new().expect("temp");
    for args in [
        &["nope"][..],
        &["jev", "nope"],
        &["jev", "run", "--nope"],
        &["jev", "run", "stray"],
        &["jev", "ask", "--check", "check.toml"],
        &["jev", "run", "--session-id", "x"],
    ] {
        assert_eq!(
            error_code(&run(dir.path(), args)),
            "INPUT_INVALID",
            "{args:?}"
        );
    }
}

#[test]
fn test_missing_and_empty_values_are_input_invalid() {
    let dir = TempDir::new().expect("temp");
    for args in [
        &["jev", "run", "--repo-root"][..],
        &["jev", "run", "--check="],
        &["jev", "ask", "--session-id", "01JC4Z7A5B0000000000000009"],
        &[
            "jev",
            "ask",
            "--session-id",
            "01JC4Z7A5B0000000000000009",
            "--text=",
        ],
    ] {
        assert_eq!(
            error_code(&run(dir.path(), args)),
            "INPUT_INVALID",
            "{args:?}"
        );
    }
}

/// `--request-json` selects its own document: `--check` and `--rev` do not
/// apply to it.
#[test]
fn test_request_json_conflicts_with_check_and_rev() {
    let dir = TempDir::new().expect("temp");
    for args in [
        &["jev", "run", "--request-json", "req.json", "--rev", "HEAD"][..],
        &[
            "jev",
            "dry-run",
            "--request-json",
            "req.json",
            "--check",
            "c.toml",
        ],
    ] {
        let output = run(dir.path(), args);
        assert_eq!(error_code(&output), "INPUT_INVALID", "{args:?}");
        assert!(message(&output).contains("cannot be used with"), "{args:?}");
    }
}

/// A value that starts with `-` is a value, not a flag (`allow_hyphen_values`),
/// and `--flag=value` binds explicitly.
#[test]
fn test_hyphen_values_are_values_not_flags() {
    let dir = TempDir::new().expect("temp");
    // `--text` takes `-1 is wrong` and `--help` as text; the session id is
    // checked first, so the failure is the id (not a ULID), never help.
    for text in [
        &["--text", "-1 is wrong"][..],
        &["--text=--help"],
        &["--text", "-h"],
    ] {
        let mut args = vec!["jev", "ask", "--session-id", "x"];
        args.extend_from_slice(text);
        let output = run(dir.path(), &args);
        assert_eq!(error_code(&output), "INPUT_INVALID", "{text:?}");
        assert!(message(&output).contains("ULID"), "{text:?}");
    }
    // `--rev -p` reaches `Rev::parse`, which names the revision, not clap.
    let output = run(dir.path(), &["jev", "dry-run", "--rev", "-p"]);
    assert_eq!(error_code(&output), "INPUT_INVALID");
    assert!(message(&output).to_ascii_lowercase().contains("revision"));
}

/// `--rev=--output=FILE` must fail in argument parsing: git never runs, so
/// no file appears.
#[test]
fn test_rev_option_injection_rejected() {
    let repo = repo();
    let target = repo.path().join("pwned.txt");
    let joined = format!("--rev=--output={}", target.display());
    let separate = format!("--output={}", target.display());
    for args in [
        vec!["jev", "dry-run", joined.as_str()],
        vec!["jev", "dry-run", "--rev", separate.as_str()],
    ] {
        let mut full = args;
        full.extend(["--repo-root", repo.path().to_str().expect("utf8")]);
        assert_eq!(
            error_code(&run(repo.path(), &full)),
            "INPUT_INVALID",
            "{full:?}"
        );
        assert!(!target.exists(), "{full:?} created a file");
    }
}

#[test]
fn test_backend_values_are_parsed_once() {
    let repo = repo();
    let root = repo.path().to_str().expect("utf8");
    // Kebab-case aliases are not backend ids.
    let alias = run(
        repo.path(),
        &[
            "jev",
            "dry-run",
            "--repo-root",
            root,
            "--backend",
            "anthropic-haiku",
        ],
    );
    assert_eq!(error_code(&alias), "INPUT_INVALID");
    // A repeated backend is a configuration error.
    let repeat = run(
        repo.path(),
        &[
            "jev",
            "dry-run",
            "--repo-root",
            root,
            "--backend",
            "jev",
            "--backend",
            "jev",
        ],
    );
    assert_eq!(error_code(&repeat), "CONFIG_INVALID");
    // A valid list works.
    let ok = run(
        repo.path(),
        &[
            "jev",
            "dry-run",
            "--repo-root",
            root,
            "--backend",
            "jev",
            "--backend",
            "luna",
        ],
    );
    let envelope: Value = serde_json::from_str(&stdout_text(&ok)).expect("envelope");
    assert_eq!(envelope["data"]["limits"][0]["backend"], "jev");
    assert_eq!(envelope["data"]["limits"][1]["backend"], "luna");
}

#[test]
fn test_chain_context_is_a_run_flag() {
    let repo = repo();
    let root = repo.path().to_str().expect("utf8");
    let chain = run(
        repo.path(),
        &["jev", "dry-run", "--repo-root", root, "--chain-context"],
    );
    let envelope: Value = serde_json::from_str(&stdout_text(&chain)).expect("envelope");
    assert_eq!(envelope["data"]["equal_input"]["mode"], "chain_context");
    // `ask` has no multi-backend mode.
    let ask = run(
        repo.path(),
        &[
            "jev",
            "ask",
            "--chain-context",
            "--session-id",
            "01JC4Z7A5B0000000000000009",
            "--text",
            "why?",
        ],
    );
    assert_eq!(error_code(&ask), "INPUT_INVALID");
}

/// Usage text names `sc-ai`, never argv\[0\].
#[test]
fn test_usage_ignores_the_binary_name() {
    let dir = TempDir::new().expect("temp");
    let text = stdout_text(&run(dir.path(), &["jev", "run", "--help"]));
    assert!(text.contains("Usage: sc-ai jev run "), "{text}");
}

#[test]
fn test_session_id_must_be_a_ulid() {
    let dir = TempDir::new().expect("temp");
    let output = run(
        dir.path(),
        &["jev", "ask", "--session-id", "not-a-ulid", "--text", "why?"],
    );
    assert_eq!(error_code(&output), "INPUT_INVALID");
    assert!(message(&output).contains("ULID"));
}
