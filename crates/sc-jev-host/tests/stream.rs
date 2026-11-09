//! `DecisionStream` and `serve_jsonl` (REQ-SAG-0720..REQ-SAG-0727,
//! ADR-SAG-0032) with in-process judge doubles. No network, no API keys.
//!
//! The tests are deterministic and bounded: parallelism is proved with a
//! rendezvous (N requests are inside `judge` at the same moment), never with
//! timing; backpressure is proved by counting what the stream holds; every
//! blocking wait has a failsafe deadline that fails the test instead of
//! hanging. No test asserts on elapsed time.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use sc_agent_types::{BackendId, CancelToken, JevError, NetworkErrorKind, RepoRoot, SendTrace};
use sc_decision_schema::test_util::MockJudgeBackend;
use sc_decision_schema::{JudgeBackend, JudgeOutcome, JudgeRequest, QuestionAnswer, QuestionId};
use sc_jev_check::registry::BackendRegistry;
use sc_jev_host::compose::ComposeInputs;
use sc_jev_host::paths::PathPolicy;
use sc_jev_host::stream::{Concurrency, DecisionStream, Poll, SubmitError, serve_jsonl};
use serde_json::{Value, json};
use tempfile::TempDir;

/// Failsafe for waits that must finish; never a measurement.
const FAILSAFE: Duration = Duration::from_secs(60);

const REQUEST: &str = r#"{"questions":[{"id":"q","prompt":"p","choices":["a","b"]}]}"#;

/// Opens when `parties` threads wait on it at once, or after the failsafe.
struct Rendezvous {
    parties: usize,
    state: Mutex<(usize, bool)>,
    changed: Condvar,
}

impl Rendezvous {
    fn new(parties: usize) -> Arc<Self> {
        Arc::new(Self {
            parties,
            state: Mutex::new((0, false)),
            changed: Condvar::new(),
        })
    }

    /// True when `parties` threads were inside at once.
    fn arrive(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.0 += 1;
        if state.0 >= self.parties {
            state.1 = true;
            self.changed.notify_all();
        }
        let (state, _) = self
            .changed
            .wait_timeout_while(state, FAILSAFE, |state| !state.1)
            .unwrap_or_else(PoisonError::into_inner);
        state.1
    }
}

/// A hold that stays shut until `open` is called.
struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
}

impl Gate {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            open: Mutex::new(false),
            changed: Condvar::new(),
        })
    }

    fn open(&self) {
        *self.open.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.changed.notify_all();
    }

    fn wait(&self) {
        let open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = self
            .changed
            .wait_timeout_while(open, FAILSAFE, |open| !*open)
            .unwrap_or_else(PoisonError::into_inner);
    }
}

/// What a test judge does before it answers.
enum Hold {
    None,
    Rendezvous(Arc<Rendezvous>),
    Gate(Arc<Gate>),
}

/// A judge double that holds each call (see [`Hold`]), counts concurrent
/// calls, then answers like [`MockJudgeBackend`].
struct HeldJudge {
    inner: MockJudgeBackend,
    hold: Hold,
    inside: AtomicUsize,
    peak: AtomicUsize,
}

impl HeldJudge {
    fn new(id: BackendId, hold: Hold) -> Arc<Self> {
        Arc::new(Self {
            inner: MockJudgeBackend::new(id, answers()),
            hold,
            inside: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
        })
    }
}

impl JudgeBackend for HeldJudge {
    fn id(&self) -> BackendId {
        self.inner.id()
    }

    fn validate_for_send(&self, request: &JudgeRequest) -> Result<(), JevError> {
        self.inner.validate_for_send(request)
    }

    fn judge(&self, request: &JudgeRequest, trace: &SendTrace) -> Result<JudgeOutcome, JevError> {
        let now = self.inside.fetch_add(1, Ordering::AcqRel) + 1;
        self.peak.fetch_max(now, Ordering::AcqRel);
        let met = match &self.hold {
            Hold::None => true,
            Hold::Rendezvous(rendezvous) => rendezvous.arrive(),
            Hold::Gate(gate) => {
                gate.wait();
                true
            }
        };
        self.inside.fetch_sub(1, Ordering::AcqRel);
        if met {
            self.inner.judge(request, trace)
        } else {
            Err(JevError::network(
                NetworkErrorKind::Timeout,
                "rendezvous not met",
            ))
        }
    }
}

fn answers() -> BTreeMap<QuestionId, QuestionAnswer> {
    serde_json::from_value(json!({"q": {"choice": "a"}})).expect("answers")
}

fn registry(judges: Vec<Arc<dyn JudgeBackend>>) -> Arc<BackendRegistry> {
    let mut registry = BackendRegistry::new();
    for judge in judges {
        registry.register(judge).expect("register");
    }
    Arc::new(registry)
}

fn inputs(repo: &Path, registry: Arc<BackendRegistry>) -> ComposeInputs {
    ComposeInputs::new(repo.to_path_buf(), PathPolicy::cli())
        .repo_root(RepoRoot::new(repo))
        .registry(registry)
}

fn line(id: &str) -> String {
    format!(r#"{{"id":"{id}","request":{REQUEST}}}"#)
}

fn start(repo: &TempDir, registry: Arc<BackendRegistry>, workers: usize) -> Arc<DecisionStream> {
    Arc::new(
        DecisionStream::start(
            inputs(repo.path(), registry),
            Concurrency::new(workers).expect("workers"),
        )
        .expect("start"),
    )
}

/// Drains every answer as parsed output lines.
fn drain(stream: &DecisionStream) -> Vec<Value> {
    let mut lines = Vec::new();
    while let Some(answer) = stream.next_answer() {
        let bytes = answer.to_line().expect("line");
        assert_eq!(bytes.last(), Some(&b'\n'));
        lines.push(serde_json::from_slice(&bytes).expect("json line"));
    }
    lines
}

fn ids(lines: &[Value]) -> BTreeSet<String> {
    lines
        .iter()
        .map(|line| line["id"].as_str().expect("id").to_owned())
        .collect()
}

/// A line source that hands out one line per `fill_buf` and counts how many
/// were consumed, so a test can see how far the reader got.
struct CountingLines {
    lines: Vec<String>,
    next: usize,
    consumed: Arc<AtomicUsize>,
}

impl std::io::Read for CountingLines {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let available = self.fill_buf()?;
        let len = available.len().min(buf.len());
        buf[..len].copy_from_slice(&available[..len]);
        self.consume(len);
        Ok(len)
    }
}

impl BufRead for CountingLines {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        Ok(self
            .lines
            .get(self.next)
            .map_or(&[][..], |line| line.as_bytes()))
    }

    fn consume(&mut self, amount: usize) {
        if let Some(line) = self.lines.get(self.next)
            && amount >= line.len()
        {
            self.next += 1;
            self.consumed.fetch_add(1, Ordering::AcqRel);
        }
    }
}

/// With the default concurrency of 8, eight requests are inside `judge` at
/// the same moment: the rendezvous opens only if all eight overlap.
#[test]
fn the_default_runs_eight_requests_in_parallel() {
    let repo = tempfile::tempdir().expect("repo");
    let judge = HeldJudge::new(BackendId::Jev, Hold::Rendezvous(Rendezvous::new(8)));
    let registry = registry(vec![judge.clone()]);
    let stream = Arc::new(
        DecisionStream::start(inputs(repo.path(), registry), Concurrency::default())
            .expect("start"),
    );
    for index in 0..8 {
        stream
            .submit_line(line(&format!("r{index}")), Some(FAILSAFE))
            .expect("submit");
    }
    stream.close_input();
    let lines = drain(&stream);
    let summary = stream.finish();
    assert_eq!(lines.len(), 8);
    assert!(lines.iter().all(|line| line["ok"] == true), "{lines:?}");
    assert_eq!(ids(&lines), (0..8).map(|i| format!("r{i}")).collect());
    assert_eq!(judge.peak.load(Ordering::Acquire), 8);
    assert_eq!(summary.exit_code(), 0);
    assert_eq!((summary.answered(), summary.failed()), (8, 0));
}

/// Every answer echoes its request's id, whatever order they finish in, and
/// a bad request fails alone: the stream keeps going.
#[test]
fn answers_echo_ids_and_a_failure_does_not_stop_the_stream() {
    let repo = tempfile::tempdir().expect("repo");
    let judge = HeldJudge::new(BackendId::Jev, Hold::None);
    let stream = start(&repo, registry(vec![judge]), 4);
    let input = [
        line("ok-1"),
        "this is not json".to_owned(),
        r#"{"id":"empty","request":{"questions":[]}}"#.to_owned(),
        r#"{"id":"extra-key","request":{"questions":[]},"other":1}"#.to_owned(),
        r#"{"id":"no-request"}"#.to_owned(),
        line("ok-2"),
        String::new(),
        format!(r#"{{"id":"{}","request":{{}}}}"#, "x".repeat(300)),
    ]
    .join("\n");
    let mut out = Vec::new();
    let summary =
        serve_jsonl(&stream, std::io::Cursor::new(input.into_bytes()), &mut out).expect("serve");
    let text = String::from_utf8(out).expect("utf8");
    let lines: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).expect("json"))
        .collect();
    assert_eq!(lines.len(), 7, "{text}");
    let by_id: BTreeMap<Option<&str>, &Value> = lines
        .iter()
        .map(|line| (line["id"].as_str(), line))
        .collect();
    for ok in ["ok-1", "ok-2"] {
        let line = by_id[&Some(ok)];
        assert_eq!(line["ok"], true);
        assert_eq!(line["envelope"]["operation"], "jev.run");
        assert!(line.get("error").is_none());
    }
    for bad in ["empty", "extra-key", "no-request"] {
        let line = by_id[&Some(bad)];
        assert_eq!(line["ok"], false, "{bad}");
        assert_eq!(line["error"]["error"], "INPUT_INVALID", "{bad}");
        assert_eq!(line["error"]["retryable"], false);
        assert!(line.get("envelope").is_none());
    }
    // Not JSON, and an over-long id: no usable id, so null.
    assert_eq!(lines.iter().filter(|line| line["id"].is_null()).count(), 2);
    assert_eq!((summary.answered(), summary.failed()), (7, 5));
    assert_eq!(summary.exit_code(), 2);
}

/// The exit rule of `docs/stability.md`, applied across the stream: 3 only
/// when every failure is retryable, otherwise 2, in any order.
#[test]
fn exit_code_is_three_only_when_every_failure_is_retryable() {
    let repo = tempfile::tempdir().expect("repo");
    let busy = Arc::new(
        MockJudgeBackend::new(BackendId::AnthropicHaiku, answers())
            .with_failure(JevError::network(NetworkErrorKind::Timeout, "slow")),
    );
    let ok = HeldJudge::new(BackendId::Jev, Hold::None);
    let registry = registry(vec![ok, busy]);
    let retryable = r#"{"id":"slow","request":{"questions":[{"id":"q","prompt":"p","choices":["a","b"]}],"backends":["anthropic_haiku"]}}"#;
    let caller_mistake = r#"{"id":"bad","request":{"questions":[]}}"#;
    let run = |lines: &[&str]| {
        let stream = start(&repo, Arc::clone(&registry), 2);
        let mut out = Vec::new();
        serve_jsonl(
            &stream,
            std::io::Cursor::new(lines.join("\n").into_bytes()),
            &mut out,
        )
        .expect("serve")
        .exit_code()
    };
    assert_eq!(run(&[&line("fine")]), 0);
    assert_eq!(run(&[&line("fine"), retryable]), 3);
    assert_eq!(run(&[retryable, retryable]), 3);
    assert_eq!(run(&[retryable, caller_mistake]), 2);
    assert_eq!(run(&[caller_mistake, &line("fine"), retryable]), 2);
}

/// A request in which a backend failed carries the envelope and the first
/// failure, like `jev run` printing stdout and stderr.
#[test]
fn a_partial_failure_line_has_both_envelope_and_error() {
    let repo = tempfile::tempdir().expect("repo");
    let failing = Arc::new(
        MockJudgeBackend::new(BackendId::AnthropicHaiku, answers())
            .with_failure(JevError::network(NetworkErrorKind::Timeout, "slow")),
    );
    let ok = HeldJudge::new(BackendId::Jev, Hold::None);
    let stream = start(&repo, registry(vec![ok, failing]), 1);
    let request = r#"{"id":"both","request":{"questions":[{"id":"q","prompt":"p","choices":["a","b"]}],"backends":["jev","anthropic_haiku"]}}"#;
    stream
        .submit_line(request.to_owned(), None)
        .expect("submit");
    stream.close_input();
    let lines = drain(&stream);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["ok"], false);
    assert_eq!(lines[0]["envelope"]["operation"], "jev.run");
    assert_eq!(lines[0]["error"]["error"], "NETWORK_ERROR");
    assert_eq!(lines[0]["error"]["retryable"], true);
    assert_eq!(stream.finish().exit_code(), 3);
}

/// Requests keep arriving while answers come back: each early answer is fed
/// back as a follow-up request on the same stream, and the stream is
/// finished only once the input is closed and everything drained.
#[test]
fn early_answers_can_become_follow_up_requests_on_the_same_stream() {
    let repo = tempfile::tempdir().expect("repo");
    let stream = start(
        &repo,
        registry(vec![HeldJudge::new(BackendId::Jev, Hold::None)]),
        2,
    );
    stream
        .submit_request("step-0", REQUEST, None)
        .expect("submit");
    let mut seen = Vec::new();
    while let Some(answer) = stream.next_answer() {
        let id = answer.id().expect("id").to_owned();
        assert!(answer.outcome().is_ok());
        let step: usize = id
            .strip_prefix("step-")
            .expect("prefix")
            .parse()
            .expect("n");
        seen.push(id);
        if step < 3 {
            // The answer to step n is the input of step n + 1.
            stream
                .submit_request(&format!("step-{}", step + 1), REQUEST, None)
                .expect("follow-up");
        } else {
            stream.close_input();
        }
    }
    assert_eq!(seen, ["step-0", "step-1", "step-2", "step-3"]);
    assert_eq!(stream.finish().answered(), 4);
}

/// Finished = input closed, queue empty, workers idle. An open input with
/// nothing queued stays pending; closing it ends the stream.
#[test]
fn a_stream_is_finished_only_when_input_is_closed_and_everything_drained() {
    let repo = tempfile::tempdir().expect("repo");
    let stream = start(
        &repo,
        registry(vec![HeldJudge::new(BackendId::Jev, Hold::None)]),
        2,
    );
    assert!(matches!(
        stream.next_answer_within(Duration::from_millis(1)),
        Poll::Pending
    ));
    stream.submit_request("a", REQUEST, None).expect("submit");
    let Poll::Answer(first) = stream.next_answer_within(FAILSAFE) else {
        panic!("the answer must arrive before the failsafe");
    };
    assert_eq!(first.id(), Some("a"));
    // Idle workers, empty queue, input still open: pending, not finished.
    assert!(matches!(
        stream.next_answer_within(Duration::from_millis(1)),
        Poll::Pending
    ));
    stream.close_input();
    assert!(matches!(
        stream.next_answer_within(FAILSAFE),
        Poll::Finished
    ));
    assert!(stream.next_answer().is_none());
    assert_eq!(
        stream.submit_request("late", REQUEST, None),
        Err(SubmitError::Closed)
    );
}

/// A bad id or non-JSON request is refused at submit, synchronously.
#[test]
fn submit_request_validates_the_id_and_the_json() {
    let repo = tempfile::tempdir().expect("repo");
    let stream = start(
        &repo,
        registry(vec![HeldJudge::new(BackendId::Jev, Hold::None)]),
        1,
    );
    for (id, body) in [("", REQUEST), ("a\nb", REQUEST), ("ok", "{nope")] {
        match stream.submit_request(id, body, None) {
            Err(SubmitError::Invalid(error)) => assert_eq!(error.code(), "INPUT_INVALID"),
            other => panic!("expected Invalid, got {other:?}"),
        }
    }
    assert_eq!(stream.in_flight(), 0);
    stream.close_input();
    assert!(stream.next_answer().is_none());
}

/// A slow consumer applies backpressure: the reader stops, the stream holds
/// at most `capacity()` requests, and every request is still answered.
#[test]
fn a_slow_writer_stops_the_reader_and_memory_stays_bounded() {
    const LINES: usize = 400;
    let repo = tempfile::tempdir().expect("repo");
    let stream = start(
        &repo,
        registry(vec![HeldJudge::new(BackendId::Jev, Hold::None)]),
        2,
    );
    let capacity = stream.capacity();
    let consumed = Arc::new(AtomicUsize::new(0));
    let source = CountingLines {
        lines: (0..LINES)
            .map(|i| format!("{}\n", line(&format!("n{i}"))))
            .collect(),
        next: 0,
        consumed: Arc::clone(&consumed),
    };
    /// Blocks its first write until the gate opens, and checks the bound on
    /// every write.
    struct SlowWriter {
        gate: Arc<Gate>,
        stream: Arc<DecisionStream>,
        capacity: usize,
        lines: usize,
        waited: bool,
    }
    impl Write for SlowWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if !self.waited {
                self.waited = true;
                self.gate.wait();
            }
            assert!(self.stream.in_flight() <= self.capacity);
            self.lines += buf.iter().filter(|byte| **byte == b'\n').count();
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let gate = Gate::new();
    let server = {
        let stream = Arc::clone(&stream);
        let gate = Arc::clone(&gate);
        thread::spawn(move || {
            let mut writer = SlowWriter {
                gate,
                stream: Arc::clone(&stream),
                capacity,
                lines: 0,
                waited: false,
            };
            let summary = serve_jsonl(&stream, source, &mut writer).expect("serve");
            (summary, writer.lines)
        })
    };
    // The writer is stuck on its first answer. Wait until the stream is
    // full: from here the reader can read no further.
    let mut spins = 0_u64;
    while stream.in_flight() < capacity {
        thread::yield_now();
        spins += 1;
        assert!(spins < 200_000_000, "stream never filled");
    }
    assert_eq!(stream.in_flight(), capacity);
    let read = consumed.load(Ordering::Acquire);
    // Everything the reader took is in the stream, or is the one line it
    // holds while blocked on a full queue, or the answer the writer holds.
    assert!(
        read <= capacity + 2,
        "reader consumed {read} lines with capacity {capacity}"
    );
    assert!(read < LINES, "the reader must be blocked, not finished");
    gate.open();
    let (summary, written) = server.join().expect("server thread");
    assert_eq!(written, LINES);
    assert_eq!(summary.answered(), LINES as u64);
    assert_eq!(consumed.load(Ordering::Acquire), LINES);
}

/// Cancel stops new work and honours the token: running requests finish,
/// queued ones are answered `cancelled` without running, new ones refused.
#[test]
fn cancel_stops_new_work_and_sets_the_engines_token() {
    let repo = tempfile::tempdir().expect("repo");
    let gate = Gate::new();
    let judge = HeldJudge::new(BackendId::Jev, Hold::Gate(Arc::clone(&gate)));
    let token = CancelToken::new();
    let stream = Arc::new(
        DecisionStream::start(
            inputs(repo.path(), registry(vec![judge.clone()])).cancel(token.clone()),
            Concurrency::new(2).expect("2"),
        )
        .expect("start"),
    );
    for index in 0..6 {
        stream
            .submit_request(&format!("c{index}"), REQUEST, Some(FAILSAFE))
            .expect("submit");
    }
    while judge.inside.load(Ordering::Acquire) < 2 {
        thread::yield_now();
    }
    stream.cancel();
    assert!(token.is_cancelled(), "the engine's token is cancelled");
    assert_eq!(
        stream.submit_request("late", REQUEST, None),
        Err(SubmitError::Closed)
    );
    gate.open();
    let lines = drain(&stream);
    let summary = stream.finish();
    assert_eq!(lines.len(), 6, "every accepted request is answered once");
    let ok = lines.iter().filter(|line| line["ok"] == true).count();
    assert_eq!(ok, 2, "only the two running requests ran");
    let cancelled: Vec<_> = lines.iter().filter(|line| line["ok"] == false).collect();
    assert_eq!(cancelled.len(), 4);
    for line in cancelled {
        assert_eq!(line["error"]["error"], "NETWORK_ERROR");
        assert!(
            line["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("cancelled")),
            "{line}"
        );
        assert!(line["id"].as_str().is_some_and(|id| id.starts_with('c')));
    }
    assert_eq!(summary.failed(), 4);
}

/// A write failure cancels the stream and is returned; nothing hangs.
#[test]
fn a_failed_write_cancels_the_stream_and_returns_the_error() {
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let repo = tempfile::tempdir().expect("repo");
    let stream = start(
        &repo,
        registry(vec![HeldJudge::new(BackendId::Jev, Hold::None)]),
        2,
    );
    let input: String = (0..50)
        .map(|i| format!("{}\n", line(&format!("w{i}"))))
        .collect();
    let error = serve_jsonl(
        &stream,
        std::io::Cursor::new(input.into_bytes()),
        &mut Broken,
    )
    .expect_err("broken pipe");
    assert_eq!(error.code(), "IO_FAILED");
}

/// The session log gets the engine's rows from concurrent workers through
/// the one shared writer: every row is a whole line.
#[test]
fn concurrent_workers_write_whole_session_log_rows() {
    let repo = tempfile::tempdir().expect("repo");
    let stream = start(
        &repo,
        registry(vec![HeldJudge::new(BackendId::Jev, Hold::None)]),
        8,
    );
    let count = 64;
    // More requests than the stream holds, so answers must be read while
    // requests are still being submitted.
    assert!(count > stream.capacity());
    let reader = {
        let stream = Arc::clone(&stream);
        thread::spawn(move || drain(&stream).len())
    };
    for index in 0..count {
        stream
            .submit_line(line(&format!("l{index}")), Some(FAILSAFE))
            .expect("submit");
    }
    stream.close_input();
    assert_eq!(reader.join().expect("reader"), count);
    assert!(stream.finish().log_flush_warning().is_none());
    let log = std::fs::read_to_string(repo.path().join(".sc-ai/logs/sc-ai.log.jsonl"))
        .expect("session log");
    let rows: Vec<Value> = log
        .lines()
        .map(|row| serde_json::from_str(row).expect("each line is whole JSON"))
        .collect();
    assert_eq!(rows.len(), count, "one row per send");
    assert!(!log.contains("\"prompt\""), "no prompt text in the log");
}
