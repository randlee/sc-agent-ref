//! Model-family variants (REQ-SAG-0145).

use sc_agent_types::{MODELS, ModelFamily, ModelId};
use sc_decision_schema::variants::{canonical, schema_for};
use sc_decision_schema::{
    CANONICAL_V1, CanonicalJudgePayload, DecisionSchemaId, PayloadBundle, canonical_json_bytes,
};

#[test]
fn test_canonical_variant_is_identity() {
    let payload: CanonicalJudgePayload = serde_json::from_value(serde_json::json!({
        "questions": [{"id": "q", "prompt": "Is it?", "choices": ["no", "yes"]}],
        "state": [{"question_id": "q", "choice": "yes"}]
    }))
    .expect("payload");
    let bundle = PayloadBundle::encode(&payload).expect("bundle");
    let input = canonical::input(&payload);
    assert!(std::ptr::eq(input, &payload));
    // Its serialization equals the bundle bytes, so prompt_sha256 is the
    // digest of what is embedded.
    let value = serde_json::to_value(input).expect("value");
    assert_eq!(canonical_json_bytes(&value).expect("bytes"), bundle.bytes());
}

#[test]
fn test_every_model_family_maps_to_a_known_schema() {
    let families = [
        ModelFamily::TypeSafeJev,
        ModelFamily::AnthropicClaude,
        ModelFamily::OpenAiGpt,
        ModelFamily::Local,
        ModelFamily::Unknown,
    ];
    for family in families {
        let schema = schema_for(family);
        assert!(DecisionSchemaId::KNOWN.contains(&schema), "{family:?}");
        assert_eq!(schema, CANONICAL_V1);
    }
    for row in MODELS {
        assert!(DecisionSchemaId::KNOWN.contains(&schema_for(row.family())));
    }
    let other: ModelId = "a-future-model".parse().expect("model");
    assert!(DecisionSchemaId::KNOWN.contains(&schema_for(other.family())));
}
