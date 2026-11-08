//! Request digest recording at the send site (REQ-SAG-0613, ADR-SAG-0005).
//!
//! Needs `--features test-util`: the tests drive a scripted transport and a
//! fake-clock busy policy, so nothing sleeps.

use std::sync::Arc;
use std::time::Duration;

use sc_agent_http::test_util::{FakeClock, RecordingSleeper, ScriptedTransport};
use sc_agent_http::{
    BusyPolicy, Endpoint, HttpRequest, HttpResponse, Method, SecretHeader, send_with_busy_retry,
};
use sc_agent_types::{BackendKeyId, CancelToken, ResolvedApiKey, SendTrace, Sha256Digest};

const BODY: &str = "{\"same\":\"body\"}";
const KEY: &str = "sk-trace-secret";

fn request() -> HttpRequest {
    let key = ResolvedApiKey::try_new(BackendKeyId::OpenAi, KEY.to_owned()).expect("key");
    HttpRequest::new(
        Method::Post,
        Endpoint::parse("https://api.example.test/v1/x").expect("https"),
        Arc::from(BODY),
    )
    .with_secret(SecretHeader::bearer(key))
}

fn busy() -> HttpResponse {
    HttpResponse::new(429, Vec::new(), Vec::new())
}

fn ok() -> HttpResponse {
    HttpResponse::new(200, Vec::new(), b"{}".to_vec())
}

fn fake_policy() -> BusyPolicy {
    let clock = FakeClock::new();
    let sleeper = RecordingSleeper::new(clock.clone());
    BusyPolicy::with_test_hooks(clock, sleeper, Duration::ZERO)
}

/// The recorded value is the digest of the body the transport saw, not of
/// the headers (the key is not hashed).
#[test]
fn test_request_sha256_is_digest_of_sent_body() {
    let transport = ScriptedTransport::responses(vec![ok()]);
    let trace = SendTrace::new();
    send_with_busy_retry(
        &transport,
        &request(),
        &fake_policy(),
        &CancelToken::new(),
        trace.clone(),
    )
    .expect("sent");
    let sent = transport.requests();
    assert_eq!(sent.len(), 1);
    assert_eq!(
        trace.request_sha256(),
        Some(Sha256Digest::of_text(sent[0].body()))
    );
    assert_eq!(trace.request_sha256(), Some(Sha256Digest::of_text(BODY)));
}

/// Four busy replies and a success: five sends, one recorded value.
#[test]
fn test_request_sha256_recorded_once_across_retries() {
    let transport = ScriptedTransport::responses(vec![busy(), busy(), busy(), busy(), ok()]);
    let trace = SendTrace::new();
    send_with_busy_retry(
        &transport,
        &request(),
        &fake_policy(),
        &CancelToken::new(),
        trace.clone(),
    )
    .expect("fifth send succeeds");
    assert_eq!(transport.sends(), 5);
    assert_eq!(trace.request_sha256(), Some(Sha256Digest::of_text(BODY)));
}

/// A cancel seen before the first attempt sends nothing and records nothing.
#[test]
fn test_cancel_before_first_send_records_nothing() {
    let transport = ScriptedTransport::responses(vec![ok()]);
    let cancel = CancelToken::new();
    cancel.cancel();
    let trace = SendTrace::new();
    send_with_busy_retry(
        &transport,
        &request(),
        &fake_policy(),
        &cancel,
        trace.clone(),
    )
    .expect_err("cancelled");
    assert_eq!(transport.sends(), 0);
    assert_eq!(trace.request_sha256(), None);
}
