//! Shared types for sc-agent crates.
//!
//! # Errors
//!
//! [`JevError`] is the only CLI and PyO3 error-id source (ADR-SAG-0001,
//! ADR-SAG-0027). [`JevError::code`] returns an [`ErrorCode`];
//! [`JevError::message`] and [`JevError::suggested_action`] are the two
//! renderers of text. The stderr JSON shape is [`ScAgentErrorResponse`].
//!
//! # Backends
//!
//! [`BACKENDS`] is the static fact table; [`BackendId::descriptor`] reads it.
//! Code outside this crate branches on descriptor facts, never on
//! `BackendId` equality (ADR-SAG-0009 r-1 amendment).
//!
//! # Models
//!
//! [`ModelId`] names which model; [`BackendId`] names how we call it. The
//! model catalog ([`MODELS`], [`ModelAliases`]) resolves aliases such as
//! `"haiku"` to the newest known model and accepts overrides.
//!
//! # Decision types
//!
//! The judge contract, the canonical judge payload and the session-log
//! schema live in `sc-decision-schema` (ADR-SAG-0030), one layer above
//! this crate. This crate re-exports none of them.
//!
//! # Cargo features (module seams)
//!
//! | Feature | Default | Gates |
//! | --- | --- | --- |
//! | (core) | always | `error`, `envelope`, `ids`, [`schema`], `cancel`, `models`, `trace` |
//! | `secret` | yes | `ResolvedApiKey` |
//! | `obs` | yes | `observability::v2` (session log port) |
//! | `git` | no | `git` (argv-only git helpers) |
//!
//! This crate links no `sc-observability*`, HTTP, Python, or tree-sitter
//! crate (REQ-SAG-0107).
//!
//! # Tests and fixtures
//!
//! Integration tests read `tests/fixtures/jev/**` at the repo root; they are
//! not part of the published crate package.

#![warn(missing_docs)]

mod cancel;
mod catalog;
mod code;
mod envelope;
mod error;
mod ids;
mod models;
pub mod schema;
mod trace;

#[cfg(feature = "secret")]
mod secret;

#[cfg(feature = "git")]
pub mod git;
#[cfg(feature = "obs")]
pub mod observability;

#[doc(inline)]
pub use cancel::CancelToken;
#[doc(inline)]
pub use catalog::{BACKENDS, BackendDescriptor, BackendFamily};
#[doc(inline)]
pub use envelope::{
    ERROR_TEXT_MAX_BYTES, EXIT_FAILURE, EXIT_RETRYABLE, ErrorDetails, ScAgentErrorResponse,
};
#[doc(inline)]
pub use error::{
    BusyCause, ErrorCategory, ErrorCode, IO_KIND_IDS, JevError, LimitUnit, NetworkErrorKind,
    PROVIDER_TEXT_MAX_BYTES, ProviderFailure, ResponseInvalidCause, bound_provider_text,
};
#[doc(inline)]
pub use ids::{
    AnswerId, BackendId, BackendKeyId, BackendSet, BlobSha, Operation, ProjectLabel,
    RepoRelativePath, RepoRoot, Rev, SessionId, Sha256Digest,
};
#[doc(inline)]
pub use models::{MODELS, ModelAliases, ModelDescriptor, ModelFamily, ModelId, UnknownModel};
#[cfg(feature = "secret")]
#[doc(inline)]
pub use secret::ResolvedApiKey;
#[doc(inline)]
pub use trace::SendTrace;
