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

/// `jev` (the latest) is requested on the wire as `jev-latest`, the value
/// `jev-api.md` documents; that mapping is UNVERIFIED against the live API
/// (nothing in this repository can call it). The answering version is kept.
#[test]
fn test_jev_latest_is_sent_as_the_documented_wire_value() {
    use sc_agent_http::HttpResponse;
    use sc_agent_types::SendTrace;
    use sc_decision_schema::{CanonicalJudgePayload, JudgeRequest, PayloadBundle};
    use std::sync::Arc;

    let reply = serde_json::json!({
        "model": "jev-1.13.0",
        "answers": {"q": {"type": "choice", "choice": "no", "confidence": 0.9,
            "probabilities": {"yes": 0.1, "no": 0.9}}},
        "usage": {}
    });
    let transport = ScriptedTransport::responses(vec![HttpResponse::new(
        200,
        Vec::new(),
        serde_json::to_vec(&reply).expect("json"),
    )]);
    let client = JevClient::new(key(), transport.clone()).with_model(ModelId::Jev);
    assert_eq!(client.model_id(), ModelId::Jev);
    let canonical: CanonicalJudgePayload = serde_json::from_value(serde_json::json!({
        "questions": [{"id": "q", "prompt": "x", "choices": ["no", "yes"]}],
        "state": []
    }))
    .expect("payload");
    let bundle = PayloadBundle::encode(&canonical).expect("bundle");
    let before = sc_jev::request_sha256(&ModelId::Jev, &bundle).expect("digest before sending");
    let trace = SendTrace::new();
    let outcome = client
        .judge(&JudgeRequest::new(Arc::new(bundle), None), &trace)
        .expect("answered");
    // The digest computed before sending is the one the send records.
    assert_eq!(trace.request_sha256(), Some(before));
    let sent = transport.requests();
    let body = String::from_utf8(sent[0].body().as_bytes().to_vec()).expect("utf8");
    assert!(body.starts_with(r#"{"model":"jev-latest","#), "{body}");
    assert_eq!(outcome.answered_model(), Some(ModelId::Jev1_13_0));
}
