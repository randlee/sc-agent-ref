//! Question and choice ids (moved from `sc-agent-types`, ADR-SAG-0030).

use std::fmt;
use std::str::FromStr;

use sc_agent_types::JevError;
use serde::{Deserialize, Serialize};

macro_rules! label_newtype {
    ($(#[$meta:meta])* $name:ident, $what:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            /// Returns the text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<&str> for $name {
            type Error = JevError;

            /// # Errors
            ///
            /// `INPUT_INVALID` when the text is empty or has a control character.
            fn try_from(text: &str) -> Result<Self, Self::Error> {
                if text.is_empty() || text.chars().any(char::is_control) {
                    Err(JevError::input_invalid(format!(
                        concat!($what, " {:?} must be non-empty and have no control characters"),
                        text
                    )))
                } else {
                    Ok(Self(text.to_owned()))
                }
            }
        }

        impl TryFrom<String> for $name {
            type Error = JevError;

            fn try_from(text: String) -> Result<Self, Self::Error> {
                if text.is_empty() || text.chars().any(char::is_control) {
                    Self::try_from(text.as_str())
                } else {
                    Ok(Self(text))
                }
            }
        }

        impl FromStr for $name {
            type Err = JevError;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                Self::try_from(text)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl PartialEq<str> for $name {
            fn eq(&self, other: &str) -> bool {
                self.as_str() == other
            }
        }

        impl PartialEq<&str> for $name {
            fn eq(&self, other: &&str) -> bool {
                self.as_str() == *other
            }
        }

        impl std::borrow::Borrow<str> for $name {
            fn borrow(&self) -> &str {
                self.as_str()
            }
        }
    };
}

label_newtype!(
    /// Caller-chosen question id (non-empty, no control characters).
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_decision_schema::QuestionId;
    ///
    /// assert_eq!(QuestionId::try_from("relevant").unwrap().as_str(), "relevant");
    /// assert!(QuestionId::try_from("").is_err());
    /// ```
    QuestionId,
    "question id"
);

impl ChoiceId {
    /// The `yes` label of a `noul` answer.
    #[must_use]
    pub fn yes() -> Self {
        Self("yes".to_owned())
    }

    /// The `no` label of a `noul` answer.
    #[must_use]
    pub fn no() -> Self {
        Self("no".to_owned())
    }
}

label_newtype!(
    /// One choice label of a question (non-empty, no control characters).
    ///
    /// # Examples
    ///
    /// ```
    /// use sc_decision_schema::ChoiceId;
    ///
    /// assert_eq!(ChoiceId::try_from("yes").unwrap().as_str(), "yes");
    /// assert!(ChoiceId::try_from("a\nb").is_err());
    /// ```
    ChoiceId,
    "choice"
);
