//! Decision schema versions (REQ-SAG-0142, ADR-SAG-0030).
//!
//! # Versioning rule
//!
//! A reader accepts every id in [`DecisionSchemaId::KNOWN`] and refuses any
//! other with `INPUT_INVALID` (input documents) or `RESPONSE_INVALID`
//! (backend output); the fix hint names the supported versions. Additive
//! optional fields keep the version; a removed, renamed or retyped field, or
//! a changed meaning, bumps it.
//!
//! The version lives outside the hashed bytes: it is never a key of
//! `CanonicalJudgePayload` and never part of `PayloadBundle::bytes()`, so
//! `payload_sha256` is unchanged. It rides on the wrappers around the payload
//! (the session-log row) and is not sent to any provider.

use std::fmt;
use std::str::FromStr;

use sc_agent_types::JevError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The schema family: which payload and answer mapping a version names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SchemaFamily {
    /// The canonical question and answer payload.
    Canonical,
}

impl SchemaFamily {
    /// The id spelling (`canonical`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Canonical => "canonical",
        }
    }
}

impl fmt::Display for SchemaFamily {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A decision schema id, written `"<family>.v<version>"` (`"canonical.v1"`).
///
/// # Examples
///
/// ```
/// use sc_decision_schema::{CANONICAL_V1, DecisionSchemaId};
///
/// assert_eq!(CANONICAL_V1.to_string(), "canonical.v1");
/// assert_eq!("canonical.v1".parse::<DecisionSchemaId>()?, DecisionSchemaId::CURRENT);
/// assert!("canonical.v2".parse::<DecisionSchemaId>().is_err());
/// # Ok::<(), sc_agent_types::JevError>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DecisionSchemaId {
    family: SchemaFamily,
    version: u16,
}

/// `canonical.v1`: the canonical payload and answer map of Phase A.1.
pub const CANONICAL_V1: DecisionSchemaId = DecisionSchemaId {
    family: SchemaFamily::Canonical,
    version: 1,
};

impl DecisionSchemaId {
    /// The id new documents are written with.
    pub const CURRENT: DecisionSchemaId = CANONICAL_V1;

    /// Every id a reader accepts.
    pub const KNOWN: &'static [DecisionSchemaId] = &[CANONICAL_V1];

    /// The schema family.
    #[must_use]
    pub const fn family(self) -> SchemaFamily {
        self.family
    }

    /// The version number within the family.
    #[must_use]
    pub const fn version(self) -> u16 {
        self.version
    }

    /// Parses an id read from backend output.
    ///
    /// # Errors
    ///
    /// `RESPONSE_INVALID` for an id that is not in [`Self::KNOWN`]; the
    /// detail names the supported versions.
    pub fn parse_response(text: &str) -> Result<Self, JevError> {
        Self::lookup(text).ok_or_else(|| {
            JevError::response_invalid(format!(
                "unknown decision schema {text:?}; supported: {}",
                Self::known_list()
            ))
        })
    }

    fn lookup(text: &str) -> Option<Self> {
        Self::KNOWN
            .iter()
            .copied()
            .find(|id| id.to_string() == text)
    }

    fn known_list() -> String {
        Self::KNOWN
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl fmt::Display for DecisionSchemaId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.v{}", self.family, self.version)
    }
}

impl FromStr for DecisionSchemaId {
    type Err = JevError;

    /// # Errors
    ///
    /// `INPUT_INVALID` for an id that is not in [`DecisionSchemaId::KNOWN`];
    /// the detail lists the supported ids.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::lookup(text).ok_or_else(|| {
            JevError::input_invalid(format!(
                "unknown decision schema {text:?}; supported: {}",
                Self::known_list()
            ))
        })
    }
}

impl Serialize for DecisionSchemaId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for DecisionSchemaId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}
