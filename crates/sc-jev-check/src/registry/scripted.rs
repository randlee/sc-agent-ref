//! In-process judges for tests (ADR-SAG-0022, r-1 amendment).
//!
//! [`register_scripted_backends`] builds a [`BackendRegistry`] of
//! [`MockJudgeBackend`]s, the workspace's one judge double. Production builds
//! leave the `test-util` feature off; the scripted registry reaches a run
//! through [`crate::run::RunConfig::registry`] (ADR-SAG-0022 r-2 amendment).
//! Each mock reports, as `input_sha256`, the digest of the bundle it
//! received, so equal-input attests it like a real backend (ADR-SAG-0022
//! r-2b amendment). Each mock also records the digest of a synthetic sent
//! body, `{"scripted":"<backend id>"}`, into the send's trace (ADR-SAG-0022 r-6
//! amendment), so a run's log rows carry a `request_sha256` and two scripted
//! backends carry different ones.

use std::collections::BTreeMap;
use std::sync::Arc;

use sc_agent_types::{BackendId, JevError, NetworkErrorKind};
use sc_decision_schema::test_util::MockJudgeBackend;
use sc_decision_schema::{JudgeBackend, QuestionAnswer, QuestionId};
use serde_json::Value;

use super::BackendRegistry;

/// The body a scripted judge for `id` pretends to have sent.
///
/// Its digest is the row's `request_sha256`: stable per backend id and
/// different between backends (tests compute the same text). No transport exists, so this is the only
/// "sent body" a scripted run has.
#[must_use]
fn scripted_body(id: BackendId) -> String {
    format!("{{\"scripted\":\"{id}\"}}")
}

/// Registers one scripted judge per `ids` entry.
///
/// `answers` is one of two JSON shapes (ADR-SAG-0022 r-2b amendment):
///
/// - *Shared form*: an object of question id to answer. An answer is an
///   answer object (`{"choice": "yes"}`, optionally with `confidence` and
///   `probabilities`) or the string shorthand `"yes"` (ADR-SAG-0029 (b)).
///   Every id in `ids` answers with it.
/// - *Per-backend form*: an object whose only key is `"backends"`, mapping
///   each backend id in `ids` (its descriptor name) to a shared-form answer
///   object or a failure entry
///   `{"failure": {"error", "kind"?, "stage"?}}`. `error` is
///   `NETWORK_ERROR` (needs `kind`, such as `"busy"`), `INPUT_TOO_LARGE` or
///   `RESPONSE_INVALID`. `stage` `"judge"` (the default) sends and fails;
///   `"validate"` refuses before any send.
///
/// Each judge is a [`MockJudgeBackend`]: it answers at `judge` time against
/// the request it receives, does not open a socket, and stores no API key.
/// Callers pass the registry, in an `Arc`, to
/// [`crate::run::RunConfig::registry`]. No run API changes (ADR-SAG-0009).
///
/// # Errors
///
/// Returns `INPUT_INVALID` when `answers` matches neither form: not an
/// object, a value that is neither a string nor a valid answer object, an
/// unknown failure `error`, `kind` or `stage`, a backend id that is not in
/// `ids`, an id in `ids` with no entry, or a shared-form map whose only
/// question id is `backends`. The fix hint names the two forms.
/// Returns `CONFIG_INVALID` ([`JevError::RegistryInvalid`]) when `ids`
/// repeats a backend.
///
/// # Examples
///
/// Needs the `test-util` feature (a doctest links the library without
/// `cfg(test)`), so plain `cargo test -p sc-jev-check` does not build it.
///
#[cfg_attr(feature = "test-util", doc = "```")]
#[cfg_attr(not(feature = "test-util"), doc = "```ignore")]
/// use sc_agent_types::BackendId;
/// use sc_jev_check::registry::register_scripted_backends;
///
/// let registry =
///     register_scripted_backends(&[BackendId::Jev], serde_json::json!({"q": "yes"})).unwrap();
/// assert!(registry.get(BackendId::Jev).is_ok());
/// assert_eq!(
///     register_scripted_backends(&[BackendId::Jev], serde_json::json!(["yes"]))
///         .unwrap_err()
///         .code(),
///     "INPUT_INVALID"
/// );
/// ```
pub fn register_scripted_backends(
    ids: &[BackendId],
    answers: Value,
) -> Result<BackendRegistry, JevError> {
    let mut registry = BackendRegistry::new();
    if is_per_backend_form(&answers) {
        let mut entries = per_backend_entries(ids, answers)?;
        for id in ids.iter().copied() {
            let entry = entries.remove(&id).ok_or_else(|| {
                forms_error(format!(
                    "backend {id} is in the run but has no entry under \"backends\""
                ))
            })?;
            registry.register(entry.into_backend(id))?;
        }
        return Ok(registry);
    }
    let answers = normalize_answers(answers)?;
    for id in ids.iter().copied() {
        registry.register(Arc::new(
            MockJudgeBackend::new(id, answers.clone()).with_sent_body(scripted_body(id)),
        ))?;
    }
    Ok(registry)
}

/// `{"backends": ...}` alone: the per-backend form. A value that is not an
/// object is rejected later, with the two-forms hint.
fn is_per_backend_form(answers: &Value) -> bool {
    answers
        .as_object()
        .is_some_and(|map| map.len() == 1 && map.contains_key("backends"))
}

/// `INPUT_INVALID` whose detail names both accepted forms.
fn forms_error(problem: impl std::fmt::Display) -> JevError {
    JevError::input_invalid(format!(
        "scripted answers: {problem}; use the shared form {{\"<question id>\": \"yes\" | {{\"choice\": ..}}}} or the per-backend form {{\"backends\": {{\"<backend id>\": <shared form> | {{\"failure\": {{\"error\": .., \"kind\"?: .., \"stage\"?: ..}}}}}}}}"
    ))
}

/// One backend's scripted behaviour.
enum Entry {
    Answers(BTreeMap<QuestionId, QuestionAnswer>),
    Failure { error: JevError, validate: bool },
}

impl Entry {
    fn into_backend(self, id: BackendId) -> Arc<dyn JudgeBackend> {
        match self {
            Self::Answers(answers) => {
                Arc::new(MockJudgeBackend::new(id, answers).with_sent_body(scripted_body(id)))
            }
            Self::Failure {
                error,
                validate: true,
            } => Arc::new(MockJudgeBackend::refusing(id, error)),
            Self::Failure {
                error,
                validate: false,
            } => Arc::new(
                MockJudgeBackend::new(id, BTreeMap::new())
                    .with_sent_body(scripted_body(id))
                    .with_failure(error),
            ),
        }
    }
}

fn per_backend_entries(
    ids: &[BackendId],
    answers: Value,
) -> Result<BTreeMap<BackendId, Entry>, JevError> {
    let Some(Value::Object(by_backend)) = answers
        .as_object()
        .and_then(|map| map.get("backends"))
        .cloned()
    else {
        return Err(forms_error("\"backends\" must be an object of backend ids"));
    };
    let mut entries = BTreeMap::new();
    for (name, entry) in by_backend {
        let id = ids
            .iter()
            .copied()
            .find(|id| id.descriptor().name() == name)
            .ok_or_else(|| {
                forms_error(format!("backend {name:?} is not in the run's backend ids"))
            })?;
        entries.insert(id, parse_entry(id, &name, entry)?);
    }
    Ok(entries)
}

fn parse_entry(id: BackendId, name: &str, entry: Value) -> Result<Entry, JevError> {
    let Value::Object(map) = &entry else {
        return Err(forms_error(format!(
            "the entry for {name} must be an answers object or a failure entry"
        )));
    };
    if map.len() == 1 && map.contains_key("failure") {
        return parse_failure(id, name, &map["failure"]);
    }
    Ok(Entry::Answers(normalize_answers(entry)?))
}

fn parse_failure(id: BackendId, name: &str, failure: &Value) -> Result<Entry, JevError> {
    let Value::Object(map) = failure else {
        return Err(forms_error(format!(
            "the failure for {name} must be an object"
        )));
    };
    if let Some(key) = map
        .keys()
        .find(|key| !["error", "kind", "stage"].contains(&key.as_str()))
    {
        return Err(forms_error(format!(
            "the failure for {name} has unknown key {key:?}"
        )));
    }
    let text = |key: &str| map.get(key).map(|value| value.as_str());
    let error = match text("error") {
        Some(Some("NETWORK_ERROR")) => {
            let Some(Some(kind)) = text("kind") else {
                return Err(forms_error(format!(
                    "the NETWORK_ERROR failure for {name} needs a string \"kind\", such as \"busy\""
                )));
            };
            let kind: NetworkErrorKind = kind.parse()?;
            JevError::network(kind, "scripted failure")
        }
        Some(Some("INPUT_TOO_LARGE")) => {
            let limit = id.descriptor().request_limit_bytes();
            JevError::input_too_large(limit, limit.saturating_add(1))
        }
        Some(Some("RESPONSE_INVALID")) => JevError::response_invalid("scripted failure"),
        _ => {
            return Err(forms_error(format!(
                "the failure for {name} needs \"error\" of NETWORK_ERROR, INPUT_TOO_LARGE or RESPONSE_INVALID"
            )));
        }
    };
    let validate = match text("stage") {
        None | Some(Some("judge")) => false,
        Some(Some("validate")) => true,
        _ => {
            return Err(forms_error(format!(
                "the failure for {name} has a \"stage\" other than \"judge\" or \"validate\""
            )));
        }
    };
    Ok(Entry::Failure { error, validate })
}

/// Expands the `"choice"` string shorthand and types every answer.
fn normalize_answers(answers: Value) -> Result<BTreeMap<QuestionId, QuestionAnswer>, JevError> {
    let Value::Object(map) = answers else {
        return Err(forms_error("answers must be a JSON object"));
    };
    if map.len() == 1 && map.contains_key("backends") {
        return Err(forms_error(
            "\"backends\" is the per-backend form's key, not a question id",
        ));
    }
    let mut normalized = BTreeMap::new();
    for (id, answer) in map {
        let wrapped = match answer {
            Value::String(choice) => serde_json::json!({ "choice": choice }),
            object @ Value::Object(_) => object,
            _ => {
                return Err(JevError::input_invalid(format!(
                    "scripted answer for {id} must be a string or object"
                )));
            }
        };
        let answer: QuestionAnswer = serde_json::from_value(wrapped).map_err(|error| {
            JevError::input_invalid(format!("scripted answer for {id} is invalid: {error}"))
        })?;
        normalized.insert(QuestionId::try_from(id)?, answer);
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_shorthand_and_object_answers_normalise_alike() {
        let shorthand = normalize_answers(serde_json::json!({"q": "yes"})).expect("shorthand");
        let object = normalize_answers(serde_json::json!({"q": {"choice": "yes"}})).expect("obj");
        assert_eq!(shorthand, object);
        assert_eq!(shorthand["q"].choice().as_str(), "yes");
        for bad in [
            serde_json::json!("yes"),
            serde_json::json!({"q": 1}),
            serde_json::json!({"q": {"choice": "yes", "confidence": 2.0}}),
        ] {
            let error = normalize_answers(bad).expect_err("bad shape");
            assert_eq!(error.code(), "INPUT_INVALID");
        }
    }
}
