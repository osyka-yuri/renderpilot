use std::fmt::Write as _;
use std::{error::Error, fmt};

use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

use crate::component::{ComponentFile, Sha256Hash};
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

impl ArtifactId {
    /// Derives a stable artifact identifier for a bundle of files.
    ///
    /// The identifier is the SHA-256 of the alphabetically sorted, concatenated
    /// per-file SHA-256 hex digests, prefixed with `artifact:`. Because every hash
    /// is a fixed-width 64-character hex string, the concatenation is unambiguous,
    /// and sorting makes the result independent of file ordering. A single-file
    /// bundle therefore yields a deterministic identifier, and two folders that
    /// contain byte-identical file sets collapse onto the same artifact.
    ///
    /// Callers must pass at least one hash; an empty set would hash to a constant,
    /// meaningless id. Use [`ArtifactId::for_component_files`] when the set may be
    /// empty (it returns `None` instead).
    #[must_use]
    pub fn for_bundle<'a, I>(shas: I) -> Self
    where
        I: IntoIterator<Item = &'a Sha256Hash>,
    {
        let mut hexes: Vec<&str> = shas.into_iter().map(Sha256Hash::as_str).collect();
        debug_assert!(
            !hexes.is_empty(),
            "ArtifactId::for_bundle requires at least one file hash; an empty bundle yields a constant, meaningless id",
        );
        hexes.sort_unstable();

        let mut hasher = Sha256::new();
        for hex in &hexes {
            hasher.update(hex.as_bytes());
        }

        let mut id = String::with_capacity("artifact:".len() + Sha256Hash::HEX_LENGTH);
        id.push_str("artifact:");
        for byte in hasher.finalize() {
            // 2 lowercase hex digits per byte; writing to a String never fails.
            let _ = write!(id, "{byte:02x}");
        }

        Self(id)
    }

    /// Derives the bundle identifier for a set of component files.
    ///
    /// Returns `None` when the set is empty or any file is missing its SHA-256,
    /// i.e. when the content identity cannot be determined. Two file sets with the
    /// same content (in any order) produce the same id, so this is the canonical
    /// way to ask "is bundle A the same content as bundle B?".
    #[must_use]
    pub fn for_component_files(files: &[ComponentFile]) -> Option<Self> {
        let shas = files
            .iter()
            .map(ComponentFile::sha256)
            .collect::<Option<Vec<&Sha256Hash>>>()?;

        if shas.is_empty() {
            return None;
        }

        Some(Self::for_bundle(shas))
    }
}

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
    use super::{ArtifactId, ComponentId, GameId, IdentifierError};
    use crate::Sha256Hash;

    #[test]
    fn artifact_id_for_bundle_is_order_independent_and_deterministic() {
        let a = Sha256Hash::new("a".repeat(Sha256Hash::HEX_LENGTH)).expect("valid hash");
        let b = Sha256Hash::new("b".repeat(Sha256Hash::HEX_LENGTH)).expect("valid hash");

        let forward = ArtifactId::for_bundle([&a, &b]);
        let reversed = ArtifactId::for_bundle([&b, &a]);

        assert_eq!(forward, reversed, "ordering must not change the bundle id");
        assert!(forward.as_str().starts_with("artifact:"));
        assert_eq!(
            forward.as_str().len(),
            "artifact:".len() + Sha256Hash::HEX_LENGTH
        );

        let single = ArtifactId::for_bundle([&a]);
        let other = ArtifactId::for_bundle([&b]);
        assert_ne!(single, other, "different content yields different ids");
    }

    #[test]
    fn artifact_id_for_component_files_is_order_independent_and_handles_missing() {
        use crate::{ComponentFile, PathRef};

        let sha_a = Sha256Hash::new("a".repeat(Sha256Hash::HEX_LENGTH)).expect("valid hash");
        let sha_b = Sha256Hash::new("b".repeat(Sha256Hash::HEX_LENGTH)).expect("valid hash");
        let file_a =
            ComponentFile::new(PathRef::new("/g/a.dll").expect("path")).with_sha256(sha_a.clone());
        let file_b =
            ComponentFile::new(PathRef::new("/g/b.dll").expect("path")).with_sha256(sha_b.clone());

        let forward = ArtifactId::for_component_files(&[file_a.clone(), file_b.clone()]);
        let reversed = ArtifactId::for_component_files(&[file_b, file_a]);
        assert_eq!(forward, reversed);
        assert_eq!(forward, Some(ArtifactId::for_bundle([&sha_a, &sha_b])));

        // Missing sha or empty set has no content identity.
        let no_sha = ComponentFile::new(PathRef::new("/g/c.dll").expect("path"));
        assert_eq!(ArtifactId::for_component_files(&[no_sha]), None);
        assert_eq!(ArtifactId::for_component_files(&[]), None);
    }

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
