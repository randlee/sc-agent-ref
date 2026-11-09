//! The static backend catalog: [`BACKENDS`] rows of [`BackendDescriptor`].
//!
//! Private module, reached through the crate-root re-exports;
//! [`BackendId::descriptor`] is the one lookup. Kept apart from `ids`
//! because sc-boundary reports a type cycle inside one module.

use crate::ids::{BackendId, BackendKeyId};
use crate::models::ModelId;

/// Broad class of a judge backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BackendFamily {
    /// The TypeSafe Jev API.
    TypeSafeJev,
    /// An API-direct LLM judge that needs a system prompt.
    Llm,
    /// A local judge (no network credential).
    Local,
}

/// Static facts about one built-in backend (one row of [`BACKENDS`]).
///
/// Fields are private; read them with the `const fn` accessors. Adding a
/// backend is a new row, not a new `match` (ADR-SAG-0027 checklist).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendDescriptor {
    id: BackendId,
    name: &'static str,
    key_id: Option<BackendKeyId>,
    family: BackendFamily,
    requires_llm_context: bool,
    supports_session_ask: bool,
    request_limit_bytes: u64,
    default_model_alias: &'static str,
    default_model: &'static str,
}

impl BackendDescriptor {
    /// The backend id this row describes.
    #[must_use]
    pub const fn id(&self) -> BackendId {
        self.id
    }

    /// The one spelling (`Display`, serde, CLI, `check.toml`): the short name
    /// `jev`, `haiku` or `luna`, which also names the model alias.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// The credential the backend sends, if any.
    #[must_use]
    pub const fn key_id(&self) -> Option<BackendKeyId> {
        self.key_id
    }

    /// The backend family.
    #[must_use]
    pub const fn family(&self) -> BackendFamily {
        self.family
    }

    /// Whether a request must carry `JudgeRequest::llm` (a system prompt).
    #[must_use]
    pub const fn requires_llm_context(&self) -> bool {
        self.requires_llm_context
    }

    /// Whether a session for this backend accepts a follow-up `ask`.
    #[must_use]
    pub const fn supports_session_ask(&self) -> bool {
        self.supports_session_ask
    }

    /// Largest wire request this backend sends, in bytes.
    #[must_use]
    pub const fn request_limit_bytes(&self) -> u64 {
        self.request_limit_bytes
    }

    /// The model alias this backend uses by default: `"jev"`, `"haiku"` or
    /// `"luna"`.
    #[must_use]
    pub const fn default_model_alias(&self) -> &'static str {
        self.default_model_alias
    }

    /// The model this row names: the alias's newest model
    /// (`ModelAliases::builtin().latest(default_model_alias())`), except for
    /// `jev`, whose default is [`ModelId::Jev`] (the latest, `jev-latest` on
    /// the wire), not the newest known version.
    ///
    /// # Panics
    ///
    /// Never for a `BACKENDS` row: `test_backend_catalog_roundtrip` proves
    /// every row names a known model.
    #[must_use]
    pub fn default_model(&self) -> ModelId {
        ModelId::known(self.default_model)
            .expect("BACKENDS rows name known models (test_backend_catalog_roundtrip)")
    }
}

/// The backend catalog: one row per built-in [`BackendId`].
///
/// `test_backend_catalog_roundtrip` checks that serde, `Display`, `FromStr`
/// and the aliases agree for every row.
pub static BACKENDS: &[BackendDescriptor] = &[JEV, HAIKU, LUNA];

pub(crate) const JEV: BackendDescriptor = BackendDescriptor {
    id: BackendId::Jev,
    name: "jev",
    key_id: Some(BackendKeyId::TypeSafe),
    family: BackendFamily::TypeSafeJev,
    requires_llm_context: false,
    supports_session_ask: false,
    request_limit_bytes: 96_000,
    default_model_alias: "jev",
    default_model: "jev",
};

pub(crate) const HAIKU: BackendDescriptor = BackendDescriptor {
    id: BackendId::Haiku,
    name: "haiku",
    key_id: Some(BackendKeyId::Anthropic),
    family: BackendFamily::Llm,
    requires_llm_context: true,
    supports_session_ask: true,
    request_limit_bytes: 96_000,
    default_model_alias: "haiku",
    default_model: "claude-haiku-5-5",
};

pub(crate) const LUNA: BackendDescriptor = BackendDescriptor {
    id: BackendId::Luna,
    name: "luna",
    key_id: Some(BackendKeyId::OpenAi),
    family: BackendFamily::Llm,
    requires_llm_context: true,
    supports_session_ask: true,
    request_limit_bytes: 96_000,
    default_model_alias: "luna",
    default_model: "gpt-6-luna",
};
