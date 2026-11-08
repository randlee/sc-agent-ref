//! The data-only decision schema (ADR-SAG-0030).
//!
//! One home for every decision type: the question and answer payload, the
//! [`JudgeBackend`] contract whose signatures name them, the Haiku/Luna
//! input/output mapping ([`llm`]), the TypeSafe Jev API request and response
//! types ([`jev`]), the per-model-family variant mechanism
//! ([`variants`]), the schema version id ([`DecisionSchemaId`]) and the
//! session-log SQLite schema with its `fields{}` projection
//! ([`session_log`], ADR-SAG-0031).
//!
//! # Layering
//!
//! This crate sits above `sc-agent-types` (ids, errors, the backend and model
//! catalogs, [`SendTrace`](sc_agent_types::SendTrace)). It depends on
//! `sc-agent-types` with `default-features = false` (core only), `serde` and
//! `serde_json`, and does no I/O.
//!
//! # Semver
//!
//! No promise beyond the schema versions this crate names ([`DecisionSchemaId`]),
//! plus one rule: [`JudgeBackend`] is open, and new methods get default
//! bodies. The trait stays dyn-compatible (`&self` receivers, no generic
//! methods, no `Self` in signatures).
//!
//! # Cargo features
//!
//! | Feature | Default | Gates |
//! | --- | --- | --- |
//! | `test-util` | no | `test_util::MockJudgeBackend` |

#![warn(missing_docs)]

mod ids;
pub mod jev;
mod judge;
pub mod llm;
mod payload;
mod probability;
pub mod session_log;
pub mod variants;
mod version;

#[cfg(feature = "test-util")]
pub mod test_util;

#[doc(inline)]
pub use ids::{ChoiceId, QuestionId};
#[doc(inline)]
pub use judge::{
    Attestation, BackendAttestation, BackendCapabilities, EqualInputRecord, JudgeBackend,
    JudgeOutcome, JudgeRequest, LlmWireContext, MultiBackendMode, QuestionAnswer,
};
#[doc(inline)]
pub use payload::{
    CanonicalJudgePayload, ChoiceAnswer, DecisionAnswers, PayloadBundle, Question, QuestionKind,
    canonical_json_bytes,
};
#[doc(inline)]
pub use probability::Probability;
#[doc(inline)]
pub use version::{CANONICAL_V1, DecisionSchemaId, SchemaFamily};
