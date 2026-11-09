//! The one place streaming writes lifecycle log events (ADR-SAG-0032).
//!
//! Streaming logs through two ports: the **session logger**
//! ([`SessionLog`](sc_agent_types::observability::v2::SessionLog)), which the
//! engine already feeds one row per judge send (workers share the one
//! adapter, so concurrent writing is `sc-observability`'s), and the
//! **general logger**, for lifecycle events: queue start and end, worker
//! counts, and per-request start and failure ids.
//!
//! The general-logging port lands on `feat/general-logging`. Until this
//! branch is rebased on it, every lifecycle call goes through [`StreamLog`]
//! and the bodies below record nothing. After the rebase, only this file
//! changes: each method sends one event through the injected general logger.
//! No other module logs.
//!
//! # What may be logged
//!
//! Counts, the caller's request id, and error codes. Never a secret, a
//! prompt, a question, a state value, a payload, or a message text: the
//! methods take no such argument, so a call site cannot pass one.

use sc_agent_types::ErrorCode;

/// Lifecycle events of one stream.
#[derive(Debug, Default)]
pub(crate) struct StreamLog;

impl StreamLog {
    /// The queue and its workers started.
    pub(crate) fn queue_started(&self, _workers: usize, _queue_capacity: usize) {}

    /// A worker began request `id` (`None` when the line had no usable id).
    pub(crate) fn request_started(&self, _id: Option<&str>) {}

    /// Request `id` ended in a typed failure with this stable `code`.
    pub(crate) fn request_failed(&self, _id: Option<&str>, _code: ErrorCode) {}

    /// The queue drained and the workers exited.
    pub(crate) fn queue_finished(&self, _answered: u64, _failed: u64, _workers_alive: usize) {}
}
