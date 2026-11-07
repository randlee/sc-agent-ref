//! `command` input contract (REQ-SAG-0312, REQ-SAG-0313, REQ-SAG-0305, D11).
//!
//! Each case writes a `check.toml` whose `[inputs.live]` command runs a fake
//! producer from `tests/fixtures/producers/`, then calls `run::prepare` (the
//! one public way to run inputs) and reads the result from the prepared
//! payload and producer. The real `sc-treesitter` end-to-end test lands in
//! r-3b.

#[path = "../../../tests/support/deadline.rs"]
mod deadline;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use sc_agent_types::observability::v2::NoopSessionLog;
use sc_agent_types::{ErrorCode, JevError, RepoRelativePath, RepoRoot};
use sc_jev_check::run::{CheckSource, PreparedRun, RunConfig, prepare};
use tempfile::TempDir;

fn python() -> &'static str {
    if cfg!(windows) { "python" } else { "python3" }
}

fn producer(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/producers")
        .join(name)
        .display()
        .to_string()
}

/// A temp repo with `src/hook.py` and a questions document that prints the
/// `live` input's subjects.
fn repo() -> TempDir {
    let dir = tempfile::Builder::new()
        .prefix("sc-jev-check-command-")
        .tempdir()
        .expect("temp");
    std::fs::create_dir(dir.path().join("src")).expect("src");
    std::fs::write(dir.path().join("src/hook.py"), "if armed:\n    fire()\n").expect("hook");
    let questions = serde_json::json!({
        "schema": "plan-anchoring/questions/1",
        "version": 1,
        "questions": {
            "relevant": {
                "type": "choice",
                "instructions": "Subjects:\n{live}",
                "criteria": {"no": "No.", "yes": "Yes."}
            }
        }
    });
    std::fs::write(dir.path().join("questions.json"), questions.to_string()).expect("questions");
    dir
}

/// `check.toml` with `[inputs.live]` running `argv`, plus `extra` lines.
fn check_toml(argv: &[String], extra: &str) -> String {
    let command = serde_json::to_string(argv).expect("argv");
    format!(
        "[questions]\npath = \"questions.json\"\n\n[inputs.live]\nkind = \"command\"\ncommand = {command}\n{extra}"
    )
}

fn path_line(path: &str) -> String {
    format!("path = \"{path}\"\n")
}

/// Writes `toml` as `check.toml` and prepares a dry run of it.
fn run(repo: &Path, toml: &str) -> Result<PreparedRun, JevError> {
    std::fs::write(repo.join("check.toml"), toml).expect("check.toml");
    let config = RunConfig::new(RepoRoot::new(repo), Arc::new(NoopSessionLog)).dry_run(true);
    prepare(
        CheckSource::CheckFile(RepoRelativePath::try_from("check.toml").expect("path")),
        config,
    )
}

fn payload_text(prepared: &PreparedRun) -> &str {
    prepared.request().payload().as_str()
}

#[test]
fn test_command_path_placeholder_from_check_toml() {
    let dir = repo();
    let argv = vec![
        python().into(),
        producer("emit_subjects.py"),
        "{path}".into(),
    ];
    let prepared = run(dir.path(), &check_toml(&argv, &path_line("src/hook.py")))
        .expect("command input with a path field runs");
    let text = payload_text(&prepared);
    assert!(
        text.contains("src/hook.py\\nif armed:\\n    fire()"),
        "the producer received {{path}} = src/hook.py: {text}"
    );
    assert_eq!(prepared.producer().expect("producer").name, "fake-producer");
}

#[test]
fn test_command_placeholder_without_path_is_config_invalid() {
    let dir = repo();
    let marker = dir.path().join("ran");
    let argv = vec![
        python().into(),
        producer("emit_subjects.py"),
        "{path}".into(),
        marker.display().to_string(),
    ];
    let error = run(dir.path(), &check_toml(&argv, "")).expect_err("no path field");
    assert_eq!(error.code(), ErrorCode::ConfigInvalid);
    assert!(
        error.fix_hint().contains("path field"),
        "{}",
        error.fix_hint()
    );
    assert!(!marker.exists(), "the producer must not run");

    // A `path` that is not repo-relative is refused when check.toml is parsed.
    for bad in ["../outside.py", "/etc/hosts"] {
        let error = run(dir.path(), &check_toml(&argv, &path_line(bad))).expect_err(bad);
        assert_eq!(error.code(), ErrorCode::ConfigInvalid, "{bad}");
    }
    // A missing file is refused before the producer runs.
    let error =
        run(dir.path(), &check_toml(&argv, &path_line("src/absent.py"))).expect_err("absent");
    assert_eq!(error.code(), ErrorCode::InputMissing);
    assert!(!marker.exists(), "the producer must not run");
}

#[cfg(unix)]
#[test]
fn test_command_path_symlink_outside_root_refused() {
    let dir = repo();
    let outside = tempfile::tempdir().expect("outside");
    std::fs::write(outside.path().join("secret.py"), "OUTSIDE-SECRET\n").expect("secret");
    std::os::unix::fs::symlink(
        outside.path().join("secret.py"),
        dir.path().join("src/leak.py"),
    )
    .expect("symlink");
    std::os::unix::fs::symlink(outside.path(), dir.path().join("linked")).expect("dir symlink");
    let marker = dir.path().join("ran");
    let argv = vec![
        python().into(),
        producer("emit_subjects.py"),
        "{path}".into(),
        marker.display().to_string(),
    ];
    for path in ["src/leak.py", "linked/secret.py"] {
        let error = run(dir.path(), &check_toml(&argv, &path_line(path))).expect_err(path);
        assert_eq!(error.code(), ErrorCode::InputInvalid, "{path}");
        assert!(
            error.fix_hint().contains("outside the repo root"),
            "{error}"
        );
        assert!(!error.to_string().contains("OUTSIDE-SECRET"));
    }
    let error = run(dir.path(), &check_toml(&argv, &path_line("src"))).expect_err("directory");
    assert_eq!(error.code(), ErrorCode::InputInvalid);
    assert!(!marker.exists(), "the producer must not run");
}

#[test]
fn test_command_stderr_error_response_reraised() {
    let dir = repo();
    let line = r#"{"error":"LANGUAGE_UNAVAILABLE","message":"Use python.","schema_version":1}"#;
    let argv = vec![
        python().into(),
        producer("fail_with_stderr.py"),
        line.into(),
    ];
    let error = run(dir.path(), &check_toml(&argv, "")).expect_err("producer failed");
    assert!(matches!(error, JevError::ProducerError { .. }), "{error:?}");
    assert_eq!(error.code(), ErrorCode::LanguageUnavailable);
    // Without a `producer` field the name is the stem of argv[0], which for
    // an interpreter-run script is the interpreter.
    let program = Path::new(python())
        .file_stem()
        .and_then(|stem| stem.to_str())
        .expect("stem");
    assert_eq!(error.fix_hint(), format!("{program}: Use python."));
    // `producer = "..."` names it explicitly (REQ-SAG-0313).
    let error = run(
        dir.path(),
        &check_toml(&argv, "producer = \"fake-producer\"\n"),
    )
    .expect_err("producer failed");
    assert_eq!(error.code(), ErrorCode::LanguageUnavailable);
    assert_eq!(error.fix_hint(), "fake-producer: Use python.");

    let producer_line = r#"{"error":"QUERY_NOT_FOUND","message":"No query branches.","schema_version":1,"details":{"producer":"sc-treesitter"}}"#;
    let argv = vec![
        python().into(),
        producer("fail_with_stderr.py"),
        producer_line.into(),
    ];
    let error = run(dir.path(), &check_toml(&argv, "")).expect_err("producer failed");
    assert_eq!(error.code(), ErrorCode::QueryNotFound);
}

#[test]
fn test_command_stderr_unknown_code_is_extraction_failed() {
    let dir = repo();
    let unknown = r#"{"error":"NOT_A_CODE","message":"x","schema_version":1}"#;
    let argv = vec![
        python().into(),
        producer("fail_with_stderr.py"),
        unknown.into(),
    ];
    let error = run(dir.path(), &check_toml(&argv, "")).expect_err("unknown id");
    assert_eq!(error.code(), ErrorCode::ExtractionFailed);
    assert!(error.to_string().contains("NOT_A_CODE"), "{error}");

    for line in ["plain text failure", r#"{"error": 1}"#] {
        let argv = vec![
            python().into(),
            producer("fail_with_stderr.py"),
            line.into(),
        ];
        let error = run(dir.path(), &check_toml(&argv, "")).expect_err(line);
        assert_eq!(error.code(), ErrorCode::ExtractionFailed, "{line}");
        assert!(error.to_string().contains("exit 2"), "{error}");
    }
}

const PROBE_KEY: &str = "SC_JEV_CHECK_COMMAND_PROBE_API_KEY";
const PROBE_VALUE: &str = "sentinel-command-target-4b1e";
const PROBE_DONE: &str = "command-env-probe-ran";

/// D11.2 / ADR-SAG-0020: re-runs this test binary with a sentinel
/// credential in its environment (`unsafe_code` is forbidden, so a test
/// cannot set the variable in its own process) and checks the producer did
/// not see it. The child is bounded by the shared deadline (FTQ-004).
#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "process spawn: re-runs this test binary with a sentinel credential in its environment"
)]
fn test_command_child_env_is_allowlisted() {
    let exe = std::env::current_exe().expect("test binary");
    let mut command = Command::new(exe);
    command
        .args([
            "--exact",
            "env_probe_child",
            "--include-ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(PROBE_KEY, PROBE_VALUE)
        .env("OPENAI_API_KEY", PROBE_VALUE);
    let output = deadline::spawn_with_deadline(&mut command, None);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "probe failed: {stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains(PROBE_DONE), "probe did not run: {stdout}");
}

#[test]
#[ignore = "child half of test_command_child_env_is_allowlisted"]
fn env_probe_child() {
    if std::env::var_os(PROBE_KEY).is_none() {
        return;
    }
    let dir = repo();
    let argv = vec![python().into(), producer("print_env.py")];
    let prepared = run(dir.path(), &check_toml(&argv, "")).expect("env probe");
    let text: String = serde_json::from_str::<serde_json::Value>(payload_text(&prepared))
        .expect("payload")["questions"][0]["prompt"]
        .as_str()
        .expect("prompt")
        .to_owned();
    assert!(!text.contains(PROBE_VALUE), "sentinel leaked: {text}");
    assert!(!text.contains(PROBE_KEY), "key name leaked: {text}");
    assert!(!text.contains("OPENAI_API_KEY"), "key name leaked: {text}");
    let names: Vec<String> = text
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once('='))
        .map(|(name, _)| name.to_ascii_uppercase())
        .collect();
    assert!(
        !names.is_empty(),
        "the producer printed its environment: {text}"
    );
    let allowed = [
        "PATH",
        "HOME",
        "USERPROFILE",
        "SYSTEMROOT",
        "TEMP",
        "TMP",
        "TMPDIR",
        "LANG",
        "TERM",
    ];
    for name in &names {
        assert!(
            allowed.contains(&name.as_str()) || name.starts_with("LC_") || is_interpreter_var(name),
            "{name} is outside the allowlist"
        );
    }
    println!("{PROBE_DONE}");
}

/// Variables the Python interpreter itself may add to `os.environ`.
fn is_interpreter_var(name: &str) -> bool {
    // macOS sets `__CF_USER_TEXT_ENCODING` for every process; Python on
    // macOS may add `__PYVENV_LAUNCHER__`. Neither comes from this process.
    name.starts_with("__")
}
