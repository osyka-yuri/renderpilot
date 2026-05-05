use std::{error::Error, fmt};

use serde::{Deserialize, Deserializer, Serialize};

use crate::text::{normalize_required_text, RequiredTextError};

macro_rules! define_identifier {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Creates an identifier from a non-empty string.
            ///
            /// Surrounding whitespace is ignored so IDs loaded from files or CLI
            /// input do not accidentally differ because of formatting.
            pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
                Ok(Self(normalize_required_text("identifier", value)?))
            }

            /// Returns the normalized identifier as a string slice.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl TryFrom<&str> for $name {
            type Error = IdentifierError;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdentifierError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

define_identifier!(
    /// Stable identifier of a game known to RenderPilot.
    GameId
);

define_identifier!(
    /// Stable identifier of a detected graphics component.
    ComponentId
);

define_identifier!(
    /// Stable identifier of an operation plan or journal entry.
    OperationId
);

define_identifier!(
    /// Stable identifier of an artifact available for replacement.
    ArtifactId
);

/// Error returned when a domain identifier cannot be created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentifierError {
    /// The provided identifier is empty after trimming whitespace.
    Empty,
}

impl fmt::Display for IdentifierError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("identifier cannot be empty"),
        }
    }
}

impl Error for IdentifierError {}

impl From<RequiredTextError> for IdentifierError {
    fn from(_: RequiredTextError) -> Self {
        Self::Empty
    }
}

#[cfg(test)]
mod tests {
    use super::{ComponentId, GameId, IdentifierError};

    #[test]
    fn game_id_trims_input() {
        let id = GameId::new("  steam:123  ").expect("valid id");

        assert_eq!(id.as_str(), "steam:123");
        assert_eq!(id.to_string(), "steam:123");
    }

    #[test]
    fn game_id_rejects_empty_input() {
        let error = GameId::new("   ").expect_err("blank ids are invalid");

        assert_eq!(error, IdentifierError::Empty);
    }

    #[test]
    fn component_id_uses_same_validation_rules() {
        let id = ComponentId::new(" component:dlss ").expect("valid id");

        assert_eq!(id.as_str(), "component:dlss");
    }
}
