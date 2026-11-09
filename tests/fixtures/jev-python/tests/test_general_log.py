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
    # Every row of the run shares one run id, and the run reports its time.
    run_ids = {row["fields"]["run_id"] for row in general}
    assert len(run_ids) == 1
    done = next(row for row in general if row["action"] == "run.done")
    assert isinstance(done["fields"]["duration_ms"], int)
    assert len(done["fields"]["session_id"]) == 26


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
    assert plain.payload_sha256 == noisy.payload_sha256
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


def test_a_real_key_reaching_a_send_path_never_reaches_the_general_log(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """The key is in the environment and is resolved for the backend; the
    request is over the backend's byte cap, so the run fails in validation
    before any send (no network). Neither the key nor the prompt text may be in
    the log."""
    monkeypatch.setenv("ANTHROPIC_API_KEY", KEY_TEXT)
    repo = tmp_path / "repo"
    repo.mkdir()
    (repo / "questions.json").write_text(
        json.dumps(
            {
                "schema": "plan-anchoring/questions/1",
                "version": 1,
                "questions": {
                    "relevant": {
                        "type": "choice",
                        "instructions": PROMPT_TEXT + " {big}",
                        "criteria": {"no": "No.", "yes": "Yes."},
                    }
                },
            }
        ),
        encoding="utf-8",
    )
    (repo / "big.txt").write_text("x" * 200_000, encoding="utf-8")
    (repo / "check.toml").write_text(
        '[questions]\npath = "questions.json"\n\n[inputs.big]\nkind = "file"\npath = "big.txt"\n',
        encoding="utf-8",
    )
    logs = tmp_path / "log"
    with pytest.raises(sc_jev_py.JevError) as caught:
        sc_jev_py.run_check(
            "check.toml",
            backends=["anthropic_haiku"],
            repo_root=repo,
            log_dir=logs,
            session_dir=logs / "sessions",
        )
    assert caught.value.code == "INPUT_TOO_LARGE"
    path = sc_jev_py.general_log_file(logs)
    text = path.read_text(encoding="utf-8")
    rows = _rows(path)
    credential = next(row for row in rows if row["action"] == "credential.resolved")
    assert credential["fields"]["source"] == "env"
    failed = next(row for row in rows if row["action"] == "run.failed")
    assert failed["fields"]["stage"] == "validate"
    assert failed["fields"]["backend"] == "anthropic_haiku"
    assert failed["fields"]["error_code"] == "INPUT_TOO_LARGE"
    assert KEY_TEXT not in text
    assert PROMPT_TEXT not in text


_CHILD = r'''
import json, sys, threading
from pathlib import Path
import sc_jev_py

tmp = Path(sys.argv[1]); root = tmp / "log"; n = int(sys.argv[2])
errors = []
def repo(i):
    r = tmp / f"r{i}"; r.mkdir()
    (r / "questions.json").write_text(json.dumps({"schema": "plan-anchoring/questions/1", "version": 1,
        "questions": {"relevant": {"type": "choice", "instructions": "Q?", "criteria": {"no": "No.", "yes": "Yes."}}}}))
    (r / "check.toml").write_text('[questions]\npath = "questions.json"\n')
    return r
repos = [repo(i) for i in range(n)]
barrier = threading.Barrier(n)
def go(i):
    barrier.wait()
    try:
        sc_jev_py._testing.run_check_scripted("check.toml", scripted_answers={"relevant": "yes"},
            backends=["anthropic_haiku"], repo_root=repos[i], log_dir=root, session_dir=root / f"s{i}")
    except Exception as error:
        errors.append(repr(error))
threads = [threading.Thread(target=go, args=(i,)) for i in range(n)]
[t.start() for t in threads]; [t.join() for t in threads]
assert not errors, errors
# Exit immediately: every row must already be on disk.
'''


@pytest.mark.parametrize(
    ("runs", "attempt"),
    [(2, n) for n in range(20)] + [(3, n) for n in range(20)] + [(8, n) for n in range(5)],
)
def test_returned_runs_are_on_disk_when_the_process_exits_at_once(
    tmp_path: Path, runs: int, attempt: int
) -> None:
    """Two or three concurrent runs in a fresh process that exits right after
    they return: every run's last rows (`session.saved`, `run.done`) are
    present. Concurrent flushes used to be refused and ignored, losing tail
    rows; two and three threads are where that showed (eight did not), so the
    small counts are repeated 20 times each. With the flush fix reverted, this
    test fails (see the commit message)."""
    import subprocess
    import sys

    work = tmp_path / f"w{runs}-{attempt}"
    work.mkdir()
    done = subprocess.run(
        [sys.executable, "-c", _CHILD, str(work), str(runs)],
        capture_output=True, text=True, timeout=120, check=False,
    )
    assert done.returncode == 0, done.stderr
    rows = _rows(sc_jev_py.general_log_file(work / "log"))
    by_run: dict[str, set[str]] = {}
    for row in rows:
        by_run.setdefault(row["fields"].get("run_id"), set()).add(row["action"])
    complete = [r for r, a in by_run.items() if {"run.start", "run.done", "session.saved"} <= a]
    assert len(complete) == runs, {r: sorted(a) for r, a in by_run.items()}
