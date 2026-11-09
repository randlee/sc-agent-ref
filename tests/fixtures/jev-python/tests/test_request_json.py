"""`request_json=` gives Python the CLI's `--request-json` (REQ-QA-002)."""

from __future__ import annotations

import pytest

import sc_jev_py

REQUEST = (
    '{"questions":[{"id":"relevant","prompt":"Does it apply?","choices":["no","yes"]}],'
    '"state":[]}'
)
CANONICAL = (
    '{"questions":[{"choices":["no","yes"],"id":"relevant","prompt":"Does it apply?"}],'
    '"state":[]}'
)
# The same document through `sc-ai jev dry-run --request-json` gives this digest
# (crates/workspace-tests/tests/consumer_parity.rs pins the same literal).
PAYLOAD_SHA256 = "79b58710e937c53180dc2f263c9e09fce6afc2168b4a3f019b4f600aac30e0a3"


def test_build_request_takes_a_questions_document(tmp_path):
    for document in (REQUEST, REQUEST.encode("utf-8")):
        built = sc_jev_py.build_request(request_json=document, repo_root=str(tmp_path))
        assert built["payload"] == CANONICAL
        assert built["payload_sha256"] == PAYLOAD_SHA256
        assert built["questions_sha256"] and set(built["request_sha256"]) == {"jev"}


def test_run_check_dry_run_matches_build_request(tmp_path):
    built = sc_jev_py.build_request(request_json=REQUEST, repo_root=str(tmp_path))
    result = sc_jev_py.run_check(
        request_json=REQUEST,
        dry_run=True,
        repo_root=str(tmp_path),
        log_dir=str(tmp_path / "log"),
        session_dir=str(tmp_path / "sessions"),
    )
    assert result.payload == built["payload"]
    assert result.payload_sha256 == PAYLOAD_SHA256
    assert result.questions_sha256 == built["questions_sha256"]
    assert dict(result.request_sha256) == built["request_sha256"]


@pytest.mark.parametrize("call", [sc_jev_py.build_request, sc_jev_py.run_check])
def test_check_path_and_request_json_are_exclusive(tmp_path, call):
    with pytest.raises(sc_jev_py.JevError) as both:
        call("check.toml", request_json=REQUEST, repo_root=str(tmp_path))
    assert both.value.code == "INPUT_INVALID"
    with pytest.raises(sc_jev_py.JevError) as neither:
        call(repo_root=str(tmp_path))
    assert neither.value.code == "INPUT_INVALID"
    with pytest.raises(sc_jev_py.JevError) as wrong:
        call(request_json=3, repo_root=str(tmp_path))
    assert wrong.value.code == "INPUT_INVALID"


def test_a_malformed_document_is_input_invalid(tmp_path):
    with pytest.raises(sc_jev_py.JevError) as caught:
        sc_jev_py.build_request(request_json="{}", repo_root=str(tmp_path))
    assert caught.value.code == "INPUT_INVALID"
