//! Decision schema versions (REQ-SAG-0142).

use std::sync::Arc;

use sc_decision_schema::{
    CANONICAL_V1, CanonicalJudgePayload, DecisionSchemaId, PayloadBundle, SchemaFamily,
};

#[test]
fn test_known_versions_accepted_unknown_rejected() {
    assert_eq!(DecisionSchemaId::CURRENT, CANONICAL_V1);
    assert_eq!(DecisionSchemaId::KNOWN, [CANONICAL_V1]);
    assert_eq!(CANONICAL_V1.family(), SchemaFamily::Canonical);
    assert_eq!(CANONICAL_V1.version(), 1);
    for known in DecisionSchemaId::KNOWN {
        let text = known.to_string();
        assert_eq!(text, "canonical.v1");
        assert_eq!(text.parse::<DecisionSchemaId>().expect("known"), *known);
        assert_eq!(
            DecisionSchemaId::parse_response(&text).expect("known"),
            *known
        );
        assert_eq!(
            serde_json::to_value(known).expect("write"),
            serde_json::json!(text)
        );
        assert_eq!(
            serde_json::from_value::<DecisionSchemaId>(serde_json::json!(text)).expect("read"),
            *known
        );
    }
    for unknown in [
        "canonical.v2",
        "canonical.v0",
        "canonical",
        "local.v1",
        "",
        "Canonical.v1",
    ] {
        let input = unknown
            .parse::<DecisionSchemaId>()
            .expect_err("input refused");
        assert_eq!(input.code(), "INPUT_INVALID", "{unknown}");
        assert!(
            input.message().contains("canonical.v1"),
            "{}",
            input.message()
        );
        let response = DecisionSchemaId::parse_response(unknown).expect_err("response refused");
        assert_eq!(response.code(), "RESPONSE_INVALID", "{unknown}");
        assert!(response.message().contains("canonical.v1"));
        assert!(serde_json::from_value::<DecisionSchemaId>(serde_json::json!(unknown)).is_err());
    }
}

/// The version is never a key of the payload and never part of its bytes.
#[test]
fn test_schema_version_not_in_canonical_bytes() {
    let payload: CanonicalJudgePayload = serde_json::from_value(serde_json::json!({
        "questions": [{"id": "q", "prompt": "Is it?", "choices": ["no", "yes"]}],
        "state": []
    }))
    .expect("payload");
    let bundle = Arc::new(PayloadBundle::encode(&payload).expect("bundle"));
    assert_eq!(
        bundle.as_str(),
        r#"{"questions":[{"choices":["no","yes"],"id":"q","prompt":"Is it?"}],"state":[]}"#
    );
    assert!(!bundle.as_str().contains("canonical"));
    assert!(!bundle.as_str().contains("decision_schema"));
    let value = serde_json::to_value(&payload).expect("value");
    let keys: Vec<&String> = value.as_object().expect("object").keys().collect();
    assert_eq!(keys, ["questions", "state"]);
    // A payload that names a version is refused, so no caller can smuggle one in.
    let with_version = serde_json::from_value::<CanonicalJudgePayload>(serde_json::json!({
        "questions": [], "state": [], "decision_schema": "canonical.v1"
    }));
    assert!(with_version.is_err());
}
