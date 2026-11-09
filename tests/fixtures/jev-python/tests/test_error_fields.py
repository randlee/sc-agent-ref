"""The Python `JevError` carries the same fields as the CLI error body (agt-2a3)."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

import sc_jev_py

run_check_scripted = sc_jev_py._testing.run_check_scripted

FIELDS = (
    "code",
    "kind",
    "message",
    "suggested_action",
    "retryable",
    "retry_after_seconds",
    "field",
    "details",
    "schema_version",
)

# (scripted failure, expected id, kind, retryable, expected details subset,
# word the suggested action must contain)
CASES = [
    (
        {"error": "PROVIDER_BUSY"},
        "PROVIDER_BUSY",
        "capacity",
        True,
        {"cause": "sends", "attempts": 4, "http_status": 429, "backend": "jev"},
        "Wait",
    ),
    (
        {"error": "QUOTA_EXHAUSTED"},
        "QUOTA_EXHAUSTED",
        "capacity",
        False,
        {"http_status": 402, "provider": "jev", "backend": "jev"},
        "credit",
    ),
    (
        {"error": "AUTH_FAILED"},
        "AUTH_FAILED",
        "permission",
        False,
        {"http_status": 401, "backend": "jev"},
        "credential",
    ),
    (
        {"error": "API_REQUEST_INVALID"},
        "API_REQUEST_INVALID",
        "validation",
        False,
        {"http_status": 422, "backend": "jev"},
        "Correct",
    ),
    (
        {"error": "API_ERROR"},
        "API_ERROR",
        "dependency",
        True,
        {"http_status": 500, "backend": "jev"},
        "status page",
    ),
    (
        {"error": "RESPONSE_INVALID"},
        "RESPONSE_INVALID",
        "dependency",
        False,
        {"cause": "schema", "backend": "jev"},
        "Retrying is unlikely",
    ),
    (
        {"error": "INPUT_TOO_LARGE"},
        "INPUT_TOO_LARGE",
        "validation",
        False,
        {"unit": "bytes", "backend": "jev"},
        "never split",
    ),
    (
        {"error": "NETWORK_ERROR", "kind": "timeout"},
        "NETWORK_ERROR",
        "timeout",
        True,
        {"network_kind": "timeout", "backend": "jev"},
        "timed out",
    ),
    (
        {"error": "NETWORK_ERROR", "kind": "tls"},
        "NETWORK_ERROR",
        "dependency",
        False,
        {"network_kind": "tls", "backend": "jev"},
        "TLS",
    ),
]


def _repo(tmp: Path) -> Path:
    repo = tmp / "repo"
    repo.mkdir()
    (repo / "questions.json").write_text(
        json.dumps(
            {
                "schema": "plan-anchoring/questions/1",
                "version": 1,
                "questions": {
                    "relevant": {
                        "type": "choice",
                        "instructions": "Does it apply?",
                        "criteria": {"no": "No.", "yes": "Yes."},
                    }
                },
            }
        ),
        encoding="utf-8",
    )
    (repo / "check.toml").write_text('[questions]\npath = "questions.json"\n', encoding="utf-8")
    return repo


@pytest.mark.parametrize(("failure", "code", "kind", "retryable", "details", "action"), CASES)
def test_exception_has_every_field(
    tmp_path: Path,
    failure: dict,
    code: str,
    kind: str,
    retryable: bool,
    details: dict,
    action: str,
) -> None:
    repo = _repo(tmp_path)
    with pytest.raises(sc_jev_py.JevError) as caught:
        run_check_scripted(
            "check.toml",
            scripted_answers={"backends": {"jev": {"failure": failure}}},
            backends=["jev"],
            repo_root=repo,
            log_dir=tmp_path / "log",
            session_dir=tmp_path / "sessions",
        )
    error = caught.value
    for name in FIELDS:
        assert hasattr(error, name), name
    assert error.code == code
    assert error.kind == kind
    assert error.retryable is retryable
    assert error.schema_version == 1
    assert isinstance(error.message, str) and error.message
    assert isinstance(error.suggested_action, str) and error.suggested_action
    # `message` says what failed; the fix is in its own attribute.
    assert error.suggested_action not in error.message
    assert action in error.suggested_action
    assert error.retry_after_seconds is None
    assert error.field is None
    assert isinstance(error.details, dict)
    for key, value in details.items():
        assert error.details[key] == value, (key, error.details)
    # The failing backend is named in the error and in the response.
    assert error.response is not None
    assert error.response["results"][0]["error"]["error"] == code
    assert error.response["results"][0]["error"]["details"]["backend"] == "jev"
    assert str(error).startswith(f"{code}: ")


def test_usage_error_names_the_field(tmp_path: Path) -> None:
    """A rejected argument is `INPUT_INVALID` with the argument in `field`."""
    repo = _repo(tmp_path)
    with pytest.raises(sc_jev_py.JevError) as caught:
        run_check_scripted(
            "check.toml",
            scripted_answers={"relevant": "yes"},
            backends=["haiku"],
            repo_root=repo,
            log_dir=tmp_path / "log",
            session_dir=tmp_path / "sessions",
            project="line\nbreak",
        )
    assert caught.value.code == "INPUT_INVALID"
    assert caught.value.kind == "validation"
    assert caught.value.retryable is False
