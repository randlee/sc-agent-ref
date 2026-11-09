"""`fork()` after logging (gunicorn and Celery prefork, `multiprocessing`).

The shared host logger's writer is a thread; a forked child has only the
forking thread, and the first row it logged used to abort the child with
SIGTRAP. The child now starts a writer of its own on the same log root, so it
neither crashes nor loses its rows.
"""

from __future__ import annotations

import json
import os
from pathlib import Path

import pytest

import sc_jev_py

pytestmark = pytest.mark.skipif(not hasattr(os, "fork"), reason="os.fork does not exist here")

run_check_scripted = sc_jev_py._testing.run_check_scripted


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


def _rows(path: Path) -> list[dict]:
    if not path.exists():
        return []
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]


def _run(repo: Path, log_dir: Path):
    return run_check_scripted(
        "check.toml",
        scripted_answers={"relevant": "yes"},
        backends=["haiku"],
        repo_root=repo,
        log_dir=log_dir,
        session_dir=log_dir / "sessions",
    )


@pytest.fixture(autouse=True)
def _no_provider_keys(monkeypatch: pytest.MonkeyPatch) -> None:
    for name in ("TYPESAFE_API_KEY", "ANTHROPIC_API_KEY", "OPENAI_API_KEY"):
        monkeypatch.delenv(name, raising=False)


def _in_child(work) -> int:
    """Runs `work()` in a forked child; returns its wait status."""
    pid = os.fork()
    if pid == 0:
        code = 0
        try:
            work()
        except BaseException:  # noqa: BLE001 - the child reports by exit code
            code = 3
        os._exit(code)
    return os.waitpid(pid, 0)[1]


def _assert_clean_exit(status: int) -> None:
    assert not os.WIFSIGNALED(status), f"the child died with signal {os.WTERMSIG(status)}"
    assert os.WEXITSTATUS(status) == 0


def test_a_child_forked_after_a_run_does_not_crash_and_still_logs(tmp_path: Path) -> None:
    repo = _repo(tmp_path)
    logs = tmp_path / "log"
    _run(repo, logs)  # the parent starts the writer thread

    _assert_clean_exit(_in_child(lambda: _run(repo, logs)))

    _run(repo, logs)  # the parent's writer is untouched
    session = _rows(sc_jev_py.log_file(logs))
    assert len(session) == 3, session
    general = _rows(sc_jev_py.general_log_file(logs))
    done = [row for row in general if row["action"] == "run.done"]
    assert len(done) == 3, [row["action"] for row in general]


def test_a_child_that_logs_through_the_host_logger_does_not_crash(tmp_path: Path) -> None:
    so = pytest.importorskip("sc_observability")
    repo = _repo(tmp_path)
    installed = sc_jev_py.install_host_logger(tmp_path / "host-log")
    root = installed.parent.parent
    _run(repo, root)
    marker = so.LogEvent(
        level="info",
        target="python.test",
        action="python.child_row",
        message="written from a forked child",
        fields={"origin": "child"},
    )

    def child() -> None:
        logger = so.get_host_logger().value
        assert isinstance(logger.log(marker), so.Ok)
        assert isinstance(logger.flush(), so.Ok)

    _assert_clean_exit(_in_child(child))
    actions = [row["action"] for row in _rows(installed)]
    assert "python.child_row" in actions, actions


def test_a_child_forked_before_any_run_works(tmp_path: Path) -> None:
    repo = _repo(tmp_path)
    logs = tmp_path / "log"
    _assert_clean_exit(_in_child(lambda: _run(repo, logs)))
    assert len(_rows(sc_jev_py.log_file(logs))) == 1
