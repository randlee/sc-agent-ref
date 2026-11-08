//! The Jev model comes from the catalog (REQ-SAG-0148, D7.4/D7.5).

use std::time::Duration;

use sc_agent_http::test_util::ScriptedTransport;
use sc_agent_types::{BackendId, BackendKeyId, ModelAliases, ModelId, ResolvedApiKey};
use sc_decision_schema::JudgeBackend;
use sc_jev::{JevClient, JevClientConfig};

fn key() -> ResolvedApiKey {
    ResolvedApiKey::try_new(BackendKeyId::TypeSafe, "k".to_owned()).expect("key")
}

#[test]
fn test_jev_model_id_is_configured_model() {
    // The packaged config takes the catalog default, not a `jev.toml` key.
    let packaged = JevClientConfig::packaged();
    assert_eq!(
        packaged.model(),
        &ModelAliases::builtin().latest("jev").expect("jev alias")
    );
    let default = JevClient::new(key(), ScriptedTransport::responses(Vec::new()));
    assert_eq!(
        default.model_id(),
        BackendId::Jev.descriptor().default_model()
    );

    // A configured model (the `[models]` override) is what the client reports.
    let pinned: ModelId = "jev-9.0.0".parse().expect("model");
    let config = JevClientConfig::try_new(
        pinned.clone(),
        "jev.invalid",
        "/v1/x",
        1,
        Duration::from_secs(1),
    )
    .expect("config");
    let configured =
        JevClient::with_config(config, key(), ScriptedTransport::responses(Vec::new()));
    assert_eq!(configured.model_id(), pinned);
    let builder =
        JevClient::new(key(), ScriptedTransport::responses(Vec::new())).with_model(pinned.clone());
    assert_eq!(builder.model_id(), pinned);
    assert_eq!(builder.config().model(), &pinned);

    // The stale `model` key of an old jev.toml is refused.
    let error = JevClientConfig::from_toml(
        "model = \"jev-1.13.0\"\nhost = \"h.example\"\npath = \"/v1/x\"\nattempts = 2\ntimeout_seconds = 5\n",
    )
    .expect_err("model key removed");
    assert_eq!(error.code(), "CONFIG_INVALID");
}
