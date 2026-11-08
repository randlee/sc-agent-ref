//! `jev::v1`: the documented TypeSafe Jev check API (`POST /v1/systemone`).
//!
//! The request is the flat object `{model, state, questions}`; `questions`
//! is keyed by question id and each question has a `type` (`choice`,
//! `score` or `noul`) and `instructions`. The reply is `{model, answers,
//! usage}` with `answers` keyed by question id. [`CheckRequest`] builds the
//! request from a [`CanonicalJudgePayload`] and can recover that payload from
//! a body (for the `input_sha256` attestation); [`CheckResponse`] checks a
//! reply against the request it answers and maps it to [`DecisionAnswers`].
//!
//! Key order is part of the wire form: [`OrderedMap`] keeps document order
//! in both directions and refuses a duplicate key.
//!
//! # Examples
//!
//! ```
//! use sc_decision_schema::jev::v1::{CheckRequest, CheckResponse};
//! use sc_decision_schema::CanonicalJudgePayload;
//!
//! let payload = CanonicalJudgePayload::from_bytes(
//!     br#"{"questions":[{"id":"q","prompt":"Is it?","choices":["no","yes"]}],"state":[]}"#,
//! )?;
//! let request = CheckRequest::from_canonical("jev-1.13.0".parse()?, &payload)?;
//! assert_eq!(
//!     request.to_body()?,
//!     r#"{"model":"jev-1.13.0","state":[],"questions":{"q":{"type":"choice","instructions":"Is it?","criteria":{"no":"no","yes":"yes"}}}}"#
//! );
//! let reply = br#"{"model":"jev-1.13.0","answers":{"q":{"type":"choice","choice":"yes","probabilities":{"no":0.1,"yes":0.9},"confidence":0.8}},"usage":{}}"#;
//! let answers = CheckResponse::parse(reply)?.into_answers(&request)?;
//! assert_eq!(answers.len(), 1);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::marker::PhantomData;

use sc_agent_types::{JevError, ModelId};
use serde::de::value::MapAccessDeserializer;
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::ids::{ChoiceId, QuestionId};
use crate::judge::QuestionAnswer;
use crate::payload::{
    CanonicalJudgePayload, ChoiceAnswer, DecisionAnswers, PayloadBundle, Question, QuestionKind,
    decode_failed,
};
use crate::probability::Probability;

/// Fewest options a `choice` or `score` question has.
const MIN_CRITERIA: usize = 2;
/// Most options a `choice` question has (the documented Jev limit).
const MAX_CHOICE_CRITERIA: usize = 255;
/// Most levels a `score` question has (the documented Jev limit).
const MAX_SCORE_CRITERIA: usize = 10;
/// How far a probability sum may sit from 1. Jev rounds its probabilities, so
/// the sum is never exact; a wider gap is a malformed reply.
const PROBABILITY_SUM_TOLERANCE: f64 = 1e-6;
/// The wire value `jev-api.md` (plan-anchor) documents for the latest Jev
/// model. UNVERIFIED against the live API: plan-anchor sends `jev-latest`
/// (`jev_judge.py` `DEFAULT_MODEL`), and nothing confirms the API accepts the
/// bare `jev`. This constant is the one place that mapping lives.
const JEV_LATEST_WIRE: &str = "jev-latest";
/// A reply that is not a check reply is reported with this text.
const MODEL_OR_SHAPE: &str = "unexpected Jev response model or shape";

/// The model string sent on the wire for `model`.
///
/// `jev` (the latest Jev model, [`ModelId::Jev`]) is sent as `jev-latest`,
/// the value `jev-api.md` documents; every other model is sent as is.
/// UNVERIFIED against the live API (see the `JEV_LATEST_WIRE` constant).
///
/// # Examples
///
/// ```
/// use sc_decision_schema::jev::v1::wire_model;
///
/// assert_eq!(wire_model(&"jev".parse()?), "jev-latest");
/// assert_eq!(wire_model(&"jev-1.13.0".parse()?), "jev-1.13.0");
/// # Ok::<(), sc_agent_types::JevError>(())
/// ```
#[must_use]
pub fn wire_model(model: &ModelId) -> &str {
    match model {
        ModelId::Jev => JEV_LATEST_WIRE,
        other => other.as_str(),
    }
}

fn duplicate_message(key: &dyn fmt::Display) -> String {
    format!("duplicate key `{key}`; give every key once")
}

/// A map serialized as a JSON object whose key order is the `Vec` order, and
/// deserialized in document order.
///
/// A duplicate key is an error that names the key. Use it wherever the wire
/// form's key order matters (question ids, criteria options, top-level state
/// keys).
///
/// # Examples
///
/// ```
/// use sc_decision_schema::jev::v1::OrderedMap;
///
/// let map: OrderedMap<String, u8> = serde_json::from_str(r#"{"zeta":1,"alpha":2}"#)?;
/// assert_eq!(serde_json::to_string(&map)?, r#"{"zeta":1,"alpha":2}"#);
/// assert!(serde_json::from_str::<OrderedMap<String, u8>>(r#"{"a":1,"a":2}"#).is_err());
/// # Ok::<(), serde_json::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct OrderedMap<K, V>(Vec<(K, V)>);

impl<K: Ord + fmt::Display, V> OrderedMap<K, V> {
    /// A map of `pairs`, in that order.
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` when two pairs share a key; the fix hint names it.
    pub fn try_from_pairs(pairs: Vec<(K, V)>) -> Result<Self, JevError> {
        match duplicate_key(&pairs) {
            Some(key) => Err(JevError::input_invalid(duplicate_message(key))),
            None => Ok(Self(pairs)),
        }
    }
}

impl<K, V> OrderedMap<K, V> {
    /// The pairs, in order.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> + '_ {
        self.0.iter().map(|(key, value)| (key, value))
    }

    /// The keys, in order.
    pub fn keys(&self) -> impl Iterator<Item = &K> + '_ {
        self.0.iter().map(|(key, _)| key)
    }

    /// The number of pairs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are no pairs.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<K: PartialEq, V> OrderedMap<K, V> {
    /// The value of `key`, if present.
    #[must_use]
    pub fn get(&self, key: &K) -> Option<&V> {
        self.0
            .iter()
            .find_map(|(candidate, value)| (candidate == key).then_some(value))
    }
}

fn duplicate_key<K: Ord + fmt::Display, V>(pairs: &[(K, V)]) -> Option<&K> {
    let mut seen = BTreeSet::new();
    pairs
        .iter()
        .map(|(key, _)| key)
        .find(|key| !seen.insert(*key))
}

impl<K: Serialize, V: Serialize> Serialize for OrderedMap<K, V> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

/// A JSON object read in document order, with duplicates kept. Raw parse
/// types hold it so their owners can refuse duplicates with a typed error.
struct Pairs<K, V>(Vec<(K, V)>);

struct PairsVisitor<K, V>(PhantomData<(K, V)>);

impl<'de, K, V> Visitor<'de> for PairsVisitor<K, V>
where
    K: Deserialize<'de>,
    V: Deserialize<'de>,
{
    type Value = Vec<(K, V)>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
        let mut pairs = Vec::with_capacity(access.size_hint().unwrap_or(0).min(1024));
        while let Some(pair) = access.next_entry::<K, V>()? {
            pairs.push(pair);
        }
        Ok(pairs)
    }
}

impl<'de, K: Deserialize<'de>, V: Deserialize<'de>> Deserialize<'de> for Pairs<K, V> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer
            .deserialize_map(PairsVisitor(PhantomData))
            .map(Self)
    }
}

impl<'de, K, V> Deserialize<'de> for OrderedMap<K, V>
where
    K: Deserialize<'de> + Ord + fmt::Display,
    V: Deserialize<'de>,
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let Pairs(pairs) = Pairs::<K, V>::deserialize(deserializer)?;
        match duplicate_key(&pairs) {
            Some(key) => Err(de::Error::custom(duplicate_message(key))),
            None => Ok(Self(pairs)),
        }
    }
}

/// One question of a check request.
///
/// Built only through [`Self::choice`], [`Self::score`], [`Self::noul`] or
/// deserialization, which all apply the criteria counts: `choice` has 2 to
/// 255 options, `score` has 2 to 10 ordered levels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", try_from = "RawJevQuestion")]
#[non_exhaustive]
pub enum JevQuestion {
    /// Pick one option; `criteria` maps option to description.
    Choice {
        /// What the question asks.
        instructions: String,
        /// Option to description, in order.
        criteria: OrderedMap<ChoiceId, String>,
    },
    /// Pick one level; `criteria` maps ordered levels to descriptions.
    Score {
        /// What the question asks.
        instructions: String,
        /// Level to description, lowest level first.
        criteria: OrderedMap<ChoiceId, String>,
    },
    /// Probability of yes; no criteria.
    Noul {
        /// What the question asks.
        instructions: String,
    },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum RawJevQuestion {
    Choice {
        instructions: String,
        criteria: Pairs<ChoiceId, String>,
    },
    Score {
        instructions: String,
        criteria: Pairs<ChoiceId, String>,
    },
    Noul {
        instructions: String,
    },
}

impl TryFrom<RawJevQuestion> for JevQuestion {
    type Error = JevError;

    fn try_from(raw: RawJevQuestion) -> Result<Self, Self::Error> {
        match raw {
            RawJevQuestion::Choice {
                instructions,
                criteria,
            } => Self::choice(instructions, OrderedMap::try_from_pairs(criteria.0)?),
            RawJevQuestion::Score {
                instructions,
                criteria,
            } => Self::score(instructions, OrderedMap::try_from_pairs(criteria.0)?),
            RawJevQuestion::Noul { instructions } => Ok(Self::noul(instructions)),
        }
    }
}

fn detail(error: &JevError) -> String {
    match error {
        JevError::InputInvalid { detail, .. } => detail.clone(),
        other => other.to_string(),
    }
}

fn check_count(kind: &str, criteria: usize, least: usize, most: usize) -> Result<(), JevError> {
    if (least..=most).contains(&criteria) {
        Ok(())
    } else {
        Err(JevError::input_invalid(format!(
            "a {kind} question needs {least} to {most} criteria, got {criteria}; fix the criteria"
        )))
    }
}

impl JevQuestion {
    /// A `choice` question.
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` unless `criteria` has 2 to 255 options.
    pub fn choice(
        instructions: String,
        criteria: OrderedMap<ChoiceId, String>,
    ) -> Result<Self, JevError> {
        check_count("choice", criteria.len(), MIN_CRITERIA, MAX_CHOICE_CRITERIA)?;
        Ok(Self::Choice {
            instructions,
            criteria,
        })
    }

    /// A `score` question.
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` unless `criteria` has 2 to 10 levels.
    pub fn score(
        instructions: String,
        criteria: OrderedMap<ChoiceId, String>,
    ) -> Result<Self, JevError> {
        check_count("score", criteria.len(), MIN_CRITERIA, MAX_SCORE_CRITERIA)?;
        Ok(Self::Score {
            instructions,
            criteria,
        })
    }

    /// A `noul` question.
    #[must_use]
    pub fn noul(instructions: String) -> Self {
        Self::Noul { instructions }
    }

    fn from_question(question: &Question) -> Result<Self, JevError> {
        let instructions = question.prompt().to_owned();
        let criteria = || -> Result<OrderedMap<ChoiceId, String>, JevError> {
            let descriptions = question.descriptions();
            let pairs = question
                .choices()
                .iter()
                .enumerate()
                .map(|(index, choice)| {
                    let description = descriptions
                        .and_then(|list| list.get(index))
                        .map_or_else(|| choice.as_str().to_owned(), Clone::clone);
                    (choice.clone(), description)
                })
                .collect();
            OrderedMap::try_from_pairs(pairs).map_err(|error| in_question(question.id(), &error))
        };
        match question.kind() {
            QuestionKind::Choice => Self::choice(instructions, criteria()?),
            QuestionKind::Score => Self::score(instructions, criteria()?),
            QuestionKind::Noul => Ok(Self::noul(instructions)),
        }
        .map_err(|error| in_question(question.id(), &error))
    }

    fn to_question(&self, id: &QuestionId) -> Result<Question, JevError> {
        let (kind, instructions, criteria) = match self {
            Self::Choice {
                instructions,
                criteria,
            } => (QuestionKind::Choice, instructions, Some(criteria)),
            Self::Score {
                instructions,
                criteria,
            } => (QuestionKind::Score, instructions, Some(criteria)),
            Self::Noul { instructions } => (QuestionKind::Noul, instructions, None),
        };
        let (choices, descriptions) = match criteria {
            Some(map) => (
                map.keys().cloned().collect(),
                Some(
                    map.iter()
                        .map(|(_, description)| description.clone())
                        .collect(),
                ),
            ),
            None => (Vec::new(), None),
        };
        Question::try_new(
            id.clone(),
            instructions.clone(),
            choices,
            kind,
            descriptions,
        )
    }
}

fn in_question(id: &QuestionId, error: &JevError) -> JevError {
    JevError::input_invalid(format!("question {id}: {}", detail(error)))
}

/// The `state` of a check: a string, an object or an array.
///
/// The top-level keys of an object keep their order. Nested values are
/// `serde_json` values, whose object keys are sorted, so only the top level
/// round-trips verbatim.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum JevState {
    /// Free text.
    Text(String),
    /// A JSON object.
    Object(OrderedMap<String, Value>),
    /// A JSON array.
    Array(Vec<Value>),
}

/// A state as read, with object duplicates kept.
enum RawState {
    Text(String),
    Object(Pairs<String, Value>),
    Array(Vec<Value>),
}

impl TryFrom<RawState> for JevState {
    type Error = JevError;

    fn try_from(raw: RawState) -> Result<Self, Self::Error> {
        Ok(match raw {
            RawState::Text(text) => Self::Text(text),
            RawState::Object(pairs) => Self::Object(OrderedMap::try_from_pairs(pairs.0)?),
            RawState::Array(items) => Self::Array(items),
        })
    }
}

struct StateVisitor;

impl<'de> Visitor<'de> for StateVisitor {
    type Value = RawState;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a string, an object or an array")
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<Self::Value, E> {
        Ok(RawState::Text(text.to_owned()))
    }

    fn visit_string<E: de::Error>(self, text: String) -> Result<Self::Value, E> {
        Ok(RawState::Text(text))
    }

    fn visit_map<A: MapAccess<'de>>(self, access: A) -> Result<Self::Value, A::Error> {
        Pairs::deserialize(MapAccessDeserializer::new(access)).map(RawState::Object)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = access.next_element::<Value>()? {
            items.push(item);
        }
        Ok(RawState::Array(items))
    }
}

impl<'de> Deserialize<'de> for RawState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(StateVisitor)
    }
}

impl<'de> Deserialize<'de> for JevState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        RawState::deserialize(deserializer)?
            .try_into()
            .map_err(de::Error::custom)
    }
}

/// The check request body: `{model, state, questions}`, in that order.
///
/// # Examples
///
/// ```
/// use sc_decision_schema::jev::v1::CheckRequest;
///
/// let body = r#"{"model":"jev-1.13.0","state":"text","questions":{"b":{"type":"noul","instructions":"Yes?"}}}"#;
/// let request = CheckRequest::parse(body)?;
/// assert_eq!(request.to_body()?, body);
/// # Ok::<(), sc_agent_types::JevError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawCheckRequest")]
pub struct CheckRequest {
    model: ModelId,
    state: JevState,
    questions: OrderedMap<QuestionId, JevQuestion>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCheckRequest {
    model: ModelId,
    state: RawState,
    questions: Pairs<QuestionId, RawJevQuestion>,
}

impl TryFrom<RawCheckRequest> for CheckRequest {
    type Error = JevError;

    fn try_from(raw: RawCheckRequest) -> Result<Self, Self::Error> {
        let pairs = raw
            .questions
            .0
            .into_iter()
            .map(|(id, question)| match JevQuestion::try_from(question) {
                Ok(question) => Ok((id, question)),
                Err(error) => Err(in_question(&id, &error)),
            })
            .collect::<Result<Vec<_>, JevError>>()?;
        Ok(Self {
            model: raw.model,
            state: raw.state.try_into()?,
            questions: OrderedMap::try_from_pairs(pairs)?,
        })
    }
}

impl CheckRequest {
    /// Maps `payload` to the Jev body for `model`.
    ///
    /// Each canonical question becomes a Jev question keyed by its id, in
    /// payload order: `instructions` is the prompt and `criteria` maps each
    /// choice to its description (the label when none is given). The
    /// canonical `state` (prior answers) is sent as the `state` array.
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` when the payload has no question, a question id or a
    /// choice repeats, or a question breaks its kind's criteria count. The
    /// fix hint names the question.
    pub fn from_canonical(
        model: ModelId,
        payload: &CanonicalJudgePayload,
    ) -> Result<Self, JevError> {
        if payload.questions().is_empty() {
            return Err(JevError::input_invalid(
                "a check needs at least one question",
            ));
        }
        let pairs = payload
            .questions()
            .iter()
            .map(|question| Ok((question.id().clone(), JevQuestion::from_question(question)?)))
            .collect::<Result<Vec<_>, JevError>>()?;
        let questions = OrderedMap::try_from_pairs(pairs)?;
        let state = payload
            .state()
            .iter()
            .map(|answer| {
                serde_json::to_value(answer)
                    .map_err(|error| JevError::encode_failed("prior answer", &error))
            })
            .collect::<Result<Vec<_>, _>>()?;
        // The wire form of the requested model (`jev` becomes `jev-latest`).
        let model = wire_model(&model)
            .parse()
            .map_err(|_| JevError::input_invalid("model id is not a valid model string"))?;
        Ok(Self {
            model,
            state: JevState::Array(state),
            questions,
        })
    }

    /// Parses a request body, keeping key order.
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` when `body` is not a check request or repeats a
    /// question id, criteria key or state key (the detail names the key), or
    /// a question breaks its criteria count. Any other failure names only
    /// the category and position, never body bytes (ADR-SAG-0020).
    pub fn parse(body: &str) -> Result<Self, JevError> {
        let raw: RawCheckRequest =
            serde_json::from_str(body).map_err(|error| decode_failed("request body", &error))?;
        Self::try_from(raw)
    }

    /// The compact JSON body: `model`, `state`, `questions`, in that order.
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` when `serde_json` cannot encode the request.
    pub fn to_body(&self) -> Result<String, JevError> {
        serde_json::to_string(self).map_err(|error| JevError::encode_failed("Jev body", &error))
    }

    /// Recovers the canonical payload this body was built from (the inverse
    /// of [`Self::from_canonical`]), for the `input_sha256` attestation.
    ///
    /// Choices are the criteria keys in body order, `kind` comes from
    /// `type`, and descriptions are dropped when each equals its label.
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` when `state` is not an array of recorded answers or a
    /// question breaks its kind's rules.
    pub fn to_canonical(&self) -> Result<CanonicalJudgePayload, JevError> {
        let questions = self
            .questions
            .iter()
            .map(|(id, question)| question.to_question(id))
            .collect::<Result<Vec<_>, _>>()?;
        let JevState::Array(items) = &self.state else {
            return Err(JevError::input_invalid(
                "Jev state is not the prior-answers array; only a canonical payload can be recovered",
            ));
        };
        let state = items
            .iter()
            .map(|item| {
                serde_json::from_value::<ChoiceAnswer>(item.clone()).map_err(|_| {
                    JevError::input_invalid("Jev state entry is not a recorded answer")
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CanonicalJudgePayload::new(questions, state))
    }

    /// The canonical bundle recovered from this request: its
    /// [`PayloadBundle::sha256`] is the `input_sha256` attestation.
    ///
    /// # Errors
    ///
    /// As [`Self::to_canonical`], plus a canonical-encoding failure.
    pub fn recovered_bundle(&self) -> Result<PayloadBundle, JevError> {
        PayloadBundle::encode(&self.to_canonical()?)
    }

    /// The model the request names on the wire (`jev-latest` for `jev`).
    #[must_use]
    pub fn model(&self) -> &ModelId {
        &self.model
    }

    /// The state the request carries.
    #[must_use]
    pub fn state(&self) -> &JevState {
        &self.state
    }

    /// The questions, in order.
    #[must_use]
    pub fn questions(&self) -> &OrderedMap<QuestionId, JevQuestion> {
        &self.questions
    }
}

/// One answer of a check reply.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum JevAnswer {
    /// The answer to a `choice` question.
    #[non_exhaustive]
    Choice {
        /// The selected option.
        choice: ChoiceId,
        /// A probability per option.
        probabilities: BTreeMap<ChoiceId, Probability>,
        /// How far to trust the call, in `[0, 1]`.
        confidence: Probability,
    },
    /// The answer to a `score` question.
    #[non_exhaustive]
    Score {
        /// The selected level.
        choice: ChoiceId,
        /// A probability per level.
        probabilities: BTreeMap<ChoiceId, Probability>,
        /// How far to trust the call, in `[0, 1]`.
        confidence: Probability,
    },
    /// The answer to a `noul` question: the probability of yes.
    #[non_exhaustive]
    Noul {
        /// The single `yes` probability.
        probabilities: NoulProbabilities,
    },
}

/// The probabilities of a `noul` answer: exactly the key `yes`.
///
/// # Examples
///
/// ```
/// use sc_decision_schema::Probability;
/// use sc_decision_schema::jev::v1::NoulProbabilities;
///
/// let noul = NoulProbabilities::new(Probability::new(0.25)?);
/// assert_eq!(noul.yes().get(), 0.25);
/// assert!(Probability::new(1.5).is_err());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoulProbabilities {
    yes: Probability,
}

impl NoulProbabilities {
    /// The probabilities of a `noul` answer whose probability of yes is `yes`.
    #[must_use]
    pub fn new(yes: Probability) -> Self {
        Self { yes }
    }

    /// The probability of yes.
    #[must_use]
    pub fn yes(&self) -> Probability {
        self.yes
    }
}

/// The check reply: `{model, answers, usage}`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(try_from = "RawCheckResponse")]
pub struct CheckResponse {
    model: ModelId,
    answers: OrderedMap<QuestionId, JevAnswer>,
    usage: Map<String, Value>,
    extra: Map<String, Value>,
}

#[derive(Deserialize)]
struct RawCheckResponse {
    model: ModelId,
    answers: Pairs<QuestionId, RawAnswer>,
    #[serde(default)]
    usage: Map<String, Value>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

/// An answer as it arrives: probabilities and confidence are plain numbers,
/// checked against `[0, 1]` once the answer id is in scope.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum RawAnswer {
    Choice {
        choice: ChoiceId,
        probabilities: BTreeMap<ChoiceId, f64>,
        confidence: f64,
    },
    Score {
        choice: ChoiceId,
        probabilities: BTreeMap<ChoiceId, f64>,
        confidence: f64,
    },
    Noul {
        probabilities: RawNoulProbabilities,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNoulProbabilities {
    yes: f64,
}

impl RawAnswer {
    /// Builds the typed answer for `id`.
    ///
    /// The error names the answer id and the field (`confidence` or
    /// `probabilities`), never the value (ADR-SAG-0020).
    fn into_answer(self, id: &QuestionId) -> Result<JevAnswer, JevError> {
        let out_of_range = |field: &str| {
            JevError::response_invalid(format!(
                "answer {id} {field} has a value that is not a number in [0, 1]"
            ))
        };
        let probabilities = |map: BTreeMap<ChoiceId, f64>| {
            map.into_iter()
                .map(|(key, value)| Probability::new(value).map(|value| (key, value)))
                .collect::<Result<BTreeMap<_, _>, _>>()
                .map_err(|_| out_of_range("probabilities"))
        };
        match self {
            Self::Choice {
                choice,
                probabilities: map,
                confidence,
            } => {
                let confidence =
                    Probability::new(confidence).map_err(|_| out_of_range("confidence"))?;
                Ok(JevAnswer::Choice {
                    choice,
                    probabilities: probabilities(map)?,
                    confidence,
                })
            }
            Self::Score {
                choice,
                probabilities: map,
                confidence,
            } => {
                let confidence =
                    Probability::new(confidence).map_err(|_| out_of_range("confidence"))?;
                Ok(JevAnswer::Score {
                    choice,
                    probabilities: probabilities(map)?,
                    confidence,
                })
            }
            Self::Noul { probabilities } => {
                let yes = Probability::new(probabilities.yes)
                    .map_err(|_| out_of_range("probabilities"))?;
                Ok(JevAnswer::Noul {
                    probabilities: NoulProbabilities::new(yes),
                })
            }
        }
    }
}

impl TryFrom<RawCheckResponse> for CheckResponse {
    type Error = JevError;

    fn try_from(raw: RawCheckResponse) -> Result<Self, Self::Error> {
        // Report the out-of-range answer with the least id, whatever order
        // the reply lists them in.
        let mut typed = Vec::with_capacity(raw.answers.0.len());
        let mut first_error: Option<(QuestionId, JevError)> = None;
        for (id, answer) in raw.answers.0 {
            match answer.into_answer(&id) {
                Ok(answer) => typed.push((id, answer)),
                Err(error) => {
                    if first_error
                        .as_ref()
                        .is_none_or(|(least, _)| id.as_str() < least.as_str())
                    {
                        first_error = Some((id, error));
                    }
                }
            }
        }
        if let Some((_, error)) = first_error {
            return Err(error);
        }
        let answers = OrderedMap::try_from_pairs(typed)
            .map_err(|error| JevError::response_invalid(detail(&error)))?;
        Ok(Self {
            model: raw.model,
            answers,
            usage: raw.usage,
            extra: raw.extra,
        })
    }
}

impl CheckResponse {
    /// Parses a reply.
    ///
    /// # Errors
    ///
    /// `RESPONSE_INVALID` when `raw` is not JSON, is not a check reply, a
    /// `noul` answer has a key other than `yes`, a probability or confidence
    /// is outside `[0, 1]`, or an answer id repeats (the detail names the id).
    pub fn parse(raw: &[u8]) -> Result<Self, JevError> {
        let parsed: RawCheckResponse =
            serde_json::from_slice(raw).map_err(|error| match error.classify() {
                serde_json::error::Category::Data => JevError::response_invalid(MODEL_OR_SHAPE),
                _ => JevError::response_invalid("response is not JSON"),
            })?;
        Self::try_from(parsed)
    }

    /// The model that answered.
    #[must_use]
    pub fn model(&self) -> &ModelId {
        &self.model
    }

    /// The `usage` object.
    #[must_use]
    pub fn usage(&self) -> &Map<String, Value> {
        &self.usage
    }

    /// Every other top-level field (such as `stop_reason`); keys sorted.
    #[must_use]
    pub fn extra(&self) -> &Map<String, Value> {
        &self.extra
    }

    /// Checks this reply against `request` and maps it to answers.
    ///
    /// The reply `model` may differ from the request model (`jev` is
    /// answered by a concrete version such as `jev-1.13.0`); it is not an
    /// error and is read with [`Self::model`]. Answer ids equal the
    /// question ids; each answer type matches its question type; a `choice`
    /// or `score` answer names a criteria option, its probability keys equal
    /// the criteria keys and sum to 1 within 1e-6, and its confidence and
    /// every probability lie in `[0, 1]`. A `noul` answer becomes `yes` when
    /// `p >= 0.5` else `no`, with probabilities `{yes: p, no: 1 - p}` and no
    /// confidence.
    ///
    /// # Errors
    ///
    /// `RESPONSE_INVALID` on any rule above.
    pub fn into_answers(self, request: &CheckRequest) -> Result<DecisionAnswers, JevError> {
        let ids_differ = || JevError::response_invalid("answer ids differ from the question ids");
        // Answer ids are unique, so equal counts and one answer per question
        // make the two id sets equal.
        if self.answers.len() != request.questions.len() {
            return Err(ids_differ());
        }
        let mut typed = BTreeMap::new();
        for (id, question) in request.questions.iter() {
            let answer = self.answers.get(id).ok_or_else(ids_differ)?;
            typed.insert(id.clone(), map_answer(id, question, answer)?);
        }
        Ok(typed)
    }
}

fn map_answer(
    id: &QuestionId,
    question: &JevQuestion,
    answer: &JevAnswer,
) -> Result<QuestionAnswer, JevError> {
    match (question, answer) {
        (
            JevQuestion::Choice { criteria, .. } | JevQuestion::Score { criteria, .. },
            JevAnswer::Choice {
                choice,
                probabilities,
                confidence,
            }
            | JevAnswer::Score {
                choice,
                probabilities,
                confidence,
            },
        ) if same_kind(question, answer) => {
            check_ranked(id, criteria, choice, probabilities)?;
            Ok(QuestionAnswer::new(
                choice.clone(),
                Some(*confidence),
                Some(probabilities.clone()),
            ))
        }
        (JevQuestion::Noul { .. }, JevAnswer::Noul { probabilities }) => {
            let yes = probabilities.yes();
            let probabilities =
                BTreeMap::from([(ChoiceId::yes(), yes), (ChoiceId::no(), yes.complement())]);
            let label = if yes.get() >= 0.5 {
                ChoiceId::yes()
            } else {
                ChoiceId::no()
            };
            Ok(QuestionAnswer::new(label, None, Some(probabilities)))
        }
        _ => Err(JevError::response_invalid(format!(
            "answer {id} type differs from its question type"
        ))),
    }
}

fn same_kind(question: &JevQuestion, answer: &JevAnswer) -> bool {
    matches!(
        (question, answer),
        (JevQuestion::Choice { .. }, JevAnswer::Choice { .. })
            | (JevQuestion::Score { .. }, JevAnswer::Score { .. })
    )
}

fn check_ranked(
    id: &QuestionId,
    criteria: &OrderedMap<ChoiceId, String>,
    choice: &ChoiceId,
    probabilities: &BTreeMap<ChoiceId, Probability>,
) -> Result<(), JevError> {
    if !criteria.keys().any(|option| option == choice) {
        return Err(JevError::response_invalid(format!(
            "answer {id} is not one of its criteria"
        )));
    }
    let wanted: BTreeSet<&ChoiceId> = criteria.keys().collect();
    let got: BTreeSet<&ChoiceId> = probabilities.keys().collect();
    if wanted != got {
        return Err(JevError::response_invalid(format!(
            "answer {id} probabilities are not exactly the question criteria"
        )));
    }
    let sum: f64 = probabilities.values().map(|value| value.get()).sum();
    if (sum - 1.0).abs() > PROBABILITY_SUM_TOLERANCE {
        return Err(JevError::response_invalid(format!(
            "answer {id} probabilities sum to {sum}, not 1"
        )));
    }
    Ok(())
}
