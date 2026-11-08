//! Session-log storage schema and the `fields{}` projection
//! (REQ-SAG-0146, ADR-SAG-0031 "Storage schema").
//!
//! The SQLite schema in `schema.sql` ([`SESSION_LOG_DDL`]) is authoritative.
//! The JSONL "projection" is the `fields{}` object of one sc-observability v2
//! log line (ADR-SAG-0013: no bespoke writer); a loader reads `fields{}` into
//! the tables. This module defines the schema and the row types; r-6 emits
//! them. No SQLite dependency or writer lands in Phase A.1.
//!
//! No secret, payload text, prompt text or raw response body belongs in any
//! column (NFR-SAG-0002): `answers_json` holds only typed answers, and
//! `repo` is the only path column. `project` is caller text, stored as given
//! and not scrubbed, but always a valid [`ProjectLabel`].

mod time;

use std::path::Path;

use sc_agent_types::{
    AnswerId, BackendId, ErrorCode, JevError, ModelId, Operation, ProjectLabel, SessionId,
    Sha256Digest,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::ids::QuestionId;
use crate::payload::DecisionAnswers;
use crate::version::DecisionSchemaId;

pub use time::TimestampMs;

/// The DDL of `schema_version` [`SESSION_LOG_SCHEMA_VERSION`].
pub const SESSION_LOG_DDL: &str = include_str!("schema.sql");

/// Version of the row layout. Adding or renaming a column, or changing a
/// type or nullability, bumps it with a migration note.
pub const SESSION_LOG_SCHEMA_VERSION: u32 = 1;

/// The `fields{}` keys, in order.
pub const COLUMNS: &[&str] = &[
    "schema_version",
    "answer_id",
    "session_id",
    "session_created_at",
    "repo",
    "project",
    "prompt_sha256",
    "decision_schema",
    "question_ids",
    "request_sha256",
    "ts",
    "operation",
    "backend",
    "model_id",
    "send_outcome",
    "error_code",
    "answers_json",
];

/// How one send ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SendOutcome {
    /// The backend answered.
    Ok,
    /// The send failed.
    Failed,
}

impl SendOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Failed => "failed",
        }
    }
}

/// Values every row carries.
///
/// The fields are public to read, but the struct is `#[non_exhaustive]`:
/// other crates build one with [`RowContextBuilder`], so a column added
/// later (a [`SESSION_LOG_SCHEMA_VERSION`] bump) is not a breaking change to
/// callers that only read or build rows.
///
/// # Examples
///
/// ```
/// use sc_agent_types::{AnswerId, BackendId, ModelId, Operation, SessionId, Sha256Digest};
/// use sc_decision_schema::session_log::{RowContextBuilder, TimestampMs};
/// use sc_decision_schema::{CANONICAL_V1, QuestionId};
///
/// let ts = TimestampMs::from_system_time(std::time::UNIX_EPOCH);
/// let context = RowContextBuilder::new()
///     .answer_id(AnswerId::new())
///     .session_id(SessionId::new())
///     .session_created_at(ts)
///     .repo("/work/repo")
///     .prompt_sha256(Sha256Digest::of_text("payload"))
///     .decision_schema(CANONICAL_V1)
///     .question_ids(vec![QuestionId::try_from("q")?])
///     .ts(ts)
///     .operation(Operation::Judge)
///     .backend(BackendId::OpenAiLuna)
///     .model_id(ModelId::Gpt6Luna)
///     .build()?;
/// assert_eq!(context.repo, "/work/repo");
/// assert!(context.project.is_none());
/// // A missing required value is a typed error, not a half-built context.
/// assert!(RowContextBuilder::new().build().is_err());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct RowContext {
    /// One id per send.
    pub answer_id: AnswerId,
    /// The session the send belongs to (a ULID after r-6).
    pub session_id: SessionId,
    /// When the session was created; fixed per session.
    pub session_created_at: TimestampMs,
    /// Absolute path text of the main repo root, else the calling folder. A
    /// non-UTF-8 byte is replaced (lossy), so two repos that differ only in
    /// such bytes can share one `answers_by_repo_project` key.
    pub repo: String,
    /// Caller text for this send, validated by [`ProjectLabel::new`]; `None`
    /// is written as `null`.
    pub project: Option<ProjectLabel>,
    /// SHA-256 of the canonical payload bytes.
    pub prompt_sha256: Sha256Digest,
    /// The decision schema of this send.
    pub decision_schema: DecisionSchemaId,
    /// Question ids in payload order.
    pub question_ids: Vec<QuestionId>,
    /// SHA-256 of the exact body sent; `None` when no body was built.
    pub request_sha256: Option<Sha256Digest>,
    /// When the send finished.
    pub ts: TimestampMs,
    /// Judge send or session ask.
    pub operation: Operation,
    /// How we called.
    pub backend: BackendId,
    /// Which model.
    pub model_id: ModelId,
}

/// Builds a [`RowContext`]; every value but `project` and `request_sha256`
/// is required.
#[derive(Debug, Clone, Default)]
pub struct RowContextBuilder {
    answer_id: Option<AnswerId>,
    session_id: Option<SessionId>,
    session_created_at: Option<TimestampMs>,
    repo: Option<String>,
    project: Option<ProjectLabel>,
    prompt_sha256: Option<Sha256Digest>,
    decision_schema: Option<DecisionSchemaId>,
    question_ids: Option<Vec<QuestionId>>,
    request_sha256: Option<Sha256Digest>,
    ts: Option<TimestampMs>,
    operation: Option<Operation>,
    backend: Option<BackendId>,
    model_id: Option<ModelId>,
}

impl RowContextBuilder {
    /// A builder with no value set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// One id per send.
    #[must_use]
    pub fn answer_id(mut self, value: AnswerId) -> Self {
        self.answer_id = Some(value);
        self
    }

    /// The session the send belongs to.
    #[must_use]
    pub fn session_id(mut self, value: SessionId) -> Self {
        self.session_id = Some(value);
        self
    }

    /// When the session was created.
    #[must_use]
    pub fn session_created_at(mut self, value: TimestampMs) -> Self {
        self.session_created_at = Some(value);
        self
    }

    /// Absolute path text of the main repo root (see [`RowContext::repo_text`]).
    #[must_use]
    pub fn repo(mut self, value: impl Into<String>) -> Self {
        self.repo = Some(value.into());
        self
    }

    /// Caller text for this send; left out, it is written as `null`.
    #[must_use]
    pub fn project(mut self, value: Option<ProjectLabel>) -> Self {
        self.project = value;
        self
    }

    /// SHA-256 of the canonical payload bytes.
    #[must_use]
    pub fn prompt_sha256(mut self, value: Sha256Digest) -> Self {
        self.prompt_sha256 = Some(value);
        self
    }

    /// The decision schema of this send.
    #[must_use]
    pub fn decision_schema(mut self, value: DecisionSchemaId) -> Self {
        self.decision_schema = Some(value);
        self
    }

    /// Question ids in payload order.
    #[must_use]
    pub fn question_ids(mut self, value: Vec<QuestionId>) -> Self {
        self.question_ids = Some(value);
        self
    }

    /// SHA-256 of the exact body sent; left out when no body was built.
    #[must_use]
    pub fn request_sha256(mut self, value: Option<Sha256Digest>) -> Self {
        self.request_sha256 = value;
        self
    }

    /// When the send finished.
    #[must_use]
    pub fn ts(mut self, value: TimestampMs) -> Self {
        self.ts = Some(value);
        self
    }

    /// Judge send or session ask.
    #[must_use]
    pub fn operation(mut self, value: Operation) -> Self {
        self.operation = Some(value);
        self
    }

    /// How we called.
    #[must_use]
    pub fn backend(mut self, value: BackendId) -> Self {
        self.backend = Some(value);
        self
    }

    /// Which model.
    #[must_use]
    pub fn model_id(mut self, value: ModelId) -> Self {
        self.model_id = Some(value);
        self
    }

    /// The context.
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` when a required value was not set; the message names it.
    pub fn build(self) -> Result<RowContext, JevError> {
        fn need<T>(value: Option<T>, name: &str) -> Result<T, JevError> {
            value.ok_or_else(|| {
                JevError::input_invalid(format!("session log row context needs `{name}`"))
            })
        }
        Ok(RowContext {
            answer_id: need(self.answer_id, "answer_id")?,
            session_id: need(self.session_id, "session_id")?,
            session_created_at: need(self.session_created_at, "session_created_at")?,
            repo: need(self.repo, "repo")?,
            project: self.project,
            prompt_sha256: need(self.prompt_sha256, "prompt_sha256")?,
            decision_schema: need(self.decision_schema, "decision_schema")?,
            question_ids: need(self.question_ids, "question_ids")?,
            request_sha256: self.request_sha256,
            ts: need(self.ts, "ts")?,
            operation: need(self.operation, "operation")?,
            backend: need(self.backend, "backend")?,
            model_id: need(self.model_id, "model_id")?,
        })
    }
}

impl RowContext {
    /// Lossy absolute-path text for [`Self::repo`].
    #[must_use]
    pub fn repo_text(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }
}

#[derive(Debug, Clone, PartialEq)]
enum RowOutcome {
    Answered(DecisionAnswers),
    Failed(ErrorCode),
}

/// One send, ready to be written as a `fields{}` object.
///
/// # Examples
///
/// ```
/// use sc_decision_schema::session_log::{COLUMNS, SendOutcome};
///
/// assert_eq!(COLUMNS.len(), 17);
/// assert_eq!(serde_json::to_string(&SendOutcome::Failed)?, "\"failed\"");
/// # Ok::<(), serde_json::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct SessionLogRow {
    context: RowContext,
    outcome: RowOutcome,
}

impl SessionLogRow {
    /// A send that was answered.
    #[must_use]
    pub fn ok(context: RowContext, answers: DecisionAnswers) -> Self {
        Self {
            context,
            outcome: RowOutcome::Answered(answers),
        }
    }

    /// A send that failed with `error_code`.
    #[must_use]
    pub fn failed(context: RowContext, error_code: ErrorCode) -> Self {
        Self {
            context,
            outcome: RowOutcome::Failed(error_code),
        }
    }

    /// The values every row carries.
    #[must_use]
    pub fn context(&self) -> &RowContext {
        &self.context
    }

    /// How the send ended.
    #[must_use]
    pub fn send_outcome(&self) -> SendOutcome {
        match self.outcome {
            RowOutcome::Answered(_) => SendOutcome::Ok,
            RowOutcome::Failed(_) => SendOutcome::Failed,
        }
    }

    /// The error code of a failed send.
    #[must_use]
    pub fn error_code(&self) -> Option<ErrorCode> {
        match self.outcome {
            RowOutcome::Answered(_) => None,
            RowOutcome::Failed(code) => Some(code),
        }
    }

    /// The answers of an answered send.
    #[must_use]
    pub fn answers(&self) -> Option<&DecisionAnswers> {
        match &self.outcome {
            RowOutcome::Answered(answers) => Some(answers),
            RowOutcome::Failed(_) => None,
        }
    }

    /// The `fields{}` object: exactly the [`COLUMNS`] keys, nullable columns
    /// present as `null`.
    #[must_use]
    pub fn to_fields(&self) -> Map<String, Value> {
        let context = &self.context;
        let text = |value: &dyn ToString| Value::String(value.to_string());
        let mut fields = Map::new();
        fields.insert("schema_version".into(), SESSION_LOG_SCHEMA_VERSION.into());
        fields.insert("answer_id".into(), text(&context.answer_id));
        fields.insert("session_id".into(), text(&context.session_id));
        fields.insert(
            "session_created_at".into(),
            Value::String(context.session_created_at.to_rfc3339()),
        );
        fields.insert("repo".into(), Value::String(context.repo.clone()));
        fields.insert(
            "project".into(),
            context
                .project
                .as_ref()
                .map_or(Value::Null, |project| text(project)),
        );
        fields.insert("prompt_sha256".into(), text(&context.prompt_sha256));
        fields.insert("decision_schema".into(), text(&context.decision_schema));
        fields.insert(
            "question_ids".into(),
            Value::Array(
                context
                    .question_ids
                    .iter()
                    .map(|id| Value::String(id.as_str().to_owned()))
                    .collect(),
            ),
        );
        fields.insert(
            "request_sha256".into(),
            context
                .request_sha256
                .as_ref()
                .map_or(Value::Null, |digest| text(digest)),
        );
        fields.insert("ts".into(), Value::String(context.ts.to_rfc3339()));
        fields.insert("operation".into(), text(&context.operation));
        fields.insert("backend".into(), text(&context.backend));
        fields.insert("model_id".into(), text(&context.model_id));
        fields.insert(
            "send_outcome".into(),
            Value::String(self.send_outcome().as_str().to_owned()),
        );
        fields.insert(
            "error_code".into(),
            self.error_code()
                .map_or(Value::Null, |code| Value::String(code.as_str().to_owned())),
        );
        fields.insert(
            "answers_json".into(),
            self.answers().map_or(Value::Null, |answers| {
                // Typed answers hold finite numbers and string keys only.
                serde_json::to_value(answers).unwrap_or(Value::Null)
            }),
        );
        fields
    }

    /// Reads a `fields{}` object back.
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` (the detail names the key) for an unknown key, a
    /// missing key, a value of the wrong type or an invalid value, and for a
    /// `schema_version` other than [`SESSION_LOG_SCHEMA_VERSION`].
    pub fn try_from_fields(fields: &Map<String, Value>) -> Result<Self, JevError> {
        if let Some(key) = fields.keys().find(|key| !COLUMNS.contains(&key.as_str())) {
            return Err(bad(key, "is not a session log column"));
        }
        if let Some(key) = COLUMNS.iter().find(|key| !fields.contains_key(**key)) {
            return Err(bad(key, "is missing"));
        }
        let version = fields["schema_version"].as_u64();
        if version != Some(u64::from(SESSION_LOG_SCHEMA_VERSION)) {
            return Err(JevError::input_invalid(format!(
                "session log schema_version {} is not supported; this sc-agent reads version {SESSION_LOG_SCHEMA_VERSION} (the row was probably written by a newer sc-agent, so upgrade sc-agent)",
                fields["schema_version"]
            )));
        }
        let context = RowContext {
            answer_id: parse(fields, "answer_id")?,
            session_id: parse(fields, "session_id")?,
            session_created_at: timestamp(fields, "session_created_at")?,
            repo: required_str(fields, "repo")?.to_owned(),
            project: optional_str(fields, "project")?
                .map(|text| {
                    text.parse::<ProjectLabel>()
                        .map_err(|e| wrap("project", &e))
                })
                .transpose()?,
            prompt_sha256: Sha256Digest::parse(required_str(fields, "prompt_sha256")?)
                .map_err(|e| wrap("prompt_sha256", &e))?,
            decision_schema: parse(fields, "decision_schema")?,
            question_ids: question_ids(fields)?,
            request_sha256: optional_str(fields, "request_sha256")?
                .map(|text| Sha256Digest::parse(text).map_err(|e| wrap("request_sha256", &e)))
                .transpose()?,
            ts: timestamp(fields, "ts")?,
            operation: parse(fields, "operation")?,
            backend: parse(fields, "backend")?,
            model_id: parse(fields, "model_id")?,
        };
        let error_code = optional_str(fields, "error_code")?;
        let answers = &fields["answers_json"];
        match (required_str(fields, "send_outcome")?, error_code, answers) {
            ("ok", None, Value::Object(_)) => {
                let answers: DecisionAnswers = serde_json::from_value(answers.clone())
                    .map_err(|error| bad("answers_json", &error.to_string()))?;
                Ok(Self::ok(context, answers))
            }
            ("failed", Some(code), Value::Null) => {
                let code: ErrorCode = code.parse().map_err(|e| wrap("error_code", &e))?;
                Ok(Self::failed(context, code))
            }
            ("ok", _, _) => Err(bad(
                "send_outcome",
                "is ok, so error_code must be null and answers_json an object",
            )),
            ("failed", _, _) => Err(bad(
                "send_outcome",
                "is failed, so error_code must be set and answers_json null",
            )),
            _ => Err(bad("send_outcome", "must be ok or failed")),
        }
    }
}

fn bad(key: &str, what: &str) -> JevError {
    JevError::input_invalid(format!("session log field {key:?} {what}"))
}

fn wrap(key: &str, error: &JevError) -> JevError {
    bad(key, &format!("is invalid: {error}"))
}

fn required_str<'a>(fields: &'a Map<String, Value>, key: &str) -> Result<&'a str, JevError> {
    fields[key]
        .as_str()
        .ok_or_else(|| bad(key, "must be a string"))
}

fn optional_str<'a>(
    fields: &'a Map<String, Value>,
    key: &str,
) -> Result<Option<&'a str>, JevError> {
    match &fields[key] {
        Value::Null => Ok(None),
        Value::String(text) => Ok(Some(text)),
        _ => Err(bad(key, "must be a string or null")),
    }
}

fn parse<T>(fields: &Map<String, Value>, key: &str) -> Result<T, JevError>
where
    T: std::str::FromStr<Err = JevError>,
{
    required_str(fields, key)?
        .parse()
        .map_err(|error| wrap(key, &error))
}

fn timestamp(fields: &Map<String, Value>, key: &str) -> Result<TimestampMs, JevError> {
    TimestampMs::parse_rfc3339(required_str(fields, key)?).ok_or_else(|| {
        bad(
            key,
            "must be RFC 3339 UTC with milliseconds (YYYY-MM-DDTHH:MM:SS.mmmZ)",
        )
    })
}

fn question_ids(fields: &Map<String, Value>) -> Result<Vec<QuestionId>, JevError> {
    fields["question_ids"]
        .as_array()
        .ok_or_else(|| bad("question_ids", "must be an array"))?
        .iter()
        .map(|item| {
            item.as_str()
                .ok_or_else(|| bad("question_ids", "must hold strings"))
                .and_then(|text| QuestionId::try_from(text).map_err(|e| wrap("question_ids", &e)))
        })
        .collect()
}
