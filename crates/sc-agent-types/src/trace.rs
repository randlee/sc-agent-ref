//! The per-send trace handle (ADR-SAG-0026 r-7 amendment, REQ-SAG-0613 seam).
//!
//! Std-only and always on, next to [`CancelToken`](crate::CancelToken).

use std::sync::{Arc, OnceLock};

use crate::ids::RequestSha256;

/// One send's request digest.
///
/// `OnceLock` behind an `Arc`: `sc-agent-http` records through one handle
/// while the engine reads through another, and a send records once. Clones
/// share the same slot.
///
/// # Examples
///
/// ```
/// use sc_agent_types::{RequestSha256, SendTrace};
///
/// let trace = SendTrace::new();
/// let read_side = trace.clone();
/// assert_eq!(read_side.request_sha256(), None);
/// trace.record(RequestSha256::of_text("body"));
/// assert!(read_side.request_sha256().is_some());
/// ```
#[derive(Debug, Clone, Default)]
pub struct SendTrace(Arc<OnceLock<RequestSha256>>);

impl SendTrace {
    /// An empty trace: nothing recorded yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the digest of the exact body sent. The first write wins; a
    /// later write is ignored.
    pub fn record(&self, digest: RequestSha256) {
        // A second write is ignored by contract (first write wins).
        let _ = self.0.set(digest);
    }

    /// The recorded digest, or `None` when no body was built.
    #[must_use]
    pub fn request_sha256(&self) -> Option<RequestSha256> {
        self.0.get().cloned()
    }
}
