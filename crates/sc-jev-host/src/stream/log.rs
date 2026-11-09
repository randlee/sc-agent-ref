//! The one place streaming writes lifecycle log events (ADR-SAG-0032,
//! ADR-SAG-0013 amendment 2026-11-09).
//!
//! A stream writes to two logs. The **session log** gets the engine's one row
//! per judge send; every worker shares the one adapter, so concurrent writing
//! is `sc-observability`'s. The **general log** gets the events here:
//!
//! | action | level | when | fields |
//! | --- | --- | --- | --- |
//! | `stream.start` | info | the queue and workers started | `workers`, `queue_capacity` |
//! | `stream.request.start` | info | a worker began a request | `stream_id`, `request_id`, `run_id` |
//! | `stream.request.done` | info | a request finished | `ok`, `duration_ms`, `failed_backends` |
//! | `stream.request.failed` | error | a request failed | `stage`, `error`, `retryable`, `duration_ms` |
//! | `stream.request.backend_failed` | warn | one backend of a request failed | `backend`, `error`, `retryable` |
//! | `stream.end` | info | the queue drained and the workers exited | `answered`, `failed`, `workers_alive`, `duration_ms` |
//!
//! # Correlation
//!
//! The stream has its own id (`stream_id`, the `run_id` of `stream.start` and
//! `stream.end`). Each request is a run with its own fresh `run_id`, stamped
//! on every event of that request, the engine's included (`prepare.*`,
//! `run.*`, `send.*`, backend and HTTP events), because the request's
//! [`CorrelatedGeneralLog`] is what the engine receives. Every request event
//! also carries `stream_id` and the caller's `request_id`, so a request's rows
//! are picked out by `run_id` and tied to the caller's id and to the stream.
//!
//! # What may be logged
//!
//! Counts, durations, the caller's request id, backend ids and stable error
//! ids. Never a secret, a prompt, a question, a state value, a payload or a
//! message text: the methods take none. A request id is caller text, stored as
//! given, so a caller must not put a secret in it.

use std::sync::Arc;
use std::time::Instant;

use sc_agent_types::JevError;
use sc_agent_types::observability::v2::{
    CorrelatedGeneralLog, GeneralEvent, SharedGeneralLog, new_run_id,
};
use sc_jev_check::run::{BackendStatus, RunResponse};

const TARGET: &str = "sc_jev_host::stream";

/// Lifecycle events of one stream.
pub(crate) struct StreamLog {
    /// The general log, stamped with the stream's own id.
    general: SharedGeneralLog,
    stream_id: String,
    started: Instant,
}

impl StreamLog {
    /// `general` is stamped with `stream_id` already (see `Logs::open`).
    pub(crate) fn new(general: SharedGeneralLog, stream_id: String) -> Self {
        Self {
            general,
            stream_id,
            started: Instant::now(),
        }
    }

    /// The queue and its workers started.
    pub(crate) fn queue_started(&self, workers: usize, queue_capacity: usize) {
        GeneralEvent::info(TARGET, "stream.start", "stream started")
            .with_field("stream_id", self.stream_id.as_str())
            .with_field("workers", workers)
            .with_field("queue_capacity", queue_capacity)
            .emit(self.general.as_ref());
    }

    /// The queue drained and the workers exited.
    pub(crate) fn queue_finished(&self, answered: u64, failed: u64, workers_alive: usize) {
        GeneralEvent::info(TARGET, "stream.end", "stream finished")
            .with_field("stream_id", self.stream_id.as_str())
            .with_field("answered", answered)
            .with_field("failed", failed)
            .with_field("workers_alive", workers_alive)
            .with_field("duration_ms", millis(self.started))
            .emit(self.general.as_ref());
    }

    /// A request begins: its own run, its own `run_id`.
    pub(crate) fn request(&self, id: Option<&str>) -> RequestLog {
        let run_id = new_run_id();
        RequestLog {
            general: Arc::new(CorrelatedGeneralLog::new(
                Arc::clone(&self.general),
                run_id.clone(),
            )),
            stream_id: self.stream_id.clone(),
            request_id: id.map(str::to_owned),
            run_id,
            started: Instant::now(),
        }
    }
}

/// The events of one request, all stamped with its `run_id`.
pub(crate) struct RequestLog {
    general: SharedGeneralLog,
    stream_id: String,
    request_id: Option<String>,
    run_id: String,
    started: Instant,
}

impl RequestLog {
    /// The log the engine receives for this request's run.
    pub(crate) fn shared(&self) -> SharedGeneralLog {
        Arc::clone(&self.general)
    }

    fn tag(&self, event: GeneralEvent) -> GeneralEvent {
        let event = event.with_field("stream_id", self.stream_id.as_str());
        match &self.request_id {
            Some(id) => event.with_field("request_id", id.as_str()),
            None => event,
        }
    }

    /// A worker began this request.
    pub(crate) fn started(&self) {
        self.tag(GeneralEvent::info(
            TARGET,
            "stream.request.start",
            "request started",
        ))
        .with_field("run_id", self.run_id.as_str())
        .emit(self.general.as_ref());
    }

    /// The request ended in a typed failure at `stage` (`read`, `parse`,
    /// `registry`, `run`, `cancelled`, `internal`).
    pub(crate) fn failed(&self, stage: &'static str, error: &JevError) {
        self.tag(GeneralEvent::error(
            TARGET,
            "stream.request.failed",
            "request failed",
        ))
        .with_error_code(error.code().as_str())
        .with_field("stage", stage)
        .with_field("retryable", error.retryable())
        .with_field("duration_ms", millis(self.started))
        .emit(self.general.as_ref());
    }

    /// The request produced a response (which may still carry failed
    /// backends, each logged by backend id and error id).
    pub(crate) fn finished(&self, response: &RunResponse) {
        let mut failed_backends = 0_u64;
        for result in response.results() {
            if let BackendStatus::Failed(failure) = result.status() {
                failed_backends += 1;
                let error = failure.error();
                self.tag(GeneralEvent::warn(
                    TARGET,
                    "stream.request.backend_failed",
                    "a backend failed",
                ))
                .with_error_code(error.code().as_str())
                .with_field("backend", result.backend().to_string())
                .with_field("retryable", error.retryable())
                .emit(self.general.as_ref());
            }
        }
        self.tag(GeneralEvent::info(
            TARGET,
            "stream.request.done",
            "request finished",
        ))
        .with_field("ok", failed_backends == 0)
        .with_field("failed_backends", failed_backends)
        .with_field("duration_ms", millis(self.started))
        .emit(self.general.as_ref());
    }
}

fn millis(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}
