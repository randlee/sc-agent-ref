//! First real end-to-end run of the `sc-treesitter` producer through
//! `check.toml` (REQ-SAG-0310, REQ-SAG-0313, ARCHP-002).
//!
//! A `check.toml` with a `kind = "command"` input whose argv runs the built
//! `sc-treesitter` binary (release, isolated target dir, `support::bin_path`)
//! goes through `sc_jev_host::compose::run_from` with `dry_run`.

#[path = "../../../tests/support/deadline.rs"]
mod deadline;
#[path = "../../../tests/support/git_env.rs"]
mod git_env;
mod support;

use std::path::Path;

use deadline::spawn_with_deadline;
use sc_agent_types::schema::subjects::SubjectsDocument;
use sc_agent_types::{BackendId, BackendSet, JevError, RepoRoot};
use sc_decision_schema::{CanonicalJudgePayload, PayloadBundle};
use sc_jev_check::run::{CheckSource, RunResponse};
use sc_jev_host::compose::{ComposeInputs, run_from};
use sc_jev_host::paths::PathPolicy;
use support::bin_path::bin_path;
use tempfile::TempDir;

const QUESTIONS: &str = r#"{
  "schema": "plan-anchoring/questions/1",
  "version": 1,
  "questions": {
    "branches": {
      "type": "choice",
      "instructions": "Which branch applies?\n\n{branches}",
      "criteria": {"no": "No.", "yes": "Yes."}
    }
  }
}"#;

/// The committed `branches` fixture source, at the path its subjects name.
const SOURCE: &str = "tests/fixtures/cross-impl/branches/hook.py";

fn fixture_root() -> std::path::PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .join("tests/fixtures/cross-impl/branches")
}

fn git(cwd: &Path, args: &[&str]) {
    let output = spawn_with_deadline(git_env::git_command(cwd).args(args), None);
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A committed repo holding the fixture source and a questions document.
fn repo() -> TempDir {
    let dir = tempfile::Builder::new()
        .prefix("workspace-tests-e2e-")
        .tempdir()
        .expect("temp");
    let source = dir.path().join(SOURCE);
    std::fs::create_dir_all(source.parent().expect("parent")).expect("dirs");
    std::fs::copy(fixture_root().join("hook.py"), &source).expect("copy hook.py");
    std::fs::write(dir.path().join("questions.json"), QUESTIONS).expect("questions");
    git(dir.path(), git_env::SHA1_INIT);
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "fixture"]);
    dir
}

fn dry_run(repo: &Path, language: &str) -> Result<RunResponse, JevError> {
    let producer = bin_path("sc-treesitter", "sc-treesitter");
    let command = serde_json::to_string(&[
        producer.to_str().expect("utf8 path"),
        "extract",
        "--language",
        language,
        "--query",
        "branches",
        "--rev",
        "HEAD",
        "{path}",
    ])
    .expect("argv");
    std::fs::write(
        repo.join("check.toml"),
        format!(
            "[questions]\npath = \"questions.json\"\n\n[inputs.branches]\nkind = \"command\"\npath = \"{SOURCE}\"\ncommand = {command}\nproducer = \"sc-treesitter\"\n"
        ),
    )
    .expect("check.toml");
    let repo_root = RepoRoot::new(repo);
    let inputs = ComposeInputs::new(repo.to_path_buf(), PathPolicy::cli())
        .repo_root(repo_root.clone())
        .backends(BackendSet::try_from(vec![BackendId::Jev]).expect("set"))
        .dry_run(true);
    let source = CheckSource::check_file(&repo_root, Path::new("check.toml")).expect("source");
    run_from(source, inputs)
}

#[test]
fn test_e2e_producer_digest_matches_fixture() {
    let repo = repo();
    let response = dry_run(repo.path(), "python").expect("dry run through the real producer");

    // The payload encoded independently from the committed subjects.
    let bytes = std::fs::read(fixture_root().join("subjects.json")).expect("subjects.json");
    let document: SubjectsDocument = serde_json::from_slice(&bytes).expect("subjects document");
    let joined = document
        .subjects()
        .iter()
        .map(|subject| subject.text())
        .collect::<Vec<_>>()
        .join("\n");
    let expected: CanonicalJudgePayload = serde_json::from_value(serde_json::json!({
        "questions": [{
            "id": "branches",
            "prompt": format!("Which branch applies?\n\n{joined}"),
            "choices": ["no", "yes"]
        }],
        "state": []
    }))
    .expect("expected payload");
    let expected = PayloadBundle::encode(&expected).expect("expected bundle");

    assert_eq!(response.prompt_sha256(), expected.sha256());
    assert!(response.dry_run());
    assert!(response.results().is_empty());
    let producer = response.equal_input().producer().expect("producer");
    assert_eq!(producer.name(), "sc-treesitter");
    assert_eq!(producer.language(), document.producer().language());
    assert_eq!(producer.query_sha256(), document.producer().query_sha256());
    assert_eq!(producer.mode().as_str(), "matches");
}

#[test]
fn test_e2e_producer_error_reraised() {
    let repo = repo();
    let error = dry_run(repo.path(), "nonesuch").expect_err("an unknown language fails");
    assert!(matches!(error, JevError::ProducerError { .. }), "{error:?}");
    assert_eq!(error.code(), "LANGUAGE_UNAVAILABLE");
    assert!(
        error.fix_hint().starts_with("sc-treesitter:"),
        "{}",
        error.fix_hint()
    );
}
