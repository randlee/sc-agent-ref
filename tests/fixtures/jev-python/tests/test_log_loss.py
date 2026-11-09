"""Audit rows are never lost silently, and log files are owner-only.

A read-only `logs/` directory (or a full disk) used to lose every session-log
row with exit 0, an empty stderr and no warning. The run keeps its answer; the
loss is a `RuntimeWarning` (stderr by default) and an entry in
`result.data["warnings"]`.
"""

from __future__ import annotations

import json
import os
import stat
import warnings
from pathlib import Path

import pytest

import sc_jev_py

run_check_scripted = sc_jev_py._testing.run_check_scripted

posix_only = pytest.mark.skipif(os.name != "posix", reason="POSIX permissions")


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


def _run(repo: Path, log_dir: Path, **kwargs):
    return run_check_scripted(
        "check.toml",
        scripted_answers={"relevant": "yes"},
        backends=["haiku"],
        repo_root=repo,
        log_dir=log_dir,
        session_dir=log_dir / "sessions",
        **kwargs,
    )


@pytest.fixture(autouse=True)
def _no_provider_keys(monkeypatch: pytest.MonkeyPatch) -> None:
    for name in ("TYPESAFE_API_KEY", "ANTHROPIC_API_KEY", "OPENAI_API_KEY"):
        monkeypatch.delenv(name, raising=False)


def _mode(path: Path) -> int:
    return stat.S_IMODE(path.stat().st_mode)


@posix_only
def test_an_unwritable_log_dir_is_a_warning_and_the_run_keeps_its_answer(tmp_path: Path) -> None:
    repo = _repo(tmp_path)
    logs = tmp_path / "log"
    (logs / "logs").mkdir(parents=True)
    (logs / "logs").chmod(0o555)
    try:
        probe = logs / "logs" / "probe"
        try:
            probe.write_text("x")
        except PermissionError:
            pass
        else:
            probe.unlink()
            pytest.skip("this process can write to a read-only directory (root)")
        with pytest.warns(RuntimeWarning, match=r"row\(s\) could not be written"):
            result = _run(repo, logs)
    finally:
        (logs / "logs").chmod(0o700)
    assert result.outcomes[0]["backend"] == "haiku"
    warnings_ = result.data["warnings"]
    assert len(warnings_) == 1, warnings_
    assert "row(s) could not be written" in warnings_[0]
    assert not sc_jev_py.log_file(logs).exists() or sc_jev_py.log_file(logs).stat().st_size == 0


def test_a_healthy_run_warns_about_nothing(tmp_path: Path) -> None:
    repo = _repo(tmp_path)
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        result = _run(repo, tmp_path / "log")
    assert "warnings" not in result.data


@posix_only
def test_log_dirs_and_files_this_call_creates_are_owner_only(tmp_path: Path) -> None:
    repo = _repo(tmp_path)
    logs = tmp_path / "new" / "log"
    _run(repo, logs)
    assert _mode(logs) == 0o700
    assert _mode(logs / "logs") == 0o700
    assert _mode(sc_jev_py.log_file(logs)) == 0o600
    assert _mode(sc_jev_py.general_log_file(logs)) == 0o600
    assert _mode(logs / "sessions") == 0o700
