use std::{error::Error, fmt};

use serde::{Deserialize, Deserializer, Serialize};

use crate::text::{normalize_required_text, RequiredTextError};

/// Platform-neutral path reference stored as normalized UTF-8 text.
///
/// `PathRef` does not touch the filesystem and does not canonicalize paths.
/// Normalization is lexical only: whitespace is trimmed, backslashes are
/// converted to `/`, and trailing separators are removed except for `/`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct PathRef(String);

impl PathRef {
    /// Creates a normalized path reference.
    pub fn new(value: impl Into<String>) -> Result<Self, PathRefError> {
        let trimmed = normalize_required_text("path", value)?;

        if trimmed.contains('\0') {
            return Err(PathRefError::ContainsNul);
        }

        let normalized = normalize_separators(&trimmed);
        Ok(Self(normalized))
    }

    /// Returns normalized path text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PathRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<&str> for PathRef {
    type Error = PathRefError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for PathRef {
    type Error = PathRefError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for PathRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Error returned when a path reference cannot be normalized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathRefError {
    /// Path text is empty after trimming whitespace.
    Empty,
    /// Path text contains a NUL byte.
    ContainsNul,
}

impl fmt::Display for PathRefError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("path cannot be empty"),
            Self::ContainsNul => formatter.write_str("path cannot contain NUL bytes"),
        }
    }
}

impl Error for PathRefError {}

impl From<RequiredTextError> for PathRefError {
    fn from(_: RequiredTextError) -> Self {
        Self::Empty
    }
}

fn normalize_separators(value: &str) -> String {
    let mut normalized = value.replace('\\', "/");

    while normalized.len() > 1 && normalized.ends_with('/') {
        normalized.pop();
    }

    normalized
}

#[cfg(test)]
mod tests {
    use super::{PathRef, PathRefError};

    #[test]
    fn path_ref_normalizes_windows_separators_and_trailing_slash() {
        let path = PathRef::new(r"  C:\Games\Cyberpunk 2077\ ").expect("valid path");

        assert_eq!(path.as_str(), "C:/Games/Cyberpunk 2077");
    }

    #[test]
    fn path_ref_preserves_unix_root() {
        let path = PathRef::new("/").expect("valid root");

        assert_eq!(path.as_str(), "/");
    }

    #[test]
    fn path_ref_rejects_blank_text() {
        let error = PathRef::new("  ").expect_err("blank path should fail");

        assert_eq!(error, PathRefError::Empty);
    }

    #[test]
    fn path_ref_rejects_nul_bytes() {
        let error = PathRef::new("C:/Games/\0Game").expect_err("NUL should fail");

        assert_eq!(error, PathRefError::ContainsNul);
    }

    #[test]
    fn path_ref_deserialization_normalizes_input() {
        let path: PathRef =
            serde_json::from_str(r#""C:\\Games\\Cyberpunk 2077\\""#).expect("valid path json");

        assert_eq!(path.as_str(), "C:/Games/Cyberpunk 2077");
    }
}
