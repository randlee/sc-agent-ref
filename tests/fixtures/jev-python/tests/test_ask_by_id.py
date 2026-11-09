"""`sc_jev_py.ask` by session id (REQ-SAG-0731)."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

import pytest

import sc_jev_py

ask_scripted = sc_jev_py._testing.ask_scripted
run_check_scripted = sc_jev_py._testing.run_check_scripted

_CHILD = """
import json, sys
from pathlib import Path
import sc_jev_py
work = Path(sys.argv[1])
result = sc_jev_py._testing.run_check_scripted(
    "check.toml", scripted_answers={"relevant": "yes"}, backends=["haiku"],
    repo_root=work / "repo", log_dir=work / "log", session_dir=work / "sessions",
)
print(result.session_id)
"""


def _repo(work: Path) -> Path:
    repo = work / "repo"
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


def _paths(work: Path) -> dict[str, Path]:
    return {"repo_root": work / "repo", "log_dir": work / "log", "session_dir": work / "sessions"}


def test_ask_by_id_matches_session_ask(tmp_path: Path) -> None:
    repo = _repo(tmp_path)
    result = run_check_scripted(
        "check.toml", scripted_answers={"relevant": "yes"}, backends=["haiku"], **_paths(tmp_path)
    )
    by_id = ask_scripted(
        result.session_id,
        "why?",
        scripted_answers={"relevant": "yes"},
        project="by-id",
        **_paths(tmp_path),
    )
    assert by_id["backend"] == "haiku"
    assert by_id["answers"]["relevant"]["choice"] == "yes"
    rows = [
        json.loads(line)["fields"]
        for line in sc_jev_py.log_file(tmp_path / "log").read_text().splitlines()
        if line
    ]
    assert [r["operation"] for r in rows] == ["judge", "ask"]
    assert rows[1]["project"] == "by-id"
    assert rows[1]["session_id"] == result.session_id
    assert repo.is_dir()
    # The general log got the follow-up too.
    assert sc_jev_py.general_log_file(tmp_path / "log").exists()


def test_ask_resumes_a_session_from_another_process(tmp_path: Path) -> None:
    _repo(tmp_path)
    done = subprocess.run(
        [sys.executable, "-c", _CHILD, str(tmp_path)],
        capture_output=True, text=True, timeout=120, check=False,
    )
    assert done.returncode == 0, done.stderr
    session_id = done.stdout.strip()
    asked = ask_scripted(
        session_id, "and now?", scripted_answers={"relevant": "no"}, **_paths(tmp_path)
    )
    assert asked["backend"] == "haiku"
    assert asked["answers"]["relevant"]["choice"] == "no"


@pytest.mark.parametrize(
    "bad", ["", "not-a-ulid", "3f2b8a9e-6c1d-4e7a-9b25-0d4c1a7e5f10", "x" * 300]
)
def test_ask_refuses_non_ulid_ids(tmp_path: Path, bad: str) -> None:
    with pytest.raises(sc_jev_py.JevError) as caught:
        sc_jev_py.ask(bad, "why?", **_paths(tmp_path))
    assert caught.value.code == "INPUT_INVALID"
    assert caught.value.field == "session_id"
    assert len(caught.value.message) < 400


def test_ask_unknown_ulid_is_session_not_found(tmp_path: Path) -> None:
    _repo(tmp_path)
    with pytest.raises(sc_jev_py.JevError) as caught:
        ask_scripted(
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            "why?",
            scripted_answers={"relevant": "yes"},
            **_paths(tmp_path),
        )
    assert caught.value.code == "SESSION_NOT_FOUND"


def test_ask_rejects_a_bad_project_and_a_non_callable_log(tmp_path: Path) -> None:
    with pytest.raises(sc_jev_py.JevError) as caught:
        sc_jev_py.ask("01ARZ3NDEKTSV4RRFFQ69G5FAV", "why?", project="bad\x07", **_paths(tmp_path))
    assert caught.value.code == "INPUT_INVALID"
    with pytest.raises(sc_jev_py.JevError) as caught:
        sc_jev_py.ask("01ARZ3NDEKTSV4RRFFQ69G5FAV", "why?", general_log="no")
    assert caught.value.code == "INPUT_INVALID"
