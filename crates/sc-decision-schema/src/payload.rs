//! Canonical judge payload bytes (ADR-SAG-0002).
//!
//! # Encoding
//!
//! The canonical payload is compact JSON encoded as UTF-8:
//!
//! - Object keys are emitted in lexicographic byte order. The guarantee is
//!   [`canonical_json_bytes`]'s recursive re-sort: it walks every object and
//!   reinserts keys in sorted order before encoding. It does not depend on
//!   which map [`serde_json::Map`] is, so the bytes stay the same even if
//!   Cargo feature unification turns on `serde_json/preserve_order`.
//! - Strings use `serde_json`'s default escapes (`\"`, `\\`, and `\u00XX` for
//!   controls). Other characters are raw UTF-8, not `\u` escapes.
//! - Arrays keep their element order. Numbers, booleans, and null use
//!   `serde_json`'s compact forms. No extra whitespace is emitted.
//!
//! `prompt_sha256` is SHA-256 of those bytes. LLM system-prompt text is not
//! included.
//!
//! Every [`PayloadBundle`] is built by [`PayloadBundle::encode`] (or
//! [`PayloadBundle::decode`], which re-encodes and compares), so every bundle
//! carries the question and choice map of the payload it encodes.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::ids::{ChoiceId, QuestionId};
use crate::judge::QuestionAnswer;
use sc_agent_types::JevError;
use sc_agent_types::Sha256Digest;

/// The answers of one decision: the map [`JudgeOutcome`](crate::JudgeOutcome)
/// carries for every backend (the same bytes as before the type existed).
pub type DecisionAnswers = BTreeMap<QuestionId, QuestionAnswer>;

/// The type of a [`Question`] (ADR-SAG-0002, r-5 amendment).
///
/// A `choice` question has two to 255 choices, a `score` question has two to
/// ten ordered levels, and a `noul` question has none: the answer is the
/// probability of yes. The default is `Choice`, which is never written to the
/// canonical bytes, so a payload without kinds keeps its Phase A bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum QuestionKind {
    /// Pick one of two to 255 options.
    #[default]
    Choice,
    /// Pick one of two to ten ordered levels.
    Score,
    /// Probability of yes, with no choices and no confidence.
    Noul,
}

impl QuestionKind {
    /// Whether this is [`Self::Choice`], the kind that is not serialized.
    #[must_use]
    pub fn is_choice(&self) -> bool {
        matches!(self, Self::Choice)
    }
}

/// Most choices a `choice` question may have (the documented Jev limit).
const MAX_CHOICE_OPTIONS: usize = 255;
/// Most levels a `score` question may have (the documented Jev limit).
const MAX_SCORE_LEVELS: usize = 10;
/// Fewest choices or levels a `choice` or `score` question may have.
const MIN_OPTIONS: usize = 2;

/// One question the judge answers.
///
/// Fields are private so a later field is additive; build with
/// [`Self::try_new`] (any kind) or
/// deserialize. Deserialization runs [`Self::try_new`], so the kind rules
/// hold for every question read from bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawQuestion")]
pub struct Question {
    id: QuestionId,
    prompt: String,
    choices: Vec<ChoiceId>,
    #[serde(skip_serializing_if = "QuestionKind::is_choice")]
    kind: QuestionKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    descriptions: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawQuestion {
    id: QuestionId,
    prompt: String,
    choices: Vec<ChoiceId>,
    #[serde(default)]
    kind: QuestionKind,
    #[serde(default)]
    descriptions: Option<Vec<String>>,
}

/// `INPUT_INVALID` for a body that did not parse: the serde category and the
/// line and column only, never body bytes, with a hint to fix the body.
pub(crate) fn decode_failed(what: &str, error: &serde_json::Error) -> JevError {
    let category = match error.classify() {
        serde_json::error::Category::Io => "I/O error",
        serde_json::error::Category::Syntax => "syntax error",
        serde_json::error::Category::Data => "data error (a field is missing, unknown or mistyped)",
        serde_json::error::Category::Eof => "unexpected end of input",
    };
    let at = if error.line() == 0 {
        String::new()
    } else {
        format!(" at line {} column {}", error.line(), error.column())
    };
    JevError::input_invalid(format!(
        "{what} is not valid: {category}{at}; fix the {what}"
    ))
}

/// A question broke its kind's rules. The text names the question id, the
/// kind and the counts, never the prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuleViolation(String);

impl fmt::Display for RuleViolation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<RuleViolation> for JevError {
    fn from(violation: RuleViolation) -> Self {
        Self::input_invalid(violation.0)
    }
}

impl TryFrom<RawQuestion> for Question {
    type Error = RuleViolation;

    fn try_from(raw: RawQuestion) -> Result<Self, Self::Error> {
        Self::build(raw.id, raw.prompt, raw.choices, raw.kind, raw.descriptions)
    }
}

impl Question {
    /// A choice question that skips every kind rule (feature `test-util`).
    ///
    /// Tests use it to build a payload the backends must refuse; production
    /// code uses [`Self::try_new`].
    #[cfg(any(test, feature = "test-util"))]
    #[must_use]
    pub fn new_unchecked(
        id: QuestionId,
        prompt: impl Into<String>,
        choices: Vec<ChoiceId>,
    ) -> Self {
        Self {
            id,
            prompt: prompt.into(),
            choices,
            kind: QuestionKind::Choice,
            descriptions: None,
        }
    }

    /// A question of any `kind`, checked against that kind's rules.
    ///
    /// `choice` has two to 255 distinct choices; `score` has two to ten
    /// distinct choices, in level order; `noul` has no choices and no
    /// descriptions. `descriptions`, when present, has exactly one entry per
    /// choice. Descriptions equal to the choice labels are the same as none,
    /// so they are dropped and a payload has one canonical form.
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` when `choices` or `descriptions` break a rule above.
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_decision_schema::{ChoiceId, Question, QuestionId, QuestionKind};
    ///
    /// let id = QuestionId::try_from("risk")?;
    /// let levels = vec![ChoiceId::try_from("low")?, ChoiceId::try_from("high")?];
    /// let question = Question::try_new(id.clone(), "How risky?", levels, QuestionKind::Score, None)?;
    /// assert_eq!(question.kind(), QuestionKind::Score);
    /// assert!(Question::try_new(id, "How risky?", Vec::new(), QuestionKind::Score, None).is_err());
    /// # Ok::<(), sc_agent_types::JevError>(())
    /// ```
    pub fn try_new(
        id: QuestionId,
        prompt: impl Into<String>,
        choices: Vec<ChoiceId>,
        kind: QuestionKind,
        descriptions: Option<Vec<String>>,
    ) -> Result<Self, JevError> {
        Ok(Self::build(id, prompt.into(), choices, kind, descriptions)?)
    }

    fn build(
        id: QuestionId,
        prompt: String,
        choices: Vec<ChoiceId>,
        kind: QuestionKind,
        descriptions: Option<Vec<String>>,
    ) -> Result<Self, RuleViolation> {
        check_kind_rules(&id, &choices, kind, descriptions.as_deref())?;
        let descriptions = descriptions.filter(|list| {
            !list
                .iter()
                .zip(&choices)
                .all(|(description, choice)| description == choice.as_str())
        });
        Ok(Self {
            id,
            prompt,
            choices,
            kind,
            descriptions,
        })
    }

    /// Parses one question from a JSON value and checks its kind's rules.
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` with a fix hint. A question that breaks a kind rule
    /// has a detail naming the id, the kind and the counts; any other
    /// malformed value names only the failure category and position.
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_decision_schema::Question;
    ///
    /// let good = serde_json::json!({"id": "q", "prompt": "?", "choices": ["a", "b"]});
    /// assert!(Question::from_json(good).is_ok());
    /// let bad = serde_json::json!({"id": "q", "prompt": "?", "choices": ["a"]});
    /// assert!(Question::from_json(bad).is_err());
    /// ```
    pub fn from_json(value: Value) -> Result<Self, JevError> {
        let raw: RawQuestion =
            serde_json::from_value(value).map_err(|error| decode_failed("question", &error))?;
        Ok(Self::try_from(raw)?)
    }

    /// The question id.
    #[must_use]
    pub fn id(&self) -> &QuestionId {
        &self.id
    }

    /// The instructions sent with the question.
    #[must_use]
    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    /// Replaces the instructions (input substitution).
    #[must_use]
    pub fn with_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = prompt.into();
        self
    }

    /// Choice labels, in display order. Empty for a `noul` question.
    #[must_use]
    pub fn choices(&self) -> &[ChoiceId] {
        &self.choices
    }

    /// The question type.
    #[must_use]
    pub fn kind(&self) -> QuestionKind {
        self.kind
    }

    /// One description per choice, when they differ from the choice labels.
    #[must_use]
    pub fn descriptions(&self) -> Option<&[String]> {
        self.descriptions.as_deref()
    }

    /// The labels an answer may name: the choices, or `yes` and `no` for a
    /// `noul` question (its answer is the probability of yes).
    fn answer_labels(&self) -> BTreeSet<ChoiceId> {
        match self.kind {
            QuestionKind::Noul => BTreeSet::from([ChoiceId::no(), ChoiceId::yes()]),
            QuestionKind::Choice | QuestionKind::Score => self.choices.iter().cloned().collect(),
        }
    }
}

fn check_kind_rules(
    id: &QuestionId,
    choices: &[ChoiceId],
    kind: QuestionKind,
    descriptions: Option<&[String]>,
) -> Result<(), RuleViolation> {
    let range = match kind {
        QuestionKind::Choice => Some((MIN_OPTIONS, MAX_CHOICE_OPTIONS)),
        QuestionKind::Score => Some((MIN_OPTIONS, MAX_SCORE_LEVELS)),
        QuestionKind::Noul => None,
    };
    match range {
        Some((least, most)) => {
            if !(least..=most).contains(&choices.len()) {
                return Err(RuleViolation(format!(
                    "question {id} is a {} question and needs {least} to {most} choices, got {}; fix the choices",
                    kind_name(kind),
                    choices.len()
                )));
            }
            let distinct: BTreeSet<&ChoiceId> = choices.iter().collect();
            if distinct.len() != choices.len() {
                return Err(RuleViolation(format!(
                    "question {id} repeats a choice; give every choice a distinct label"
                )));
            }
        }
        None => {
            if !choices.is_empty() || descriptions.is_some() {
                return Err(RuleViolation(format!(
                    "question {id} is a noul question and takes no choices and no descriptions"
                )));
            }
        }
    }
    if let Some(list) = descriptions
        && list.len() != choices.len()
    {
        return Err(RuleViolation(format!(
            "question {id} has {} descriptions for {} choices; give one description per choice",
            list.len(),
            choices.len()
        )));
    }
    Ok(())
}

fn kind_name(kind: QuestionKind) -> &'static str {
    match kind {
        QuestionKind::Choice => "choice",
        QuestionKind::Score => "score",
        QuestionKind::Noul => "noul",
    }
}

/// A recorded choice for one question: prior state inside the payload.
///
/// This is an *input* (part of the `prompt_sha256` bytes). A backend's output
/// for one question is `QuestionAnswer`; `QuestionAnswer::to_choice_answer`
/// is the one conversion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceAnswer {
    /// The question this answer belongs to.
    pub question_id: QuestionId,
    /// Selected choice label.
    pub choice: ChoiceId,
}

/// Questions plus state, before canonical encoding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawPayload")]
pub struct CanonicalJudgePayload {
    questions: Vec<Question>,
    state: Vec<ChoiceAnswer>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPayload {
    questions: Vec<RawQuestion>,
    state: Vec<ChoiceAnswer>,
}

impl TryFrom<RawPayload> for CanonicalJudgePayload {
    type Error = RuleViolation;

    fn try_from(raw: RawPayload) -> Result<Self, Self::Error> {
        let questions = raw
            .questions
            .into_iter()
            .map(Question::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            questions,
            state: raw.state,
        })
    }
}

impl CanonicalJudgePayload {
    /// A payload of `questions` with prior `state`.
    #[must_use]
    pub fn new(questions: Vec<Question>, state: Vec<ChoiceAnswer>) -> Self {
        Self { questions, state }
    }

    /// Parses and validates canonical payload bytes.
    ///
    /// The one validator both backend families call (ADR-SAG-0002, r-5
    /// amendment): it deserializes with `deny_unknown_fields`, and the
    /// question-kind rules run inside the deserialization.
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` when the bytes are not JSON, have an unknown or
    /// missing field, or a question breaks its kind's rules. A rule error
    /// names the question id, the kind and the counts; every other error
    /// names only the failure category and position, never payload bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_decision_schema::CanonicalJudgePayload;
    ///
    /// let bytes = br#"{"questions":[{"id":"q","prompt":"?","choices":["no","yes"]}],"state":[]}"#;
    /// assert_eq!(CanonicalJudgePayload::from_bytes(bytes)?.questions().len(), 1);
    /// assert!(CanonicalJudgePayload::from_bytes(b"{}").is_err());
    /// # Ok::<(), sc_agent_types::JevError>(())
    /// ```
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, JevError> {
        let raw: RawPayload = serde_json::from_slice(bytes)
            .map_err(|error| decode_failed("canonical payload", &error))?;
        Ok(Self::try_from(raw)?)
    }

    /// Questions the backends must judge.
    #[must_use]
    pub fn questions(&self) -> &[Question] {
        &self.questions
    }

    /// Prior answers carried into this request.
    #[must_use]
    pub fn state(&self) -> &[ChoiceAnswer] {
        &self.state
    }
}

/// Canonical judge payload bytes, their SHA-256, and the questions they ask.
///
/// [`Self::sha256`] is computed on [`Self::bytes`] only. LLM system-prompt
/// text is not part of the digest (ADR-SAG-0019). Fields are private so the
/// digest and the question map always match the bytes; equal-input
/// attestation and `JudgeOutcome::new` rely on that invariant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadBundle {
    /// Canonical JSON text; `serde_json` output, so UTF-8 by construction.
    text: String,
    sha256: Sha256Digest,
    question_choices: BTreeMap<QuestionId, BTreeSet<ChoiceId>>,
}

impl PayloadBundle {
    /// Encodes `payload` as canonical bytes and records its digest.
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` when two questions share an id or `serde_json` cannot
    /// encode the payload.
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_decision_schema::{CanonicalJudgePayload, PayloadBundle};
    ///
    /// let payload: CanonicalJudgePayload = serde_json::from_value(serde_json::json!({
    ///     "questions": [{"id": "q", "prompt": "Is it?", "choices": ["no", "yes"]}],
    ///     "state": []
    /// }))?;
    /// let bundle = PayloadBundle::encode(&payload)?;
    /// assert_eq!(
    ///     bundle.bytes(),
    ///     br#"{"questions":[{"choices":["no","yes"],"id":"q","prompt":"Is it?"}],"state":[]}"#
    /// );
    /// assert_eq!(bundle.question_ids().count(), 1);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn encode(payload: &CanonicalJudgePayload) -> Result<Self, JevError> {
        let mut question_choices = BTreeMap::new();
        for question in payload.questions() {
            let choices = question.answer_labels();
            if question_choices
                .insert(question.id().clone(), choices)
                .is_some()
            {
                return Err(JevError::input_invalid(format!(
                    "question id {} is duplicated",
                    question.id()
                )));
            }
        }
        let value = serde_json::to_value(payload)
            .map_err(|error| JevError::encode_failed("canonical payload", &error))?;
        let text = serde_json::to_string(&sorted(&value))
            .map_err(|error| JevError::encode_failed("canonical JSON", &error))?;
        let sha256 = Sha256Digest::of_text(&text);
        Ok(Self {
            text,
            sha256,
            question_choices,
        })
    }

    /// Rebuilds a bundle from bytes that must already be canonical.
    ///
    /// The bytes are parsed into a [`CanonicalJudgePayload`] and re-encoded;
    /// the result must equal `bytes`. Used to reload stored payloads.
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` when the bytes are not a canonical payload.
    pub fn decode(bytes: &[u8]) -> Result<Self, JevError> {
        let payload: CanonicalJudgePayload = serde_json::from_slice(bytes)
            .map_err(|error| decode_failed("canonical payload", &error))?;
        let bundle = Self::encode(&payload)?;
        if bundle.text.as_bytes() != bytes {
            return Err(JevError::input_invalid(
                "payload bytes are not canonical sorted-key compact JSON",
            ));
        }
        Ok(bundle)
    }

    /// Sorted-key compact JSON, UTF-8.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        self.text.as_bytes()
    }

    /// The canonical JSON as text. The bytes are always UTF-8, so no caller
    /// needs to re-check them.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Returns the text and its digest, consuming the bundle.
    #[must_use]
    pub fn into_text(self) -> (String, Sha256Digest) {
        (self.text, self.sha256)
    }

    /// SHA-256 of [`Self::bytes`].
    #[must_use]
    pub fn sha256(&self) -> &Sha256Digest {
        &self.sha256
    }

    /// Each question id and the set of its choices, computed by [`Self::encode`].
    #[must_use]
    pub fn question_choices(&self) -> &BTreeMap<QuestionId, BTreeSet<ChoiceId>> {
        &self.question_choices
    }

    /// The question ids, in sorted order.
    pub fn question_ids(&self) -> impl Iterator<Item = &QuestionId> + '_ {
        self.question_choices.keys()
    }

    /// Returns the bytes and their digest, consuming the bundle.
    #[must_use]
    pub fn into_parts(self) -> (Vec<u8>, Sha256Digest) {
        (self.text.into_bytes(), self.sha256)
    }
}

/// Encodes `value` as canonical judge-payload bytes.
///
/// # Errors
///
/// Returns `INPUT_INVALID` when `serde_json` cannot encode `value`.
///
/// # Examples
///
/// ```
/// let value = serde_json::json!({"b": 1, "a": {"d": true, "c": "ok"}});
/// let bytes = sc_decision_schema::canonical_json_bytes(&value).expect("finite JSON");
/// assert_eq!(bytes, br#"{"a":{"c":"ok","d":true},"b":1}"#);
/// ```
pub fn canonical_json_bytes(value: &Value) -> Result<Vec<u8>, JevError> {
    serde_json::to_vec(&sorted(value))
        .map_err(|error| JevError::encode_failed("canonical JSON", &error))
}

fn sorted(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by(|left, right| left.0.cmp(right.0));
            let mut ordered = Map::new();
            for (key, child) in entries {
                ordered.insert(key.clone(), sorted(child));
            }
            Value::Object(ordered)
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}
