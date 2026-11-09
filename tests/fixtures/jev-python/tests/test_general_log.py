"""The general log beside the session log (ADR-SAG-0013, 2026-11-09).

Two separate files, both injectable, with sc-agent defaults. Rust and Python
rows land in one general log through the shared host logger, and a key or
prompt text never reaches it.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

import sc_jev_py

run_check_scripted = sc_jev_py._testing.run_check_scripted

PROMPT_TEXT = "PROMPT-TEXT-SECRET-5521"
KEY_TEXT = "sk-test-SECRET-KEY-9917"


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
                        "instructions": PROMPT_TEXT,
                        "criteria": {"no": "No.", "yes": "Yes."},
                    }
                },
            }
        ),
        encoding="utf-8",
    )
    (repo / "check.toml").write_text('[questions]\npath = "questions.json"\n', encoding="utf-8")
    return repo


def _rows(path: Path) -> list[dict]:
    if not path.exists():
        return []
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]


def _run(repo: Path, log_dir: Path, **kwargs):
    return run_check_scripted(
        "check.toml",
        scripted_answers={"relevant": "yes"},
        backends=["anthropic_haiku"],
        repo_root=repo,
        log_dir=log_dir,
        session_dir=log_dir / "sessions",
        **kwargs,
    )


@pytest.fixture(autouse=True)
def _no_provider_keys(monkeypatch: pytest.MonkeyPatch) -> None:
    for name in ("TYPESAFE_API_KEY", "ANTHROPIC_API_KEY", "OPENAI_API_KEY"):
        monkeypatch.delenv(name, raising=False)


def test_default_loggers_write_two_separate_files(tmp_path: Path) -> None:
    repo = _repo(tmp_path)
    logs = tmp_path / "log"
    _run(repo, logs)
    session = _rows(sc_jev_py.log_file(logs))
    general_path = sc_jev_py.general_log_file(logs)
    general = _rows(general_path)
    assert sc_jev_py.log_file(logs) != general_path
    assert len(session) == 1
    actions = {row["action"] for row in general}
    assert {"run.start", "run.done", "prepare.done", "send.done"} <= actions
    text = general_path.read_text(encoding="utf-8")
    assert PROMPT_TEXT not in text
    assert KEY_TEXT not in text


def test_injected_loggers_replace_the_defaults(tmp_path: Path) -> None:
    repo = _repo(tmp_path)
    logs = tmp_path / "log"
    sessions: list[dict] = []
    events: list[dict] = []
    _run(repo, logs, session_log=sessions.append, general_log=events.append)
    assert len(sessions) == 1
    assert sessions[0]["operation"] == "judge"
    assert sessions[0]["fields"]["backend"] == "anthropic_haiku"
    assert {"run.start", "run.done"} <= {event["action"] for event in events}
    assert all(event["level"] in {"debug", "info", "warn", "error"} for event in events)
    assert PROMPT_TEXT not in json.dumps(events)
    assert not sc_jev_py.log_file(logs).exists()
    assert not sc_jev_py.general_log_file(logs).exists()


def test_a_raising_logger_changes_nothing(tmp_path: Path) -> None:
    repo = _repo(tmp_path)

    def boom(_event: dict) -> None:
        raise RuntimeError("logger exploded")

    plain = _run(repo, tmp_path / "a")
    noisy = _run(repo, tmp_path / "b", session_log=boom, general_log=boom)
    assert plain.prompt_sha256 == noisy.prompt_sha256
    assert [o["backend"] for o in plain.outcomes] == [o["backend"] for o in noisy.outcomes]


def test_a_non_callable_logger_is_input_invalid(tmp_path: Path) -> None:
    repo = _repo(tmp_path)
    with pytest.raises(sc_jev_py.JevError) as caught:
        _run(repo, tmp_path / "log", general_log="not callable")
    assert caught.value.code == "INPUT_INVALID"


def test_rust_and_python_rows_land_in_one_log(tmp_path: Path) -> None:
    import sc_observability  # pinned in constraints.txt; a missing wheel must fail

    repo = _repo(tmp_path)
    installed = sc_jev_py.install_host_logger(tmp_path / "host-log")
    root = installed.parent.parent
    attached = sc_observability.get_host_logger()
    assert isinstance(attached, sc_observability.Ok), attached
    logger = attached.value
    marker = sc_observability.LogEvent(
        level="info",
        target="python.test",
        action="python.row",
        message="written from python",
        fields={"origin": "python"},
    )
    assert isinstance(logger.log(marker), sc_observability.Ok)
    _run(repo, root)
    assert isinstance(logger.flush(), sc_observability.Ok)
    rows = _rows(installed)
    actions = [row["action"] for row in rows]
    assert "python.row" in actions
    assert "run.done" in actions
    assert installed == sc_jev_py.general_log_file(root)
    assert sc_jev_py.install_host_logger(tmp_path / "other") == installed
    assert PROMPT_TEXT not in installed.read_text(encoding="utf-8")
