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

# (scripted failure, expected id, kind, retryable)
CASES = [
    ({"error": "PROVIDER_BUSY"}, "PROVIDER_BUSY", "capacity", True),
    ({"error": "QUOTA_EXHAUSTED"}, "QUOTA_EXHAUSTED", "capacity", False),
    ({"error": "AUTH_FAILED"}, "AUTH_FAILED", "permission", False),
    ({"error": "API_REQUEST_INVALID"}, "API_REQUEST_INVALID", "validation", False),
    ({"error": "API_ERROR"}, "API_ERROR", "dependency", True),
    ({"error": "RESPONSE_INVALID"}, "RESPONSE_INVALID", "dependency", False),
    ({"error": "INPUT_TOO_LARGE"}, "INPUT_TOO_LARGE", "validation", False),
    ({"error": "NETWORK_ERROR", "kind": "timeout"}, "NETWORK_ERROR", "timeout", True),
    ({"error": "NETWORK_ERROR", "kind": "tls"}, "NETWORK_ERROR", "dependency", False),
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


@pytest.mark.parametrize(("failure", "code", "kind", "retryable"), CASES)
def test_exception_has_every_field(
    tmp_path: Path, failure: dict, code: str, kind: str, retryable: bool
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
    assert error.retry_after_seconds is None or isinstance(error.retry_after_seconds, int)
    assert error.field is None or isinstance(error.field, str)
    assert error.details is None or isinstance(error.details, dict)
    assert str(error).startswith(f"{code}: ")


def test_usage_error_names_the_field(tmp_path: Path) -> None:
    """A rejected argument is `INPUT_INVALID` with the argument in `field`."""
    repo = _repo(tmp_path)
    with pytest.raises(sc_jev_py.JevError) as caught:
        run_check_scripted(
            "check.toml",
            scripted_answers={"relevant": "yes"},
            backends=["anthropic_haiku"],
            repo_root=repo,
            log_dir=tmp_path / "log",
            session_dir=tmp_path / "sessions",
            project="line\nbreak",
        )
    assert caught.value.code == "INPUT_INVALID"
    assert caught.value.kind == "validation"
    assert caught.value.retryable is False
