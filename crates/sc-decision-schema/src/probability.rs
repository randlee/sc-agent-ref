//! [`Probability`]: a finite number in `[0, 1]`.

use sc_agent_types::JevError;
use serde::de;
use serde::{Deserialize, Deserializer, Serialize};

/// Text of the serde error a [`Probability`] out of range raises.
const PROBABILITY_RANGE: &str = "not a finite number in [0, 1]";

/// A finite number in `[0, 1]`: a probability or a confidence.
///
/// The documented Jev API defines every `probabilities` value as a
/// probability and every `confidence` as a number in `[0, 1]` (the Jev API
/// reference `jev-api.md` and `verdict-gating.md`: "a `choice` and a
/// `confidence` in [0, 1]"); the LLM wire form uses the same ranges.
/// [`Self::new`] and `Deserialize` enforce it, so an out-of-range reply is
/// refused at parse time and a [`QuestionAnswer`](crate::QuestionAnswer)
/// cannot hold one. It serializes as the plain number.
///
/// # Examples
///
/// ```
/// use sc_decision_schema::Probability;
///
/// let p = Probability::new(0.94)?;
/// assert_eq!(p.get(), 0.94);
/// assert_eq!(serde_json::to_string(&p)?, "0.94");
/// assert!(Probability::new(1.2).is_err());
/// assert!(Probability::new(f64::NAN).is_err());
/// assert!(serde_json::from_str::<Probability>("-0.1").is_err());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Probability(f64);

impl Probability {
    /// A probability of `value`.
    ///
    /// The error text names the range and never echoes `value`, which may
    /// come from a model reply (ADR-SAG-0020).
    ///
    /// # Errors
    ///
    /// `RESPONSE_INVALID` when `value` is NaN, infinite, below 0 or above 1.
    pub fn new(value: f64) -> Result<Self, JevError> {
        if value.is_finite() && (0.0..=1.0).contains(&value) {
            Ok(Self(value))
        } else {
            Err(JevError::response_invalid(format!(
                "a probability or confidence is {PROBABILITY_RANGE}"
            )))
        }
    }

    /// The number, in `[0, 1]`.
    #[must_use]
    pub fn get(self) -> f64 {
        self.0
    }

    /// `1 - self`, which is again in `[0, 1]`.
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_decision_schema::Probability;
    ///
    /// assert_eq!(Probability::new(0.25)?.complement().get(), 0.75);
    /// # Ok::<(), sc_agent_types::JevError>(())
    /// ```
    #[must_use]
    pub fn complement(self) -> Self {
        Self(1.0 - self.0)
    }
}

impl From<Probability> for f64 {
    fn from(value: Probability) -> Self {
        value.0
    }
}

impl<'de> Deserialize<'de> for Probability {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // The message never echoes the value (ADR-SAG-0020).
        Self::new(f64::deserialize(deserializer)?).map_err(|_| de::Error::custom(PROBABILITY_RANGE))
    }
}
