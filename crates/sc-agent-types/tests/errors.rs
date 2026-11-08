//! Error taxonomy contract (REQ-SAG-0101 amended, REQ-SAG-0106, REQ-SAG-0701,
//! NFR-SAG-0001, ADR-SAG-0027 as amended).
//!
//! An integration target cannot build `#[non_exhaustive]` variants with
//! literals, so every error here comes from its public constructor; the
//! golden test therefore also proves each constructor exists.
//!
//! Set `SC_UPDATE_GOLDEN=1` to rewrite the golden files from the current
//! behavior, then review the diff.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::PathBuf;
use std::str::FromStr;

use sc_agent_types::{
    BackendKeyId, BusyCause, ErrorCategory, ErrorCode, JevError, LimitUnit, NetworkErrorKind,
    PROVIDER_TEXT_MAX_BYTES, ProviderFailure, ResponseInvalidCause, ScAgentErrorResponse,
    SessionId, bound_provider_text,
};

fn repo_fixture(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn crate_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn update_golden() -> bool {
    std::env::var_os("SC_UPDATE_GOLDEN").is_some()
}

fn failure(status: u16) -> ProviderFailure {
    ProviderFailure::new("Jev", status, "the provider said no")
        .with_type("some_error")
        .with_request_id("req_1")
}

fn session_nil() -> SessionId {
    "00000000000000000000000000".parse().expect("nil ULID")
}

/// One value per variant (and per interesting shape), keyed by a stable name.
fn every_variant() -> Vec<(&'static str, JevError)> {
    let json_error = serde_json::from_str::<serde_json::Value>("{").expect_err("eof");
    vec![
        ("ConfigInvalid", JevError::config_invalid("missing model")),
        (
            "ConfigInvalid.Recovery",
            JevError::config_invalid_with(
                "no git toplevel was found",
                "Pass --repo-root with the repository root.",
            ),
        ),
        (
            "CredentialStoreInvalid",
            JevError::credential_store_invalid(
                "the Jev API key",
                "key contains whitespace",
                "Store the key again without spaces.",
            ),
        ),
        (
            "RegistryInvalid",
            JevError::registry_invalid(
                "Backend jev is not registered",
                "Register it before the run.",
            ),
        ),
        (
            "SessionDocumentInvalid",
            JevError::session_document_invalid(
                "Session file s is not a session document",
                "Delete it and start a new session.",
            ),
        ),
        (
            "EncodeFailed",
            JevError::encode_failed("response JSON", &json_error),
        ),
        (
            "IoFailed",
            JevError::io_failed("stdout", &io::Error::from(io::ErrorKind::BrokenPipe)),
        ),
        (
            "IoFailed.TimedOut",
            JevError::io_failed("git output", &io::Error::from(io::ErrorKind::TimedOut)),
        ),
        (
            "IoFailed.Interrupted",
            JevError::io_failed("stdin", &io::Error::from(io::ErrorKind::Interrupted)),
        ),
        (
            "InputMissing.Recovery",
            JevError::input_missing_with("/nope", "Pass --repo-root with an existing directory."),
        ),
        (
            "NetworkError.Other",
            JevError::network(NetworkErrorKind::Other, "bad uri"),
        ),
        (
            "Annotated.Backend",
            JevError::api_error(failure(500)).with_backend("anthropic_haiku"),
        ),
        (
            "IoFailed.Recovery",
            JevError::io_failed_with(
                "git",
                io::ErrorKind::NotFound,
                Some("Check that git is installed.".to_owned()),
            ),
        ),
        ("KeyMissing", JevError::key_missing(BackendKeyId::TypeSafe)),
        ("InputTooLarge.Bytes", JevError::input_too_large(10, 11)),
        (
            "InputTooLarge.Tokens",
            JevError::input_too_large_tokens(32_000, 40_000),
        ),
        (
            "InputTooLarge.Provider",
            JevError::input_too_large_by_provider(LimitUnit::Tokens, failure(400)),
        ),
        ("InputEmpty", JevError::InputEmpty),
        ("InputInvalid", JevError::input_invalid("bad json")),
        (
            "InputInvalid.Field",
            JevError::input_invalid_field("--backend", "unknown backend nope"),
        ),
        ("ExtractionEmpty", JevError::ExtractionEmpty),
        (
            "ExtractionParseError",
            JevError::extraction_parse_error("a.py", 3),
        ),
        ("ExtractionInvalid", JevError::extraction_invalid("schema")),
        ("ExtractionFailed", JevError::extraction_failed("exit 1")),
        ("ExtractionTimeout", JevError::extraction_timeout(30)),
        ("GitTimeout", JevError::git_timeout(60)),
        (
            "LanguageUnavailable",
            JevError::language_unavailable("sc-treesitter", "cobol"),
        ),
        (
            "QueryInvalid",
            JevError::query_invalid("sc-treesitter", "syntax"),
        ),
        (
            "QueryNotFound",
            JevError::query_not_found("sc-treesitter", "missing"),
        ),
        (
            "ProducerError",
            JevError::producer_error("sc-treesitter", ErrorCode::QueryNotFound, "Use branches."),
        ),
        ("InputMissing", JevError::input_missing("gone.toml")),
        (
            "ResponseInvalid.Schema",
            JevError::response_invalid("missing answer"),
        ),
        (
            "ResponseInvalid.Truncated",
            JevError::response_invalid_because(ResponseInvalidCause::Truncated, "cut short"),
        ),
        (
            "ResponseInvalid.TooLarge",
            JevError::response_invalid_because(ResponseInvalidCause::TooLarge, "over the cap"),
        ),
        (
            "ResponseInvalid.NotJson",
            JevError::response_invalid_because(ResponseInvalidCause::NotJson, "not JSON"),
        ),
        (
            "NetworkError.Timeout",
            JevError::network(NetworkErrorKind::Timeout, "Jev connection failed"),
        ),
        (
            "NetworkError.Connect",
            JevError::network(NetworkErrorKind::Connect, "Jev connection failed"),
        ),
        (
            "NetworkError.Tls",
            JevError::network(NetworkErrorKind::Tls, "Jev handshake failed"),
        ),
        (
            "NetworkError.Cancelled",
            JevError::network(NetworkErrorKind::Cancelled, "request cancelled"),
        ),
        ("AuthFailed", JevError::auth_failed(failure(401))),
        (
            "ApiRequestInvalid",
            JevError::api_request_invalid(failure(422).with_param("questions")),
        ),
        ("QuotaExhausted", JevError::quota_exhausted(failure(429))),
        ("ApiError.500", JevError::api_error(failure(500))),
        (
            "ApiError.503",
            JevError::api_error(failure(503).with_retry_after(Some(12))),
        ),
        ("ApiError.404", JevError::api_error(failure(404))),
        (
            "ProviderBusy.Sends",
            JevError::provider_busy(
                "Jev HTTP 429 busy; 4 sends exhausted",
                429,
                4,
                BusyCause::Sends,
                Some(7),
            ),
        ),
        (
            "ProviderBusy.Budget",
            JevError::provider_busy(
                "Jev HTTP 529 busy after 3 sends; retry budget exhausted",
                529,
                3,
                BusyCause::Budget,
                None,
            ),
        ),
        (
            "PromptFileMissing",
            JevError::prompt_file_missing("prompt.md"),
        ),
        (
            "SessionNotFound",
            JevError::session_not_found(session_nil()),
        ),
        ("SessionExpired", JevError::session_expired(session_nil())),
        (
            "LogFlushFailed",
            JevError::InputEmpty.with_log_flush_failure("disk full"),
        ),
    ]
}

/// Compares `actual` with the JSON golden file, or rewrites it.
fn check_golden(name: &str, actual: &BTreeMap<String, serde_json::Value>) {
    let path = crate_fixture(name);
    let rendered = format!("{}\n", serde_json::to_string_pretty(actual).expect("json"));
    if update_golden() {
        std::fs::write(&path, rendered).expect("write golden");
        return;
    }
    let text = std::fs::read_to_string(&path).expect("golden");
    let golden: BTreeMap<String, serde_json::Value> =
        serde_json::from_str(&text).expect("golden json");
    assert_eq!(
        &golden, actual,
        "{name} differs; rerun with SC_UPDATE_GOLDEN=1 and review"
    );
}

#[test]
fn test_every_variant_golden() {
    let mut actual = BTreeMap::new();
    for (name, error) in every_variant() {
        let body =
            serde_json::to_value(ScAgentErrorResponse::from_jev_error(&error)).expect("body json");
        actual.insert(name.to_owned(), body);
    }
    check_golden("error-codes.json", &actual);
}

#[test]
fn test_error_codes_match_errors_json() {
    let text = std::fs::read_to_string(repo_fixture(
        "tests/fixtures/jev/README-contract/errors.json",
    ))
    .expect("errors.json");
    let document: serde_json::Value = serde_json::from_str(&text).expect("json");
    let expected: BTreeSet<String> = document["error_ids"]
        .as_array()
        .expect("error_ids")
        .iter()
        .map(|id| id.as_str().expect("id").to_owned())
        .collect();
    let actual: BTreeSet<String> = ErrorCode::ALL
        .iter()
        .map(|code| code.as_str().to_owned())
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), ErrorCode::ALL.len(), "ids are unique");
    for code in ErrorCode::ALL {
        assert_eq!(ErrorCode::from_str(code.as_str()).expect("parse"), *code);
        assert_eq!(code.to_string(), code.as_str());
    }
    let produced: BTreeSet<&str> = every_variant()
        .iter()
        .map(|(_, error)| error.code().as_str())
        .collect();
    assert!(produced.iter().all(|code| expected.contains(*code)));
    // Every id is produced by at least one sample, except the ones only a
    // producer can re-raise (the producer sample covers QUERY_NOT_FOUND).
    for id in &expected {
        assert!(produced.contains(id.as_str()), "no sample produces {id}");
    }
}

#[test]
fn test_error_response_stderr_golden() {
    let golden = std::fs::read_to_string(crate_fixture("error-stderr.jsonl")).expect("golden");
    let errors = [
        JevError::InputEmpty,
        JevError::provider_busy(
            "Jev HTTP 429 busy; 4 sends exhausted",
            429,
            4,
            BusyCause::Sends,
            Some(7),
        ),
        JevError::producer_error(
            "sc-treesitter",
            ErrorCode::LanguageUnavailable,
            "language cobol is unavailable in sc-treesitter",
        ),
        JevError::api_request_invalid(failure(422).with_param("questions")),
    ];
    let mut actual = String::new();
    for error in &errors {
        let line =
            serde_json::to_string(&ScAgentErrorResponse::from_jev_error(error)).expect("json");
        actual.push_str(&line);
        actual.push('\n');
        let back: ScAgentErrorResponse = serde_json::from_str(&line).expect("reads back");
        assert_eq!(back, ScAgentErrorResponse::from_jev_error(error));
        assert_eq!(back.schema_version(), 1);
    }
    if update_golden() {
        std::fs::write(crate_fixture("error-stderr.jsonl"), &actual).expect("write golden");
        return;
    }
    assert_eq!(actual, golden, "rerun with SC_UPDATE_GOLDEN=1 and review");
}

/// The retryable decision of every id, with the reason in the doc of
/// `JevError::retryable`.
#[test]
fn test_retryable_decisions() {
    let expected_true = [
        "NetworkError.Timeout",
        "NetworkError.Connect",
        "ApiError.500",
        "ApiError.503",
        "GitTimeout",
        "Annotated.Backend",
        "IoFailed.TimedOut",
        "IoFailed.Interrupted",
        "ProviderBusy.Sends",
        "ProviderBusy.Budget",
    ];
    for (name, error) in every_variant() {
        let expected = expected_true.contains(&name);
        assert_eq!(error.retryable(), expected, "{name}");
        let value =
            serde_json::to_value(ScAgentErrorResponse::from_jev_error(&error)).expect("json");
        assert_eq!(value["retryable"], expected, "{name}: always present");
    }
    for kind in NetworkErrorKind::ALL.iter().copied() {
        assert_eq!(
            JevError::network(kind, "x").retryable(),
            matches!(kind, NetworkErrorKind::Timeout | NetworkErrorKind::Connect),
            // `other` is an unclassified error and is never assumed transient.
            "{kind}"
        );
    }
    // A 504 and a 500 are retryable, a 408 and a 404 are not.
    for (status, retryable) in [
        (500, true),
        (502, true),
        (503, true),
        (504, true),
        (404, false),
        (408, false),
    ] {
        assert_eq!(
            JevError::api_error(failure(status)).retryable(),
            retryable,
            "{status}"
        );
    }
}

#[test]
fn test_busy_exhaustion_machine_fields() {
    let sends = JevError::provider_busy("busy", 429, 4, BusyCause::Sends, Some(7));
    let body = ScAgentErrorResponse::from_jev_error(&sends);
    assert_eq!(body.retry_after_seconds(), Some(7));
    let details = body.details().expect("details");
    assert_eq!(details.cause.as_deref(), Some("sends"));
    assert_eq!(details.attempts, Some(4));
    assert_eq!(details.http_status, Some(429));
    let budget = JevError::provider_busy("busy", 529, 2, BusyCause::Budget, None);
    let body = ScAgentErrorResponse::from_jev_error(&budget);
    assert_eq!(body.retry_after_seconds(), None);
    assert_eq!(
        body.details().and_then(|d| d.cause.as_deref()),
        Some("budget")
    );
    let value = serde_json::to_value(&body).expect("json");
    assert!(value.get("retry_after_seconds").is_none());
}

#[test]
fn test_message_never_holds_the_fix() {
    for (name, error) in every_variant() {
        let message = error.message();
        let action = error.suggested_action();
        assert!(!message.trim().is_empty(), "{name}");
        assert!(!action.trim().is_empty(), "{name}");
        assert_eq!(message, error.to_string(), "{name}");
        assert_ne!(message, action, "{name}");
        for fix_phrase in ["check.toml", "Retry", "Check that", "Wait"] {
            assert!(
                !message.contains(fix_phrase),
                "{name}: the message holds a fix ({fix_phrase}): {message}"
            );
        }
    }
}

#[test]
fn test_actions_agree_with_the_retry_flag() {
    for (name, error) in every_variant() {
        let action = error.suggested_action();
        if !error.retryable() {
            // A non-retryable error never tells the caller to just retry it.
            for blunt in ["Retry the request", "Retry the same", "retry the request"] {
                assert!(!action.contains(blunt), "{name}: {action}");
            }
        }
    }
    let invalid = [
        ResponseInvalidCause::Truncated,
        ResponseInvalidCause::TooLarge,
        ResponseInvalidCause::NotJson,
        ResponseInvalidCause::Schema,
    ];
    let actions: BTreeSet<String> = invalid
        .iter()
        .map(|cause| JevError::response_invalid_because(*cause, "x").suggested_action())
        .collect();
    assert_eq!(actions.len(), invalid.len(), "one action per cause");
    for cause in invalid {
        let error = JevError::response_invalid_because(cause, "x");
        assert!(!error.retryable());
        assert_eq!(
            error.details().and_then(|d| d.cause),
            Some(cause.as_str().to_owned())
        );
    }
}

#[test]
fn test_categories() {
    let expected = [
        ("AuthFailed", ErrorCategory::Permission),
        ("ApiRequestInvalid", ErrorCategory::Validation),
        ("QuotaExhausted", ErrorCategory::Capacity),
        ("ProviderBusy.Sends", ErrorCategory::Capacity),
        ("ApiError.500", ErrorCategory::Dependency),
        ("IoFailed", ErrorCategory::Dependency),
        ("InputMissing", ErrorCategory::NotFound),
        ("NetworkError.Timeout", ErrorCategory::Timeout),
        ("NetworkError.Connect", ErrorCategory::Dependency),
        ("InputTooLarge.Bytes", ErrorCategory::Validation),
        ("SessionExpired", ErrorCategory::InvalidState),
        ("KeyMissing", ErrorCategory::Permission),
    ];
    let all: BTreeMap<_, _> = every_variant().into_iter().collect();
    for (name, category) in expected {
        assert_eq!(all[name].category(), category, "{name}");
    }
    for code in ErrorCode::ALL {
        // Every id has a category and it round-trips through serde.
        let value = serde_json::to_value(code.category()).expect("json");
        let back: ErrorCategory = serde_json::from_value(value).expect("serde");
        assert_eq!(back, code.category());
        assert_eq!(code.category().to_string(), code.category().as_str());
    }
}

#[test]
fn test_one_id_for_both_limits() {
    let bytes = JevError::input_too_large(10, 11);
    let tokens = JevError::input_too_large_tokens(32_000, 40_000);
    assert_eq!(bytes.code(), "INPUT_TOO_LARGE");
    assert_eq!(tokens.code(), "INPUT_TOO_LARGE");
    let details = ScAgentErrorResponse::from_jev_error(&tokens);
    let details = details.details().expect("details");
    assert_eq!(details.limit, Some(32_000));
    assert_eq!(details.actual, Some(40_000));
    assert_eq!(details.unit.as_deref(), Some("tokens"));
    let by_provider = JevError::input_too_large_by_provider(LimitUnit::Bytes, failure(413));
    let body = ScAgentErrorResponse::from_jev_error(&by_provider);
    let details = body.details().expect("details");
    assert_eq!(details.limit, None);
    assert_eq!(details.unit.as_deref(), Some("bytes"));
    assert_eq!(details.http_status, Some(413));
}

#[test]
fn test_io_failed_carries_what_and_kind() {
    let error = JevError::io_failed("stdout", &io::Error::from(io::ErrorKind::PermissionDenied));
    let body = ScAgentErrorResponse::from_jev_error(&error);
    let details = body.details().expect("details");
    assert_eq!(details.what.as_deref(), Some("stdout"));
    assert_eq!(details.io_kind.as_deref(), Some("permission_denied"));
    // One rule for a read of a named path.
    let gone = io::Error::from(io::ErrorKind::NotFound);
    assert_eq!(
        JevError::read_failed("a.json", "request file", &gone).code(),
        "INPUT_MISSING"
    );
    let denied = io::Error::from(io::ErrorKind::PermissionDenied);
    assert_eq!(
        JevError::read_failed("a.json", "request file", &denied).code(),
        "IO_FAILED"
    );
}

#[test]
fn test_provider_text_is_bounded_and_single_line() {
    let long = format!("start\n{}", "é".repeat(2000));
    let bounded = bound_provider_text(&long);
    assert!(
        bounded.len() <= PROVIDER_TEXT_MAX_BYTES,
        "{}",
        bounded.len()
    );
    assert!(bounded.ends_with("..."));
    assert!(!bounded.contains('\n'));
    let error = JevError::api_error(ProviderFailure::new("Jev", 500, &long));
    assert!(error.message().len() < PROVIDER_TEXT_MAX_BYTES + 100);
}

#[test]
fn test_log_flush_failure_is_its_own_field() {
    for (name, error) in every_variant() {
        let flushed = error.clone().with_log_flush_failure("disk full");
        assert_eq!(flushed.code(), error.code(), "{name}");
        assert_eq!(flushed.message(), error.message(), "{name}");
        assert_eq!(
            flushed.suggested_action(),
            error.suggested_action(),
            "{name}"
        );
        assert_eq!(flushed.retryable(), error.retryable(), "{name}");
        assert_eq!(flushed.log_flush_failure(), Some("disk full"), "{name}");
        let body = ScAgentErrorResponse::from_jev_error(&flushed);
        assert_eq!(
            body.details().and_then(|d| d.log_flush_failure.as_deref()),
            Some("disk full"),
            "{name}"
        );
    }
}

#[test]
fn test_from_error_response_round_trip() {
    for code in ErrorCode::ALL.iter().copied() {
        let line = format!(
            r#"{{"error":"{code}","message":"It broke.","suggested_action":"Do the thing.","retryable":true,"retry_after_seconds":9,"field":"q","details":{{"cause":"x","path":"p"}},"schema_version":1}}"#
        );
        let response: ScAgentErrorResponse = serde_json::from_str(&line).expect("response");
        let error = JevError::from_error_response("sc-treesitter", &response);
        assert!(
            matches!(error, JevError::ProducerError { .. }),
            "{code}: {error:?}"
        );
        assert_eq!(error.code(), code);
        assert_eq!(error.suggested_action(), "sc-treesitter: Do the thing.");
        // Nothing the producer said is dropped on the way through.
        assert!(error.retryable(), "{code}");
        assert_eq!(error.retry_after_seconds(), Some(9));
        assert_eq!(error.field(), Some("q"));
        let again = ScAgentErrorResponse::from_jev_error(&error);
        assert_eq!(again.error(), code.as_str());
        assert_eq!(again.producer(), Some("sc-treesitter"));
        assert!(again.retryable());
        assert_eq!(again.retry_after_seconds(), Some(9));
        assert_eq!(again.field(), Some("q"));
        let details = again.details().expect("details");
        assert_eq!(details.cause.as_deref(), Some("x"));
        assert_eq!(details.path.as_deref(), Some("p"));
    }
    let unknown: ScAgentErrorResponse =
        serde_json::from_str(r#"{"error":"NOT_A_CODE","message":"m"}"#).expect("short shape");
    let error = JevError::from_error_response("tool", &unknown);
    assert_eq!(error.code(), "EXTRACTION_FAILED");
    assert!(error.message().contains("NOT_A_CODE"));
}

#[test]
fn test_network_error_kind_spelling() {
    let expected = [
        (NetworkErrorKind::Timeout, "timeout"),
        (NetworkErrorKind::Connect, "connect"),
        (NetworkErrorKind::Tls, "tls"),
        (NetworkErrorKind::Cancelled, "cancelled"),
        (NetworkErrorKind::Other, "other"),
    ];
    assert_eq!(expected.len(), NetworkErrorKind::ALL.len());
    for (kind, text) in expected {
        assert_eq!(kind.to_string(), text);
        assert_eq!(serde_json::to_value(kind).expect("json"), text);
        assert_eq!(
            serde_json::from_value::<NetworkErrorKind>(serde_json::json!(text)).expect("serde"),
            kind
        );
        assert_eq!(text.parse::<NetworkErrorKind>().expect("from_str"), kind);
        // The kind is visible to callers in `details`.
        let body = ScAgentErrorResponse::from_jev_error(&JevError::network(kind, "x"));
        assert_eq!(
            body.details().and_then(|d| d.network_kind.as_deref()),
            Some(text)
        );
    }
    for gone in ["Busy", "busy", "status", "body_too_large"] {
        let error = gone.parse::<NetworkErrorKind>().expect_err("not a kind");
        assert_eq!(error.code(), "INPUT_INVALID");
    }
}

#[test]
fn test_error_source_is_none_for_redacted_variants() {
    use std::error::Error as _;
    for (name, error) in every_variant() {
        assert!(error.source().is_none(), "{name}");
    }
}

#[test]
fn test_explicit_recovery_variants_do_not_blame_check_toml() {
    for (name, error) in every_variant() {
        let action = error.suggested_action();
        assert!(!action.trim().is_empty(), "{name}");
        if !matches!(name, "ConfigInvalid" | "InputMissing" | "PromptFileMissing") {
            assert!(!action.contains("check.toml"), "{name}: {action}");
        }
    }
}

#[test]
fn test_map_detail_reaches_provider_text_and_details() {
    let error = JevError::api_request_invalid(
        failure(422)
            .with_param("secret-param")
            .with_code("secret-code"),
    );
    let mapped = error.map_detail(|text| text.replace("secret", "[redacted]"));
    let rendered = format!(
        "{} {:?}",
        serde_json::to_string(&ScAgentErrorResponse::from_jev_error(&mapped)).expect("json"),
        mapped
    );
    assert!(!rendered.contains("secret"), "{rendered}");
}

#[test]
fn test_backend_is_a_machine_field() {
    let error = JevError::api_error(failure(500)).with_backend("anthropic_haiku");
    assert_eq!(error.backend(), Some("anthropic_haiku"));
    let body = ScAgentErrorResponse::from_jev_error(&error);
    let details = body.details().expect("details");
    assert_eq!(details.backend.as_deref(), Some("anthropic_haiku"));
    assert_eq!(details.http_status, Some(500));
    // Only the first backend counts, and flush failure composes with it.
    let both = error
        .with_backend("jev")
        .with_log_flush_failure("disk full");
    assert_eq!(both.backend(), Some("anthropic_haiku"));
    assert_eq!(both.log_flush_failure(), Some("disk full"));
}

#[test]
fn test_unknown_keys_in_a_producer_line_are_tolerated() {
    let line = r#"{"error":"QUERY_NOT_FOUND","message":"m","suggested_action":"a","newer_key":{"x":1},"details":{"path":"p","newer_detail":true},"schema_version":1}"#;
    let response: ScAgentErrorResponse = serde_json::from_str(line).expect("tolerated");
    let error = JevError::from_error_response("sc-treesitter", &response);
    assert_eq!(error.code(), "QUERY_NOT_FOUND");
    assert_eq!(error.details().and_then(|d| d.path).as_deref(), Some("p"));
}

#[test]
fn test_retryable_io_kinds_are_the_transient_ones() {
    for kind in [io::ErrorKind::Interrupted, io::ErrorKind::TimedOut] {
        assert!(JevError::io_failed("x", &io::Error::from(kind)).retryable());
    }
    for kind in [
        io::ErrorKind::NotFound,
        io::ErrorKind::PermissionDenied,
        io::ErrorKind::BrokenPipe,
    ] {
        assert!(!JevError::io_failed("x", &io::Error::from(kind)).retryable());
    }
    // One rule for both timeouts: a stalled git is transient, a command
    // extractor over its configured limit is not.
    assert!(JevError::git_timeout(60).retryable());
    assert!(!JevError::extraction_timeout(30).retryable());
}
