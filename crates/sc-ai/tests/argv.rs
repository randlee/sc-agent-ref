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
fn test_help_banner_names_every_jev_command() {
    let dir = TempDir::new().expect("temp");
    let text = stdout_text(&run(dir.path(), &["--help"]));
    assert!(
        text.contains("Jev check client: run, dry-run, ask, stream, and models."),
        "{text}"
    );
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

/// `--help` pins the short names, model-id pinning, the models file and the
/// `jev models` command (agt-akp).
#[test]
fn test_help_pins_short_names_and_models() {
    let dir = TempDir::new().expect("temp");
    let flags = stdout_text(&run(dir.path(), &["jev", "run", "--help"]));
    for needle in ["--backend", "claude-haiku-5-5", "--models-file"] {
        assert!(flags.contains(needle), "{needle:?} missing from {flags}");
    }
    let text = stdout_text(&run(dir.path(), &["--help"]));
    for needle in [
        "--backend is jev, haiku, or luna (each means the latest model of that line)",
        "or a model id of one of them from the crate table or the models file, such as\nclaude-haiku-5-5 or jev-1.15.0 (that backend pinned to that model)",
        "sonnet, opus, fable,",
        "terra, sol, astra) are defined but not selectable yet",
        "jev models lists the resolved models",
        "~/.sc-agent/models.toml",
        "--models-file",
        "check.toml\n[models] still wins",
        "CONFIG_INVALID",
    ] {
        assert!(text.contains(needle), "{needle:?} missing from {text}");
    }
    for old in ["anthropic_haiku", "openai_luna"] {
        assert!(!text.contains(old), "{old}");
    }
    let models = stdout_text(&run(dir.path(), &["jev", "models", "--help"]));
    assert!(models.contains("--models-file"), "{models}");
}

#[test]
fn test_backend_takes_a_model_id_and_refuses_definition_only_models() {
    let repo = repo();
    let root = repo.path().to_str().expect("utf8");
    let pinned = run(
        repo.path(),
        &[
            "jev",
            "dry-run",
            "--repo-root",
            root,
            "--backend",
            "claude-haiku-5-5",
        ],
    );
    let envelope: Value = serde_json::from_str(&stdout_text(&pinned)).expect("envelope");
    assert_eq!(envelope["data"]["limits"][0]["backend"], "haiku");
    for refused in [
        "sonnet",
        "claude-sonnet-5-5",
        "gpt-6.1-sol",
        "openai_luna",
        "anthropic_haiku",
    ] {
        let output = run(
            repo.path(),
            &["jev", "dry-run", "--repo-root", root, "--backend", refused],
        );
        assert_eq!(error_code(&output), "INPUT_INVALID", "{refused}");
        let text = message(&output);
        for valid in ["jev", "haiku", "luna", "claude-haiku-5-5"] {
            assert!(text.contains(valid), "{refused}: {text}");
        }
    }
}

/// REQ-SAG-0149: a model id from `--models-file` is a `--backend` (a pin of
/// that backend); an id the catalog lacks, or one of a definition-only alias,
/// is still `INPUT_INVALID`.
#[test]
fn test_backend_takes_a_model_id_from_the_models_file() {
    let repo = repo();
    let root = repo.path().to_str().expect("utf8");
    let dir = TempDir::new().expect("temp");
    let file = dir.path().join("models.toml");
    std::fs::write(
        &file,
        "[[model]]\nid = \"jev-1.15.0\"\nalias = \"jev\"\nreleased = \"2026-11-01\"\n\
         [[model]]\nid = \"claude-haiku-5-6\"\nalias = \"haiku\"\nreleased = \"2026-11-02\"\n\
         [[model]]\nid = \"claude-sonnet-6\"\nalias = \"sonnet\"\nreleased = \"2026-11-02\"\n",
    )
    .expect("file");
    let models = file.to_str().expect("utf8");
    let accepted = run(
        repo.path(),
        &[
            "jev",
            "dry-run",
            "--repo-root",
            root,
            "--models-file",
            models,
            "--backend",
            "jev-1.15.0",
            "--backend",
            "claude-haiku-5-6",
        ],
    );
    let envelope: Value = serde_json::from_str(&stdout_text(&accepted)).expect("envelope");
    assert_eq!(envelope["data"]["limits"][0]["backend"], "jev");
    assert_eq!(envelope["data"]["limits"][1]["backend"], "haiku");
    for refused in ["jev-9.9.9", "claude-sonnet-6", "claude-haiku-9"] {
        let output = run(
            repo.path(),
            &[
                "jev",
                "dry-run",
                "--repo-root",
                root,
                "--models-file",
                models,
                "--backend",
                refused,
            ],
        );
        assert_eq!(error_code(&output), "INPUT_INVALID", "{refused}");
        let text = message(&output);
        // The list names the file's ids too.
        assert!(text.contains("jev-1.15.0"), "{refused}: {text}");
        assert_eq!(error_of(&output)["field"], "--backend");
    }
    // Without the file the same id is refused.
    let output = run(
        repo.path(),
        &[
            "jev",
            "dry-run",
            "--repo-root",
            root,
            "--models-file",
            dir.path().join("none.toml").to_str().expect("utf8"),
            "--backend",
            "jev-1.15.0",
        ],
    );
    assert_eq!(error_code(&output), "INPUT_INVALID");
}

/// Every file under `dir`, joined: the log rows of a run.
fn all_text(dir: &Path) -> String {
    let mut text = String::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return text;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            text.push_str(&all_text(&path));
        } else if let Ok(content) = std::fs::read_to_string(&path) {
            text.push_str(&content);
        }
    }
    text
}

fn files_under(dir: &Path) -> usize {
    std::fs::read_dir(dir).map_or(0, |entries| {
        entries
            .flatten()
            .map(|entry| {
                let path = entry.path();
                if path.is_dir() { files_under(&path) } else { 1 }
            })
            .sum()
    })
}

/// A `--backend` value no catalog could make valid is refused before any I/O,
/// as it was at parse time: not `INPUT_MISSING` for a missing `--repo-root`,
/// not `CONFIG_INVALID` for a malformed models file, and no log row.
#[test]
fn test_unusable_backend_is_refused_before_any_io() {
    let dir = TempDir::new().expect("temp");
    let missing = dir.path().join("no-such-repo");
    let logs = dir.path().join("logs");
    let bad_file = dir.path().join("models.toml");
    std::fs::write(&bad_file, "[[model]]\nid = 1\n").expect("bad file");
    let missing = missing.to_str().expect("utf8");
    let logs_arg = logs.to_str().expect("utf8");
    let bad_file_arg = bad_file.to_str().expect("utf8");
    for command in [["jev", "run"], ["jev", "dry-run"], ["jev", "stream"]] {
        // `claude-sonnet-5-5` is a crate definition-only model: no file can fix it.
        for refused in [
            "nonsense",
            "anthropic-haiku",
            "jev,haiku",
            "claude-sonnet-5-5",
        ] {
            for extra in [
                vec!["--repo-root", missing],
                vec!["--models-file", bad_file_arg],
            ] {
                let mut args = command.to_vec();
                args.extend(["--backend", refused, "--log-dir", logs_arg]);
                args.extend(extra.iter().copied());
                let output = run(dir.path(), &args);
                let value = error_of(&output);
                assert_eq!(value["error"], "INPUT_INVALID", "{args:?}");
                assert_eq!(value["field"], "--backend", "{args:?}");
            }
        }
    }
    assert_eq!(files_under(&logs), 0, "no log row, no log directory");
}

/// A well-formed id the crate table lacks is checked against the catalog once
/// the models file is loaded, through the run's general log: a malformed file
/// leaves its `models.file_invalid` warning, with `--backend` given or not.
#[test]
fn test_well_formed_backend_waits_for_the_models_file_and_its_log_row() {
    let repo = repo();
    let root = repo.path().to_str().expect("utf8");
    let dir = TempDir::new().expect("temp");
    let bad_file = dir.path().join("models.toml");
    std::fs::write(&bad_file, "[[model]]\nid = 1\n").expect("bad file");
    let bad = bad_file.to_str().expect("utf8");
    for backend in [Some("jev-1.15.0"), Some("haiku"), None] {
        let logs = dir
            .path()
            .join(format!("logs-{}", backend.unwrap_or("none")));
        let logs_arg = logs.to_str().expect("utf8");
        let mut args = vec![
            "jev",
            "run",
            "--repo-root",
            root,
            "--models-file",
            bad,
            "--log-dir",
            logs_arg,
        ];
        if let Some(backend) = backend {
            args.extend(["--backend", backend]);
        }
        let output = run(repo.path(), &args);
        assert_eq!(error_code(&output), "CONFIG_INVALID", "{backend:?}");
        assert!(
            all_text(&logs).contains("models.file_invalid"),
            "{backend:?}: no models.file_invalid row"
        );
    }
    // The same id with a good file that lacks it: INPUT_INVALID, `--backend`.
    let good = dir.path().join("good.toml");
    std::fs::write(&good, "").expect("good");
    let output = run(
        repo.path(),
        &[
            "jev",
            "dry-run",
            "--repo-root",
            root,
            "--models-file",
            good.to_str().expect("utf8"),
            "--backend",
            "jev-1.15.0",
        ],
    );
    assert_eq!(error_of(&output)["error"], "INPUT_INVALID");
    assert_eq!(error_of(&output)["field"], "--backend");
}

#[test]
fn test_models_lists_resolved_models_with_their_source() {
    let dir = TempDir::new().expect("temp");
    let file = dir.path().join("models.toml");
    std::fs::write(
        &file,
        "[[model]]\nid = \"claude-haiku-9\"\nalias = \"haiku\"\nreleased = \"2099-01-01\"\n",
    )
    .expect("file");
    let output = run(
        dir.path(),
        &[
            "jev",
            "models",
            "--models-file",
            file.to_str().expect("utf8"),
        ],
    );
    let envelope: Value = serde_json::from_str(&stdout_text(&output)).expect("envelope");
    assert_eq!(envelope["operation"], "models");
    assert_eq!(envelope["data"]["models_file_present"], true);
    let rows = envelope["data"]["models"].as_array().expect("rows");
    let row = |alias: &str| rows.iter().find(|row| row["alias"] == alias).expect(alias);
    assert_eq!(row("haiku")["model"], "claude-haiku-9");
    assert_eq!(row("haiku")["source"], "file");
    assert_eq!(row("haiku")["backend"], "haiku");
    assert_eq!(row("jev")["model"], "jev-1.14.0");
    assert_eq!(row("jev")["source"], "crate");
    assert_eq!(row("sonnet")["model"], "claude-sonnet-5-5");
    assert_eq!(row("sonnet")["backend"], Value::Null);
    assert_eq!(rows.len(), 9);

    // Missing file: the crate table. Malformed file: CONFIG_INVALID.
    let missing = dir.path().join("none.toml");
    let output = run(
        dir.path(),
        &[
            "jev",
            "models",
            "--models-file",
            missing.to_str().expect("utf8"),
        ],
    );
    let envelope: Value = serde_json::from_str(&stdout_text(&output)).expect("envelope");
    assert_eq!(envelope["data"]["models_file_present"], false);
    assert_eq!(envelope["data"]["models"][0]["source"], "crate");
    std::fs::write(&file, "[[model]]\nid = \"x\"\n").expect("bad");
    let output = run(
        dir.path(),
        &[
            "jev",
            "models",
            "--models-file",
            file.to_str().expect("utf8"),
        ],
    );
    assert_eq!(error_code(&output), "CONFIG_INVALID");
}

/// The Quick start and the reference page show the tested files verbatim, and
/// those files work: `check.toml` and `--request-json` both dry-run.
#[test]
fn test_quickstart_files_work_and_docs_show_them() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/jev/quickstart");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |path: &Path| std::fs::read_to_string(path).expect("read");
    let check = read(&fixture.join("check.toml"));
    let questions = read(&fixture.join("questions.json"));
    let request = read(&fixture.join("request.json"));
    for doc in [
        root.join("README.md"),
        root.join("crates/sc-ai/README.md"),
        root.join("docs/check-reference.md"),
    ] {
        let text = read(&doc);
        assert!(text.contains(check.trim_end()), "{doc:?} lacks check.toml");
    }
    let reference = read(&root.join("docs/check-reference.md"));
    assert!(reference.contains(questions.trim_end()), "questions.json");
    assert!(reference.contains(request.trim_end()), "request.json");
    for doc in [root.join("README.md"), root.join("crates/sc-ai/README.md")] {
        let text = read(&doc);
        assert!(
            text.contains("\"schema\": \"plan-anchoring/questions/1\""),
            "{doc:?}"
        );
        assert!(text.contains("TYPESAFE_API_KEY"), "{doc:?}");
    }

    let dir = TempDir::new().expect("temp");
    std::fs::create_dir(dir.path().join("notes")).expect("notes");
    for name in [
        "check.toml",
        "questions.json",
        "request.json",
        "notes/readme.txt",
    ] {
        std::fs::copy(fixture.join(name), dir.path().join(name)).expect("copy");
    }
    for args in [
        &[
            "jev",
            "dry-run",
            "--repo-root",
            ".",
            "--check",
            "check.toml",
        ][..],
        &[
            "jev",
            "dry-run",
            "--repo-root",
            ".",
            "--request-json",
            "request.json",
        ],
    ] {
        let value: Value =
            serde_json::from_str(&stdout_text(&run(dir.path(), args))).expect("envelope");
        assert_eq!(value["operation"], "jev.dry-run", "{args:?}");
    }
}
