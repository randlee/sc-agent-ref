//! The model catalog: [`ModelId`], [`MODELS`] and [`ModelAliases`]
//! (REQ-SAG-0147, REQ-SAG-0148, ADR-SAG-0030).
//!
//! [`BackendId`](crate::BackendId) says how we call (Jev API, Anthropic API,
//! OpenAI API); [`ModelId`] says which model. Private module, reached through
//! the crate-root re-exports.
//!
//! A [`ModelId`] is open: a model released after this build parses as
//! [`ModelId::Other`], is sent verbatim and is logged.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::JevError;

/// A model string this build does not know.
///
/// The field is private and there is no public constructor. The only ways to
/// get one are [`ModelId::from_str`] (which returns the known variant for a
/// known string, so [`ModelId::Other`] never holds a known string and the
/// derived `PartialEq` and `Hash` stay correct) and `Deserialize`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnknownModel(Arc<str>);

impl UnknownModel {
    /// The model string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Which model: the string a backend sends and the session log records.
///
/// # Examples
///
/// ```
/// use sc_agent_types::ModelId;
///
/// let haiku: ModelId = "claude-haiku-5-5".parse()?;
/// assert_eq!(haiku, ModelId::ClaudeHaiku5_5);
/// let later: ModelId = "claude-haiku-9".parse()?;
/// assert_eq!(later.as_str(), "claude-haiku-9");
/// assert!("".parse::<ModelId>().is_err());
/// # Ok::<(), sc_agent_types::JevError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ModelId {
    /// `jev`: the latest TypeSafe Jev model, whichever version that is.
    ///
    /// sc-agent accepts `jev` or any supported version such as
    /// `jev-1.13.0`. `jev` follows the vendor's latest release, so an API
    /// change this build does not know can break it; that is expected, and
    /// the remedy is upgrading sc-agent. Pin a version (`jev-1.13.0`) to
    /// avoid the exposure. The wire value `jev` is sent as is decided in one
    /// place, `sc_decision_schema::jev::v1::wire_model`.
    Jev,
    /// `jev-1.13.0`.
    Jev1_13_0,
    /// `claude-haiku-5-5`.
    ClaudeHaiku5_5,
    /// `gpt-6-luna`.
    Gpt6Luna,
    /// Any other valid model string; never a known string.
    Other(UnknownModel),
}

impl ModelId {
    /// The known variant for `text`, if the catalog has one.
    pub(crate) fn known(text: &str) -> Option<Self> {
        match text {
            "jev" => Some(Self::Jev),
            "jev-1.13.0" => Some(Self::Jev1_13_0),
            "claude-haiku-5-5" => Some(Self::ClaudeHaiku5_5),
            "gpt-6-luna" => Some(Self::Gpt6Luna),
            _ => None,
        }
    }

    /// The model string (`Display`, serde, the wire).
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Jev => "jev",
            Self::Jev1_13_0 => "jev-1.13.0",
            Self::ClaudeHaiku5_5 => "claude-haiku-5-5",
            Self::Gpt6Luna => "gpt-6-luna",
            Self::Other(unknown) => unknown.as_str(),
        }
    }

    /// The model family; [`ModelFamily::Unknown`] for [`Self::Other`].
    ///
    /// This is the only family `schema_for` in `sc-decision-schema`
    /// consumes. It is never derived from `BackendFamily`.
    #[must_use]
    pub fn family(&self) -> ModelFamily {
        // `jev` is not a `MODELS` row: it names no one release, so it has no
        // release date and must not become the `jev` alias default.
        if matches!(self, Self::Jev) {
            return ModelFamily::TypeSafeJev;
        }
        MODELS
            .iter()
            .find(|row| row.model == self.as_str())
            .map_or(ModelFamily::Unknown, |row| row.family)
    }
}

impl fmt::Display for ModelId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ModelId {
    type Err = JevError;

    /// Parses a model string: the known variant when the catalog has it,
    /// else [`ModelId::Other`].
    ///
    /// # Errors
    ///
    /// `INPUT_INVALID` when the text is empty or has a control character.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text.is_empty() || text.chars().any(char::is_control) {
            return Err(JevError::input_invalid(format!(
                "model id {text:?} must be non-empty with no control characters; use a non-empty model string or an alias such as haiku"
            )));
        }
        Ok(Self::known(text).unwrap_or_else(|| Self::Other(UnknownModel(Arc::from(text)))))
    }
}

impl Serialize for ModelId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ModelId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// The family of a model, the key `schema_for` maps to a decision schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ModelFamily {
    /// The TypeSafe Jev models.
    TypeSafeJev,
    /// Anthropic Claude models.
    AnthropicClaude,
    /// OpenAI GPT models.
    OpenAiGpt,
    /// A local decision model.
    Local,
    /// A model string this build does not know.
    Unknown,
}

/// One row of [`MODELS`].
#[derive(Debug, Clone, Copy)]
pub struct ModelDescriptor {
    model: &'static str,
    family: ModelFamily,
    alias: &'static str,
    released: &'static str,
}

impl ModelDescriptor {
    /// The model of this row.
    ///
    /// # Panics
    ///
    /// Never for a `MODELS` row: `test_known_models_roundtrip` proves every
    /// row string is a valid model string (M-PANIC-ON-BUG).
    #[must_use]
    pub fn model(&self) -> ModelId {
        self.model
            .parse()
            .expect("MODELS rows are valid model strings (test_known_models_roundtrip)")
    }

    /// The model family.
    #[must_use]
    pub const fn family(&self) -> ModelFamily {
        self.family
    }

    /// The alias this row answers to (`"jev"`, `"haiku"`, `"luna"`).
    #[must_use]
    pub const fn alias(&self) -> &'static str {
        self.alias
    }

    /// Release date, `YYYY-MM-DD`.
    #[must_use]
    pub const fn released(&self) -> &'static str {
        self.released
    }
}

/// The known models. Per alias, the newest `released` row is the default.
pub static MODELS: &[ModelDescriptor] = &[
    ModelDescriptor {
        model: "jev-1.13.0",
        family: ModelFamily::TypeSafeJev,
        alias: "jev",
        released: "2026-06-01",
    },
    ModelDescriptor {
        model: "claude-haiku-5-5",
        family: ModelFamily::AnthropicClaude,
        alias: "haiku",
        released: "2026-09-01",
    },
    ModelDescriptor {
        model: "gpt-6-luna",
        family: ModelFamily::OpenAiGpt,
        alias: "luna",
        released: "2026-08-01",
    },
];

/// Alias to model lookup: the newest known model per alias, with overrides.
///
/// # Examples
///
/// ```
/// use sc_agent_types::{ModelAliases, ModelId};
///
/// let aliases = ModelAliases::builtin();
/// assert_eq!(aliases.latest("haiku"), Some(ModelId::ClaudeHaiku5_5));
/// let pinned = aliases.with_override("haiku", "claude-haiku-9".parse()?)?;
/// assert_eq!(pinned.latest("haiku").unwrap().as_str(), "claude-haiku-9");
/// # Ok::<(), sc_agent_types::JevError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelAliases(BTreeMap<&'static str, ModelId>);

impl ModelAliases {
    /// The newest `MODELS` row per alias.
    #[must_use]
    pub fn builtin() -> Self {
        let mut newest: BTreeMap<&'static str, &ModelDescriptor> = BTreeMap::new();
        for row in MODELS {
            let replace = newest
                .get(row.alias)
                .is_none_or(|current| row.released >= current.released);
            if replace {
                newest.insert(row.alias, row);
            }
        }
        Self(
            newest
                .into_iter()
                .map(|(alias, row)| (alias, row.model()))
                .collect(),
        )
    }

    /// The model `alias` currently names, or `None` for an unknown alias.
    #[must_use]
    pub fn latest(&self, alias: &str) -> Option<ModelId> {
        self.0.get(alias).cloned()
    }

    /// Makes `alias` name `model`.
    ///
    /// # Errors
    ///
    /// `CONFIG_INVALID` for an unknown alias; the fix hint lists the valid
    /// aliases.
    pub fn with_override(mut self, alias: &str, model: ModelId) -> Result<Self, JevError> {
        match self.0.get_mut(alias) {
            Some(slot) => {
                *slot = model;
                Ok(self)
            }
            None => Err(JevError::config_invalid(format!(
                "unknown model alias {alias:?}; valid aliases: {}",
                self.aliases().join(", ")
            ))),
        }
    }

    /// The valid aliases, sorted.
    #[must_use]
    pub fn aliases(&self) -> Vec<&'static str> {
        self.0.keys().copied().collect()
    }
}
