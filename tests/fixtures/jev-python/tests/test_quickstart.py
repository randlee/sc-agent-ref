"""The Quick start files in docs/check-reference.md run through Python."""

from __future__ import annotations

import shutil
from pathlib import Path

import sc_jev_py

QUICKSTART = Path(__file__).resolve().parents[2] / "jev" / "quickstart"


def test_quickstart_dry_runs_from_check_toml_and_request_json(tmp_path: Path) -> None:
    repo = tmp_path / "repo"
    shutil.copytree(QUICKSTART, repo)
    from_check = sc_jev_py.run_check(
        "check.toml",
        dry_run=True,
        repo_root=repo,
        log_dir=tmp_path / "log",
        session_dir=tmp_path / "sessions",
    )
    assert len(from_check.payload_sha256) == 64
    from_request = sc_jev_py.run_check(
        request_json=(repo / "request.json").read_text(encoding="utf-8"),
        backends=["jev"],
        dry_run=True,
        repo_root=repo,
        log_dir=tmp_path / "log",
        session_dir=tmp_path / "sessions",
    )
    assert len(from_request.payload_sha256) == 64
