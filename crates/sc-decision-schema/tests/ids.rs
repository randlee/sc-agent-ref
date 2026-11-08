//! `QuestionId` and `ChoiceId` (moved from `sc-agent-types`, REQ-SAG-0141).

use sc_decision_schema::{ChoiceId, QuestionId};

#[test]
fn test_question_and_choice_ids_validate_on_every_path() {
    assert_eq!(
        QuestionId::try_from("relevant").expect("id").as_str(),
        "relevant"
    );
    assert_eq!(ChoiceId::try_from("yes").expect("id").as_str(), "yes");
    for bad in ["", "a\nb", "\t"] {
        assert!(QuestionId::try_from(bad).is_err());
        assert!(ChoiceId::try_from(bad).is_err());
        assert!(serde_json::from_value::<ChoiceId>(serde_json::json!(bad)).is_err());
        assert!(serde_json::from_value::<QuestionId>(serde_json::json!(bad)).is_err());
    }
}
