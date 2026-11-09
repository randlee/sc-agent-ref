//! Python `JevError` with the same fields as the CLI error body.

use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use sc_agent_types::ScAgentErrorResponse;

pyo3::create_exception!(
    sc_jev_py,
    JevError,
    PyException,
    "Jev check failed. The attributes are the keys of the CLI stderr body: `code` (the CLI `error` key, the stable id), `kind`, `message` (what failed, never the fix), `suggested_action`, `retryable` (always a bool), `retry_after_seconds`, `field`, `details` (a dict or None) and `schema_version`."
);

/// The attribute names of the exception, in CLI-body order. `code` is the
/// CLI `error` key.
const FIELDS: [&str; 9] = [
    "code",
    "kind",
    "message",
    "suggested_action",
    "retryable",
    "retry_after_seconds",
    "field",
    "details",
    "schema_version",
];

/// Builds a Python `JevError` from `error`.
///
/// The attributes are exactly the fields of the CLI stderr body
/// ([`ScAgentErrorResponse::from_jev_error`], serialized and read back with
/// `json.loads`, so the two cannot drift): `code` is the body's `error`, the
/// optional keys are `None` when the body omits them. The display text is
/// `CODE: message. suggested_action`. If an attribute cannot be set, that
/// failure becomes the exception's `__cause__`. None of them holds a secret.
pub(crate) fn to_py_err(py: Python<'_>, error: &sc_agent_types::JevError) -> PyErr {
    let body = ScAgentErrorResponse::from_jev_error(error);
    // A full stop joins the two parts unless the message already ends in one.
    let joint = if body.message().ends_with('.') {
        " "
    } else {
        ". "
    };
    let err = PyErr::new::<JevError, _>(format!(
        "{}: {}{joint}{}",
        body.error(),
        body.message(),
        body.suggested_action()
    ));
    if let Err(failure) = set_fields(py, &err, &body) {
        // A fresh exception instance accepts new attributes, so this fails
        // only on an interpreter-level error such as `MemoryError`. The
        // display text still carries `CODE: message. action`; chain the
        // failure as `__cause__` so it is never silent (RBP-F002).
        err.set_cause(py, Some(failure));
    }
    err
}

fn set_fields(py: Python<'_>, err: &PyErr, body: &ScAgentErrorResponse) -> PyResult<()> {
    let text = serde_json::to_string(body)
        .map_err(|failure| pyo3::exceptions::PyValueError::new_err(failure.to_string()))?;
    let parsed = py.import("json")?.call_method1("loads", (text,))?;
    let dict = parsed.cast::<PyDict>()?;
    let value = err.value(py);
    for name in FIELDS {
        let key = if name == "code" { "error" } else { name };
        let attribute = dict.get_item(key)?;
        match attribute {
            Some(found) => value.setattr(name, found)?,
            None => value.setattr(name, py.None())?,
        }
    }
    Ok(())
}

pub(crate) fn map_err<T>(
    py: Python<'_>,
    result: Result<T, sc_agent_types::JevError>,
) -> PyResult<T> {
    result.map_err(|error| to_py_err(py, &error))
}

/// Like [`to_py_err`], with `response` (the run's response body as a dict)
/// set as the exception's `response` attribute (REQ-SAG-0604).
pub(crate) fn to_py_err_with_response(
    py: Python<'_>,
    error: &sc_agent_types::JevError,
    response: Py<PyAny>,
) -> PyErr {
    let err = to_py_err(py, error);
    if let Err(failure) = err.value(py).setattr("response", response) {
        err.set_cause(py, Some(failure));
    }
    err
}

#[cfg(test)]
mod tests {
    use super::*;
    use sc_agent_types::{
        BusyCause, ErrorCode, JevError as Core, LimitUnit, NetworkErrorKind, ProviderFailure,
    };

    fn samples() -> Vec<Core> {
        let failure = || {
            ProviderFailure::new("Jev", 422, "bad question")
                .with_type("invalid_request_error")
                .with_param("questions")
                .with_request_id("req_1")
        };
        vec![
            Core::InputEmpty,
            Core::input_invalid_field("--backend", "unknown backend"),
            Core::config_invalid("missing model"),
            Core::io_failed_with(
                "stdout",
                std::io::ErrorKind::BrokenPipe,
                Some("Retry.".into()),
            ),
            Core::input_too_large_tokens(32_000, 40_000),
            Core::input_too_large_by_provider(LimitUnit::Tokens, failure()),
            Core::network(NetworkErrorKind::Timeout, "Jev slow"),
            Core::provider_busy("HTTP 429 busy", 429, 4, BusyCause::Sends, Some(7)),
            Core::provider_busy("HTTP 529 busy", 529, 2, BusyCause::Budget, None),
            Core::auth_failed(failure()),
            Core::api_request_invalid(failure()),
            Core::quota_exhausted(failure()),
            Core::api_error(failure().with_retry_after(Some(3))),
            Core::response_invalid("missing answer"),
            Core::producer_error("sc-treesitter", ErrorCode::QueryNotFound, "no query"),
            Core::InputEmpty.with_log_flush_failure("disk full"),
        ]
    }

    /// The exception attributes are the keys of the CLI body, value for
    /// value (`code` is the body's `error`; an omitted key is `None`).
    #[test]
    fn exception_fields_equal_the_cli_body() {
        Python::initialize();
        Python::attach(|py| {
            let json = py.import("json").expect("json");
            for error in samples() {
                let body = serde_json::to_value(
                    sc_agent_types::ScAgentErrorResponse::from_jev_error(&error),
                )
                .expect("body");
                let err = to_py_err(py, &error);
                let value = err.value(py);
                for name in FIELDS {
                    let key = if name == "code" { "error" } else { name };
                    let attribute = value.getattr(name).expect(name);
                    let text = json
                        .call_method1("dumps", (&attribute,))
                        .expect("dumps")
                        .extract::<String>()
                        .expect("text");
                    let seen: serde_json::Value = serde_json::from_str(&text).expect("json");
                    let expected = body.get(key).cloned().unwrap_or(serde_json::Value::Null);
                    assert_eq!(seen, expected, "{} / {name}", error.code());
                }
                // `retryable` is always a bool, `schema_version` always 1.
                assert!(
                    value
                        .getattr("retryable")
                        .expect("retryable")
                        .is_instance_of::<pyo3::types::PyBool>()
                );
                assert_eq!(
                    value
                        .getattr("schema_version")
                        .expect("schema_version")
                        .extract::<u32>()
                        .expect("u32"),
                    1
                );
                // The display text carries the id, the message and the action.
                let shown = err.to_string();
                assert!(
                    shown.starts_with(&format!("JevError: {}: ", error.code())),
                    "{shown}"
                );
                assert!(shown.contains(&error.message()), "{shown}");
                assert!(shown.contains(&error.suggested_action()), "{shown}");
            }
        });
    }
}
