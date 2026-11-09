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
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use sc_agent_types::{BackendId, JevError, NetworkErrorKind, RepoRoot, SendTrace};
use sc_decision_schema::test_util::MockJudgeBackend;
use sc_decision_schema::{JudgeBackend, JudgeOutcome, JudgeRequest, QuestionAnswer, QuestionId};
use sc_jev_check::registry::BackendRegistry;
use sc_jev_host::compose::ComposeInputs;
use sc_jev_host::paths::PathPolicy;
use sc_jev_host::stream::{
    Concurrency, DecisionStream, Poll, StreamRequest, StreamSummary, SubmitError, serve_jsonl,
};
use serde_json::{Value, json};
use tempfile::TempDir;

/// Failsafe for waits that must finish; never a measurement.
const FAILSAFE: Duration = Duration::from_secs(60);

/// Polls `condition` every millisecond until it holds, failing with `what`
/// after `FAILSAFE`. Replaces spins: a regression fails, it does not hang.
fn wait_until(what: &str, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + FAILSAFE;
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for: {what}");
        thread::sleep(Duration::from_millis(1));
    }
}

/// Runs `work` on a helper thread and waits for its result at most
/// `FAILSAFE`, so a hung call fails the test with `what` instead of hanging.
fn bounded<T: Send + 'static>(what: &str, work: impl FnOnce() -> T + Send + 'static) -> T {
    let (sender, receiver) = mpsc::channel();
    // Detached on purpose: if `work` hangs, the test fails and the process
    // ends the thread.
    drop(thread::spawn(move || {
        let _ = sender.send(work());
    }));
    receiver
        .recv_timeout(FAILSAFE)
        .unwrap_or_else(|_| panic!("timed out (or panicked) in: {what}"))
}

/// A write target a helper thread can own while the test reads it back.
#[derive(Clone, Default)]
struct SharedOut(Arc<Mutex<Vec<u8>>>);

impl Write for SharedOut {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// `serve_jsonl` over `input` on a helper thread, bounded by `FAILSAFE`.
fn serve(
    stream: &Arc<DecisionStream>,
    input: impl BufRead + Send + 'static,
    mut out: impl Write + Send + 'static,
) -> Result<StreamSummary, JevError> {
    let stream = Arc::clone(stream);
    bounded("serve_jsonl", move || serve_jsonl(&stream, input, &mut out))
}

fn req(id: &str) -> StreamRequest {
    StreamRequest::new(id, REQUEST).expect("valid request")
}

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

    /// True when the gate opened before the failsafe.
    fn wait(&self) -> bool {
        let open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        let (open, _) = self
            .changed
            .wait_timeout_while(open, FAILSAFE, |open| !*open)
            .unwrap_or_else(PoisonError::into_inner);
        *open
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
    /// How many answered calls saw a cancelled token on their request.
    saw_cancelled: AtomicUsize,
}

impl HeldJudge {
    fn new(id: BackendId, hold: Hold) -> Arc<Self> {
        Arc::new(Self {
            inner: MockJudgeBackend::new(id, answers()),
            hold,
            inside: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
            saw_cancelled: AtomicUsize::new(0),
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
            Hold::Gate(gate) => gate.wait(),
        };
        self.inside.fetch_sub(1, Ordering::AcqRel);
        if request.cancel().is_cancelled() {
            self.saw_cancelled.fetch_add(1, Ordering::AcqRel);
        }
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
    loop {
        match stream.next_answer_within(FAILSAFE) {
            Poll::Answer(answer) => {
                let bytes = answer.to_line().expect("line");
                assert_eq!(bytes.last(), Some(&b'\n'));
                lines.push(serde_json::from_slice(&bytes).expect("json line"));
            }
            Poll::Finished => return lines,
            Poll::Pending => panic!("no answer within the failsafe; got {} so far", lines.len()),
        }
    }
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
    let out = SharedOut::default();
    let summary = serve(
        &stream,
        std::io::Cursor::new(input.into_bytes()),
        out.clone(),
    )
    .expect("serve");
    let text = String::from_utf8(out.0.lock().expect("out").clone()).expect("utf8");
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
        MockJudgeBackend::new(BackendId::Haiku, answers())
            .with_failure(JevError::network(NetworkErrorKind::Timeout, "slow")),
    );
    let ok = HeldJudge::new(BackendId::Jev, Hold::None);
    let registry = registry(vec![ok, busy]);
    let retryable = r#"{"id":"slow","request":{"questions":[{"id":"q","prompt":"p","choices":["a","b"]}],"backends":["haiku"]}}"#;
    let caller_mistake = r#"{"id":"bad","request":{"questions":[]}}"#;
    let run = |lines: &[&str]| {
        let stream = start(&repo, Arc::clone(&registry), 2);
        serve(
            &stream,
            std::io::Cursor::new(lines.join("\n").into_bytes()),
            SharedOut::default(),
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
        MockJudgeBackend::new(BackendId::Haiku, answers())
            .with_failure(JevError::network(NetworkErrorKind::Timeout, "slow")),
    );
    let ok = HeldJudge::new(BackendId::Jev, Hold::None);
    let stream = start(&repo, registry(vec![ok, failing]), 1);
    let request = r#"{"id":"both","request":{"questions":[{"id":"q","prompt":"p","choices":["a","b"]}],"backends":["jev","haiku"]}}"#;
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
    stream.submit_request(req("step-0"), None).expect("submit");
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
                .submit_request(req(&format!("step-{}", step + 1)), None)
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
    stream.submit_request(req("a"), None).expect("submit");
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
        stream.submit_request(req("late"), None),
        Err(SubmitError::Closed)
    );
}

/// A bad id or non-JSON request cannot be built into a `StreamRequest`, so
/// it never reaches the queue.
#[test]
fn a_bad_id_or_body_cannot_become_a_request() {
    for (id, body) in [("", REQUEST), ("a\nb", REQUEST), ("ok", "{nope")] {
        let error = StreamRequest::new(id, body).expect_err("invalid");
        assert_eq!(error.code(), "INPUT_INVALID");
    }
}

/// Past `capacity()` unread answers a timed submit returns `Full` instead of
/// blocking, and no more than `capacity()` requests were accepted.
#[test]
fn submitting_past_capacity_times_out_instead_of_deadlocking() {
    let repo = tempfile::tempdir().expect("repo");
    let stream = start(
        &repo,
        registry(vec![HeldJudge::new(BackendId::Jev, Hold::None)]),
        1,
    );
    let total = stream.capacity() + 10;
    let (mut accepted, mut full) = (0, 0);
    for index in 0..total {
        match stream.submit_request(req(&format!("p{index}")), Some(Duration::from_millis(20))) {
            Ok(()) => accepted += 1,
            Err(SubmitError::Full) => full += 1,
            Err(other) => panic!("unexpected {other:?}"),
        }
    }
    assert!(accepted <= stream.capacity(), "accepted {accepted}");
    assert_eq!(accepted + full, total);
    assert!(full >= 10, "at least the surplus was refused: {full}");
    stream.close_input();
    assert_eq!(drain(&stream).len(), accepted, "every accepted one answers");
}

/// `finish` closes the input and does not hang on answers nobody read.
#[test]
fn finish_closes_input_and_never_hangs_on_unread_answers() {
    let repo = tempfile::tempdir().expect("repo");
    let stream = start(
        &repo,
        registry(vec![HeldJudge::new(BackendId::Jev, Hold::None)]),
        1,
    );
    let mut accepted = 0_u64;
    for index in 0..stream.capacity() {
        if stream
            .submit_request(req(&format!("u{index}")), Some(Duration::from_millis(20)))
            .is_ok()
        {
            accepted += 1;
        }
    }
    let finishing = Arc::clone(&stream);
    let summary = bounded("finish with unread answers", move || finishing.finish());
    assert_eq!(summary.answered(), accepted);
    assert_eq!(
        stream.submit_request(req("late"), None),
        Err(SubmitError::Closed)
    );
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
        entered: Arc<Gate>,
        stream: Arc<DecisionStream>,
        capacity: usize,
        lines: usize,
        waited: bool,
    }
    impl Write for SlowWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if !self.waited {
                self.waited = true;
                self.entered.open();
                assert!(self.gate.wait(), "the test never released the writer");
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
    let entered = Gate::new();
    let (done_send, done_recv) = mpsc::channel();
    {
        let stream = Arc::clone(&stream);
        let gate = Arc::clone(&gate);
        let entered = Arc::clone(&entered);
        drop(thread::spawn(move || {
            let mut writer = SlowWriter {
                gate,
                entered,
                stream: Arc::clone(&stream),
                capacity,
                lines: 0,
                waited: false,
            };
            let summary = serve_jsonl(&stream, source, &mut writer).expect("serve");
            let _ = done_send.send((summary, writer.lines));
        }));
    }
    // Handshake: the writer holds its first answer. From here nothing is
    // read, so the stream fills and the reader stops.
    assert!(entered.wait(), "the writer never received an answer");
    wait_until("the stream to fill", || stream.in_flight() >= capacity);
    let read = consumed.load(Ordering::Acquire);
    // Everything the reader took is in the stream, or is the one line it
    // holds while blocked on a full queue, or the answer the writer holds.
    assert!(
        read <= capacity + 2,
        "reader consumed {read} lines with capacity {capacity}"
    );
    assert!(read < LINES, "the reader must be blocked, not finished");
    gate.open();
    let (summary, written) = done_recv
        .recv_timeout(FAILSAFE)
        .expect("serve_jsonl did not finish after the writer was released");
    assert_eq!(written, LINES);
    assert_eq!(summary.answered(), LINES as u64);
    assert_eq!(consumed.load(Ordering::Acquire), LINES);
}

/// Cancel stops new work and honours the token: running requests finish,
/// queued ones are answered `cancelled` without running, new ones refused.
#[test]
fn cancel_stops_new_work_and_sets_the_streams_token() {
    let repo = tempfile::tempdir().expect("repo");
    let gate = Gate::new();
    let judge = HeldJudge::new(BackendId::Jev, Hold::Gate(Arc::clone(&gate)));
    let stream = Arc::new(
        DecisionStream::start(
            inputs(repo.path(), registry(vec![judge.clone()])),
            Concurrency::new(2).expect("2"),
        )
        .expect("start"),
    );
    for index in 0..6 {
        stream
            .submit_request(req(&format!("c{index}")), Some(FAILSAFE))
            .expect("submit");
    }
    wait_until("two requests inside judge", || {
        judge.inside.load(Ordering::Acquire) >= 2
    });
    stream.cancel();
    assert_eq!(
        stream.submit_request(req("late"), None),
        Err(SubmitError::Closed)
    );
    gate.open();
    let lines = drain(&stream);
    let summary = stream.finish();
    assert_eq!(lines.len(), 6, "every accepted request is answered once");
    let ok = lines.iter().filter(|line| line["ok"] == true).count();
    assert_eq!(ok, 2, "only the two running requests ran");
    assert_eq!(
        judge.saw_cancelled.load(Ordering::Acquire),
        2,
        "the running requests carry the stream's cancelled token"
    );
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
    let error =
        serve(&stream, std::io::Cursor::new(input.into_bytes()), Broken).expect_err("broken pipe");
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
    let (read_send, read_recv) = mpsc::channel();
    {
        let stream = Arc::clone(&stream);
        drop(thread::spawn(move || {
            let _ = read_send.send(drain(&stream).len());
        }));
    }
    for index in 0..count {
        stream
            .submit_line(line(&format!("l{index}")), Some(FAILSAFE))
            .expect("submit");
    }
    stream.close_input();
    assert_eq!(
        read_recv
            .recv_timeout(FAILSAFE)
            .expect("the reader did not finish"),
        count
    );
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

/// A parallel stream writes a real general log: every row is whole JSON, the
/// stream's own rows carry the stream's id and counts, and each request is a
/// run with its own `run_id` that picks out its rows (the engine's included)
/// and ties them to the caller's request id. Prompt text never appears.
#[test]
fn a_parallel_stream_writes_a_whole_general_log_correlated_by_run_id() {
    const MARKER: &str = "PROMPT-MARKER-7f3a9c";
    const COUNT: usize = 40;
    let repo = tempfile::tempdir().expect("repo");
    let failing = Arc::new(
        MockJudgeBackend::new(BackendId::Haiku, answers())
            .with_failure(JevError::network(NetworkErrorKind::Timeout, "slow")),
    );
    let ok = HeldJudge::new(BackendId::Jev, Hold::None);
    let stream = start(&repo, registry(vec![ok, failing]), 8);
    let good = format!(r#"{{"questions":[{{"id":"q","prompt":"{MARKER}","choices":["a","b"]}}]}}"#);
    let partial = format!(
        r#"{{"questions":[{{"id":"q","prompt":"{MARKER}","choices":["a","b"]}}],"backends":["jev","haiku"]}}"#
    );
    let reader = {
        let stream = Arc::clone(&stream);
        let (send, recv) = mpsc::channel();
        drop(thread::spawn(move || {
            let _ = send.send(drain(&stream).len());
        }));
        recv
    };
    for index in 0..COUNT {
        let id = format!("g{index}");
        let body = match index % 10 {
            3 => r#"{"questions":[]}"#.to_owned(),
            7 => partial.clone(),
            _ => good.clone(),
        };
        let request = StreamRequest::new(&id, &body).expect("request");
        stream
            .submit_request(request, Some(FAILSAFE))
            .expect("submit");
    }
    stream.close_input();
    assert_eq!(
        reader
            .recv_timeout(FAILSAFE)
            .expect("the reader did not finish"),
        COUNT
    );
    let summary = bounded("finish", {
        let stream = Arc::clone(&stream);
        move || stream.finish()
    });
    assert_eq!(summary.answered(), COUNT as u64);

    let text = std::fs::read_to_string(repo.path().join(".sc-ai/logs/sc-ai-general.log.jsonl"))
        .expect("general log");
    assert!(!text.contains(MARKER), "prompt text must never be logged");
    let rows: Vec<Value> = text
        .lines()
        .map(|row| serde_json::from_str(row).expect("every row is whole JSON"))
        .collect();
    let action = |row: &Value| row["action"].as_str().unwrap_or_default().to_owned();
    let field = |row: &Value, key: &str| row["fields"][key].clone();

    // The stream's own rows share one id and carry the counts.
    let start_row = rows
        .iter()
        .find(|row| action(row) == "stream.start")
        .expect("stream.start");
    let end_row = rows
        .iter()
        .find(|row| action(row) == "stream.end")
        .expect("stream.end");
    let stream_id = field(start_row, "run_id");
    assert_eq!(field(end_row, "run_id"), stream_id);
    assert_eq!(field(start_row, "stream_id"), stream_id);
    assert_eq!(field(start_row, "workers"), 8);
    assert_eq!(field(end_row, "answered"), COUNT);
    assert_eq!(field(end_row, "failed"), 8, "4 empty + 4 partial");

    // Each request is a run with its own run_id.
    let mut run_ids = BTreeSet::new();
    for index in 0..COUNT {
        let id = format!("g{index}");
        let started = rows
            .iter()
            .filter(|row| {
                action(row) == "stream.request.start" && field(row, "request_id") == id.as_str()
            })
            .collect::<Vec<_>>();
        assert_eq!(started.len(), 1, "{id}");
        let run_id = field(started[0], "run_id");
        assert!(run_ids.insert(run_id.to_string()), "run ids are unique");
        assert_ne!(run_id, stream_id);
        let run_rows: Vec<&Value> = rows
            .iter()
            .filter(|row| field(row, "run_id") == run_id)
            .collect();
        if index % 10 == 3 {
            // Rejected at parse: started, then failed, in its own run.
            let failed = run_rows
                .iter()
                .find(|row| action(row) == "stream.request.failed")
                .unwrap_or_else(|| panic!("{id}: no failed row"));
            assert_eq!(field(failed, "stage"), "parse");
            assert_eq!(field(failed, "error_code"), "INPUT_INVALID");
            assert_eq!(field(failed, "stream_id"), stream_id);
            assert_eq!(field(failed, "request_id"), id.as_str());
            continue;
        }
        let actions: Vec<String> = run_rows.iter().map(|row| action(row)).collect();
        for expected in [
            "stream.request.start",
            "run.start",
            "run.done",
            "stream.request.done",
        ] {
            assert!(
                actions.iter().any(|a| a == expected),
                "{id}: {expected} in {actions:?}"
            );
        }
        assert!(
            run_rows
                .iter()
                .all(|row| row["fields"].get("request_id").is_none()
                    || field(row, "request_id") == id.as_str()),
            "{id}: a row of another request has this run_id"
        );
        let done = run_rows
            .iter()
            .find(|row| action(row) == "stream.request.done")
            .expect("done");
        if index % 10 == 7 {
            assert_eq!(field(done, "ok"), false, "{id}");
            let backend = run_rows
                .iter()
                .find(|row| action(row) == "stream.request.backend_failed")
                .unwrap_or_else(|| panic!("{id}: no backend_failed row"));
            assert_eq!(field(backend, "backend"), "haiku");
            assert_eq!(field(backend, "error_code"), "NETWORK_ERROR");
        } else {
            assert_eq!(field(done, "ok"), true, "{id}");
        }
        assert!(field(done, "duration_ms").is_u64(), "{id}");
    }
    assert_eq!(run_ids.len(), COUNT, "one run per request");

    // The session log is whole too: one row per send (36 jev + 4 haiku).
    let session = std::fs::read_to_string(repo.path().join(".sc-ai/logs/sc-ai.log.jsonl"))
        .expect("session log");
    let session_rows: Vec<Value> = session
        .lines()
        .map(|row| serde_json::from_str(row).expect("whole session row"))
        .collect();
    assert_eq!(session_rows.len(), COUNT - 4 + 4);
}

/// Injected logs replace the defaults: the stream's events reach the injected
/// general log, no default log file is opened, and the caller owns the flush.
#[test]
fn injected_logs_replace_the_defaults() {
    use sc_agent_types::observability::v2::{NoopSessionLog, RecordingGeneralLog};
    let repo = tempfile::tempdir().expect("repo");
    let recording = RecordingGeneralLog::new();
    let stream = Arc::new(
        DecisionStream::start(
            inputs(
                repo.path(),
                registry(vec![HeldJudge::new(BackendId::Jev, Hold::None)]),
            )
            .general_log(Arc::new(recording.clone()))
            .session_log(Arc::new(NoopSessionLog)),
            Concurrency::new(2).expect("2"),
        )
        .expect("start"),
    );
    stream.submit_request(req("one"), None).expect("submit");
    stream.close_input();
    assert_eq!(drain(&stream).len(), 1);
    let _ = bounded("finish", {
        let stream = Arc::clone(&stream);
        move || stream.finish()
    });
    let actions = recording.actions();
    for expected in [
        "stream.start",
        "stream.request.start",
        "run.done",
        "stream.end",
    ] {
        assert!(actions.contains(&expected), "{expected} in {actions:?}");
    }
    assert!(
        !repo.path().join(".sc-ai/logs").exists(),
        "an injected pair opens no default log file"
    );
}

fn general_actions(repo: &TempDir) -> Vec<String> {
    let path = repo.path().join(".sc-ai/logs/sc-ai-general.log.jsonl");
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|row| {
            let row: Value = serde_json::from_str(row).expect("whole row");
            row["action"].as_str().unwrap_or_default().to_owned()
        })
        .collect()
}

/// A repeated `finish` returns the summary again and writes one `stream.end`.
#[test]
fn a_repeated_finish_logs_stream_end_once() {
    let repo = tempfile::tempdir().expect("repo");
    let stream = start(
        &repo,
        registry(vec![HeldJudge::new(BackendId::Jev, Hold::None)]),
        2,
    );
    stream.submit_request(req("a"), None).expect("submit");
    let first = bounded("first finish", {
        let stream = Arc::clone(&stream);
        move || stream.finish()
    });
    let second = bounded("second finish", {
        let stream = Arc::clone(&stream);
        move || stream.finish()
    });
    assert_eq!(first.answered(), second.answered());
    let ends = general_actions(&repo)
        .iter()
        .filter(|action| *action == "stream.end")
        .count();
    assert_eq!(ends, 1);
}

/// Every way out that is not `finish` still logs `stream.end` and flushes the
/// session log: `abandon` (a `with` exit, an interrupted batch) and a plain
/// drop. The rows are on disk when the call returns.
#[test]
fn abandon_and_drop_log_stream_end_and_flush() {
    for by_drop in [false, true] {
        let repo = tempfile::tempdir().expect("repo");
        let stream = start(
            &repo,
            registry(vec![HeldJudge::new(BackendId::Jev, Hold::None)]),
            4,
        );
        for index in 0..12 {
            stream
                .submit_request(req(&format!("x{index}")), Some(FAILSAFE))
                .expect("submit");
        }
        if by_drop {
            let stream = Arc::into_inner(stream).expect("sole owner");
            bounded("drop", move || drop(stream));
        } else {
            bounded("abandon", {
                let stream = Arc::clone(&stream);
                move || stream.abandon()
            });
        }
        let actions = general_actions(&repo);
        assert!(
            actions.contains(&"stream.end".to_owned()),
            "by_drop={by_drop}: {actions:?}"
        );
        assert!(
            actions.contains(&"session_log.flushed".to_owned()),
            "by_drop={by_drop}: {actions:?}"
        );
        // Whatever the session log holds is whole rows.
        let session = std::fs::read_to_string(repo.path().join(".sc-ai/logs/sc-ai.log.jsonl"))
            .unwrap_or_default();
        for row in session.lines() {
            serde_json::from_str::<Value>(row).expect("whole session row");
        }
    }
}

/// A leading UTF-8 byte order mark is not part of the first line.
#[test]
fn a_leading_byte_order_mark_is_stripped() {
    let repo = tempfile::tempdir().expect("repo");
    let stream = start(
        &repo,
        registry(vec![HeldJudge::new(BackendId::Jev, Hold::None)]),
        1,
    );
    let input = format!("\u{feff}{}\n{}\n", line("bom-1"), line("bom-2"));
    let out = SharedOut::default();
    let summary = serve(
        &stream,
        std::io::Cursor::new(input.into_bytes()),
        out.clone(),
    )
    .expect("serve");
    assert_eq!((summary.answered(), summary.failed()), (2, 0));
}
