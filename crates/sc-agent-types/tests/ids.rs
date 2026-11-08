//! Backend catalog and id newtypes (REQ-SAG-0105, ADR-SAG-0009/0027 r-1).

use std::collections::BTreeSet;

use sc_agent_types::{
    AnswerId, BACKENDS, BackendFamily, BackendId, BackendKeyId, BackendSet, BlobSha, ModelAliases,
    ProjectLabel, RepoRelativePath,
};

/// serde, `Display`, `FromStr`, and the aliases agree for every descriptor.
#[test]
fn test_backend_catalog_roundtrip() {
    let mut names = BTreeSet::new();
    for row in BACKENDS {
        let id = row.id();
        assert_eq!(id.descriptor(), row, "{}", row.name());
        assert!(names.insert(row.name()), "duplicate name {}", row.name());
        assert_eq!(id.to_string(), row.name());
        assert_eq!(id.as_str(), row.name());
        assert_eq!(row.name().parse::<BackendId>().expect("name"), id);
        assert_eq!(serde_json::to_value(id).expect("serialize"), row.name());
        assert_eq!(
            serde_json::from_value::<BackendId>(serde_json::json!(row.name())).expect("serde"),
            id
        );
        for alias in row.aliases() {
            assert!(names.insert(alias), "alias {alias} collides");
            assert_eq!(alias.parse::<BackendId>().expect("alias"), id);
            assert_eq!(
                serde_json::from_value::<BackendId>(serde_json::json!(alias)).expect("alias"),
                id
            );
        }
        assert_eq!(id.key_id(), row.key_id());
        assert_eq!(id.family(), row.family());
        assert_eq!(
            row.requires_llm_context(),
            row.family() == BackendFamily::Llm
        );
        assert!(row.request_limit_bytes() > 0);
        // D7.2: the default alias resolves to the row's default model.
        assert_eq!(
            ModelAliases::builtin().latest(row.default_model_alias()),
            Some(row.default_model()),
            "{}",
            row.name()
        );
    }
    for rejected in ["anthropic-haiku", "openai-luna", "Jev", "", "haiku"] {
        let error = rejected.parse::<BackendId>().expect_err("rejected");
        assert_eq!(error.code(), "INPUT_INVALID");
    }
}

#[test]
fn test_backend_serde_documented_spelling() {
    let wire = |id: BackendId| serde_json::to_value(id).expect("serialize");
    assert_eq!(wire(BackendId::Jev), "jev");
    assert_eq!(wire(BackendId::AnthropicHaiku), "anthropic_haiku");
    assert_eq!(wire(BackendId::OpenAiLuna), "openai_luna");
    assert_eq!(
        serde_json::to_value(BackendKeyId::OpenAi).expect("key"),
        "openai"
    );
    assert_eq!(
        serde_json::to_value(BackendKeyId::TypeSafe).expect("key"),
        "typesafe"
    );
    for key in BackendKeyId::ALL.iter().copied() {
        assert_eq!(serde_json::to_value(key).expect("key"), key.to_string());
        assert_eq!(key.to_string().parse::<BackendKeyId>().expect("parse"), key);
    }
}

#[test]
fn test_backend_serde_reads_old_alias() {
    let luna: BackendId = serde_json::from_str("\"open_ai_luna\"").expect("old luna");
    assert_eq!(luna, BackendId::OpenAiLuna);
    let key: BackendKeyId = serde_json::from_str("\"open_ai\"").expect("old key");
    assert_eq!(key, BackendKeyId::OpenAi);
    // The alias is read only; writing uses the documented spelling.
    assert_eq!(
        serde_json::to_string(&luna).expect("write"),
        "\"openai_luna\""
    );
}

#[test]
fn test_backend_set_rules() {
    let set: BackendSet = "jev, anthropic_haiku".parse().expect("set");
    assert_eq!(set.as_slice(), [BackendId::Jev, BackendId::AnthropicHaiku]);
    assert_eq!(set.len(), 2);
    for bad in ["", "jev,jev"] {
        assert_eq!(
            bad.parse::<BackendSet>().expect_err(bad).code(),
            "CONFIG_INVALID"
        );
    }
    assert_eq!(
        BackendSet::try_from(Vec::new()).expect_err("empty").code(),
        "CONFIG_INVALID"
    );
    assert_eq!(
        "nope".parse::<BackendSet>().expect_err("unknown").code(),
        "INPUT_INVALID"
    );
}

#[test]
fn test_newtypes_validate_on_every_path() {
    assert!(RepoRelativePath::try_from("src/a.py").is_ok());
    for bad in ["", "/abs", "../up", "a/../b", "a\\b", "c:x", "a\0b"] {
        assert!(RepoRelativePath::try_from(bad).is_err(), "{bad:?}");
        assert!(
            serde_json::from_value::<RepoRelativePath>(serde_json::json!(bad)).is_err(),
            "{bad:?}"
        );
    }
    assert!(BlobSha::try_from("a".repeat(64).as_str()).is_ok());
    for bad in ["a".repeat(39), "A".repeat(40), "g".repeat(40)] {
        assert!(BlobSha::try_from(bad.as_str()).is_err(), "{bad}");
    }
}

/// The one validator the CLI, Python inputs and `try_from_fields` share.
#[test]
fn test_project_label_limits() {
    assert!(ProjectLabel::new("billing").is_ok());
    assert!(ProjectLabel::new("").is_ok());
    assert!(ProjectLabel::new("é".repeat(128)).is_ok(), "256 bytes");
    assert_eq!(
        ProjectLabel::new("a".repeat(256))
            .expect("limit")
            .as_str()
            .len(),
        256
    );
    for bad in [
        "a".repeat(257),
        "é".repeat(129),
        "a\nb".to_owned(),
        "\u{7f}".to_owned(),
    ] {
        let error = ProjectLabel::new(bad.clone()).expect_err("rejected");
        assert_eq!(error.code(), "INPUT_INVALID");
        let hint = error.fix_hint();
        assert!(hint.contains("256 bytes"), "{hint}");
        assert!(hint.contains("control"), "{hint}");
        assert!(bad.parse::<ProjectLabel>().is_err());
        assert!(serde_json::from_value::<ProjectLabel>(serde_json::json!(bad)).is_err());
    }
    let label: ProjectLabel = serde_json::from_str("\"alpha\"").expect("serde");
    assert_eq!(label.to_string(), "alpha");
    assert_eq!(serde_json::to_string(&label).expect("write"), "\"alpha\"");
    assert_eq!(format!("{label:?}"), "ProjectLabel(\"alpha\")");
}

#[test]
fn test_answer_id_is_ulid_text() {
    let id = AnswerId::new();
    assert_eq!(id.to_string().len(), 26);
    assert_eq!(id.to_string().parse::<AnswerId>().expect("roundtrip"), id);
    assert_ne!(AnswerId::new(), AnswerId::new());
    for bad in [
        "",
        "nope",
        &"Z".repeat(26),
        &"0".repeat(25),
        &"i".repeat(26),
    ] {
        let error = bad.parse::<AnswerId>().expect_err("rejected");
        assert_eq!(error.code(), "INPUT_INVALID");
        assert!(
            error.fix_hint().contains("answer_id"),
            "{}",
            error.fix_hint()
        );
    }
}
