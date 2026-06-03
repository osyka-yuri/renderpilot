use std::path::Path;
use std::{error::Error, fmt};

use serde::{Deserialize, Deserializer, Serialize};

use crate::text::{normalize_required_text, RequiredTextError};

const PATH_SEPARATOR: char = '/';
const WINDOWS_SEPARATOR: char = '\\';
const NUL: char = '\0';
const WINDOWS_DRIVE_ROOT_LEN: usize = 3;

/// Platform-neutral path reference stored as normalized UTF-8 text.
///
/// `PathRef` does not touch the filesystem and does not canonicalize paths.
/// Normalization is lexical only:
///
/// - surrounding whitespace is trimmed;
/// - backslashes are converted to `/`;
/// - redundant trailing separators are removed;
/// - root separators are preserved:
///   - `/` remains `/`;
///   - `D:\` becomes `D:/`, not `D:`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct PathRef(
    /// Stored normalized path text.
    String,
);

impl PathRef {
    /// Creates a normalized path reference.
    pub fn new(value: impl Into<String>) -> Result<Self, PathRefError> {
        let value = normalize_required_text("path", value).map_err(PathRefError::from)?;

        validate_path_text(&value)?;

        Ok(Self(normalize_path_text(&value)))
    }

    /// Returns normalized path text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the final path component (the file name), when present.
    ///
    /// The path is already forward-slash normalized, so this is a pure lexical
    /// lookup that never touches the filesystem.
    pub fn file_name(&self) -> Option<&str> {
        Path::new(&self.0)
            .file_name()
            .and_then(|name| name.to_str())
    }

    /// Returns the parent directory as normalized text, when present.
    ///
    /// A bare file name yields `Some("")`; a root (`/`, `C:/`) yields `None`.
    pub fn parent(&self) -> Option<&str> {
        Path::new(&self.0)
            .parent()
            .and_then(|parent| parent.to_str())
    }
}

impl fmt::Display for PathRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl AsRef<str> for PathRef {
    fn as_ref(&self) -> &str {
        self.as_str()
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

fn validate_path_text(value: &str) -> Result<(), PathRefError> {
    if contains_nul(value) {
        return Err(PathRefError::ContainsNul);
    }

    Ok(())
}

fn contains_nul(value: &str) -> bool {
    value.contains(NUL)
}

fn normalize_path_text(value: &str) -> String {
    let mut normalized = normalize_path_separators(value);

    trim_redundant_trailing_separators(&mut normalized);

    normalized
}

fn normalize_path_separators(value: &str) -> String {
    value.replace(WINDOWS_SEPARATOR, PATH_SEPARATOR.encode_utf8(&mut [0; 4]))
}

fn trim_redundant_trailing_separators(path: &mut String) {
    while has_redundant_trailing_separator(path) {
        path.pop();
    }
}

fn has_redundant_trailing_separator(path: &str) -> bool {
    path.len() > 1 && path.ends_with(PATH_SEPARATOR) && !is_windows_drive_root(path)
}

fn is_windows_drive_root(path: &str) -> bool {
    let bytes = path.as_bytes();

    bytes.len() == WINDOWS_DRIVE_ROOT_LEN
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && bytes[2] == b'/'
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
    fn path_ref_preserves_windows_drive_root_slash() {
        let path = PathRef::new("D:\\").expect("valid Windows root");

        assert_eq!(path.as_str(), "D:/");
    }

    #[test]
    fn path_ref_preserves_windows_drive_root_with_forward_slash() {
        let path = PathRef::new("D:/").expect("valid Windows root");

        assert_eq!(path.as_str(), "D:/");
    }

    #[test]
    fn path_ref_trims_duplicate_trailing_separators_after_windows_drive_root() {
        let path = PathRef::new("D://").expect("valid Windows root");

        assert_eq!(path.as_str(), "D:/");
    }

    #[test]
    fn path_ref_preserves_unix_root() {
        let path = PathRef::new("/").expect("valid root");

        assert_eq!(path.as_str(), "/");
    }

    #[test]
    fn path_ref_trims_duplicate_unix_root_separators_to_single_root() {
        let path = PathRef::new("///").expect("valid root-like path");

        assert_eq!(path.as_str(), "/");
    }

    #[test]
    fn path_ref_removes_multiple_trailing_separators() {
        let path = PathRef::new("C:/Games/Game///").expect("valid path");

        assert_eq!(path.as_str(), "C:/Games/Game");
    }

    #[test]
    fn path_ref_preserves_internal_duplicate_separators() {
        let path = PathRef::new("C:/Games//Game").expect("valid path");

        assert_eq!(path.as_str(), "C:/Games//Game");
    }

    #[test]
    fn path_ref_does_not_canonicalize_relative_segments() {
        let path = PathRef::new("C:/Games/../Game").expect("valid lexical path");

        assert_eq!(path.as_str(), "C:/Games/../Game");
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

    #[test]
    fn path_ref_serializes_as_plain_string() {
        let path = PathRef::new("C:/Games/Game").expect("valid path");

        let json = serde_json::to_string(&path).expect("path should serialize");

        assert_eq!(json, r#""C:/Games/Game""#);
    }

    #[test]
    fn path_ref_as_ref_returns_normalized_text() {
        let path = PathRef::new(r"C:\Games\Game\").expect("valid path");

        assert_eq!(path.as_ref(), "C:/Games/Game");
    }

    #[test]
    fn path_ref_file_name_returns_final_component() {
        assert_eq!(
            PathRef::new("/games/game/nvngx_dlss.dll")
                .unwrap()
                .file_name(),
            Some("nvngx_dlss.dll")
        );
        assert_eq!(
            PathRef::new("nvngx_dlss.dll").unwrap().file_name(),
            Some("nvngx_dlss.dll")
        );
    }

    #[test]
    fn path_ref_parent_returns_directory() {
        assert_eq!(
            PathRef::new("/games/game/nvngx_dlss.dll").unwrap().parent(),
            Some("/games/game")
        );
        // A bare file name has an empty parent; a root has none.
        assert_eq!(PathRef::new("nvngx_dlss.dll").unwrap().parent(), Some(""));
        assert_eq!(PathRef::new("/").unwrap().parent(), None);
    }

    #[cfg(windows)]
    #[test]
    fn path_ref_parent_preserves_windows_drive_root() {
        // Drive semantics only apply on Windows targets, where the bundle engine runs.
        assert_eq!(
            PathRef::new("C:/nvngx_dlss.dll").unwrap().parent(),
            Some("C:/")
        );
        assert_eq!(
            PathRef::new("C:/Games/Game/nvngx_dlss.dll")
                .unwrap()
                .parent(),
            Some("C:/Games/Game")
        );
    }
}
