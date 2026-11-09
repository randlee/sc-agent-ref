//! Streaming decision calls: many requests through one process
//! (ADR-SAG-0032, REQ-SAG-0720..REQ-SAG-0727).
//!
//! A [`DecisionStream`] is a bounded queue drained by N worker threads
//! around the engine's existing per-request work: each worker does
//! `sc_jev_check::run::prepare` then `run` for one caller-built request, with
//! the one shared session logger and the one shared backend registry. Nothing
//! in the engine changed. Callers (the `sc-ai jev stream` command, the Python
//! `DecisionStream`) feed requests in and read answers out, so they do not
//! spawn a process per query.
//!
//! # Wire format
//!
//! One request per input line (JSONL):
//!
//! ```text
//! {"id":"q-17","request":{"questions":[...],"state":[...],"backends":["jev"]}}
//! ```
//!
//! `request` is the existing caller-built questions document
//! (`CallerRequest`); `id` is a caller-chosen string (1 to
//! [`MAX_ID_BYTES`] bytes, no control characters) that is echoed on that
//! request's one output line. Ids are not checked for uniqueness. Any other
//! key is `INPUT_INVALID`. One output line per request, in completion order,
//! not input order:
//!
//! ```text
//! {"id":"q-17","ok":true,"envelope":{"version":"1","operation":"jev.run","data":{...}}}
//! {"id":"q-18","ok":false,"error":{"error":"INPUT_INVALID","kind":...,"retryable":false,...}}
//! {"id":"q-19","ok":false,"envelope":{...},"error":{...}}
//! ```
//!
//! `envelope` is the same envelope `sc-ai jev run` prints (without the
//! path `readback`). `error` is the existing `ScAgentErrorResponse`. `ok` is
//! false whenever `error` is present: a failed request has only `error`; a
//! request in which some backends failed carries both, as `jev run` prints
//! the envelope on stdout and the first failure on stderr. A client must
//! therefore read `envelope` whenever it is present, even when `ok` is false.
//! `id` is echoed whenever a usable id could be read from the line, even if
//! the rest of the line is not a valid request; it is `null` only when no
//! usable id could be read (not JSON, no string `id`, an invalid `id`, a line
//! over [`MAX_LINE_BYTES`] or not UTF-8).
//!
//! # Lifecycle
//!
//! A stream is finished when its input is closed, the queue is empty and no
//! worker is running. [`DecisionStream::next_answer`] returns `None` exactly
//! then. Requests can keep arriving while earlier answers come back, so a
//! caller may turn an early answer into a follow-up request on the same
//! stream.
//!
//! # Exit code
//!
//! [`StreamSummary::exit_code`] applies the existing rule of
//! `docs/stability.md` across the stream: 0 when no request failed; else 3
//! when every failure (a failed request, or a failed backend inside a request)
//! is retryable; else 2.
//!
//! # Concurrency and thread safety
//!
//! [`Concurrency::DEFAULT`] is 8, a hardcoded default that was exercised
//! only against scripted and loopback backends, never against a live
//! provider. `JevClient` holds no interior mutability and its transport is
//! `Send + Sync` (a compile-time fact: `JudgeBackend: Send + Sync`), so one
//! client per backend is shared by all workers; there is no client per
//! worker. Each request keeps its own busy retry; there is no shared rate
//! limiter.

mod log;
mod pool;

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use sc_agent_types::observability::v2::SharedSessionLog;
use sc_agent_types::{
    BackendId, BackendSet, CancelToken, EXIT_FAILURE, EXIT_RETRYABLE, JevError, ModelAliases,
    NetworkErrorKind, ScAgentErrorResponse,
};
use sc_jev_check::registry::BackendRegistry;
use sc_jev_check::run::{
    CallerRequest, CheckSource, Envelope, LogContext, RunResponse, prepare, run,
};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::compose::{ComposeInputs, ResolvedPaths};
use crate::obs::ObservabilityAdapter;
use log::StreamLog;
use pool::{Popped, PushError, Skipped, WorkerPool};

/// Longest request id, in bytes.
pub const MAX_ID_BYTES: usize = 256;

/// Longest input line, in bytes. A request is capped near 96 KB per backend,
/// so this is generous; it bounds memory for a hostile or broken producer.
pub const MAX_LINE_BYTES: usize = 1 << 20;

/// The number of worker threads. A global setting, validated at 1 to 64.
///
/// The default is [`Concurrency::DEFAULT`] (8). It is a hardcoded choice that
/// was **not** measured against live providers: the mechanism is tested with
/// scripted and loopback backends only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Concurrency(usize);

impl Concurrency {
    /// The hardcoded default: 8 workers.
    pub const DEFAULT: Self = Self(8);
    /// The largest accepted value.
    pub const MAX: usize = 64;

    /// A concurrency of `workers`.
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` naming `concurrency` when `workers` is 0 or above
    /// [`Self::MAX`].
    pub fn new(workers: usize) -> Result<Self, JevError> {
        if (1..=Self::MAX).contains(&workers) {
            Ok(Self(workers))
        } else {
            Err(JevError::input_invalid_field(
                "concurrency",
                format!("concurrency must be between 1 and {}", Self::MAX),
            ))
        }
    }

    /// The number of workers.
    #[must_use]
    pub fn get(self) -> usize {
        self.0
    }
}

impl Default for Concurrency {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// A validated request id: 1 to [`MAX_ID_BYTES`] bytes, no control character.
///
/// Built once at the boundary (a line, a Python call), so nothing downstream
/// re-checks it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestId(String);

impl RequestId {
    /// Validates `id`.
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` naming `id` when it is empty, too long, or has a
    /// control character.
    pub fn new(id: &str) -> Result<Self, JevError> {
        if id.is_empty() || id.len() > MAX_ID_BYTES || id.chars().any(char::is_control) {
            Err(JevError::input_invalid_field(
                "id",
                format!("id must be 1 to {MAX_ID_BYTES} bytes with no control characters"),
            ))
        } else {
            Ok(Self(id.to_owned()))
        }
    }

    /// The id text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A request that is valid before it is queued: a [`RequestId`] and a
/// request document that is well-formed JSON.
#[derive(Debug, Clone)]
pub struct StreamRequest {
    id: RequestId,
    json: String,
}

impl StreamRequest {
    /// Validates `id` and that `request_json` is JSON. Whether it is a valid
    /// questions document is decided by the worker and reported on its answer.
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` for a bad id (field `id`) or text that is not JSON
    /// (field `request`).
    pub fn new(id: &str, request_json: &str) -> Result<Self, JevError> {
        let id = RequestId::new(id)?;
        serde_json::from_str::<&RawValue>(request_json).map_err(|error| {
            JevError::input_invalid_field("request", format!("request is not JSON: {error}"))
        })?;
        Ok(Self {
            id,
            json: request_json.to_owned(),
        })
    }
}

/// Why a request was not accepted.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum SubmitError {
    /// The input is closed or the stream was cancelled.
    Closed,
    /// The queue stayed full for the whole wait.
    Full,
}

/// One unit of work in the queue.
enum Job {
    /// A raw input line: `{"id": ..., "request": ...}`.
    Line(String),
    /// An already validated request.
    Request(StreamRequest),
    /// An input that failed before it reached a worker (too long, not UTF-8).
    Rejected(JevError),
}

/// The answer to one request.
#[derive(Debug, Clone)]
pub struct StreamAnswer {
    id: Option<String>,
    outcome: Result<RunResponse, JevError>,
}

impl StreamAnswer {
    fn failed(id: Option<String>, error: JevError) -> Self {
        Self {
            id,
            outcome: Err(error),
        }
    }

    /// The caller's request id; `None` when the line had no usable one.
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    /// The run response, or the typed error that ended the request.
    pub fn outcome(&self) -> &Result<RunResponse, JevError> {
        &self.outcome
    }

    /// The error body for this answer: the request's error, else the first
    /// failed backend's. `None` when the request fully succeeded.
    #[must_use]
    pub fn failure(&self) -> Option<ScAgentErrorResponse> {
        match &self.outcome {
            Err(error) => Some(ScAgentErrorResponse::from_jev_error(error)),
            Ok(response) if response.is_partial() => {
                response.first_failure().map(|failure| failure.response())
            }
            Ok(_) => None,
        }
    }

    /// Whether this answer failed and every failure in it is retryable.
    /// `None` when it did not fail.
    fn all_failures_retryable(&self) -> Option<bool> {
        match &self.outcome {
            Err(error) => Some(error.retryable()),
            Ok(response) if response.is_partial() => Some(response.all_failures_retryable()),
            Ok(_) => None,
        }
    }

    /// The answer as one output line (compact JSON plus a newline).
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` (`encode_failed`) if the line cannot be serialized.
    pub fn to_line(&self) -> Result<Vec<u8>, JevError> {
        #[derive(Serialize)]
        struct Line<'a> {
            id: Option<&'a str>,
            ok: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            envelope: Option<Envelope>,
            #[serde(skip_serializing_if = "Option::is_none")]
            error: Option<ScAgentErrorResponse>,
        }
        let failure = self.failure();
        let line = Line {
            id: self.id(),
            ok: failure.is_none(),
            envelope: self.outcome.as_ref().ok().map(Envelope::run),
            error: failure,
        };
        let mut bytes = serde_json::to_vec(&line)
            .map_err(|error| JevError::encode_failed("stream answer line", &error))?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}

/// How a finished (or finishing) stream went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamSummary {
    answered: u64,
    failed: u64,
    all_retryable: bool,
    log_flush_warning: Option<String>,
}

impl StreamSummary {
    /// Answers handed to the caller.
    #[must_use]
    pub fn answered(&self) -> u64 {
        self.answered
    }

    /// Answers that carried a failure.
    #[must_use]
    pub fn failed(&self) -> u64 {
        self.failed
    }

    /// The exit code for the stream: 0 with no failure; else 3 only when
    /// every failure is retryable, else 2 (`docs/stability.md`).
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        match (self.failed, self.all_retryable) {
            (0, _) => 0,
            (_, true) => EXIT_RETRYABLE,
            (_, false) => EXIT_FAILURE,
        }
    }

    /// Set when the session log could not be flushed at the end. The log is
    /// best effort, so this never changes the exit code.
    #[must_use]
    pub fn log_flush_warning(&self) -> Option<&str> {
        self.log_flush_warning.as_deref()
    }
}

#[derive(Debug)]
struct Tally {
    answered: u64,
    failed: u64,
    all_retryable: bool,
}

/// A running queue of decision requests (see the module documentation).
pub struct DecisionStream {
    pool: WorkerPool<Job, StreamAnswer>,
    adapter: ObservabilityAdapter,
    tally: Mutex<Tally>,
    log: Arc<StreamLog>,
}

impl std::fmt::Debug for DecisionStream {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DecisionStream")
            .field("workers", &self.pool.workers())
            .field("in_flight", &self.pool.in_flight())
            .finish_non_exhaustive()
    }
}

impl DecisionStream {
    /// Opens the session logger once and starts `concurrency` workers.
    ///
    /// `inputs` carries what `sc-ai jev run` would use: repo root, log and
    /// session directories, `--backend` override, registry, project label,
    /// mode and system prompt override. The stream owns its cancel token:
    /// it replaces any token set on `inputs`, and [`Self::cancel`] is the
    /// only way to cancel, so every request in flight sees it.
    ///
    /// # Errors
    ///
    /// Any error of root discovery or logger start.
    pub fn start(inputs: ComposeInputs, concurrency: Concurrency) -> Result<Self, JevError> {
        let cancel = CancelToken::new();
        let inputs = inputs.cancel(cancel.clone());
        let resolved = inputs.resolve()?;
        let adapter = ObservabilityAdapter::open(inputs.policy(), &resolved.log_root)?;
        let log = Arc::new(StreamLog);
        let engine = Arc::new(Engine {
            log_context: inputs.log_context(&resolved),
            session_log: adapter.shared(),
            registries: Mutex::new(HashMap::new()),
            log: Arc::clone(&log),
            inputs,
            resolved,
        });
        let workers = concurrency.get();
        // Two jobs of headroom per worker on each side keeps workers fed
        // without letting the queues grow.
        let queue = workers * 2;
        log.queue_started(workers, queue);
        let on_run = Arc::clone(&engine);
        let on_skip = Arc::clone(&engine);
        let pool = WorkerPool::start(
            workers,
            queue,
            cancel,
            Arc::new(move |job: &Job| on_run.answer(job)),
            Arc::new(move |job: &Job, why| on_skip.skipped(job, why)),
            Arc::new(|job: &Job| lost(job)),
        );
        Ok(Self {
            pool,
            adapter,
            tally: Mutex::new(Tally {
                answered: 0,
                failed: 0,
                all_retryable: true,
            }),
            log,
        })
    }

    /// Queues one input line (`{"id":..,"request":..}`), waiting at most
    /// `wait` (forever when `None`) for room.
    ///
    /// # Errors
    ///
    /// [`SubmitError::Closed`] or [`SubmitError::Full`].
    pub fn submit_line(&self, line: String, wait: Option<Duration>) -> Result<(), SubmitError> {
        self.push(Job::Line(line), wait)
    }

    /// Queues one validated request, waiting at most `wait` (forever when
    /// `None`) for room.
    ///
    /// An answer that is not read holds a place in the stream: a caller that
    /// submits more than [`Self::capacity`] requests without reading answers
    /// from another thread blocks forever with `wait == None`. Read answers
    /// concurrently, or pass a `wait`.
    ///
    /// # Errors
    ///
    /// [`SubmitError::Closed`] or [`SubmitError::Full`].
    pub fn submit_request(
        &self,
        request: StreamRequest,
        wait: Option<Duration>,
    ) -> Result<(), SubmitError> {
        self.push(Job::Request(request), wait)
    }

    fn submit_rejected(&self, error: JevError) -> Result<(), SubmitError> {
        self.push(Job::Rejected(error), None)
    }

    fn push(&self, job: Job, wait: Option<Duration>) -> Result<(), SubmitError> {
        self.pool.submit(job, wait).map_err(|error| match error {
            PushError::Closed(_) => SubmitError::Closed,
            PushError::Full(_) => SubmitError::Full,
        })
    }

    /// No more requests will be submitted. The stream finishes when the
    /// queue drains and the workers go idle.
    pub fn close_input(&self) {
        self.pool.close_input();
    }

    /// Stops new work and cancels the token carried by every request.
    ///
    /// Requests already queued are not run: each is answered with a
    /// `NETWORK_ERROR` of kind `cancelled`. A request already sent is not
    /// interrupted, but its busy-retry wait ends within 100 ms. Input is
    /// closed. Idempotent.
    pub fn cancel(&self) {
        self.pool.cancel();
    }

    /// The next answer, in completion order; waits for one. `None` once the
    /// stream is finished (input closed, queue empty, workers idle) and every
    /// answer was returned.
    #[must_use]
    pub fn next_answer(&self) -> Option<StreamAnswer> {
        self.pool.recv().map(|answer| self.record(answer))
    }

    /// Like [`Self::next_answer`] but waits at most `wait`.
    #[must_use]
    pub fn next_answer_within(&self, wait: Duration) -> Poll {
        match self.pool.recv_within(Some(wait)) {
            Popped::Item(answer) => Poll::Answer(Box::new(self.record(answer))),
            Popped::Empty => Poll::Pending,
            Popped::Finished => Poll::Finished,
        }
    }

    fn record(&self, answer: StreamAnswer) -> StreamAnswer {
        let mut tally = self.tally.lock().unwrap_or_else(PoisonError::into_inner);
        tally.answered += 1;
        if let Some(retryable) = answer.all_failures_retryable() {
            tally.failed += 1;
            tally.all_retryable &= retryable;
        }
        answer
    }

    /// Requests accepted whose answer was not yet returned.
    #[must_use]
    pub fn in_flight(&self) -> usize {
        self.pool.in_flight()
    }

    /// Workers running a request right now.
    #[must_use]
    pub fn running(&self) -> usize {
        self.pool.running()
    }

    /// The most requests that can be in the stream at once.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.pool.capacity()
    }

    /// Closes the input, discards answers not yet read (they still count in
    /// the summary), waits for the workers, flushes the session log once and
    /// returns the summary. It never hangs on unread answers; it does wait
    /// for requests already sent, which are not interrupted.
    #[must_use]
    pub fn finish(&self) -> StreamSummary {
        self.close_input();
        while self.next_answer().is_some() {}
        self.pool.join();
        let log_flush_warning = self
            .adapter
            .flush()
            .err()
            .map(|error| format!("session log was not flushed: {}", error.message()));
        let tally = self.tally.lock().unwrap_or_else(PoisonError::into_inner);
        self.log
            .queue_finished(tally.answered, tally.failed, self.pool.alive());
        StreamSummary {
            answered: tally.answered,
            failed: tally.failed,
            all_retryable: tally.all_retryable,
            log_flush_warning,
        }
    }

    /// Stops everything and drops unreturned answers, for a caller that is
    /// leaving. Does not wait for in-flight requests.
    pub fn abandon(&self) {
        self.pool.abandon();
    }
}

/// The result of a timed wait for an answer.
#[derive(Debug)]
pub enum Poll {
    /// An answer.
    Answer(Box<StreamAnswer>),
    /// Nothing yet; the stream is still open or still working.
    Pending,
    /// Finished: input closed, queue empty, workers idle.
    Finished,
}

/// A fixed list of validated requests run on a [`DecisionStream`]: a feeder
/// thread submits them while the caller reads answers, so neither side can
/// fill a bounded queue and block the other.
///
/// The caller polls [`Batch::next_within`] (so it can check for interrupts
/// between waits) and ends with [`Batch::finish`]. Dropping a batch that was
/// not finished cancels the stream and drops unread answers, on every early
/// exit.
pub struct Batch {
    stream: Arc<DecisionStream>,
    feeder: Option<std::thread::JoinHandle<()>>,
    finished: bool,
}

impl Batch {
    /// Starts feeding `requests` into `stream` and closes its input after the
    /// last one.
    #[must_use]
    pub fn start(stream: Arc<DecisionStream>, requests: Vec<StreamRequest>) -> Self {
        let feeding = Arc::clone(&stream);
        let feeder = std::thread::spawn(move || {
            for request in requests {
                if feeding.submit_request(request, None).is_err() {
                    break;
                }
            }
            feeding.close_input();
        });
        Self {
            stream,
            feeder: Some(feeder),
            finished: false,
        }
    }

    /// Waits at most `wait` for the next answer.
    #[must_use]
    pub fn next_within(&self, wait: Duration) -> Poll {
        self.stream.next_answer_within(wait)
    }

    /// Stops the batch (cancelling anything not started), waits for the
    /// workers and returns the summary.
    #[must_use]
    pub fn finish(mut self) -> StreamSummary {
        self.finished = true;
        let summary = self.stream.finish();
        if let Some(feeder) = self.feeder.take() {
            // The feeder ends once the input is closed; a panic in it has
            // nobody left to be reported to.
            let _ = feeder.join();
        }
        summary
    }
}

impl Drop for Batch {
    fn drop(&mut self) {
        if !self.finished {
            self.stream.cancel();
            self.stream.abandon();
        }
    }
}

/// Serves JSONL: reads requests from `input` until end of input, writes one
/// answer line per request to `output` in completion order, flushing each
/// line, and returns the summary.
///
/// A reader thread feeds the queue, so a slow `output` applies backpressure
/// all the way to `input`. If writing fails, the stream is cancelled, the
/// remaining answers are dropped, and the write error is returned. A line
/// longer than [`MAX_LINE_BYTES`], or not UTF-8, is answered with an
/// `INPUT_INVALID` line with `"id": null`. Blank lines are skipped.
///
/// # Errors
///
/// `IO_FAILED` when `input` cannot be read or `output` cannot be written.
pub fn serve_jsonl<R, W>(
    stream: &Arc<DecisionStream>,
    input: R,
    output: &mut W,
) -> Result<StreamSummary, JevError>
where
    R: BufRead + Send + 'static,
    W: Write + ?Sized,
{
    let read_failure = Arc::new(Mutex::new(None::<JevError>));
    {
        let stream = Arc::clone(stream);
        let read_failure = Arc::clone(&read_failure);
        // Detached on purpose: after a cancel the reader may sit in a blocking
        // read of stdin that nothing can interrupt; the process ends it.
        drop(std::thread::spawn(move || {
            if let Err(error) = feed(&stream, input) {
                *read_failure.lock().unwrap_or_else(PoisonError::into_inner) = Some(error);
            }
            stream.close_input();
        }));
    }
    let mut write_failure: Option<JevError> = None;
    while let Some(answer) = stream.next_answer() {
        if write_failure.is_some() {
            continue;
        }
        let written = answer.to_line().and_then(|line| {
            output
                .write_all(&line)
                .and_then(|()| output.flush())
                .map_err(|error| JevError::io_failed("stdout", &error))
        });
        if let Err(error) = written {
            write_failure = Some(error);
            stream.cancel();
        }
    }
    let summary = stream.finish();
    if let Some(error) = write_failure {
        return Err(error);
    }
    if let Some(error) = read_failure
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
    {
        return Err(error);
    }
    Ok(summary)
}

/// Reads lines from `input` into the queue until end of input or until the
/// stream refuses work.
fn feed<R: BufRead>(stream: &DecisionStream, mut input: R) -> Result<(), JevError> {
    let mut buffer = Vec::new();
    loop {
        let read = read_line_bounded(&mut input, MAX_LINE_BYTES, &mut buffer)
            .map_err(|error| JevError::io_failed("stream input", &error))?;
        let submitted = match read {
            LineRead::End => return Ok(()),
            LineRead::TooLong => stream.submit_rejected(JevError::input_invalid(format!(
                "input line is longer than {MAX_LINE_BYTES} bytes"
            ))),
            LineRead::Line => match std::str::from_utf8(&buffer) {
                Ok(text) => {
                    let text = text.trim_end_matches(['\n', '\r']);
                    if text.trim().is_empty() {
                        continue;
                    }
                    stream.submit_line(text.to_owned(), None)
                }
                Err(_) => {
                    stream.submit_rejected(JevError::input_invalid("input line is not valid UTF-8"))
                }
            },
        };
        if submitted.is_err() {
            // Closed or cancelled: stop reading.
            return Ok(());
        }
    }
}

enum LineRead {
    /// End of input with no further bytes.
    End,
    /// A line (with its newline, if any) is in the buffer.
    Line,
    /// The line exceeded the cap; it was consumed and discarded.
    TooLong,
}

/// Reads one line into `out`, holding at most `max` bytes: the rest of a
/// longer line is consumed and dropped, so memory stays bounded.
fn read_line_bounded<R: BufRead>(
    reader: &mut R,
    max: usize,
    out: &mut Vec<u8>,
) -> std::io::Result<LineRead> {
    out.clear();
    let mut too_long = false;
    let mut any = false;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(match (too_long, any) {
                (true, _) => LineRead::TooLong,
                (false, true) => LineRead::Line,
                (false, false) => LineRead::End,
            });
        }
        any = true;
        let newline = available.iter().position(|byte| *byte == b'\n');
        let take = newline.map_or(available.len(), |at| at + 1);
        if !too_long {
            if out.len() + take > max {
                too_long = true;
                out.clear();
            } else {
                out.extend_from_slice(&available[..take]);
            }
        }
        reader.consume(take);
        if newline.is_some() {
            return Ok(if too_long {
                LineRead::TooLong
            } else {
                LineRead::Line
            });
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRequest<'a> {
    id: String,
    #[serde(borrow)]
    request: &'a RawValue,
}

#[derive(Debug)]
/// A line that is not a request: the id it still had, if any, and why.
struct BadLine {
    id: Option<String>,
    error: JevError,
}

/// Splits an input line into its id and request text. On failure returns the
/// id when one could still be read, with the error.
fn split_line(line: &str) -> Result<(RequestId, &str), Box<BadLine>> {
    match serde_json::from_str::<WireRequest<'_>>(line) {
        Ok(wire) => match RequestId::new(&wire.id) {
            Ok(id) => Ok((id, wire.request.get())),
            Err(error) => Err(Box::new(BadLine { id: None, error })),
        },
        Err(error) => {
            let id = serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .and_then(|value| RequestId::new(value.get("id")?.as_str()?).ok())
                .map(|id| id.0);
            Err(Box::new(BadLine {
                id,
                error: JevError::input_invalid(format!(
                    "stream line must be {{\"id\": string, \"request\": object}}: {error}"
                )),
            }))
        }
    }
}

/// The caller's id of `job`, when it has a usable one. Pure: no logging.
fn job_id(job: &Job) -> Option<String> {
    match job {
        Job::Rejected(_) => None,
        Job::Request(request) => Some(request.id.0.clone()),
        Job::Line(line) => split_line(line).map_or_else(|failed| failed.id, |(id, _)| Some(id.0)),
    }
}

/// The answer of last resort when even the skipped-job path panicked: the
/// caller's id and a typed error, and nothing that can fail.
fn lost(job: &Job) -> StreamAnswer {
    StreamAnswer::failed(
        job_id(job),
        JevError::io_failed_with(
            "stream worker (internal error)",
            std::io::ErrorKind::Other,
            Some("Report this if it repeats.".to_owned()),
        ),
    )
}

/// Shared by every worker: everything one request needs.
struct Engine {
    inputs: ComposeInputs,
    resolved: ResolvedPaths,
    session_log: SharedSessionLog,
    log_context: LogContext,
    /// Registries built from the environment, one per backend selection, so
    /// a backend's client (and its connection pool) is shared by all workers.
    registries: Mutex<HashMap<Vec<BackendId>, Arc<BackendRegistry>>>,
    log: Arc<StreamLog>,
}

impl Engine {
    fn answer(&self, job: &Job) -> StreamAnswer {
        let answer = match job {
            Job::Rejected(error) => StreamAnswer::failed(None, error.clone()),
            Job::Line(line) => match split_line(line) {
                Ok((id, request)) => self.answer_request(id, request),
                Err(failed) => StreamAnswer::failed(failed.id, failed.error),
            },
            Job::Request(request) => self.answer_request(request.id.clone(), &request.json),
        };
        if let Err(error) = answer.outcome() {
            self.log.request_failed(answer.id(), error.code());
        }
        answer
    }

    fn answer_request(&self, id: RequestId, request_json: &str) -> StreamAnswer {
        self.log.request_started(Some(id.as_str()));
        let outcome = CallerRequest::from_json(request_json.as_bytes())
            .and_then(|request| self.run_request(request));
        StreamAnswer {
            id: Some(id.0),
            outcome,
        }
    }

    fn skipped(&self, job: &Job, why: Skipped) -> StreamAnswer {
        let id = job_id(job);
        let error = match why {
            Skipped::Cancelled => JevError::network(
                NetworkErrorKind::Cancelled,
                "the stream was cancelled before this request started",
            ),
            Skipped::Panicked => JevError::io_failed_with(
                "stream worker (internal error while running this request)",
                std::io::ErrorKind::Other,
                Some("Retry the request; report this if it repeats.".to_owned()),
            ),
        };
        self.log.request_failed(id.as_deref(), error.code());
        StreamAnswer::failed(id, error)
    }

    fn run_request(&self, request: CallerRequest) -> Result<RunResponse, JevError> {
        let selected = self
            .inputs
            .selected_backends()
            .or_else(|| request.backends())
            .cloned()
            .unwrap_or_default();
        let registry = self.registry_for(&selected)?;
        let config = self
            .inputs
            .run_config(
                &self.resolved,
                Arc::clone(&self.session_log),
                self.log_context.clone(),
            )
            .registry(registry);
        run(prepare(CheckSource::CallerRequest(request), config)?)
    }

    fn registry_for(&self, selected: &BackendSet) -> Result<Arc<BackendRegistry>, JevError> {
        if let Some(injected) = self.inputs.injected_registry() {
            return Ok(Arc::clone(injected));
        }
        let key: Vec<BackendId> = selected.iter().collect();
        let mut cache = self
            .registries
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(found) = cache.get(&key) {
            return Ok(Arc::clone(found));
        }
        let built = Arc::new(BackendRegistry::from_env_with_models(
            &self.resolved.repo_root,
            selected,
            &ModelAliases::builtin(),
        )?);
        cache.insert(key, Arc::clone(&built));
        Ok(built)
    }
}

#[cfg(test)]
mod tests;
