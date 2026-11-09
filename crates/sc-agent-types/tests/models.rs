//! Model catalog (REQ-SAG-0147, REQ-SAG-0148, ADR-SAG-0030).

use sc_agent_types::{MODELS, ModelAliases, ModelFamily, ModelId, ModelSource};

/// Every `MODELS` string parses to its known variant, never `Other`, and
/// round-trips through `Display` and serde.
#[test]
fn test_known_models_roundtrip() {
    for row in MODELS {
        let model = row.model();
        assert!(
            !matches!(model, ModelId::Other(_)),
            "{} parsed as Other",
            row.alias()
        );
        let text = model.to_string();
        assert_eq!(text.parse::<ModelId>().expect("parse"), model);
        assert_eq!(
            serde_json::to_value(&model).expect("serialize"),
            serde_json::json!(text)
        );
        assert_eq!(
            serde_json::from_value::<ModelId>(serde_json::json!(text)).expect("deserialize"),
            model
        );
        assert_eq!(model.family(), row.family());
    }
    assert_eq!(ModelId::Jev1_13_0.as_str(), "jev-1.13.0");
    assert_eq!(ModelId::Jev1_14_0.as_str(), "jev-1.14.0");
    assert_eq!(ModelId::ClaudeHaiku5_5.as_str(), "claude-haiku-5-5");
    assert_eq!(ModelId::Gpt6Luna.as_str(), "gpt-6-luna");
}

/// The definition-only models round-trip as known variants (agt-akp).
#[test]
fn test_defined_models_roundtrip() {
    let defined = [
        (ModelId::ClaudeSonnet5_5, "claude-sonnet-5-5", "sonnet"),
        (ModelId::ClaudeOpus5_5, "claude-opus-5-5", "opus"),
        (ModelId::ClaudeFable5_1, "claude-fable-5-1", "fable"),
        (ModelId::Gpt6_1Terra, "gpt-6.1-terra", "terra"),
        (ModelId::Gpt6_1Sol, "gpt-6.1-sol", "sol"),
        (ModelId::Gpt6Astra, "gpt-6-astra", "astra"),
    ];
    for (variant, id, alias) in defined {
        assert_eq!(variant.as_str(), id);
        assert_eq!(id.parse::<ModelId>().expect("parse"), variant);
        assert_eq!(ModelAliases::builtin().latest(alias), Some(variant));
    }
}

#[test]
fn test_file_rows_resolve_to_the_newest_released() {
    let base = ModelAliases::builtin();
    let newer = base
        .clone()
        .with_file_model("claude-haiku-9", "haiku", "2099-01-01")
        .expect("row");
    assert_eq!(
        newer.latest("haiku").expect("model").as_str(),
        "claude-haiku-9"
    );
    assert_eq!(newer.source("haiku"), Some(ModelSource::File));
    assert_eq!(newer.source("luna"), Some(ModelSource::Crate));
    // An older row loses to the crate.
    let older = base
        .clone()
        .with_file_model("claude-haiku-4", "haiku", "2000-01-01")
        .expect("row");
    assert_eq!(older.latest("haiku"), Some(ModelId::ClaudeHaiku5_5));
    assert_eq!(older.source("haiku"), Some(ModelSource::Crate));
    // An override stays on top of a file row.
    let pinned = newer
        .with_override("haiku", ModelId::ClaudeHaiku5_5)
        .expect("override")
        .with_file_model("claude-haiku-10", "haiku", "2099-02-02")
        .expect("row");
    assert_eq!(pinned.source("haiku"), Some(ModelSource::Override));
    for (id, alias, released) in [
        ("claude-haiku-9", "nope", "2099-01-01"),
        ("", "haiku", "2099-01-01"),
        ("claude-haiku-9", "haiku", "2099-13-01"),
        ("claude-haiku-9", "haiku", "soon"),
    ] {
        let error = base
            .clone()
            .with_file_model(id, alias, released)
            .expect_err(released);
        assert_eq!(error.code(), "CONFIG_INVALID", "{id} {alias} {released}");
    }
}

#[test]
fn test_unknown_model_string_parses_as_other() {
    let model: ModelId = "claude-haiku-9".parse().expect("unknown is open");
    let ModelId::Other(unknown) = &model else {
        panic!("expected Other, got {model:?}");
    };
    assert_eq!(unknown.as_str(), "claude-haiku-9");
    assert_eq!(model.as_str(), "claude-haiku-9");
    assert_eq!(model.family(), ModelFamily::Unknown);
    assert_eq!(model.to_string(), "claude-haiku-9");
    let back: ModelId = serde_json::from_str("\"claude-haiku-9\"").expect("serde");
    assert_eq!(back, model);
    for bad in ["", "a\nb", "\t"] {
        let error = bad.parse::<ModelId>().expect_err("rejected");
        assert_eq!(error.code(), "INPUT_INVALID");
        assert!(
            error
                .message()
                .contains("non-empty model string or an alias")
        );
        assert!(serde_json::from_value::<ModelId>(serde_json::json!(bad)).is_err());
    }
}

#[test]
fn test_latest_alias_lookup() {
    let aliases = ModelAliases::builtin();
    assert_eq!(aliases.latest("jev"), Some(ModelId::Jev1_14_0));
    assert_eq!(aliases.latest("haiku"), Some(ModelId::ClaudeHaiku5_5));
    assert_eq!(aliases.latest("luna"), Some(ModelId::Gpt6Luna));
    assert_eq!(aliases.latest("nope"), None);
    for row in MODELS {
        let date = row.released();
        let parts: Vec<&str> = date.split('-').collect();
        assert_eq!(parts.len(), 3, "{date}");
        let numbers: Vec<u32> = parts
            .iter()
            .map(|part| part.parse().expect("digits"))
            .collect();
        assert_eq!(parts[0].len(), 4, "{date}");
        assert_eq!(parts[1].len(), 2, "{date}");
        assert_eq!(parts[2].len(), 2, "{date}");
        assert!((1..=12).contains(&numbers[1]), "{date}");
        assert!((1..=31).contains(&numbers[2]), "{date}");
        assert!(aliases.latest(row.alias()).is_some());
    }
}

#[test]
fn test_alias_override_wins() {
    let pinned: ModelId = "claude-haiku-9".parse().expect("model");
    let aliases = ModelAliases::builtin()
        .with_override("haiku", pinned.clone())
        .expect("known alias");
    assert_eq!(aliases.latest("haiku"), Some(pinned));
    assert_eq!(aliases.latest("luna"), Some(ModelId::Gpt6Luna));
    let error = ModelAliases::builtin()
        .with_override("nope", ModelId::Gpt6Luna)
        .expect_err("unknown alias");
    assert_eq!(error.code(), "CONFIG_INVALID");
    let hint = error.message();
    for alias in ["jev", "haiku", "luna"] {
        assert!(hint.contains(alias), "{hint}");
    }
}

/// `jev` is accepted as the latest Jev model (agt-cac) and is not a `MODELS`
/// row, so it never becomes the `jev` alias default.
#[test]
fn test_bare_jev_is_the_latest_model() {
    let model: ModelId = "jev".parse().expect("jev");
    assert_eq!(model, ModelId::Jev);
    assert_eq!(model.as_str(), "jev");
    assert_eq!(model.family(), ModelFamily::TypeSafeJev);
    assert_eq!(
        serde_json::from_value::<ModelId>(serde_json::json!("jev")).expect("serde"),
        ModelId::Jev
    );
    assert!(MODELS.iter().all(|row| row.model().as_str() != "jev"));
    assert_eq!(
        ModelAliases::builtin().latest("jev"),
        Some(ModelId::Jev1_14_0)
    );
}

/// An alias takes only ids of its own family, by prefix (file rows,
/// `check.toml` `[models]` and pins all go through these two methods).
#[test]
fn test_alias_takes_only_its_own_family() {
    let base = ModelAliases::builtin();
    for (alias, id) in [
        ("haiku", "gpt-6-luna"),
        ("jev", "claude-haiku-5-5"),
        ("luna", "claude-haiku-5-5"),
        ("sol", "jev-1.14.0"),
        ("opus", "gpt-6.1-sol"),
        ("jevx", "x"),
    ] {
        let from_file = base.clone().with_file_model(id, alias, "2099-01-01");
        let from_override = base.clone().with_override(alias, id.parse().expect("id"));
        for result in [from_file, from_override] {
            assert_eq!(
                result.expect_err(id).code(),
                "CONFIG_INVALID",
                "{alias} {id}"
            );
        }
    }
    for (alias, id) in [
        ("haiku", "claude-haiku-9-preview"),
        ("jev", "jev-1.15.0"),
        ("jev", "jev"),
        ("astra", "gpt-7-astra"),
        ("fable", "claude-fable-6"),
    ] {
        base.clone()
            .with_file_model(id, alias, "2099-01-01")
            .expect(id);
        base.clone()
            .with_override(alias, id.parse().expect("id"))
            .expect(id);
    }
}
