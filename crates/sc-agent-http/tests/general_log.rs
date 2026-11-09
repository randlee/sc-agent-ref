//! The HTTP layer's general log never carries a URL credential (REQ-QA-002).

use std::sync::Arc;

use sc_agent_http::send_with_busy_retry;
use sc_agent_http::{BusyPolicy, Endpoint, HttpRequest, HttpResponse, Method, Transport};
use sc_agent_types::observability::v2::RecordingGeneralLog;
use sc_agent_types::{CancelToken, JevError, SendTrace};

struct Ok200;

impl Transport for Ok200 {
    fn send(&self, _request: &HttpRequest) -> Result<HttpResponse, JevError> {
        Ok(HttpResponse::new(200, Vec::new(), b"{}".to_vec()))
    }
}

#[test]
fn userinfo_and_query_secrets_are_absent_from_recorded_events() {
    let endpoint = Endpoint::parse(
        "https://user:USERINFO-SECRET@api.example.test:8443/v1/x?key=QUERY-SECRET#frag",
    )
    .expect("endpoint");
    let request = HttpRequest::new(Method::Post, endpoint, Arc::from("{}"));
    let log = RecordingGeneralLog::new();
    let policy = BusyPolicy::default().with_general_log(Arc::new(log.clone()));
    send_with_busy_retry(
        &Ok200,
        &request,
        &policy,
        &CancelToken::new(),
        SendTrace::new(),
    )
    .expect("sent");
    let sent = log
        .events()
        .into_iter()
        .find(|event| event.action() == "http.send")
        .expect("send event");
    assert_eq!(sent.fields()["endpoint"], "https://api.example.test/v1/x");
    let rendered = log.rendered();
    for secret in ["USERINFO-SECRET", "QUERY-SECRET", "user:", "frag", "8443"] {
        assert!(!rendered.contains(secret), "{secret} in {rendered}");
    }
}
