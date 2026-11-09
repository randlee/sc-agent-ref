//! The global models file through the composition root (agt-akp): where it
//! lives, what it changes, and what is logged.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sc_agent_types::observability::v2::RecordingGeneralLog;
use sc_agent_types::{BackendSet, ModelSource, RepoRoot};
use sc_jev_check::run::CheckSource;
use sc_jev_host::compose::{ComposeInputs, prepare_from};
use sc_jev_host::paths::{PathPolicy, models_file_with};

fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp");
    std::fs::write(
        dir.path().join("questions.json"),
        r#"{"schema":"plan-anchoring/questions/1","version":1,"questions":{"relevant":{"type":"choice","instructions":"Applies?","criteria":{"no":"No.","yes":"Yes."}}}}"#,
    )
    .expect("questions");
    std::fs::write(
        dir.path().join("check.toml"),
        "[questions]\npath = \"questions.json\"\n",
    )
    .expect("check");
    dir
}

fn inputs(
    repo: &Path,
    file: &Path,
    backends: &str,
    recorder: &RecordingGeneralLog,
) -> ComposeInputs {
    ComposeInputs::new(repo.to_path_buf(), PathPolicy::cli())
        .repo_root(RepoRoot::new(repo))
        .backends(backends.parse::<BackendSet>().expect("set"))
        .models_file(file)
        .dry_run(true)
        .general_log(Arc::new(recorder.clone()))
}

fn source(repo: &Path) -> CheckSource {
    CheckSource::check_file(&RepoRoot::new(repo), Path::new("check.toml")).expect("source")
}

#[test]
fn the_file_lives_under_home_dot_sc_agent() {
    let env = |name: &str| (name == "HOME").then(|| OsString::from("/home/me"));
    assert_eq!(
        models_file_with(&env),
        Some(PathBuf::from("/home/me/.sc-agent/models.toml"))
    );
    let profile = |name: &str| (name == "USERPROFILE").then(|| OsString::from("/u"));
    assert_eq!(
        models_file_with(&profile),
        Some(PathBuf::from("/u/.sc-agent/models.toml"))
    );
    assert_eq!(models_file_with(&|_| None), None);
    assert_eq!(models_file_with(&|_| Some(OsString::new())), None);
}

#[test]
fn a_file_row_changes_the_model_and_is_logged_with_its_source() {
    let repo = repo();
    let file = repo.path().join("models.toml");
    std::fs::write(
        &file,
        "[[model]]\nid = \"claude-haiku-9\"\nalias = \"haiku\"\nreleased = \"2099-01-01\"\n",
    )
    .expect("file");
    let recorder = RecordingGeneralLog::new();
    let run = prepare_from(
        source(repo.path()),
        inputs(repo.path(), &file, "jev,haiku", &recorder),
    )
    .expect("prepare");
    let models = run.prepared().models();
    assert_eq!(
        models.latest("haiku").expect("m").as_str(),
        "claude-haiku-9"
    );
    assert_eq!(models.source("haiku"), Some(ModelSource::File));
    assert_eq!(models.source("jev"), Some(ModelSource::Crate));
    assert!(recorder.actions().contains(&"models.file"));
}

#[test]
fn a_pin_beats_the_file_and_a_malformed_file_fails_the_call() {
    let repo = repo();
    let file = repo.path().join("models.toml");
    std::fs::write(
        &file,
        "[[model]]\nid = \"claude-haiku-9\"\nalias = \"haiku\"\nreleased = \"2099-01-01\"\n",
    )
    .expect("file");
    let recorder = RecordingGeneralLog::new();
    let run = prepare_from(
        source(repo.path()),
        inputs(repo.path(), &file, "claude-haiku-5-5", &recorder),
    )
    .expect("prepare");
    assert_eq!(
        run.prepared().models().latest("haiku").expect("m").as_str(),
        "claude-haiku-5-5"
    );

    std::fs::write(
        &file,
        "[[model]]\nid = \"x\"\nalias = \"nope\"\nreleased = \"2099-01-01\"\n",
    )
    .expect("bad");
    let recorder = RecordingGeneralLog::new();
    let error = prepare_from(
        source(repo.path()),
        inputs(repo.path(), &file, "haiku", &recorder),
    )
    .expect_err("malformed");
    assert_eq!(error.code(), "CONFIG_INVALID");
    assert!(recorder.actions().contains(&"models.file_invalid"));
}
