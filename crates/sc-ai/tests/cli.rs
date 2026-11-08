//! CLI parity for `jev run`, `dry-run`, and `ask`.
//!
//! Every test drives the built `sc-ai` binary (the one library seam, a-10's
//! `execute_with_overrides`, is gone). Backends come from the hidden
//! `--scripted-answers <file>` flag, which exists only under `test-util`
//! (`[[test]] required-features`); the file is the r-2b D5 scripted-answers
//! JSON. Every subprocess goes through [`spawn_with_deadline`], so a hung
//! child fails the test after 60 seconds instead of stalling CI.

#[path = "../../../tests/support/deadline.rs"]
mod deadline;
#[path = "../../../tests/support/git_env.rs"]
mod git_env;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use deadline::spawn_with_deadline;
use sc_jev_check::config::PACKAGED_SYSTEM_PROMPT;
use sc_jev_check::run::{OP_ASK, OP_DRY_RUN, OP_RUN};
use serde_json::{Value, json};
use tempfile::TempDir;

#[expect(
    clippy::disallowed_methods,
    reason = "process spawn: runs the built sc-ai binary, the unit under test"
)]
fn sc_ai() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sc-ai"))
}

fn temp_dir(label: &str) -> TempDir {
    tempfile::Builder::new()
        .prefix(&format!("sc-ai-{label}-"))
        .tempdir()
        .expect("temp")
}

fn text(path: &Path) -> &str {
    path.to_str().expect("utf8 path")
}

fn write_repo(dir: &Path) {
    std::fs::write(
        dir.join("questions.json"),
        r#"{
  "schema": "plan-anchoring/questions/1",
  "version": 1,
  "questions": {
    "relevant": {
      "type": "choice",
      "instructions": "Does it apply?",
      "criteria": {"no": "No.", "yes": "Yes."}
    }
  }
}"#,
    )
    .expect("questions");
    std::fs::write(
        dir.join("check.toml"),
        "[questions]\npath = \"questions.json\"\n",
    )
    .expect("check");
}

/// Writes `answers` (the r-2b D5 JSON) and returns the file path.
fn scripted(dir: &Path, answers: &Value) -> PathBuf {
    let path = dir.join("scripted.json");
    std::fs::write(&path, answers.to_string()).expect("scripted answers");
    path
}

/// The shared form: every backend answers `choice` to `relevant`.
fn shared(choice: &str) -> Value {
    json!({"relevant": choice})
}

/// A `sc-ai` command with the environment that could redirect it removed.
fn cli(args: &[&str]) -> Command {
    let mut command = sc_ai();
    command
        .args(args)
        .env_remove("SC_LOG_ROOT")
        .env_remove("TYPESAFE_API_KEY")
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY");
    if let Some(profile) = child_profile_file() {
        command.env("LLVM_PROFILE_FILE", profile);
    }
    command
}

/// A coverage profile file name that only this child process writes, when the
/// test run is instrumented (`LLVM_PROFILE_FILE` is set).
///
/// Hypothesis, not a proven cause: a coverage run's intermittent failure may
/// come from several instrumented children sharing one profile file and the
/// runtime reporting a merge problem on stderr, which these tests require to
/// be exactly one JSON line. `%p` (process id) and `%m` (binary signature)
/// give each child its own file in the same directory. Stderr is not filtered.
fn child_profile_file() -> Option<PathBuf> {
    let base = std::env::var_os("LLVM_PROFILE_FILE")?;
    let dir = Path::new(&base).parent().map(Path::to_path_buf);
    let name = "sc-ai-cli-child-%p-%m.profraw";
    Some(match dir {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(name),
        _ => PathBuf::from(name),
    })
}

fn run_cli(args: &[&str]) -> Output {
    spawn_with_deadline(&mut cli(args), None)
}

fn stdout_json(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "status {:?}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "stderr must be empty on success, got: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout json")
}

/// The one stderr line as JSON. The exit code must be the one the error asks
/// for: 3 when `retryable`, otherwise 2 (`docs/stability.md`).
fn stderr_json(output: &Output) -> Value {
    let stderr = output.stderr.strip_suffix(b"\n").unwrap_or(&output.stderr);
    assert!(
        !stderr.contains(&b'\n'),
        "stderr must be one line, got: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(stderr).expect("stderr json");
    let expected = if value["retryable"] == true { 3 } else { 2 };
    assert_eq!(
        output.status.code(),
        Some(expected),
        "status {:?} for {value}",
        output.status
    );
    value
}

fn canon(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

fn assert_no_prompt_leak(text: &str) {
    let packaged_first_line = PACKAGED_SYSTEM_PROMPT.lines().next().expect("prompt line");
    assert!(
        !text.contains(packaged_first_line),
        "packaged prompt leaked into {text}"
    );
    for token in [
        "OVERRIDE-PROMPT-TOKEN-a7",
        "EVALUATOR-PROMPT-a7",
        "sk-ant-test-secret",
        "sk-test-whitespace",
    ] {
        assert!(!text.contains(token), "{token} leaked into {text}");
    }
}

/// The saved session document for `session_id`.
fn session_doc(sessions: &Path, session_id: &str) -> Value {
    let path = sessions.join(format!("{session_id}.json"));
    serde_json::from_slice(&std::fs::read(&path).expect("session file")).expect("session json")
}

#[test]
fn test_multi_backend_cli_equal_input() {
    let repo = temp_dir("multi");
    write_repo(repo.path());
    let sessions = temp_dir("multi-sessions");
    let answers = scripted(
        repo.path(),
        &json!({"backends": {
            "jev": {"relevant": "yes"},
            "anthropic_haiku": {"relevant": "no"}
        }}),
    );
    let output = run_cli(&[
        "jev",
        "run",
        "--repo-root",
        text(repo.path()),
        "--check",
        "check.toml",
        "--sessions-dir",
        text(sessions.path()),
        "--backend",
        "jev",
        "--backend",
        "anthropic_haiku",
        "--scripted-answers",
        text(&answers),
    ]);
    let value = stdout_json(&output);
    assert_eq!(value["version"], "1");
    assert_eq!(value["operation"], OP_RUN);
    let data = &value["data"];
    assert!(data["session_id"].as_str().is_some());
    assert_eq!(data["session_saved"], true);
    assert_eq!(data["primary_backend"], "jev");
    assert_eq!(data["answers"]["relevant"]["choice"], "yes");
    assert_eq!(data["results"][0]["backend"], "jev");
    assert_eq!(data["results"][0]["status"], "ok");
    assert_eq!(data["results"][1]["backend"], "anthropic_haiku");
    assert_eq!(
        data["results"][1]["outcome"]["answers"]["relevant"]["choice"],
        "no"
    );
    assert!(data.get("backend").is_none());
    assert!(data.get("provider_meta").is_none());
    assert!(data.get("probabilities").is_none());
    assert!(data.get("outcomes").is_none(), "results replaces outcomes");
    assert!(data.get("warnings").is_none());
    assert_eq!(data["results"].as_array().expect("results").len(), 2);
    // r-2b: the engine default is `independent`; the scripted judges report
    // the digest of the bundle they received, so each is `matched`.
    assert_eq!(data["equal_input"]["equal"], true);
    assert_eq!(data["equal_input"]["leak"], false);
    assert_eq!(data["equal_input"]["mode"], "independent");
    assert_eq!(data["equal_input"]["prompt_sha256"], data["prompt_sha256"]);
    assert_eq!(
        data["equal_input"]["backends"],
        json!([
            {"backend": "jev", "attestation": {"status": "matched"}},
            {"backend": "anthropic_haiku", "attestation": {"status": "matched"}}
        ])
    );
    // Independent by default: the saved Haiku prompt is the packaged one.
    let session = session_doc(sessions.path(), data["session_id"].as_str().expect("id"));
    assert_eq!(session["system_prompt"], PACKAGED_SYSTEM_PROMPT);
    assert!(
        !session["system_prompt"]
            .as_str()
            .expect("prompt")
            .contains("Session context:")
    );
    assert_no_prompt_leak(&value.to_string());
}

/// REQ-SAG-0604, REQ-SAG-0603, ADR-SAG-0025: a partial response prints the
/// envelope with the successful answers on stdout, the first failure on
/// stderr, and exits 2, or 3 when that failure is retryable. An all-failed run
/// does the same with no answers, so no backend's error is lost (ERR-QA-1).
#[test]
fn test_cli_partial_failure_both_streams_exit_2() {
    let repo = temp_dir("partial");
    write_repo(repo.path());
    let sessions = temp_dir("partial-sessions");
    let answers = scripted(
        repo.path(),
        &json!({"backends": {
            "jev": {"relevant": "yes"},
            "anthropic_haiku": {"failure": {"error": "PROVIDER_BUSY"}}
        }}),
    );
    let output = run_cli(&[
        "jev",
        "run",
        "--repo-root",
        text(repo.path()),
        "--sessions-dir",
        text(sessions.path()),
        "--backend",
        "jev",
        "--backend",
        "anthropic_haiku",
        "--scripted-answers",
        text(&answers),
    ]);
    // The failure is PROVIDER_BUSY, which is retryable: exit 3.
    assert_eq!(output.status.code(), Some(3), "{:?}", output.status);
    // stdout: the envelope, with the successful answer.
    let envelope: Value = serde_json::from_slice(&output.stdout).expect("stdout envelope");
    assert_eq!(envelope["version"], "1");
    assert_eq!(envelope["operation"], OP_RUN);
    let data = &envelope["data"];
    assert_eq!(data["primary_backend"], "jev");
    assert_eq!(data["answers"]["relevant"]["choice"], "yes");
    assert_eq!(data["results"][0]["status"], "ok");
    assert_eq!(data["results"][1]["backend"], "anthropic_haiku");
    assert_eq!(data["results"][1]["status"], "failed");
    // stderr: exactly one ScAgentErrorResponse, the first failure.
    let error = stderr_json(&output);
    assert_eq!(error["schema_version"], 1);
    assert_eq!(error["error"], "PROVIDER_BUSY");
    assert_eq!(error["error"], data["results"][1]["error"]["error"]);
    assert_eq!(error["kind"], "capacity");
    assert_eq!(error["retryable"], true);
    assert!(
        error["suggested_action"]
            .as_str()
            .expect("action")
            .contains("Wait")
    );
    assert_eq!(error["details"]["backend"], "anthropic_haiku");
    assert_eq!(error["details"]["cause"], "sends");
    assert_eq!(error["details"]["attempts"], 4);
    assert_eq!(
        data["results"][1]["error"]["details"]["backend"],
        "anthropic_haiku"
    );
    assert_no_prompt_leak(&String::from_utf8_lossy(&output.stderr));

    // All backends failed: the same envelope with no answers and every
    // backend's own typed error, plus the first failure on stderr.
    let all_failed = scripted(
        repo.path(),
        &json!({"backends": {
            "jev": {"failure": {"error": "AUTH_FAILED"}},
            "anthropic_haiku": {"failure": {"error": "QUOTA_EXHAUSTED"}}
        }}),
    );
    let output = run_cli(&[
        "jev",
        "run",
        "--repo-root",
        text(repo.path()),
        "--sessions-dir",
        text(sessions.path()),
        "--backend",
        "jev",
        "--backend",
        "anthropic_haiku",
        "--scripted-answers",
        text(&all_failed),
    ]);
    let envelope: Value = serde_json::from_slice(&output.stdout).expect("stdout envelope");
    let data = &envelope["data"];
    assert!(data.get("answers").is_none(), "no answers when all failed");
    assert!(data.get("primary_backend").is_none());
    assert_eq!(data["session_saved"], false);
    assert_eq!(data["results"][0]["backend"], "jev");
    assert_eq!(data["results"][0]["error"]["error"], "AUTH_FAILED");
    assert_eq!(data["results"][0]["error"]["details"]["backend"], "jev");
    assert_eq!(data["results"][1]["backend"], "anthropic_haiku");
    assert_eq!(data["results"][1]["error"]["error"], "QUOTA_EXHAUSTED");
    assert_eq!(
        data["results"][1]["error"]["details"]["backend"],
        "anthropic_haiku"
    );
    // The first failure is on stderr; a non-retryable one exits 2.
    let error = stderr_json(&output);
    assert_eq!(error["error"], "AUTH_FAILED");
    assert_eq!(error["kind"], "permission");
    assert_eq!(error["retryable"], false);
    assert_eq!(error["details"]["backend"], "jev");
    assert_eq!(output.status.code(), Some(2));
}

/// REQ-SAG-0605, ADR-SAG-0008: independent is the default; the flag selects
/// chain context. `equal_input.mode` matches the request.
#[test]
fn test_cli_chain_context_flag() {
    let repo = temp_dir("chain");
    write_repo(repo.path());
    let answers = scripted(repo.path(), &shared("yes"));
    let run = |chain: bool| {
        let sessions = temp_dir("chain-sessions");
        let mut args = vec![
            "jev",
            "run",
            "--repo-root",
            text(repo.path()),
            "--sessions-dir",
            text(sessions.path()),
            "--backend",
            "jev",
            "--backend",
            "anthropic_haiku",
            "--scripted-answers",
            text(&answers),
        ];
        if chain {
            args.push("--chain-context");
        }
        stdout_json(&run_cli(&args))
    };
    let independent = run(false);
    assert_eq!(independent["data"]["equal_input"]["mode"], "independent");
    let chained = run(true);
    assert_eq!(chained["data"]["equal_input"]["mode"], "chain_context");
    // Both backends answered in both modes.
    for value in [&independent, &chained] {
        assert_eq!(value["data"]["results"][0]["status"], "ok");
        assert_eq!(value["data"]["results"][1]["status"], "ok");
    }
    // Same payload either way.
    assert_eq!(
        independent["data"]["prompt_sha256"],
        chained["data"]["prompt_sha256"]
    );
}

/// CR-014, REQ-SAG-0702: a log-flush failure is a warning in `data.warnings`,
/// not an error. The log file is a FIFO with no reader, so the writer thread
/// cannot drain and the flush times out.
#[cfg(unix)]
#[test]
fn test_cli_log_flush_warning_in_data() {
    let repo = temp_dir("flush");
    write_repo(repo.path());
    let sessions = temp_dir("flush-sessions");
    let logs = temp_dir("flush-logs");
    let log_dir = logs.path().join("logs");
    std::fs::create_dir_all(&log_dir).expect("logs dir");
    make_fifo(&log_dir.join("sc-ai.log.jsonl"));
    let answers = scripted(repo.path(), &shared("yes"));
    let output = run_cli(&[
        "jev",
        "run",
        "--repo-root",
        text(repo.path()),
        "--sessions-dir",
        text(sessions.path()),
        "--log-dir",
        text(logs.path()),
        "--backend",
        "anthropic_haiku",
        "--scripted-answers",
        text(&answers),
    ]);
    let value = stdout_json(&output);
    assert_eq!(value["operation"], OP_RUN);
    assert_eq!(value["data"]["answers"]["relevant"]["choice"], "yes");
    let warnings = value["data"]["warnings"].as_array().expect("warnings");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    let warning = warnings[0].as_str().expect("warning text");
    assert!(
        warning.starts_with("session log was not flushed:"),
        "{warning}"
    );
    assert!(!warning.contains("check.toml"), "{warning}");
}

#[cfg(unix)]
#[expect(
    clippy::disallowed_methods,
    reason = "process spawn: mkfifo creates the unreadable log file; std has no safe mkfifo"
)]
fn make_fifo(path: &Path) {
    let mut command = Command::new("mkfifo");
    let output = spawn_with_deadline(command.arg(path), None);
    assert!(
        output.status.success(),
        "mkfifo {}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// ADR-SAG-0019: `--llm-system-prompt-file` (clap) reaches
/// `ComposeInputs::system_prompt_override` and wins over `check.toml` and the
/// packaged default. The saved session records the flag's prompt text.
#[test]
fn test_cli_system_prompt_file_overrides_check_toml() {
    let repo = temp_dir("prompt");
    write_repo(repo.path());
    std::fs::write(repo.path().join("evaluator.md"), "EVALUATOR-PROMPT-a7\n").expect("evaluator");
    std::fs::write(
        repo.path().join("check.toml"),
        "[questions]\npath = \"questions.json\"\n\n[llm_judge]\nsystem_prompt_file = \"evaluator.md\"\n",
    )
    .expect("check with a prompt file");
    let override_path = repo.path().join("override.md");
    std::fs::write(&override_path, "OVERRIDE-PROMPT-TOKEN-a7\n").expect("override");
    let answers = scripted(repo.path(), &shared("yes"));
    let run = |extra: &[&str]| {
        let sessions = temp_dir("prompt-sessions");
        let mut args = vec![
            "jev",
            "run",
            "--repo-root",
            text(repo.path()),
            "--sessions-dir",
            text(sessions.path()),
            "--backend",
            "anthropic_haiku",
            "--scripted-answers",
            text(&answers),
        ];
        args.extend_from_slice(extra);
        let value = stdout_json(&run_cli(&args));
        let id = value["data"]["session_id"].as_str().expect("id").to_owned();
        let prompt = session_doc(sessions.path(), &id)["system_prompt"]
            .as_str()
            .expect("saved prompt")
            .to_owned();
        (value, prompt)
    };
    let (from_check, check_prompt) = run(&[]);
    assert!(
        check_prompt.contains("EVALUATOR-PROMPT-a7"),
        "{check_prompt}"
    );
    let (from_flag, flag_prompt) = run(&["--llm-system-prompt-file", text(&override_path)]);
    assert!(
        flag_prompt.contains("OVERRIDE-PROMPT-TOKEN-a7"),
        "{flag_prompt}"
    );
    assert!(
        !flag_prompt.contains("EVALUATOR-PROMPT-a7"),
        "{flag_prompt}"
    );
    assert!(!flag_prompt.contains(PACKAGED_SYSTEM_PROMPT.trim()));
    // The prompt changes the wire, never the payload digest.
    assert_eq!(
        from_check["data"]["prompt_sha256"],
        from_flag["data"]["prompt_sha256"]
    );
    assert_eq!(
        from_flag["data"]["readback"]["llm_system_prompt_file"],
        canon(&override_path)
    );
    assert_eq!(
        from_flag["data"]["readback"]["repo_root"],
        canon(repo.path())
    );
    assert_eq!(
        from_flag["data"]["readback"]["check_toml"],
        canon(&repo.path().join("check.toml"))
    );
    assert_no_prompt_leak(&from_check["data"].to_string());
    assert_no_prompt_leak(&from_flag["data"].to_string());
}

#[test]
fn cli_dry_run_readback_run_ask_and_expiry() {
    let repo = temp_dir("ask");
    write_repo(repo.path());
    let sessions = temp_dir("ask-sessions");
    let prompt = repo.path().join("prompt.md");
    std::fs::write(&prompt, "EVALUATOR-PROMPT-a7\n").expect("prompt");
    let answers = scripted(repo.path(), &shared("yes"));
    let dry = stdout_json(&run_cli(&[
        "jev",
        "dry-run",
        "--repo-root",
        text(repo.path()),
        "--check",
        "check.toml",
        "--sessions-dir",
        text(sessions.path()),
        "--backend",
        "anthropic_haiku",
        "--llm-system-prompt-file",
        text(&prompt),
    ]));
    assert_eq!(dry["version"], "1");
    assert_eq!(dry["operation"], OP_DRY_RUN);
    assert!(dry["data"]["session_id"].is_null());
    assert_eq!(dry["data"]["session_saved"], false);
    assert_eq!(
        dry["data"]["prompt_sha256"].as_str().expect("sha").len(),
        64
    );
    assert!(dry["data"]["payload"]["questions"].is_array());
    assert_eq!(dry["data"]["limits"][0]["backend"], "anthropic_haiku");
    assert_eq!(dry["data"]["limits"][0]["over_limit"], false);
    assert_eq!(dry["data"]["results"], json!([]));
    assert_eq!(
        dry["data"]["equal_input"]["backends"][0]["attestation"]["status"],
        "not_sent"
    );
    assert_eq!(dry["data"]["readback"]["repo_root"], canon(repo.path()));
    assert_eq!(
        dry["data"]["readback"]["check_toml"],
        canon(&repo.path().join("check.toml"))
    );
    assert_eq!(
        dry["data"]["readback"]["llm_system_prompt_file"],
        canon(&prompt)
    );
    assert!(dry["data"]["readback"]["request_json"].is_null());
    assert_no_prompt_leak(&dry.to_string());

    let ran = stdout_json(&run_cli(&[
        "jev",
        "run",
        "--repo-root",
        text(repo.path()),
        "--sessions-dir",
        text(sessions.path()),
        "--backend",
        "anthropic_haiku",
        "--llm-system-prompt-file",
        text(&prompt),
        "--scripted-answers",
        text(&answers),
    ]));
    assert_eq!(ran["operation"], OP_RUN);
    let session_id = ran["data"]["session_id"]
        .as_str()
        .expect("session")
        .to_owned();
    assert_eq!(ran["data"]["prompt_sha256"], dry["data"]["prompt_sha256"]);
    let saved = session_doc(sessions.path(), &session_id);
    assert!(
        saved["system_prompt"]
            .as_str()
            .expect("prompt")
            .contains("EVALUATOR-PROMPT-a7")
    );
    assert!(
        !saved["payload"].to_string().contains("EVALUATOR-PROMPT-a7"),
        "the prompt is wire context, not payload"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(sessions.path().join(format!("{session_id}.json")))
            .expect("session file")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    let ask = |text_arg: &str| {
        run_cli(&[
            "jev",
            "ask",
            "--repo-root",
            text(repo.path()),
            "--sessions-dir",
            text(sessions.path()),
            "--session-id",
            &session_id,
            "--text",
            text_arg,
            "--scripted-answers",
            text(&answers),
        ])
    };
    let asked = stdout_json(&ask("why the choice?"));
    assert_eq!(asked["version"], "1");
    assert_eq!(asked["operation"], OP_ASK);
    assert_eq!(asked["data"]["session_id"], session_id);
    assert!(asked["data"].get("backend").is_none());
    assert!(asked["data"].get("provider_meta").is_none());
    assert_eq!(asked["data"]["answers"]["relevant"]["choice"], "yes");
    assert_eq!(
        asked["data"]["readback"]["sessions_dir"],
        canon(sessions.path())
    );
    assert!(
        asked["data"].get("results").is_none(),
        "ask data is the outcome"
    );
    assert_eq!(asked["data"]["prompt_sha256"], ran["data"]["prompt_sha256"]);
    assert!(asked["data"].get("warnings").is_none(), "no flush failure");
    assert_no_prompt_leak(&asked.to_string());

    // Expiry: the CLI keeps only the error-shape check, against a session
    // file whose `created_at` is in the past (the clock-injected case is
    // covered by sc-jev-check's session tests).
    let path = sessions.path().join(format!("{session_id}.json"));
    let mut doc = session_doc(sessions.path(), &session_id);
    // 25 h old: past the 24 h TTL, inside the retention grace that keeps
    // the file (older documents are swept when a store opens).
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_millis();
    doc["created_at"] = json!(now_ms - 25 * 60 * 60 * 1000);
    std::fs::write(&path, doc.to_string()).expect("age the session");
    let expired = ask("still there?");
    let error = stderr_json(&expired);
    assert_eq!(error["error"], "SESSION_EXPIRED");
    assert!(
        error["message"]
            .as_str()
            .expect("message")
            .contains(&session_id)
    );
    assert_no_prompt_leak(&String::from_utf8_lossy(&expired.stderr));
}

#[test]
fn cli_logger_adapter_records_session_without_secrets() {
    let repo = temp_dir("log");
    write_repo(repo.path());
    let sessions = temp_dir("log-sessions");
    let logs = temp_dir("log-root");
    let prompt = repo.path().join("prompt.md");
    std::fs::write(&prompt, "EVALUATOR-PROMPT-a7 sk-ant-test-secret\n").expect("prompt");
    let answers = scripted(repo.path(), &shared("yes"));
    let value = stdout_json(&run_cli(&[
        "jev",
        "run",
        "--repo-root",
        text(repo.path()),
        "--sessions-dir",
        text(sessions.path()),
        "--log-dir",
        text(logs.path()),
        "--backend",
        "anthropic_haiku",
        "--llm-system-prompt-file",
        text(&prompt),
        "--scripted-answers",
        text(&answers),
    ]));
    let digest = value["data"]["prompt_sha256"].as_str().expect("sha");
    let session_id = value["data"]["session_id"].as_str().expect("session");
    let log_path = logs.path().join("logs").join("sc-ai.log.jsonl");
    let log = std::fs::read_to_string(&log_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", log_path.display()));
    assert!(log.contains(session_id), "{log}");
    assert!(log.contains(digest), "{log}");
    assert!(log.contains("judge"), "{log}");
    assert!(log.contains("anthropic_haiku"), "{log}");
    assert_no_prompt_leak(&log);
    assert_no_prompt_leak(&value.to_string());
    // The secret-looking prompt text reached the saved wire context only.
    let saved = session_doc(sessions.path(), session_id);
    assert!(
        saved["system_prompt"]
            .as_str()
            .expect("prompt")
            .contains("sk-ant-test-secret")
    );
}

/// A UUID session id is one `INPUT_INVALID` line (exit 2) whose fix hint says
/// session ids are ULIDs (REQ-SAG-0612, REQ-SAG-0701).
#[test]
fn test_cli_uuid_session_id_is_input_invalid() {
    let repo = temp_dir("uuid-id");
    write_repo(repo.path());
    let output = run_cli(&[
        "jev",
        "ask",
        "--repo-root",
        text(repo.path()),
        "--session-id",
        "3f2b8a9e-6c1d-4e7a-9b25-0d4c1a7e5f10",
        "--text",
        "why?",
    ]);
    let error = stderr_json(&output);
    assert_eq!(error["error"], "INPUT_INVALID");
    let message = error["message"].as_str().expect("message");
    assert!(message.contains("ULID"), "{message}");
    assert!(message.contains("sc-ai jev run"), "{message}");
}

/// `--project` reaches the log row of a run and, per call, of an `ask`
/// (REQ-SAG-0615). The log row also carries the ULID session id and a model.
#[test]
fn test_cli_project_flag_reaches_log() {
    let repo = temp_dir("project");
    write_repo(repo.path());
    let sessions = temp_dir("project-sessions");
    let logs = temp_dir("project-logs");
    let answers = scripted(repo.path(), &shared("yes"));
    let common = |extra: &[&str]| {
        let mut args = vec!["jev"];
        args.extend_from_slice(extra);
        args.extend_from_slice(&[
            "--repo-root",
            text(repo.path()),
            "--sessions-dir",
            text(sessions.path()),
            "--log-dir",
            text(logs.path()),
            "--scripted-answers",
            text(&answers),
        ]);
        run_cli(&args)
    };
    let run = stdout_json(&common(&[
        "run",
        "--backend",
        "anthropic_haiku",
        "--project",
        "billing-service",
    ]));
    let session_id = run["data"]["session_id"]
        .as_str()
        .expect("session")
        .to_owned();
    assert_eq!(session_id.len(), 26, "{session_id}");
    stdout_json(&common(&[
        "ask",
        "--session-id",
        &session_id,
        "--text",
        "why?",
        "--project",
        "-follow-up",
    ]));
    stdout_json(&common(&[
        "ask",
        "--session-id",
        &session_id,
        "--text",
        "again?",
    ]));

    let log =
        std::fs::read_to_string(logs.path().join("logs").join("sc-ai.log.jsonl")).expect("log");
    let rows: Vec<Value> = log
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str::<Value>(line).expect("line")["fields"].clone())
        .collect();
    assert_eq!(rows.len(), 3, "{log}");
    assert_eq!(rows[0]["project"], "billing-service");
    assert_eq!(rows[1]["project"], "-follow-up");
    assert!(rows[2]["project"].is_null(), "project is per call");
    for row in &rows {
        assert_eq!(row["session_id"], session_id.as_str());
        assert_eq!(row["send_outcome"], "ok");
        assert!(row["model_id"].is_string());
        assert!(row["request_sha256"].is_string());
    }
    assert_eq!(rows[0]["operation"], "judge");
    assert_eq!(rows[1]["operation"], "ask");
}

/// A label over 256 bytes, or with a control character, is `INPUT_INVALID`
/// before anything runs.
#[test]
fn test_cli_project_over_limit_is_input_invalid() {
    let repo = temp_dir("project-bad");
    write_repo(repo.path());
    let long = "p".repeat(257);
    for bad in [long.as_str(), "line\nbreak"] {
        let output = run_cli(&[
            "jev",
            "dry-run",
            "--repo-root",
            text(repo.path()),
            "--project",
            bad,
        ]);
        assert_eq!(stderr_json(&output)["error"], "INPUT_INVALID", "{bad:?}");
    }
    let exact = "p".repeat(256);
    let output = run_cli(&[
        "jev",
        "dry-run",
        "--repo-root",
        text(repo.path()),
        "--project",
        &exact,
    ]);
    stdout_json(&output);
}

#[test]
fn cli_invalid_request_json_is_input_invalid() {
    let output = spawn_with_deadline(
        &mut cli(&["jev", "run", "--request-json", "-"]),
        Some(br#"{"invalid":true}"#),
    );
    let error = stderr_json(&output);
    assert_eq!(error["error"], "INPUT_INVALID");
    assert!(
        !error["message"]
            .as_str()
            .expect("message")
            .contains("invalid\":true")
    );
}

/// `--request-json <file>` with a relative path resolves against
/// `--repo-root`; the readback shows the absolute path.
#[test]
fn cli_request_json_relative_file_succeeds_with_absolute_readback() {
    let repo = temp_dir("request-file");
    let sessions = temp_dir("request-file-sessions");
    std::fs::write(
        repo.path().join("req.json"),
        r#"{"questions":[{"id":"q","prompt":"p","choices":["a","b"]}]}"#,
    )
    .expect("request file");
    let answers = scripted(repo.path(), &json!({"q": "a"}));
    let ran = stdout_json(&run_cli(&[
        "jev",
        "run",
        "--repo-root",
        text(repo.path()),
        "--sessions-dir",
        text(sessions.path()),
        "--request-json",
        "req.json",
        "--scripted-answers",
        text(&answers),
    ]));
    assert_eq!(ran["operation"], OP_RUN);
    assert_eq!(
        ran["data"]["readback"]["request_json"],
        canon(&repo.path().join("req.json"))
    );
    assert!(
        Path::new(
            ran["data"]["readback"]["request_json"]
                .as_str()
                .expect("path")
        )
        .is_absolute()
    );
}

/// A missing `--request-json` file is `INPUT_MISSING`.
#[test]
fn cli_request_json_missing_file_is_input_missing() {
    let repo = temp_dir("request-missing");
    let output = run_cli(&[
        "jev",
        "run",
        "--repo-root",
        text(repo.path()),
        "--request-json",
        "absent.json",
    ]);
    let error = stderr_json(&output);
    assert_eq!(error["error"], "INPUT_MISSING");
    assert!(
        error["message"]
            .as_str()
            .expect("message")
            .contains("absent.json")
    );
}

/// An unreadable `--request-json` path (a directory) is `IO_FAILED`, with
/// the operation and the `io::ErrorKind` as machine fields, and is told apart
/// from `INPUT_MISSING` by its id, not by its message.
#[test]
fn cli_request_json_directory_is_io_failed() {
    let repo = temp_dir("request-dir");
    std::fs::create_dir(repo.path().join("req.json")).expect("directory");
    let output = run_cli(&[
        "jev",
        "run",
        "--repo-root",
        text(repo.path()),
        "--request-json",
        "req.json",
    ]);
    let error = stderr_json(&output);
    assert_eq!(error["error"], "IO_FAILED");
    assert_eq!(error["kind"], "dependency");
    assert_eq!(error["retryable"], false);
    assert!(
        error["details"]["what"]
            .as_str()
            .expect("what")
            .contains("request JSON"),
        "{error}"
    );
    assert!(error["details"]["io_kind"].is_string(), "{error}");
}

/// One rule for the look-alike ids: a usage error is the caller's input
/// (`INPUT_INVALID`, naming the argument), never `CONFIG_INVALID`; a named
/// path that does not exist is `INPUT_MISSING` whichever path reads it.
#[test]
fn cli_usage_errors_are_input_invalid_and_name_the_argument() {
    let output = run_cli(&["jev", "run", "--no-such-flag"]);
    let error = stderr_json(&output);
    assert_eq!(error["error"], "INPUT_INVALID");
    assert_eq!(error["kind"], "validation");
    assert_eq!(error["field"], "--no-such-flag");
    let output = run_cli(&[
        "jev",
        "ask",
        "--session-id",
        "not-a-ulid",
        "--text",
        "hello",
    ]);
    let error = stderr_json(&output);
    assert_eq!(error["error"], "INPUT_INVALID");
    assert_eq!(error["field"], "--session-id");
    let long = "x".repeat(100_000);
    let output = run_cli(&["jev", "ask", "--session-id", &long, "--text", "hello"]);
    let error = stderr_json(&output);
    assert!(
        error["message"].as_str().expect("message").len() < 400,
        "{error}"
    );
    let output = run_cli(&["jev", "run", "--backend", "nope"]);
    let error = stderr_json(&output);
    assert_eq!(error["error"], "INPUT_INVALID");
    assert_eq!(error["field"], "--backend");
}

#[test]
fn cli_request_json_rejects_inline_system_prompt() {
    let output = spawn_with_deadline(
        &mut cli(&["jev", "run", "--request-json", "-"]),
        Some(
            br#"{"questions":[{"id":"q","prompt":"p","choices":["a","b"]}],"system_prompt":"INLINE-PROMPT-TOKEN"}"#,
        ),
    );
    let error = stderr_json(&output);
    assert_eq!(error["error"], "INPUT_INVALID");
    let hint = error["message"].as_str().expect("message");
    assert!(hint.contains("--llm-system-prompt-file"));
    assert!(hint.contains("packaged default"));
    assert!(!hint.contains("INLINE-PROMPT-TOKEN"));
}

/// A Jev-only run saves no session (REQ-SAG-0607) and prints the Jev answers
/// at the top level of `data`.
#[test]
fn cli_jev_only_run_saves_no_session() {
    let repo = temp_dir("verbatim");
    write_repo(repo.path());
    let sessions = temp_dir("verbatim-sessions");
    let answers = scripted(repo.path(), &shared("yes"));
    let value = stdout_json(&run_cli(&[
        "jev",
        "run",
        "--repo-root",
        text(repo.path()),
        "--sessions-dir",
        text(sessions.path()),
        "--backend",
        "jev",
        "--scripted-answers",
        text(&answers),
    ]));
    let data = &value["data"];
    assert_eq!(data["primary_backend"], "jev");
    assert!(data["session_id"].is_null());
    assert_eq!(data["session_saved"], false);
    assert!(
        std::fs::read_dir(sessions.path())
            .map(|entries| entries.count())
            .unwrap_or(0)
            == 0
    );
    assert_eq!(data["answers"]["relevant"]["choice"], "yes");
    assert!(data.get("provider_meta").is_none());
    assert!(data.get("backend").is_none());
}

#[test]
fn cli_log_and_git_errors_do_not_blame_check_toml() {
    let root = temp_dir("logblock");
    write_repo(root.path());
    let file = root.path().join("not-a-directory");
    std::fs::write(&file, b"x").expect("file");
    let answers = scripted(root.path(), &shared("yes"));
    let blocked = run_cli(&[
        "jev",
        "run",
        "--repo-root",
        text(root.path()),
        "--sessions-dir",
        text(&root.path().join("sessions")),
        "--log-dir",
        text(&file.join("nested")),
        "--backend",
        "anthropic_haiku",
        "--scripted-answers",
        text(&answers),
    ]);
    let error = stderr_json(&blocked);
    assert_eq!(error["error"], "IO_FAILED");
    let hint = error["suggested_action"]
        .as_str()
        .expect("suggested_action");
    assert!(!hint.contains("check.toml"), "{hint}");
    assert!(hint.contains("--log-dir"), "{hint}");

    let outside = temp_dir("nogit");
    let output = spawn_with_deadline(
        cli(&["jev", "dry-run"])
            .current_dir(outside.path())
            .env("GIT_DIR", outside.path().join("missing-git"))
            .env("GIT_CONFIG_GLOBAL", git_env::null_device())
            .env("GIT_TERMINAL_PROMPT", "0"),
        None,
    );
    // Outside git the repo root is the current directory (ADR-SAG-0003 r-2
    // amendment), so the failure names the missing check file, not git.
    let err = stderr_json(&output);
    assert_eq!(err["error"], "INPUT_MISSING");
    let message = err["message"].as_str().expect("message");
    assert!(message.contains("check.toml"), "{message}");
    assert!(!message.contains("schema"), "{message}");
    assert!(!message.to_ascii_lowercase().contains("git"), "{message}");
}

#[test]
fn cli_process_stderr_is_one_typed_line() {
    let output = spawn_with_deadline(
        &mut cli(&["jev", "run", "--request-json", "-"]),
        Some(br#"{"invalid":true}"#),
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    // REQ-SAG-0701 (r-1): every stderr line is one `ScAgentErrorResponse`
    // with `schema_version: 1`.
    let stderr = String::from_utf8(output.stderr.clone()).expect("utf8 stderr");
    for line in stderr.lines() {
        let parsed: sc_agent_types::ScAgentErrorResponse =
            serde_json::from_str(line).expect("stderr line is an error response");
        assert_eq!(parsed.schema_version(), 1);
    }
    let err = stderr_json(&output);
    let object = err.as_object().expect("object");
    // The always-present keys; the optional ones (`retry_after_seconds`,
    // `field`, `details`) are absent for this error.
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "error",
            "kind",
            "message",
            "retryable",
            "schema_version",
            "suggested_action"
        ]
    );
    assert_eq!(err["error"], "INPUT_INVALID");
    assert_eq!(err["kind"], "validation");
    assert_eq!(err["retryable"], false);
    assert_eq!(err["schema_version"], 1);
    assert!(err["message"].is_string());
    assert!(err["suggested_action"].is_string());
    let codes: Value = serde_json::from_str(
        &std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/jev/README-contract/errors.json"),
        )
        .expect("errors.json"),
    )
    .expect("codes");
    let ids = codes["error_ids"].as_array().expect("ids");
    assert!(ids.iter().any(|id| id == "INPUT_INVALID"));
}

#[test]
fn cli_run_resolves_dotenv_key_and_dry_run_does_not() {
    let repo = temp_dir("dotenv");
    write_repo(repo.path());
    std::fs::write(
        repo.path().join(".env"),
        "TYPESAFE_API_KEY=sk-test-whitespace key\n",
    )
    .expect("env");
    let sessions = temp_dir("dotenv-sessions");
    let logs = temp_dir("dotenv-logs");
    let args = |sub: &'static str| {
        [
            "jev",
            sub,
            "--repo-root",
            text(repo.path()),
            "--sessions-dir",
            text(sessions.path()),
            "--log-dir",
            text(logs.path()),
            "--backend",
            "jev",
        ]
        .map(str::to_owned)
    };
    let dry_args = args("dry-run");
    let dry = run_cli(&dry_args.each_ref().map(String::as_str));
    assert!(
        dry.status.success(),
        "dry-run stderr {}",
        String::from_utf8_lossy(&dry.stderr)
    );
    assert!(dry.stderr.is_empty());
    let dry_json: Value = serde_json::from_slice(&dry.stdout).expect("dry json");
    assert_eq!(dry_json["operation"], OP_DRY_RUN);
    assert!(dry_json["data"]["session_id"].is_null());
    assert_no_prompt_leak(&String::from_utf8_lossy(&dry.stdout));

    let run_args = args("run");
    let run = run_cli(&run_args.each_ref().map(String::as_str));
    assert_eq!(run.status.code(), Some(2), "run should refuse the bad key");
    assert!(run.stdout.is_empty());
    let err = stderr_json(&run);
    assert_eq!(err["error"], "CONFIG_INVALID");
    let rendered = format!("{} {}", err["error"], err["message"]);
    assert_no_prompt_leak(&rendered);
    assert!(
        !rendered.to_ascii_lowercase().contains("bearer"),
        "{rendered}"
    );
}

#[test]
fn cli_dry_run_writes_nothing_and_ask_uses_default_log_root() {
    let repo = temp_dir("logroot");
    write_repo(repo.path());
    let sessions_parent = temp_dir("logroot-sessions");
    let sessions = sessions_parent.path().join("sessions");
    let dry = run_cli(&[
        "jev",
        "dry-run",
        "--repo-root",
        text(repo.path()),
        "--sessions-dir",
        text(&sessions),
    ]);
    assert!(
        dry.status.success(),
        "{}",
        String::from_utf8_lossy(&dry.stderr)
    );
    assert!(
        !repo.path().join(".sc-ai").exists(),
        "dry-run made a log root"
    );
    assert!(!sessions.exists(), "dry-run made a sessions dir");

    let absent = "01JC4Z7A5B0000000000000009";
    let ask = |log_root: Option<&Path>| {
        let mut command = cli(&[
            "jev",
            "ask",
            "--repo-root",
            text(repo.path()),
            "--sessions-dir",
            text(&sessions),
            "--session-id",
            absent,
            "--text",
            "why?",
        ]);
        if let Some(root) = log_root {
            command.env("SC_LOG_ROOT", root);
        }
        spawn_with_deadline(&mut command, None)
    };
    let default_root = ask(None);
    assert_eq!(stderr_json(&default_root)["error"], "SESSION_NOT_FOUND");
    assert!(repo.path().join(".sc-ai").is_dir());

    // Precedence (ADR-SAG-0021): the environment variable moves the log root.
    let custom = temp_dir("logroot-env");
    let env_root = custom.path().join("root");
    let env_run = ask(Some(&env_root));
    assert_eq!(env_run.status.code(), Some(2));
    assert!(env_root.is_dir());
}

/// Fixture git through the shared isolation helper and deadline (FTQ-002).
fn git(cwd: &Path, args: &[&str]) {
    let output = spawn_with_deadline(git_env::git_command(cwd).args(args), None);
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cli_check_that_exists_only_at_rev_loads_with_rev() {
    let repo = temp_dir("rev-only");
    write_repo(repo.path());
    git(repo.path(), git_env::SHA1_INIT);
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-q", "-m", "init"]);
    std::fs::remove_file(repo.path().join("check.toml")).expect("remove worktree copy");
    let value = stdout_json(&run_cli(&[
        "jev",
        "dry-run",
        "--repo-root",
        text(repo.path()),
        "--check",
        "check.toml",
        "--rev",
        "HEAD",
    ]));
    assert_eq!(value["operation"], OP_DRY_RUN);
    assert!(value["data"]["payload"]["questions"].is_array());
}
