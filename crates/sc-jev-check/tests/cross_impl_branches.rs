//! Consumer digest for the a-11 branches fixture (README tighten #2).
//!
//! The payload is built through `run::prepare` (the one public assembly
//! path, ADR-SAG-0005) and compared with a payload encoded independently
//! from the fixture's subjects.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use sc_agent_types::observability::v2::NoopSessionLog;
use sc_agent_types::schema::subjects::SubjectsDocument;
use sc_agent_types::{
    BackendId, BackendSet, CanonicalJudgePayload, PayloadBundle, RepoRelativePath, RepoRoot,
};
use sc_jev_check::config::{CheckSpec, InputSpec};
use sc_jev_check::run::{CheckSource, RunConfig, prepare, run};

const QUESTIONS: &str = r#"{
  "schema": "plan-anchoring/questions/1",
  "version": 1,
  "questions": {
    "branches": {
      "type": "choice",
      "instructions": "Which branch applies?\n\n{branches}",
      "criteria": {"no": "No.", "yes": "Yes."}
    }
  }
}"#;

#[test]
fn cross_impl_branches() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/cross-impl/branches/subjects.json");
    let bytes = std::fs::read(&fixture).expect("subjects.json");
    let document: SubjectsDocument = serde_json::from_slice(&bytes).expect("subjects document");
    assert_eq!(document.subjects().len(), 6);
    let joined = document
        .subjects()
        .iter()
        .map(|subject| subject.text())
        .collect::<Vec<_>>()
        .join("\n");
    // The canonical form of QUESTIONS: id, prompt, sorted choices.
    let expected: CanonicalJudgePayload = serde_json::from_value(serde_json::json!({
        "questions": [{
            "id": "branches",
            "prompt": format!("Which branch applies?\n\n{joined}"),
            "choices": ["no", "yes"]
        }],
        "state": []
    }))
    .expect("expected payload");
    let expected = PayloadBundle::encode(&expected).expect("expected bundle");

    let temp = tempfile::tempdir().expect("temp");
    let dir = temp.path();
    std::fs::write(dir.join("subjects.json"), &bytes).expect("copy subjects");
    std::fs::write(dir.join("questions.json"), QUESTIONS).expect("questions file");
    let mut spec = CheckSpec::new(RepoRelativePath::try_from("questions.json").expect("path"));
    spec.inputs = BTreeMap::from([(
        "branches".into(),
        InputSpec::Subjects {
            path: RepoRelativePath::try_from("subjects.json").expect("path"),
        },
    )]);
    let config = RunConfig::new(RepoRoot::new(dir), Arc::new(NoopSessionLog))
        .backends(BackendSet::try_from(vec![BackendId::Jev]).expect("set"))
        .dry_run(true);
    let prepared = prepare(CheckSource::Spec(spec), config).expect("prepare");
    let payload = prepared.request().payload().as_str();
    assert!(payload.contains("Transition.GIVE_BACK"));
    assert!(payload.contains("Transition.KEEP"));
    assert!(payload.contains("Transition.STOP"));
    assert_eq!(prepared.prompt_sha256(), expected.sha256());
    assert_eq!(prepared.request().payload().bytes(), expected.bytes());

    let producer = prepared.producer().expect("producer");
    assert_eq!(producer.name, "sc-treesitter");
    assert_eq!(producer.language, document.producer().language);
    assert_eq!(producer.query_sha256, document.producer().query_sha256);
    assert_eq!(producer.mode, "matches");

    let dry = run(prepared).expect("dry-run");
    assert_eq!(dry.prompt_sha256(), expected.sha256());
    assert!(dry.session_id().is_none());
    assert!(dry.results().is_empty());
    assert_eq!(dry.equal_input().producer(), Some(producer_of(&document)));
}

fn producer_of(document: &SubjectsDocument) -> &sc_agent_types::schema::subjects::SubjectsProducer {
    document.producer()
}
