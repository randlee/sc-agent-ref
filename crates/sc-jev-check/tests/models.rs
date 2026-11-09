//! `check.toml` `[models]` and `RunConfig::models` (REQ-SAG-0148, REQ-SAG-0301).
//!
//! Needs `--features test-util` (`[[test]] required-features`, D0 outcome).

use std::path::Path;
use std::sync::Arc;

use sc_agent_types::observability::v2::NoopSessionLog;
use sc_agent_types::{BackendId, BackendSet, ModelAliases, ModelId, RepoRoot, ResolvedApiKey};
use sc_jev_check::registry::BackendRegistry;
use sc_jev_check::run::{CheckSource, PreparedRun, RunConfig, prepare};

fn repo(models: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp");
    let questions = serde_json::json!({
        "schema": "plan-anchoring/questions/1",
        "version": 1,
        "questions": {
            "relevant": {
                "type": "choice",
                "instructions": "Does it apply?",
                "criteria": {"no": "No.", "yes": "Yes."}
            }
        }
    });
    std::fs::write(dir.path().join("questions.json"), questions.to_string()).expect("questions");
    std::fs::write(
        dir.path().join("check.toml"),
        format!("backends = [\"jev\", \"haiku\", \"luna\"]\n\n[questions]\npath = \"questions.json\"\n{models}"),
    )
    .expect("check.toml");
    dir
}

fn prepared(dir: &tempfile::TempDir, config: impl FnOnce(RunConfig) -> RunConfig) -> PreparedRun {
    let root = RepoRoot::new(dir.path());
    let base = RunConfig::new(root.clone(), Arc::new(NoopSessionLog)).dry_run(true);
    let source = CheckSource::check_file(&root, Path::new("check.toml")).expect("check path");
    prepare(source, config(base)).expect("prepare")
}

fn model_of(prepared: &PreparedRun, id: BackendId) -> ModelId {
    let backends = BackendSet::try_from(vec![id]).expect("set");
    let registry =
        BackendRegistry::from_resolver_with_models(&backends, prepared.models(), |key_id| {
            ResolvedApiKey::try_new(key_id, "placeholder-key-never-sent".to_owned())
        })
        .expect("production backends");
    registry.get(id).expect("registered").model_id()
}

/// A `[models]` entry reaches the production backend the factory builds, and
/// `RunConfig::models` wins over it.
#[test]
fn test_check_toml_models_override_reaches_backend() {
    let dir = repo("\n[models]\nhaiku = \"claude-haiku-9-preview\"\nluna = \"gpt-6-luna\"\n");
    let from_toml = prepared(&dir, |config| config);
    assert_eq!(
        model_of(&from_toml, BackendId::Haiku).as_str(),
        "claude-haiku-9-preview"
    );
    assert_eq!(model_of(&from_toml, BackendId::Luna), ModelId::Gpt6Luna);
    // Unlisted aliases keep the catalog default; `jev` is the latest.
    assert_eq!(model_of(&from_toml, BackendId::Jev), ModelId::Jev);

    // No `[models]` table: the catalog defaults for every backend.
    let plain = prepared(&repo(""), |config| config);
    assert_eq!(plain.models(), &ModelAliases::builtin());
    for row in sc_agent_types::BACKENDS {
        assert_eq!(model_of(&plain, row.id()), row.default_model());
    }

    // `RunConfig::models` replaces the check's table.
    let pinned = ModelAliases::builtin()
        .with_override("luna", "gpt-7-luna".parse().expect("model"))
        .expect("alias");
    let config_wins = prepared(&dir, |config| config.models(pinned));
    assert_eq!(
        model_of(&config_wins, BackendId::Luna).as_str(),
        "gpt-7-luna"
    );
    assert_eq!(
        model_of(&config_wins, BackendId::Haiku),
        ModelId::ClaudeHaiku5_5
    );
}

#[test]
fn test_unknown_model_alias_is_config_invalid() {
    for (models, needle) in [
        ("\n[models]\nnope = \"claude-sonnet-9\"\n", "nope"),
        ("\n[models]\nhaiku = \"\"\n", "[models].haiku"),
        ("\n[models]\nluna = \"a\\nb\"\n", "[models].luna"),
        ("\n[models]\nluna = 7\n", "luna"),
    ] {
        let dir = repo(models);
        let root = RepoRoot::new(dir.path());
        let config = RunConfig::new(root.clone(), Arc::new(NoopSessionLog)).dry_run(true);
        let source = CheckSource::check_file(&root, Path::new("check.toml")).expect("check path");
        // `prepare` constructs no backend and sends nothing: the error is
        // raised while the check is loaded.
        let error = prepare(source, config).expect_err(models);
        assert_eq!(error.code(), "CONFIG_INVALID", "{models}");
        let rendered = format!("{error} {}", error.message());
        assert!(rendered.contains(needle), "{needle}: {rendered}");
        if needle == "nope" {
            for alias in ["jev", "haiku", "luna"] {
                assert!(rendered.contains(alias), "{rendered}");
            }
        }
    }
}

/// The global file layer sits under `check.toml` `[models]`, and a
/// `--backend` model pin sits over both.
#[test]
fn test_global_models_check_toml_and_pin_layers() {
    let global = sc_jev_check::models_file::models_from_str(
        "[[model]]\nid = \"claude-haiku-9\"\nalias = \"haiku\"\nreleased = \"2099-01-01\"\n\
         [[model]]\nid = \"gpt-9-luna\"\nalias = \"luna\"\nreleased = \"2099-01-01\"\n",
    )
    .expect("file");
    let dir = repo("\n[models]\nluna = \"gpt-7-luna\"\n");

    // File row for haiku; check.toml [models] beats the file row for luna.
    let layered = prepared(&dir, |config| config.global_models(global.clone()));
    assert_eq!(
        model_of(&layered, BackendId::Haiku).as_str(),
        "claude-haiku-9"
    );
    assert_eq!(model_of(&layered, BackendId::Luna).as_str(), "gpt-7-luna");
    assert_eq!(
        layered.models().source("haiku"),
        Some(sc_agent_types::ModelSource::File)
    );
    assert_eq!(
        layered.models().source("luna"),
        Some(sc_agent_types::ModelSource::Override)
    );

    // A pin (`--backend claude-haiku-5-5`) beats the file row.
    let pinned: BackendSet = "jev,claude-haiku-5-5".parse().expect("set");
    let with_pin = prepared(&dir, |config| {
        config.global_models(global.clone()).backends(pinned)
    });
    assert_eq!(
        model_of(&with_pin, BackendId::Haiku),
        ModelId::ClaudeHaiku5_5
    );
    assert_eq!(
        with_pin.models().source("haiku"),
        Some(sc_agent_types::ModelSource::Override)
    );
}

/// `jev` means the latest (`jev-latest` on the wire); `jev-1.14.0` is a pin.
/// They are different requests, so their digests differ (agt-akp).
#[test]
fn test_jev_is_latest_and_a_version_is_a_pin() {
    let dir = repo("");
    let digest = |backends: &str| {
        let set: BackendSet = backends.parse().expect("set");
        let run = prepared(&dir, |config| config.backends(set));
        let model = model_of(&run, BackendId::Jev);
        let digest = run.request_sha256()[0].1.clone();
        (model, digest)
    };
    let (latest, latest_digest) = digest("jev");
    let (pinned, pinned_digest) = digest("jev-1.14.0");
    assert_eq!(latest, ModelId::Jev);
    assert_eq!(pinned, ModelId::Jev1_14_0);
    assert_ne!(latest_digest, pinned_digest);

    // A models.toml row for `jev` does not pin `jev`; it only lists.
    let global = sc_jev_check::models_file::models_from_str(
        "[[model]]\nid = \"jev-9.9.9\"\nalias = \"jev\"\nreleased = \"2099-01-01\"\n",
    )
    .expect("file");
    let with_file = prepared(&dir, |config| config.global_models(global.clone()));
    assert_eq!(model_of(&with_file, BackendId::Jev), ModelId::Jev);
    assert_eq!(
        with_file.models().latest("jev").expect("jev").as_str(),
        "jev-9.9.9"
    );
}

#[test]
fn test_check_toml_models_reject_another_family() {
    for models in [
        "\n[models]\nhaiku = \"gpt-6-luna\"\n",
        "\n[models]\njev = \"claude-haiku-5-5\"\n",
    ] {
        let dir = repo(models);
        let root = RepoRoot::new(dir.path());
        let config = RunConfig::new(root.clone(), Arc::new(NoopSessionLog)).dry_run(true);
        let source = CheckSource::check_file(&root, Path::new("check.toml")).expect("source");
        let error = prepare(source, config).expect_err(models);
        assert_eq!(error.code(), "CONFIG_INVALID", "{models}");
    }
}
