"""REQ-SAG-0726: `DecisionStream` and `run_many` (ADR-SAG-0032).

Scripted backends only: no network, no API keys. Every wait is bounded and
no test asserts on elapsed time.
"""

from __future__ import annotations

import sys
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


def _read_rows(path: Path) -> list[dict]:
    import json

    if not path.exists():
        return []
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]


def test_a_stream_takes_session_and_general_log_callables(tmp_path: Path) -> None:
    general: list[dict] = []
    session: list[dict] = []
    with _stream(tmp_path, general_log=general.append, session_log=session.append) as stream:
        assert stream.submit("a", REQUEST)
        stream.close_input()
        assert [a["id"] for a in stream] == ["a"]
        stream.finish()
    actions = [event["action"] for event in general]
    for expected in ("stream.start", "stream.request.start", "run.done", "stream.end"):
        assert expected in actions, actions
    assert len(session) == 1, session
    started = next(e for e in general if e["action"] == "stream.request.start")
    assert started["fields"]["run_id"] != next(
        e for e in general if e["action"] == "stream.start"
    )["fields"]["run_id"]


def test_the_default_general_log_is_the_shared_host_logger_file(tmp_path: Path) -> None:
    with _stream(tmp_path) as stream:
        assert stream.submit("a", REQUEST)
        stream.close_input()
        assert len(list(stream)) == 1
        stream.finish()
    rows = _read_rows(sc_jev_py.general_log_file(tmp_path / "log"))
    actions = [row["action"] for row in rows]
    assert "stream.start" in actions and "stream.end" in actions, actions
    assert actions.count("stream.end") == 1


def test_a_repeated_finish_writes_one_stream_end(tmp_path: Path) -> None:
    stream = _stream(tmp_path)
    stream.close_input()
    first = stream.finish()
    second = stream.finish()
    assert first["answered"] == second["answered"] == 0
    rows = _read_rows(sc_jev_py.general_log_file(tmp_path / "log"))
    assert [r["action"] for r in rows].count("stream.end") == 1


_EXIT_CHILD = r"""
import os, signal, sys, threading
from pathlib import Path
import sc_jev_py

work = Path(sys.argv[1]); mode = sys.argv[2]; workers = int(sys.argv[3])
repo = work / "repo"; repo.mkdir()
request = {"questions": [{"id": "q", "prompt": "p", "choices": ["a", "b"]}]}
common = dict(scripted_answers={"q": "a"}, repo_root=repo, log_dir=work / "log",
              session_dir=work / "sessions", concurrency=workers)

def with_exit():
    stream = sc_jev_py._testing.decision_stream_scripted(**common)
    with stream:
        for i in range(stream.capacity):
            stream.submit(f"r{i}", request, timeout=0.05)

def plain_iteration():
    stream = sc_jev_py._testing.decision_stream_scripted(**common)
    for i in range(stream.capacity):
        stream.submit(f"r{i}", request, timeout=0.05)
    stream.close_input()
    for _ in stream:
        pass

def interrupted_run_many():
    # Interrupt only once the stream is running (its `stream.start` row is on
    # disk), so the test is about leaving a started stream and not about how
    # fast the process reaches it. `raise_signal` works on every platform;
    # `os.kill(pid, SIGINT)` terminates the process on Windows.
    import time
    log = sc_jev_py.general_log_file(work / "log")

    def interrupt():
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            try:
                if "stream.start" in log.read_text(encoding="utf-8"):
                    break
            except OSError:
                pass
            time.sleep(0.001)
        signal.raise_signal(signal.SIGINT)

    threading.Thread(target=interrupt, daemon=True).start()
    try:
        sc_jev_py._testing.run_many_scripted({f"r{i}": request for i in range(20000)}, **common)
    except KeyboardInterrupt:
        pass

try:
    {"with": with_exit, "iter": plain_iteration, "sigint": interrupted_run_many}[mode]()
except KeyboardInterrupt:
    pass
"""


@pytest.mark.parametrize("attempt", range(8))
@pytest.mark.parametrize("mode", ["with", "iter", "sigint"])
@pytest.mark.parametrize("workers", [2, 8])
def test_every_way_out_of_a_stream_leaves_its_rows_on_disk(
    tmp_path: Path, mode: str, workers: int, attempt: int
) -> None:
    """A fresh process that leaves a busy stream and exits at once still has
    `stream.end` and the session flush row, and every row is whole JSON."""
    import subprocess
    import sys

    work = tmp_path / f"{mode}-{workers}-{attempt}"
    work.mkdir()
    done = subprocess.run(
        [sys.executable, "-c", _EXIT_CHILD, str(work), mode, str(workers)],
        capture_output=True, text=True, timeout=120, check=False,
    )
    assert done.returncode == 0, done.stderr
    rows = _read_rows(sc_jev_py.general_log_file(work / "log"))
    actions = [row["action"] for row in rows]
    assert actions.count("stream.start") == 1, actions[:5]
    assert actions.count("stream.end") == 1, actions[-5:]
    assert "session_log.flushed" in actions
    _read_rows(sc_jev_py.log_file(work / "log"))  # every session row is whole JSON


_RATE_CHILD = r"""
import json, sys, threading
from pathlib import Path
import sc_jev_py

work = Path(sys.argv[1]); mode = sys.argv[2]; count = int(sys.argv[3]); py_rows = int(sys.argv[4])
repo = work / "repo"; repo.mkdir()
request = {"questions": [{"id": "q", "prompt": "p", "choices": ["a", "b"]}]}
stream = sc_jev_py._testing.decision_stream_scripted(
    scripted_answers={"q": "a"}, repo_root=repo, log_dir=work / "log",
    session_dir=work / "sessions", concurrency=16)

admitted = []
def python_rows(k):
    import time
    import sc_observability as so
    logger = so.get_host_logger().value
    done = 0
    for i in range(py_rows):
        for _attempt in range(10000):   # bounded retry: a full queue is not an error
            result = logger.log(so.LogEvent(level="info", target="py.rate", action="py.row",
                                            message="m", fields={"k": k, "i": i}))
            if isinstance(result, so.Ok):
                done += 1
                break
            time.sleep(0.0005)
    admitted.append(done)

def feed():
    for i in range(count):
        stream.submit(f"r{i}", request)
    stream.close_input()

threads = [threading.Thread(target=feed)]
threads += [threading.Thread(target=python_rows, args=(k,)) for k in range(2 if py_rows else 0)]
for t in threads: t.start()
summary = None
if mode == "with":
    with stream:
        answered = sum(1 for _ in stream)
else:
    answered = sum(1 for _ in stream)
    if mode == "finish":
        summary = stream.finish()
for t in threads: t.join()
assert answered == count, answered
print(json.dumps({"summary": summary, "admitted": admitted}))
"""


@pytest.mark.parametrize("mode", ["finish", "with", "iter"])
def test_a_stream_at_full_rate_keeps_every_general_log_row(tmp_path: Path, mode: str) -> None:
    """3,000 requests at concurrency 16 in a fresh process: no general-log row
    is lost, however the stream is left. Every request's run has the same
    number of rows, there is one run per request, and the stream's own start
    and end rows are present. (A full runtime queue used to drop about 80% of
    the rows with no warning.)"""
    import json
    import subprocess
    import sys

    count = 3000
    done = subprocess.run(
        [sys.executable, "-c", _RATE_CHILD, str(tmp_path), mode, str(count), "0"],
        capture_output=True, text=True, timeout=300, check=False,
    )
    assert done.returncode == 0, done.stderr
    if mode == "finish":
        report = json.loads(done.stdout.strip().splitlines()[-1])["summary"]
        assert report["general_log_rows_dropped"] == 0
        assert report["log_flush_warning"] is None
    rows = _read_rows(sc_jev_py.general_log_file(tmp_path / "log"))
    actions = [row["action"] for row in rows]
    assert actions.count("stream.start") == 1 and actions.count("stream.end") == 1
    assert "general_log.rows_dropped" not in actions
    by_run: dict[str, int] = {}
    for row in rows:
        run_id = row["fields"].get("run_id")
        by_run[run_id] = by_run.get(run_id, 0) + 1
    request_runs = {
        row["fields"]["run_id"] for row in rows if row["action"] == "stream.request.start"
    }
    assert len(request_runs) == count
    sizes = {by_run[run_id] for run_id in request_runs}
    assert len(sizes) == 1, f"requests have different row counts: {sorted(sizes)}"
    for action in ("stream.request.done", "run.start", "run.done", "send.done"):
        assert actions.count(action) == count, action


def test_python_rows_on_the_shared_logger_are_not_starved(tmp_path: Path) -> None:
    """Two Python threads log through `get_host_logger()` into the file the
    stream writes at full rate: every Python row is admitted (with bounded
    retries) and every one is on disk, and no stream row is lost."""
    import json
    import subprocess
    import sys

    pytest.importorskip("sc_observability")
    count, py_rows = 3000, 500
    done = subprocess.run(
        [sys.executable, "-c", _RATE_CHILD, str(tmp_path), "finish", str(count), str(py_rows)],
        capture_output=True, text=True, timeout=300, check=False,
    )
    assert done.returncode == 0, done.stderr
    report = json.loads(done.stdout.strip().splitlines()[-1])
    assert report["admitted"] == [py_rows, py_rows]
    rows = _read_rows(sc_jev_py.general_log_file(tmp_path / "log"))
    actions = [row["action"] for row in rows]
    assert actions.count("py.row") == 2 * py_rows
    assert actions.count("stream.request.done") == count
    assert actions.count("run.done") == count


def test_streams_take_models_file(tmp_path: Path) -> None:
    """`models_file=` reaches a stream as it reaches `run_check`: a malformed
    file is `CONFIG_INVALID`, a missing one is fine (no file, no pins)."""
    bad = tmp_path / "models.toml"
    bad.write_text("this is = = not toml", encoding="utf-8")
    repo = tmp_path / "repo"
    repo.mkdir()
    with pytest.raises(sc_jev_py.JevError) as caught:
        sc_jev_py.DecisionStream(repo_root=repo, log_dir=tmp_path / "log", models_file=bad)
    assert caught.value.code == "CONFIG_INVALID"
    with pytest.raises(sc_jev_py.JevError) as caught:
        sc_jev_py.run_many([("a", REQUEST)], repo_root=repo, log_dir=tmp_path / "log2",
                           models_file=bad)
    assert caught.value.code == "CONFIG_INVALID"
    stream = sc_jev_py.DecisionStream(
        repo_root=repo, log_dir=tmp_path / "log3", models_file=tmp_path / "absent.toml"
    )
    stream.close_input()
    assert stream.finish()["answered"] == 0


_STUCK_CHILD = r"""
import json, os, sys, threading, time
from pathlib import Path
import sc_jev_py

work = Path(sys.argv[1]); repo = work / "repo"; repo.mkdir()
log_file = sc_jev_py.general_log_file(work / "log")
log_file.parent.mkdir(parents=True)
os.mkfifo(log_file)
reader = os.open(log_file, os.O_RDONLY | os.O_NONBLOCK)   # opened, never read: a stuck disk
request = {"questions": [{"id": "q", "prompt": "p", "choices": ["a", "b"]}]}
stream = sc_jev_py._testing.decision_stream_scripted(
    scripted_answers={"q": "a"}, repo_root=repo, log_dir=work / "log",
    session_dir=work / "sessions", concurrency=16)

def feed():
    try:
        for i in range(3000):
            stream.submit(f"r{i}", request)
    except Exception:
        pass
def read():
    try:
        for _ in stream:
            pass
    except Exception:
        pass
threading.Thread(target=feed, daemon=True).start()
threading.Thread(target=read, daemon=True).start()
# Let the writer fill the pipe and the workers start waiting for log room.
time.sleep(2.0)
started = time.monotonic()
stream.cancel()
summary = stream.finish()
print(json.dumps({"seconds": time.monotonic() - started, "dropped": summary["general_log_rows_dropped"],
                  "answered": summary["answered"]}), flush=True)
os._exit(0)
"""


@pytest.mark.skipif(sys.platform == "win32", reason="needs a FIFO as the log file")
def test_cancel_stops_waiting_for_log_room_on_a_stuck_disk(tmp_path: Path) -> None:
    """The log file is a FIFO nobody reads, so the writer stalls and the
    queue stays full. After `cancel()`, `finish()` must not wait out the row
    wait of every worker: it returns promptly and reports the rows it gave up
    on (it used to take 18 to 19 s)."""
    import json
    import subprocess

    done = subprocess.run(
        [sys.executable, "-c", _STUCK_CHILD, str(tmp_path)],
        capture_output=True, text=True, timeout=120, check=False,
    )
    assert done.returncode == 0, done.stderr
    report = json.loads(done.stdout.strip().splitlines()[-1])
    assert report["dropped"] > 0, report
    # A failure bound, not a measurement: the flush of a stuck writer takes
    # its own 5 s wait (about 8 s measured); without the cancel it also waits
    # out the 10 s row wait of every worker.
    assert report["seconds"] < 12, report
