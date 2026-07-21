use std::{error::Error, fmt};

use serde::{Deserialize, Deserializer, Serialize};

use crate::Architecture;

/// Canonical, case-sensitive set of named PE exports.
///
/// This is an export-surface compatibility signal. It does not model a
/// function's signature, calling convention, data layout, or the complete ABI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct PeExportSet(Vec<String>);

impl PeExportSet {
    /// Maximum number of named exports accepted from one image.
    pub const MAX_NAMES: usize = 16_384;
    /// Maximum byte length of one printable-ASCII export name.
    pub const MAX_NAME_BYTES: usize = 256;

    /// Validates an already-canonical sequence.
    ///
    /// Names must be sorted, unique, non-empty printable ASCII.
    pub fn from_canonical_names(names: Vec<String>) -> Result<Self, PeExportSetError> {
        if names.is_empty() || names.len() > Self::MAX_NAMES {
            return Err(PeExportSetError);
        }
        let mut previous: Option<&str> = None;
        for name in &names {
            if name.is_empty()
                || name.len() > Self::MAX_NAME_BYTES
                || !name.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
                || previous.is_some_and(|value| value >= name.as_str())
            {
                return Err(PeExportSetError);
            }
            previous = Some(name);
        }
        Ok(Self(names))
    }

    /// Canonicalizes names observed from a PE table without hiding duplicates.
    ///
    /// Sorting is safe because export order is not part of this compatibility
    /// signal. Duplicate names remain adjacent and are rejected by the strict
    /// canonical validator.
    pub fn from_observed_names(mut names: Vec<String>) -> Result<Self, PeExportSetError> {
        names.sort();
        Self::from_canonical_names(names)
    }

    /// Returns canonical named exports.
    pub fn names(&self) -> &[String] {
        &self.0
    }

    /// Returns whether every export required by `other` is present.
    #[must_use]
    pub fn is_superset_of(&self, other: &Self) -> bool {
        let mut candidate = self.0.iter();
        let mut current = candidate.next();
        for required in &other.0 {
            loop {
                match current {
                    Some(name) if name < required => current = candidate.next(),
                    Some(name) if name == required => break,
                    _ => return false,
                }
            }
        }
        true
    }
}

impl<'de> Deserialize<'de> for PeExportSet {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let names = Vec::<String>::deserialize(deserializer)?;
        Self::from_canonical_names(names).map_err(serde::de::Error::custom)
    }
}

/// PE facts used by the export-surface compatibility guard.
///
/// Architecture and exports are atomic: a partially readable image does not
/// produce a profile that could accidentally be treated as transition-safe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeCompatibilityProfile {
    architecture: Architecture,
    named_exports: PeExportSet,
}

impl PeCompatibilityProfile {
    /// Creates a complete compatibility profile.
    #[must_use]
    pub const fn new(architecture: Architecture, named_exports: PeExportSet) -> Self {
        Self {
            architecture,
            named_exports,
        }
    }

    /// Returns the observed COFF architecture.
    #[must_use]
    pub const fn architecture(&self) -> Architecture {
        self.architecture
    }

    /// Returns the observed named export surface.
    #[must_use]
    pub const fn named_exports(&self) -> &PeExportSet {
        &self.named_exports
    }
}

/// Invalid named PE export set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeExportSetError;

impl fmt::Display for PeExportSetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "PE exports must contain 1..=16384 sorted unique printable-ASCII names of at most 256 bytes",
        )
    }
}

impl Error for PeExportSetError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_compares_export_sets() {
        let current = PeExportSet::from_canonical_names(vec!["A".into(), "C".into()]).unwrap();
        let candidate =
            PeExportSet::from_canonical_names(vec!["A".into(), "B".into(), "C".into()]).unwrap();
        assert!(candidate.is_superset_of(&current));
        assert!(!current.is_superset_of(&candidate));
    }

    #[test]
    fn serde_rejects_unsorted_or_duplicate_names() {
        assert!(serde_json::from_str::<PeExportSet>(r#"["B","A"]"#).is_err());
        assert!(serde_json::from_str::<PeExportSet>(r#"["A","A"]"#).is_err());
    }

    #[test]
    fn observed_names_are_sorted_but_duplicates_stay_invalid() {
        let set = PeExportSet::from_observed_names(vec!["C".into(), "A".into(), "B".into()])
            .expect("observed exports");
        assert_eq!(set.names(), &["A", "B", "C"]);
        assert!(
            PeExportSet::from_observed_names(vec!["B".into(), "A".into(), "A".into()]).is_err()
        );
    }

    #[test]
    fn accepts_the_complete_printable_ascii_and_length_contract() {
        assert!(PeExportSet::from_canonical_names(vec!["VR Init".into()]).is_ok());
        assert!(
            PeExportSet::from_canonical_names(vec!["A".repeat(PeExportSet::MAX_NAME_BYTES)])
                .is_ok()
        );
        assert!(
            PeExportSet::from_canonical_names(vec!["A".repeat(PeExportSet::MAX_NAME_BYTES + 1)])
                .is_err()
        );
        assert!(PeExportSet::from_canonical_names(vec!["VR\u{1}Init".into()]).is_err());
    }

    #[test]
    fn enforces_the_export_count_boundary() {
        let maximum = (0..PeExportSet::MAX_NAMES)
            .map(|index| format!("Export{index:05}"))
            .collect::<Vec<_>>();
        assert!(PeExportSet::from_canonical_names(maximum.clone()).is_ok());

        let mut excessive = maximum;
        excessive.push("Export99999".to_owned());
        assert!(PeExportSet::from_canonical_names(excessive).is_err());
    }
}
