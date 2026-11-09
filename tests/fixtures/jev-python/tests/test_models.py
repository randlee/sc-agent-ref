"""Short backend names, model pins and the global models file (agt-akp)."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

import sc_jev_py

REQUEST = (
    '{"questions":[{"id":"relevant","prompt":"Does it apply?","choices":["no","yes"]}],'
    '"state":[]}'
)
NEWER_HAIKU = (
    '[[model]]\nid = "claude-haiku-9"\nalias = "haiku"\nreleased = "2099-01-01"\n'
)


def _dry(tmp_path: Path, backends: list[str]):
    return sc_jev_py.run_check(
        request_json=REQUEST,
        backends=backends,
        dry_run=True,
        repo_root=str(tmp_path),
        log_dir=str(tmp_path / "log"),
        session_dir=str(tmp_path / "sessions"),
    )


def _home(monkeypatch: pytest.MonkeyPatch, home: Path) -> None:
    monkeypatch.setenv("HOME", str(home))
    monkeypatch.setenv("USERPROFILE", str(home))


def test_short_names_and_model_ids_select_backends(tmp_path, monkeypatch):
    _home(monkeypatch, tmp_path / "home")
    short = _dry(tmp_path, ["jev", "haiku", "luna"])
    assert set(dict(short.request_sha256)) == {"jev", "haiku", "luna"}
    pinned = _dry(tmp_path, ["claude-haiku-5-5"])
    assert set(dict(pinned.request_sha256)) == {"haiku"}


@pytest.mark.parametrize(
    "name",
    ["sonnet", "claude-sonnet-5-5", "gpt-6.1-sol", "anthropic_haiku", "openai_luna"],
)
def test_other_names_are_input_invalid_and_list_the_valid_ones(tmp_path, name):
    with pytest.raises(sc_jev_py.JevError) as caught:
        _dry(tmp_path, [name])
    assert caught.value.code == "INPUT_INVALID"
    for valid in ("jev", "haiku", "luna", "claude-haiku-5-5"):
        assert valid in caught.value.message


def test_models_file_ids_are_backends_and_unknown_ids_stay_refused(tmp_path, monkeypatch):
    _home(monkeypatch, tmp_path / "home")
    file = tmp_path / "models.toml"
    file.write_text(
        '[[model]]\nid = "jev-1.15.0"\nalias = "jev"\nreleased = "2026-11-01"\n'
        '[[model]]\nid = "claude-haiku-5-6"\nalias = "haiku"\nreleased = "2026-11-02"\n'
        '[[model]]\nid = "claude-sonnet-6"\nalias = "sonnet"\nreleased = "2026-11-02"\n',
        encoding="utf-8",
    )

    def dry(backends):
        return sc_jev_py.run_check(
            request_json=REQUEST,
            backends=backends,
            models_file=str(file),
            dry_run=True,
            repo_root=str(tmp_path),
            log_dir=str(tmp_path / "log"),
            session_dir=str(tmp_path / "sessions"),
        )

    accepted = dry(["jev-1.15.0", "claude-haiku-5-6"])
    assert set(dict(accepted.request_sha256)) == {"jev", "haiku"}
    for refused in ("jev-9.9.9", "claude-sonnet-6", "claude-haiku-9"):
        with pytest.raises(sc_jev_py.JevError) as caught:
            dry([refused])
        assert caught.value.code == "INPUT_INVALID"
        assert "jev-1.15.0" in caught.value.message
    # Without the file the same id is refused.
    with pytest.raises(sc_jev_py.JevError) as caught:
        _dry(tmp_path, ["jev-1.15.0"])
    assert caught.value.code == "INPUT_INVALID"


@pytest.mark.parametrize("joined", ["jev,haiku", "haiku, luna", " jev"])
def test_a_backends_item_is_one_name_and_is_never_split_on_commas(tmp_path, joined):
    # `--backend jev,haiku` is refused by the CLI; a list item is refused alike.
    with pytest.raises(sc_jev_py.JevError) as caught:
        _dry(tmp_path, [joined])
    assert caught.value.code == "INPUT_INVALID"
    assert joined in caught.value.message


def test_malformed_models_file_with_backends_leaves_the_warning_row(tmp_path, monkeypatch):
    _home(monkeypatch, tmp_path / "home")
    bad = tmp_path / "bad.toml"
    bad.write_text('[[model]]\nid = "x"\nalias = "nope"\nreleased = "2099-01-01"\n', encoding="utf-8")
    (tmp_path / "check.toml").write_text(
        '[questions]\npath = "questions.json"\n', encoding="utf-8"
    )
    (tmp_path / "questions.json").write_text(
        '{"schema":"plan-anchoring/questions/1","version":1,"questions":{"relevant":'
        '{"type":"choice","instructions":"Applies?","criteria":{"no":"No.","yes":"Yes."}}}}',
        encoding="utf-8",
    )
    for backends in (["haiku"], None):
        logs = tmp_path / ("with" if backends else "without")
        kwargs = {"backends": backends} if backends else {}
        with pytest.raises(sc_jev_py.JevError) as caught:
            sc_jev_py.run_check(
                "check.toml",
                models_file=str(bad),
                repo_root=str(tmp_path),
                log_dir=str(logs),
                session_dir=str(logs / "sessions"),
                **kwargs,
            )
        assert caught.value.code == "CONFIG_INVALID"
        path = sc_jev_py.general_log_file(logs)
        rows = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]
        assert "models.file_invalid" in {row["action"] for row in rows}, backends


def test_models_lists_crate_and_file_rows(tmp_path):
    file = tmp_path / "models.toml"
    file.write_text(
        '[[model]]\nid = "claude-haiku-9"\nalias = "haiku"\nreleased = "2099-01-01"\n',
        encoding="utf-8",
    )
    listing = sc_jev_py.models(models_file=file)
    assert listing["models_file_present"] is True
    rows = {row["alias"]: row for row in listing["models"]}
    assert len(rows) == 9
    assert (rows["haiku"]["model"], rows["haiku"]["source"]) == ("claude-haiku-9", "file")
    assert rows["haiku"]["backend"] == "haiku"
    assert (rows["jev"]["model"], rows["jev"]["source"]) == ("jev-1.14.0", "crate")
    assert rows["sonnet"]["backend"] is None

    absent = sc_jev_py.models(models_file=tmp_path / "none.toml")
    assert absent["models_file_present"] is False
    assert {row["source"] for row in absent["models"]} == {"crate"}


def test_malformed_models_file_is_config_invalid(tmp_path):
    file = tmp_path / "models.toml"
    file.write_text('[[model]]\nid = "x"\nalias = "nope"\nreleased = "2099-01-01"\n')
    with pytest.raises(sc_jev_py.JevError) as caught:
        sc_jev_py.models(models_file=file)
    assert caught.value.code == "CONFIG_INVALID"


def test_run_check_reads_the_home_models_file(tmp_path, monkeypatch):
    home = tmp_path / "home"
    (home / ".sc-agent").mkdir(parents=True)
    _home(monkeypatch, home)
    before = dict(_dry(tmp_path, ["haiku"]).request_sha256)["haiku"]
    (home / ".sc-agent" / "models.toml").write_text(NEWER_HAIKU, encoding="utf-8")
    after = dict(_dry(tmp_path, ["haiku"]).request_sha256)["haiku"]
    assert before != after
    # A pin beats the file row.
    pinned = dict(_dry(tmp_path, ["claude-haiku-5-5"]).request_sha256)["haiku"]
    assert pinned == before
    # `jev` is the latest; a file row for it changes nothing, a pin does.
    latest = dict(_dry(tmp_path, ["jev"]).request_sha256)["jev"]
    assert dict(_dry(tmp_path, ["jev-1.14.0"]).request_sha256)["jev"] != latest
    (home / ".sc-agent" / "models.toml").write_text("[[model]]\nid = 1\n")
    with pytest.raises(sc_jev_py.JevError) as caught:
        _dry(tmp_path, ["jev"])
    assert caught.value.code == "CONFIG_INVALID"


def test_a_model_of_another_family_is_config_invalid(tmp_path):
    file = tmp_path / "models.toml"
    file.write_text('[[model]]\nid = "gpt-6-luna"\nalias = "haiku"\nreleased = "2099-01-01"\n')
    with pytest.raises(sc_jev_py.JevError) as caught:
        sc_jev_py.models(models_file=file)
    assert caught.value.code == "CONFIG_INVALID"


def _file(tmp_path: Path, text: str) -> Path:
    path = tmp_path / "custom-models.toml"
    path.write_text(text, encoding="utf-8")
    return path


def test_models_file_kwarg_on_run_check_and_build_request(tmp_path, monkeypatch):
    _home(monkeypatch, tmp_path / "home")  # no file at HOME
    file = _file(tmp_path, NEWER_HAIKU)
    plain = dict(_dry(tmp_path, ["haiku"]).request_sha256)["haiku"]
    with_file = sc_jev_py.run_check(
        request_json=REQUEST,
        backends=["haiku"],
        dry_run=True,
        repo_root=str(tmp_path),
        log_dir=str(tmp_path / "log"),
        session_dir=str(tmp_path / "sessions"),
        models_file=file,
    )
    assert dict(with_file.request_sha256)["haiku"] != plain
    # build_request builds for the default backend, `jev`, which is the
    # latest whatever the file says: the kwarg is accepted and changes nothing.
    built = sc_jev_py.build_request(
        request_json=REQUEST, repo_root=str(tmp_path), models_file=file
    )
    plain_built = sc_jev_py.build_request(request_json=REQUEST, repo_root=str(tmp_path))
    assert built["request_sha256"] == plain_built["request_sha256"]


def test_models_file_kwarg_malformed_is_config_invalid(tmp_path):
    bad = _file(tmp_path, "[[model]]\nid = 1\n")
    with pytest.raises(sc_jev_py.JevError) as caught:
        sc_jev_py.run_check(
            request_json=REQUEST, dry_run=True, repo_root=str(tmp_path), models_file=bad
        )
    assert caught.value.code == "CONFIG_INVALID"
    with pytest.raises(sc_jev_py.JevError) as caught:
        sc_jev_py.build_request(request_json=REQUEST, repo_root=str(tmp_path), models_file=bad)
    assert caught.value.code == "CONFIG_INVALID"
    for start in (
        lambda: sc_jev_py.DecisionStream(
            repo_root=tmp_path, log_dir=tmp_path / "l", models_file=bad
        ),
        lambda: sc_jev_py.run_many(
            {}, repo_root=tmp_path, log_dir=tmp_path / "l", models_file=bad
        ),
    ):
        with pytest.raises(sc_jev_py.JevError) as caught:
            start()
        assert caught.value.code == "CONFIG_INVALID"


def test_models_file_kwarg_on_a_good_stream_starts(tmp_path):
    good = _file(tmp_path, NEWER_HAIKU)
    stream = sc_jev_py.DecisionStream(
        repo_root=tmp_path, log_dir=tmp_path / "l", models_file=good
    )
    stream.close_input()
    stream.finish()
