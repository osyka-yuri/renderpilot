use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

use super::ComponentError;

/// Lowercase hexadecimal SHA-256 hash.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Sha256Hash(String);

impl Sha256Hash {
    /// Number of hexadecimal characters in a SHA-256 hash.
    pub const HEX_LENGTH: usize = 64;

    /// Creates a normalized SHA-256 hash from a 64-character hexadecimal string.
    pub fn new(value: impl Into<String>) -> Result<Self, ComponentError> {
        let value = value.into();
        let trimmed = value.trim();

        if trimmed.len() != Self::HEX_LENGTH {
            return Err(ComponentError::InvalidSha256Hash);
        }

        if !trimmed.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(ComponentError::InvalidSha256Hash);
        }

        Ok(Self(trimmed.to_ascii_lowercase()))
    }

    /// Returns the hash as lowercase hexadecimal text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Sha256Hash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Sha256Hash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Backward-compatible alias for the SHA-256 value object.
pub type Sha256Digest = Sha256Hash;
