"""REQ-SAG-0604/0605 and the consumer-parity golden (r-3b)."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

import sc_jev_py

run_check_scripted = sc_jev_py._testing.run_check_scripted

FIXTURES = Path(__file__).resolve().parents[2]
GOLDEN = FIXTURES / "jev" / "run-response" / "dry_run_cli.json"


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


def test_run_data_matches_golden(tmp_path: Path) -> None:
    """`sc-ai`'s `data` (minus `readback`) and Python's agree on one golden."""
    result = sc_jev_py.run_check(
        "check.toml",
        dry_run=True,
        repo_root=FIXTURES / "jev" / "git-repo",
        log_dir=tmp_path / "log",
        session_dir=tmp_path / "sessions",
    )
    assert result.data == json.loads(GOLDEN.read_text(encoding="utf-8"))


def test_partial_failure_raises_with_response(tmp_path: Path) -> None:
    repo = _repo(tmp_path)
    answers = {
        "backends": {
            "jev": {"relevant": "yes"},
            "anthropic_haiku": {"failure": {"error": "PROVIDER_BUSY"}},
        }
    }
    with pytest.raises(sc_jev_py.JevError) as caught:
        run_check_scripted(
            "check.toml",
            scripted_answers=answers,
            backends=["jev", "anthropic_haiku"],
            repo_root=repo,
            log_dir=tmp_path / "log",
            session_dir=tmp_path / "sessions",
        )
    error = caught.value
    assert error.code == "PROVIDER_BUSY"
    response = error.response
    assert isinstance(response, dict)
    assert response["primary_backend"] == "jev"
    assert response["answers"]["relevant"]["choice"] == "yes"
    assert [entry["status"] for entry in response["results"]] == ["ok", "failed"]

    # All backends failed.
    failing = {"failure": {"error": "PROVIDER_BUSY"}}
    with pytest.raises(sc_jev_py.JevError) as plain:
        run_check_scripted(
            "check.toml",
            scripted_answers={"backends": {"jev": failing, "anthropic_haiku": failing}},
            backends=["jev", "anthropic_haiku"],
            repo_root=repo,
            log_dir=tmp_path / "log",
            session_dir=tmp_path / "sessions",
        )
    assert plain.value.code == "PROVIDER_BUSY"
    # All backends failed: the response is still set (no answers, one typed
    # error per backend, each naming its backend).
    response = plain.value.response
    assert isinstance(response, dict)
    assert "answers" not in response
    assert [entry["status"] for entry in response["results"]] == ["failed", "failed"]
    assert [entry["error"]["details"]["backend"] for entry in response["results"]] == [
        "jev",
        "anthropic_haiku",
    ]


def test_chain_context_kwarg(tmp_path: Path) -> None:
    repo = _repo(tmp_path)
    kwargs = dict(
        scripted_answers={"relevant": "yes"},
        backends=["jev", "anthropic_haiku"],
        repo_root=repo,
        log_dir=tmp_path / "log",
        session_dir=tmp_path / "sessions",
    )
    independent = run_check_scripted("check.toml", **kwargs)
    assert independent.equal_input["mode"] == "independent"
    chained = run_check_scripted("check.toml", chain_context=True, **kwargs)
    assert chained.equal_input["mode"] == "chain_context"
    assert chained.prompt_sha256 == independent.prompt_sha256
