"""REQ-SAG-0726: `DecisionStream` and `run_many` (ADR-SAG-0032).

Scripted backends only: no network, no API keys. Every wait is bounded and
no test asserts on elapsed time.
"""

from __future__ import annotations

import threading
from pathlib import Path

import pytest

import sc_jev_py

REQUEST = {"questions": [{"id": "q", "prompt": "p", "choices": ["a", "b"]}]}
ANSWERS = {"q": "a"}
FAILSAFE = 60.0


def _stream(tmp: Path, **kwargs):
    repo = tmp / "repo"
    repo.mkdir(exist_ok=True)
    return sc_jev_py._testing.decision_stream_scripted(
        scripted_answers=ANSWERS,
        repo_root=repo,
        log_dir=tmp / "log",
        session_dir=tmp / "sessions",
        **kwargs,
    )


def test_run_many_answers_every_id_and_a_bad_request_does_not_raise(tmp_path: Path) -> None:
    repo = tmp_path / "repo"
    repo.mkdir()
    requests = {f"r{i}": REQUEST for i in range(30)}
    requests["bad"] = {"questions": []}
    answers, summary = sc_jev_py._testing.run_many_scripted(
        requests,
        scripted_answers=ANSWERS,
        concurrency=4,
        repo_root=repo,
        log_dir=tmp_path / "log",
        session_dir=tmp_path / "sessions",
    )
    assert sorted(a["id"] for a in answers) == sorted(requests)
    by_id = {a["id"]: a for a in answers}
    assert by_id["r0"]["ok"] is True
    assert by_id["r0"]["envelope"]["operation"] == "jev.run"
    assert by_id["bad"]["ok"] is False
    assert by_id["bad"]["error"]["error"] == "INPUT_INVALID"
    assert by_id["bad"]["error"]["retryable"] is False
    # The summary carries what the CLI's exit code and stderr would.
    assert summary["answered"] == 31
    assert summary["failed"] == 1
    assert summary["exit_code"] == 2
    assert "log_flush_warning" in summary


def test_run_many_accepts_pairs_and_json_text(tmp_path: Path) -> None:
    import json

    repo = tmp_path / "repo"
    repo.mkdir()
    answers, summary = sc_jev_py._testing.run_many_scripted(
        [("a", json.dumps(REQUEST)), ("b", REQUEST)],
        scripted_answers=ANSWERS,
        repo_root=repo,
        log_dir=tmp_path / "log",
        session_dir=tmp_path / "sessions",
    )
    assert sorted(a["id"] for a in answers) == ["a", "b"]
    assert all(a["ok"] for a in answers)
    assert summary["exit_code"] == 0


def test_run_many_validates_every_id_and_body_before_anything_starts(tmp_path: Path) -> None:
    """A bad id after good requests raises, and nothing was sent or logged."""
    repo = tmp_path / "repo"
    repo.mkdir()
    for bad in (("", REQUEST), ("x", "{nope")):
        with pytest.raises(sc_jev_py.JevError) as caught:
            sc_jev_py._testing.run_many_scripted(
                [("first", REQUEST), ("second", REQUEST), bad],
                scripted_answers=ANSWERS,
                repo_root=repo,
                log_dir=tmp_path / "log",
            )
        assert caught.value.code == "INPUT_INVALID"
    # No stream started: no session log was opened, so nothing was sent.
    assert not (tmp_path / "log").exists()


def test_submit_line_takes_the_cli_jsonl_form(tmp_path: Path) -> None:
    import json

    stream = _stream(tmp_path)
    assert stream.submit_line(json.dumps({"id": "l1", "request": REQUEST}))
    assert stream.submit_line("not json")
    assert stream.submit_line(json.dumps({"id": "l2", "request": {"questions": []}}))
    stream.close_input()
    answers = {a["id"]: a for a in stream}
    assert answers["l1"]["ok"] is True
    assert answers["l2"]["ok"] is False
    assert answers["l2"]["error"]["error"] == "INPUT_INVALID"
    assert answers[None]["error"]["error"] == "INPUT_INVALID"


def test_finish_closes_input_and_never_hangs_on_unread_answers(tmp_path: Path) -> None:
    stream = _stream(tmp_path, concurrency=1)
    accepted = sum(
        1 for i in range(stream.capacity) if stream.submit(f"u{i}", REQUEST, timeout=0.05)
    )
    summary = stream.finish()
    assert summary["answered"] == accepted
    with pytest.raises(sc_jev_py.JevError):
        stream.submit("late", REQUEST)


def test_a_single_thread_submitting_past_capacity_times_out(tmp_path: Path) -> None:
    """Unread answers hold capacity: with `timeout=` the caller is told."""
    stream = _stream(tmp_path, concurrency=1)
    total = stream.capacity + 10
    results = [stream.submit(f"p{i}", REQUEST, timeout=0.02) for i in range(total)]
    assert results.count(True) <= stream.capacity
    assert results.count(False) >= 10
    stream.close_input()
    assert sum(1 for _ in stream) == results.count(True)


def test_early_answers_become_follow_up_requests(tmp_path: Path) -> None:
    seen = []
    with _stream(tmp_path, concurrency=2) as stream:
        assert stream.submit("step-0", REQUEST)
        for answer in stream:
            assert answer["ok"]
            seen.append(answer["id"])
            step = int(answer["id"].removeprefix("step-"))
            if step < 3:
                assert stream.submit(f"step-{step + 1}", REQUEST)
            else:
                stream.close_input()
        summary = stream.finish()
    assert seen == ["step-0", "step-1", "step-2", "step-3"]
    assert summary["answered"] == 4
    assert summary["failed"] == 0
    assert summary["exit_code"] == 0


def test_exit_code_in_the_summary_follows_the_stability_rule(tmp_path: Path) -> None:
    stream = _stream(tmp_path)
    assert stream.submit("ok", REQUEST)
    assert stream.submit("bad", {"questions": []})
    stream.close_input()
    answers = list(stream)
    assert {a["id"] for a in answers} == {"ok", "bad"}
    summary = stream.finish()
    assert (summary["answered"], summary["failed"], summary["exit_code"]) == (2, 1, 2)


def test_next_times_out_while_open_and_ends_after_close(tmp_path: Path) -> None:
    stream = _stream(tmp_path)
    with pytest.raises(TimeoutError):
        stream.next(timeout=0.01)
    stream.close_input()
    assert stream.next(timeout=FAILSAFE) is None
    with pytest.raises(sc_jev_py.JevError):
        stream.submit("late", REQUEST)


def test_submit_validates_id_and_json(tmp_path: Path) -> None:
    stream = _stream(tmp_path)
    for bad_id, body in [("", REQUEST), ("a\nb", REQUEST), ("x", "{nope")]:
        with pytest.raises(sc_jev_py.JevError) as caught:
            stream.submit(bad_id, body)
        assert caught.value.code == "INPUT_INVALID"
    assert stream.in_flight == 0
    stream.close_input()


def test_concurrency_is_validated(tmp_path: Path) -> None:
    for bad in (0, 65):
        with pytest.raises(sc_jev_py.JevError) as caught:
            sc_jev_py.DecisionStream(concurrency=bad, repo_root=tmp_path, log_dir=tmp_path / "l")
        assert caught.value.code == "INPUT_INVALID"
        assert caught.value.field == "concurrency"


def test_a_full_queue_applies_backpressure(tmp_path: Path) -> None:
    """With nobody reading, the stream holds at most `capacity` requests."""
    stream = _stream(tmp_path, concurrency=1)
    capacity = stream.capacity
    attempts = 0
    while stream.in_flight < capacity:
        attempts += 1
        assert attempts < 5_000_000, "stream never filled"
        stream.submit(f"b{attempts}", REQUEST, timeout=0.0)
        assert stream.in_flight <= capacity
    assert stream.in_flight == capacity
    assert stream.submit("over", REQUEST, timeout=0.0) is False
    assert stream.in_flight == capacity
    stream.close_input()
    drained = sum(1 for _ in stream)
    assert drained == capacity


def test_cancel_stops_new_work(tmp_path: Path) -> None:
    stream = _stream(tmp_path, concurrency=1)
    stream.cancel()
    with pytest.raises(sc_jev_py.JevError):
        stream.submit("late", REQUEST)
    assert list(stream) == []
    assert stream.finish()["answered"] == 0


def test_the_gil_is_released_while_waiting_for_an_answer(tmp_path: Path) -> None:
    """A thread blocked in `next()` must not stop another thread submitting."""
    stream = _stream(tmp_path)
    entering = threading.Event()
    result: list[object] = []

    def waiter() -> None:
        entering.set()
        result.append(stream.next(timeout=FAILSAFE))

    thread = threading.Thread(target=waiter)
    thread.start()
    assert entering.wait(FAILSAFE)
    assert stream.submit("from-main", REQUEST)
    thread.join(FAILSAFE)
    assert not thread.is_alive()
    answer = result[0]
    assert isinstance(answer, dict) and answer["id"] == "from-main"
    stream.close_input()
    assert stream.next(timeout=FAILSAFE) is None


@pytest.mark.parametrize("bad", [float("nan"), -1.0, -0.5])
def test_a_nan_or_negative_timeout_is_input_invalid(tmp_path: Path, bad: float) -> None:
    stream = _stream(tmp_path)
    for call in (
        lambda: stream.submit("a", REQUEST, timeout=bad),
        lambda: stream.submit_line("{}", timeout=bad),
        lambda: stream.next(timeout=bad),
    ):
        with pytest.raises(sc_jev_py.JevError) as caught:
            call()
        assert caught.value.code == "INPUT_INVALID"
        assert caught.value.field == "timeout"
    assert stream.in_flight == 0
    stream.close_input()


@pytest.mark.parametrize("huge", [float("inf"), 1e19, 1.7976931348623157e308])
def test_an_infinite_or_huge_timeout_means_no_deadline(tmp_path: Path, huge: float) -> None:
    """`inf` and values too large for a deadline must not panic."""
    stream = _stream(tmp_path)
    assert stream.submit("a", REQUEST, timeout=huge) is True
    assert stream.submit_line('{"id":"b","request":{"questions":[]}}', timeout=huge) is True
    got = {stream.next(timeout=huge)["id"], stream.next(timeout=huge)["id"]}
    assert got == {"a", "b"}
    stream.close_input()
    assert stream.next(timeout=huge) is None
