//! Judge contract (REQ-SAG-0103 re-frozen at r-1 merge, D10-break).

use std::collections::BTreeMap;
use std::sync::Arc;

use sc_agent_types::{BackendId, JevError, PayloadSha256, SendTrace};
use sc_decision_schema::{
    Attestation, BackendAttestation, BackendCapabilities, CanonicalJudgePayload, ChoiceId,
    EqualInputRecord, JudgeBackend, JudgeOutcome, JudgeRequest, LlmWireContext, MultiBackendMode,
    PayloadBundle, Probability, QuestionAnswer, QuestionId,
};
use serde_json::json;

fn bundle() -> PayloadBundle {
    let payload: CanonicalJudgePayload = serde_json::from_value(json!({
        "questions": [
            {"id": "relevant", "prompt": "Does it apply?", "choices": ["no", "yes"]},
            {"id": "risk", "prompt": "Is it risky?", "choices": ["high", "low"]}
        ],
        "state": [{"question_id": "relevant", "choice": "yes"}]
    }))
    .expect("payload");
    PayloadBundle::encode(&payload).expect("bundle")
}

fn request() -> JudgeRequest {
    JudgeRequest::new(Arc::new(bundle()), None)
}

fn choice(text: &str) -> ChoiceId {
    ChoiceId::try_from(text).expect("choice")
}

fn prob(value: f64) -> Probability {
    Probability::new(value).expect("probability")
}

fn qid(text: &str) -> QuestionId {
    QuestionId::try_from(text).expect("id")
}

fn answers(relevant: &str, risk: &str) -> BTreeMap<QuestionId, QuestionAnswer> {
    BTreeMap::from([
        (
            qid("relevant"),
            QuestionAnswer::new(choice(relevant), None, None),
        ),
        (qid("risk"), QuestionAnswer::new(choice(risk), None, None)),
    ])
}

/// Byte-identical serde for the three answer keys: a real a-10 Jev answer
/// (with `confidence`) and an LLM answer without it.
#[test]
fn test_answers_serde_golden() {
    let jev = r#"{"choice":"no","confidence":0.9,"probabilities":{"no":0.9,"yes":0.1}}"#;
    let parsed: QuestionAnswer = serde_json::from_str(jev).expect("jev answer");
    assert_eq!(parsed.confidence().map(Probability::get), Some(0.9));
    assert_eq!(
        parsed.probabilities().expect("probabilities")[&choice("yes")].get(),
        0.1
    );
    assert_eq!(serde_json::to_string(&parsed).expect("write"), jev);

    let llm = r#"{"choice":"yes"}"#;
    let parsed: QuestionAnswer = serde_json::from_str(llm).expect("llm answer");
    assert_eq!(parsed.confidence(), None);
    assert!(parsed.probabilities().is_none());
    assert_eq!(serde_json::to_string(&parsed).expect("write"), llm);

    let map: BTreeMap<QuestionId, QuestionAnswer> =
        serde_json::from_str(&format!(r#"{{"q":{jev},"r":{llm}}}"#)).expect("map");
    assert_eq!(
        serde_json::to_string(&map).expect("write"),
        format!(r#"{{"q":{jev},"r":{llm}}}"#)
    );
    assert_eq!(
        map[&qid("r")].to_choice_answer(&qid("r")),
        sc_decision_schema::ChoiceAnswer {
            question_id: qid("r"),
            choice: choice("yes")
        }
    );
}

#[test]
fn test_question_answer_rejects_out_of_range_on_read() {
    // The range is held by `Probability`, so a typed answer cannot carry a bad
    // number and the wire reader refuses one.
    for text in [
        r#"{"choice":"yes","confidence":1.5}"#,
        r#"{"choice":"yes","confidence":-0.1}"#,
        r#"{"choice":"yes","probabilities":{"yes":-1}}"#,
        r#"{"choice":"yes","probabilities":{"yes":2}}"#,
    ] {
        let error = serde_json::from_str::<QuestionAnswer>(text).expect_err(text);
        assert!(error.to_string().contains("number in [0, 1]"), "{error}");
    }
    for text in [
        r#"{"choice":"yes","confidence":0}"#,
        r#"{"choice":"yes","confidence":1}"#,
    ] {
        assert!(
            serde_json::from_str::<QuestionAnswer>(text).is_ok(),
            "{text}"
        );
    }
}

#[test]
fn test_outcome_rejects_mismatched_answer_keys() {
    let request = request();
    let mut missing = answers("yes", "low");
    missing.remove(&qid("risk"));
    let error = JudgeOutcome::new(BackendId::Jev, &request, missing).expect_err("missing");
    assert_eq!(error.code(), "RESPONSE_INVALID");
    let mut extra = answers("yes", "low");
    extra.insert(qid("other"), QuestionAnswer::new(choice("yes"), None, None));
    let error = JudgeOutcome::new(BackendId::Jev, &request, extra).expect_err("extra");
    assert_eq!(error.code(), "RESPONSE_INVALID");
    let ok = JudgeOutcome::new(BackendId::Jev, &request, answers("yes", "low")).expect("ok");
    assert_eq!(ok.answers().len(), 2);
    assert!(ok.input_sha256().is_none());
}

#[test]
fn test_outcome_rejects_choice_not_offered() {
    let request = request();
    let error =
        JudgeOutcome::new(BackendId::Jev, &request, answers("maybe", "low")).expect_err("choice");
    assert_eq!(error.code(), "RESPONSE_INVALID");
    let mut with_probabilities = answers("yes", "low");
    with_probabilities.insert(
        qid("relevant"),
        QuestionAnswer::new(
            choice("yes"),
            Some(prob(0.8)),
            Some(BTreeMap::from([
                (choice("yes"), prob(0.8)),
                (choice("perhaps"), prob(0.2)),
            ])),
        ),
    );
    let error = JudgeOutcome::new(BackendId::Jev, &request, with_probabilities)
        .expect_err("probability key");
    assert_eq!(error.code(), "RESPONSE_INVALID");
}

#[test]
fn test_equal_input_equal_is_derived() {
    let digest = bundle().sha256().clone();
    let matched = EqualInputRecord::new(
        digest.clone(),
        MultiBackendMode::Independent,
        vec![
            BackendAttestation::new(BackendId::Jev, Attestation::Matched),
            BackendAttestation::new(BackendId::AnthropicHaiku, Attestation::Matched),
        ],
        false,
        None,
    )
    .expect("record");
    assert!(matched.equal());
    let value = serde_json::to_value(&matched).expect("json");
    assert_eq!(value["equal"], true);
    assert_eq!(value["mode"], "independent");
    assert_eq!(
        serde_json::from_value::<EqualInputRecord>(value.clone()).expect("read"),
        matched
    );

    let mut forged = value;
    forged["equal"] = json!(false);
    assert!(serde_json::from_value::<EqualInputRecord>(forged).is_err());

    for other in [
        Attestation::Mismatched {
            reported: PayloadSha256::of_text("other"),
        },
        Attestation::Unattested,
        Attestation::Failed,
        Attestation::NotSent,
    ] {
        let record = EqualInputRecord::new(
            digest.clone(),
            MultiBackendMode::ChainContext,
            vec![
                BackendAttestation::new(BackendId::Jev, Attestation::Matched),
                BackendAttestation::new(BackendId::OpenAiLuna, other.clone()),
            ],
            false,
            None,
        )
        .expect("record");
        assert!(!record.equal(), "{other:?}");
        let value = serde_json::to_value(&record).expect("json");
        assert_eq!(value["equal"], false);
        let back: EqualInputRecord = serde_json::from_value(value).expect("read");
        assert_eq!(back, record);
    }
    let empty = EqualInputRecord::new(
        digest.clone(),
        MultiBackendMode::default(),
        Vec::new(),
        false,
        None,
    );
    assert_eq!(empty.expect_err("empty").code(), "INPUT_INVALID");
    let twice = EqualInputRecord::new(
        digest,
        MultiBackendMode::default(),
        vec![
            BackendAttestation::new(BackendId::Jev, Attestation::Matched),
            BackendAttestation::new(BackendId::Jev, Attestation::Matched),
        ],
        false,
        None,
    );
    assert_eq!(twice.expect_err("duplicate").code(), "INPUT_INVALID");
    assert_eq!(MultiBackendMode::default(), MultiBackendMode::Independent);
}

#[test]
fn test_payload_bundle_encode_and_question_choices() {
    let bundle = bundle();
    assert_eq!(bundle.sha256(), &PayloadSha256::of_text(bundle.as_str()));
    let ids: Vec<&str> = bundle.question_ids().map(QuestionId::as_str).collect();
    assert_eq!(ids, ["relevant", "risk"]);
    assert!(bundle.question_choices()[&qid("risk")].contains(&choice("high")));
    assert_eq!(
        PayloadBundle::decode(bundle.bytes()).expect("decode"),
        bundle
    );
    let duplicate: CanonicalJudgePayload = serde_json::from_value(json!({
        "questions": [
            {"id": "q", "prompt": "a", "choices": ["x", "z"]},
            {"id": "q", "prompt": "b", "choices": ["y", "z"]}
        ],
        "state": []
    }))
    .expect("payload");
    assert_eq!(
        PayloadBundle::encode(&duplicate).expect_err("dup").code(),
        "INPUT_INVALID"
    );
}

/// `capabilities()` is a provided method that reads the descriptor; an
/// implementor restates nothing.
#[test]
fn test_capabilities_come_from_the_descriptor() {
    struct Minimal;

    impl JudgeBackend for Minimal {
        fn id(&self) -> BackendId {
            BackendId::OpenAiLuna
        }

        fn validate_for_send(&self, _req: &JudgeRequest) -> Result<(), JevError> {
            Ok(())
        }

        fn judge(&self, req: &JudgeRequest, _trace: &SendTrace) -> Result<JudgeOutcome, JevError> {
            JudgeOutcome::new(self.id(), req, BTreeMap::new())
        }
    }

    let caps = Minimal.capabilities();
    assert_eq!(
        caps,
        BackendCapabilities::from_descriptor(BackendId::OpenAiLuna.descriptor())
    );
    assert!(caps.requires_llm_context());
    assert!(caps.supports_session_ask());
    assert_eq!(caps.request_limit_bytes(), 96_000);
    let request = JudgeRequest::new(Arc::new(bundle()), Some(LlmWireContext::new("p")));
    assert!(request.llm().is_some());
    assert!(!request.cancel().is_cancelled());
    let cancel = sc_agent_types::CancelToken::new();
    let request = request.with_cancel(cancel.clone());
    cancel.cancel();
    assert!(request.cancel().is_cancelled());
}

/// Question-kind rules (ADR-SAG-0002 r-5 amendment): one validating
/// constructor, also run by deserialization.
#[test]
fn test_question_kind_rules() {
    use sc_decision_schema::{Question, QuestionKind};

    let labels = |count: usize| -> Vec<ChoiceId> {
        (0..count)
            .map(|index| choice(&format!("c{index}")))
            .collect()
    };
    let make = |kind, count: usize, descriptions: Option<Vec<String>>| {
        Question::try_new(qid("q"), "p", labels(count), kind, descriptions)
    };
    let text = |count: usize| {
        (0..count)
            .map(|index| format!("d{index}"))
            .collect::<Vec<_>>()
    };

    // Accepted shapes at the limits.
    assert!(make(QuestionKind::Choice, 2, None).is_ok());
    assert!(make(QuestionKind::Choice, 255, None).is_ok());
    assert!(make(QuestionKind::Score, 2, None).is_ok());
    assert!(make(QuestionKind::Score, 10, Some(text(10))).is_ok());
    assert!(make(QuestionKind::Noul, 0, None).is_ok());

    // Each violation is INPUT_INVALID, for the constructor and for bytes.
    let violations = [
        make(QuestionKind::Choice, 1, None),
        make(QuestionKind::Choice, 256, None),
        make(QuestionKind::Score, 1, None),
        make(QuestionKind::Score, 11, None),
        make(QuestionKind::Noul, 2, None),
        make(QuestionKind::Noul, 0, Some(text(1))),
        make(QuestionKind::Choice, 3, Some(text(2))),
        Question::try_new(
            qid("q"),
            "p",
            vec![choice("a"), choice("a")],
            QuestionKind::Choice,
            None,
        ),
    ];
    for result in violations {
        assert_eq!(result.expect_err("violation").code(), "INPUT_INVALID");
    }
    for bytes in [
        r#"{"id":"q","prompt":"p","choices":["a"]}"#,
        r#"{"id":"q","prompt":"p","choices":["a","b"],"kind":"noul"}"#,
        r#"{"id":"q","prompt":"p","choices":["a","b"],"descriptions":["x"]}"#,
        r#"{"id":"q","prompt":"p","choices":["a","b"],"kind":"other"}"#,
        r#"{"id":"q","prompt":"p","choices":["a","b"],"extra":1}"#,
    ] {
        assert!(serde_json::from_str::<Question>(bytes).is_err(), "{bytes}");
    }

    // A choice question writes no `kind`, and descriptions equal to the
    // labels are dropped: the Phase A bytes are unchanged.
    let plain = make(
        QuestionKind::Choice,
        2,
        Some(vec!["c0".to_owned(), "c1".to_owned()]),
    )
    .expect("plain");
    assert_eq!(plain.descriptions(), None);
    assert_eq!(
        serde_json::to_string(&plain).expect("json"),
        r#"{"id":"q","prompt":"p","choices":["c0","c1"]}"#
    );
    let score = make(QuestionKind::Score, 2, Some(text(2))).expect("score");
    assert_eq!(
        serde_json::to_string(&score).expect("json"),
        r#"{"id":"q","prompt":"p","choices":["c0","c1"],"kind":"score","descriptions":["d0","d1"]}"#
    );
    let noul = make(QuestionKind::Noul, 0, None).expect("noul");
    let back: Question =
        serde_json::from_str(&serde_json::to_string(&noul).expect("json")).expect("round trip");
    assert_eq!(back, noul);

    // A noul question's answer names yes or no.
    let payload = CanonicalJudgePayload::new(vec![noul], Vec::new());
    let bundle = PayloadBundle::encode(&payload).expect("bundle");
    assert_eq!(bundle.question_choices()[&qid("q")].len(), 2);
}
