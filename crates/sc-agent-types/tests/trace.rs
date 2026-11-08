//! The per-send trace handle (boundary:send-trace).

use sc_agent_types::{SendTrace, Sha256Digest};

#[test]
fn test_send_trace_first_write_wins() {
    let trace = SendTrace::new();
    let reader = trace.clone();
    assert_eq!(reader.request_sha256(), None);
    let first = Sha256Digest::of_text("first");
    trace.record(first.clone());
    trace.record(Sha256Digest::of_text("second"));
    assert_eq!(reader.request_sha256(), Some(first.clone()));
    // A fresh trace is independent of the first.
    assert_eq!(SendTrace::new().request_sha256(), None);
    // A clone made after the write sees the same digest.
    assert_eq!(trace.clone().request_sha256(), Some(first));
}
