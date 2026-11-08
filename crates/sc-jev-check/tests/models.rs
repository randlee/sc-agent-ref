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
        format!("backends = [\"jev\", \"anthropic_haiku\", \"openai_luna\"]\n\n[questions]\npath = \"questions.json\"\n{models}"),
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
        model_of(&from_toml, BackendId::AnthropicHaiku).as_str(),
        "claude-haiku-9-preview"
    );
    assert_eq!(
        model_of(&from_toml, BackendId::OpenAiLuna),
        ModelId::Gpt6Luna
    );
    // Unlisted aliases keep the catalog default.
    assert_eq!(model_of(&from_toml, BackendId::Jev), ModelId::Jev1_13_0);

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
        model_of(&config_wins, BackendId::OpenAiLuna).as_str(),
        "gpt-7-luna"
    );
    assert_eq!(
        model_of(&config_wins, BackendId::AnthropicHaiku),
        ModelId::ClaudeHaiku5_5
    );
}

#[test]
fn test_unknown_model_alias_is_config_invalid() {
    for (models, needle) in [
        ("\n[models]\nsonnet = \"claude-sonnet-9\"\n", "sonnet"),
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
        let rendered = format!("{error} {}", error.fix_hint());
        assert!(rendered.contains(needle), "{needle}: {rendered}");
        if needle == "sonnet" {
            for alias in ["jev", "haiku", "luna"] {
                assert!(rendered.contains(alias), "{rendered}");
            }
        }
    }
}
