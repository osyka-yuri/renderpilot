use std::{cmp::Ordering, error::Error, fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Normalized dotted numeric version, for example `3.7.20`.
///
/// Display text keeps the parsed segment count (`2.9.0` stays `2.9.0`), but
/// equality and ordering treat trailing zero segments as insignificant so a PE
/// file version `2.9.0.0` matches a manifest label `2.9.0`.
#[derive(Debug, Clone)]
pub struct Version {
    text: String,
    segments: Vec<u64>,
}

impl Version {
    /// Parses and normalizes a dotted numeric version.
    pub fn parse(value: impl Into<String>) -> Result<Self, VersionParseError> {
        let value = value.into();
        let trimmed = value.trim();

        if trimmed.is_empty() {
            return Err(VersionParseError::Empty);
        }

        let mut segments = Vec::new();

        for part in trimmed.split('.') {
            if part.is_empty() {
                return Err(VersionParseError::EmptySegment);
            }

            if !part.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(VersionParseError::InvalidSegment);
            }

            let segment = part
                .parse::<u64>()
                .map_err(|_| VersionParseError::SegmentOverflow)?;
            segments.push(segment);
        }

        let text = segments
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(".");

        Ok(Self { text, segments })
    }

    /// Returns normalized version text.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Returns numeric version segments.
    pub fn segments(&self) -> &[u64] {
        &self.segments
    }

    /// Returns the major version number (first segment).
    pub fn major(&self) -> u64 {
        self.segments[0]
    }
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

// Eq / Ord / Hash all use trailing-zero-insensitive segment identity so PE
// `2.9.0.0` and manifest `2.9.0` compare and hash as the same release.

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Version {}

impl std::hash::Hash for Version {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        for segment in significant_segments(&self.segments) {
            segment.hash(state);
        }
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_segments_trailing_zeros(&self.segments, &other.segments)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Compares version segments as if the shorter side were padded with trailing zeros.
fn compare_segments_trailing_zeros(left: &[u64], right: &[u64]) -> Ordering {
    let len = left.len().max(right.len());
    for index in 0..len {
        let left_seg = left.get(index).copied().unwrap_or(0);
        let right_seg = right.get(index).copied().unwrap_or(0);
        match left_seg.cmp(&right_seg) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    Ordering::Equal
}

/// Segments that participate in [`Hash`]: drop trailing zeros (all-zero → empty).
/// Must agree with [`compare_segments_trailing_zeros`] equality.
fn significant_segments(segments: &[u64]) -> &[u64] {
    match segments.iter().rposition(|&segment| segment != 0) {
        Some(last) => &segments[..=last],
        None => &[],
    }
}

impl FromStr for Version {
    type Err = VersionParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl Serialize for Version {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(serde::de::Error::custom)
    }
}

/// Error returned when version parsing fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionParseError {
    /// Version text is empty after trimming whitespace.
    Empty,
    /// Version contains an empty segment, for example `1..2`.
    EmptySegment,
    /// Version contains a non-numeric segment.
    InvalidSegment,
    /// Version contains a segment larger than `u64`.
    SegmentOverflow,
}

impl fmt::Display for VersionParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("version cannot be empty"),
            Self::EmptySegment => formatter.write_str("version segments cannot be empty"),
            Self::InvalidSegment => formatter.write_str("version segments must be numeric"),
            Self::SegmentOverflow => formatter.write_str("version segment is too large"),
        }
    }
}

impl Error for VersionParseError {}

#[cfg(test)]
mod tests {
    use super::{Version, VersionParseError};

    #[test]
    fn version_parse_trims_valid_version() {
        let version = Version::parse(" 03.007.20 ").expect("valid version");

        assert_eq!(version.as_str(), "3.7.20");
        assert_eq!(version.segments(), &[3, 7, 20]);
        assert_eq!(version.to_string(), "3.7.20");
    }

    #[test]
    fn version_parse_rejects_empty_text() {
        let error = Version::parse(" ").expect_err("blank version should fail");

        assert_eq!(error, VersionParseError::Empty);
    }

    #[test]
    fn version_parse_rejects_empty_segment() {
        let error = Version::parse("1..2").expect_err("empty segment should fail");

        assert_eq!(error, VersionParseError::EmptySegment);
    }

    #[test]
    fn version_parse_rejects_non_numeric_segment() {
        let error = Version::parse("1.beta.2").expect_err("non-numeric segment should fail");

        assert_eq!(error, VersionParseError::InvalidSegment);
    }

    #[test]
    fn version_ordering_is_numeric() {
        let older = Version::parse("2.0").expect("valid version");
        let newer = Version::parse("10.0").expect("valid version");

        assert!(older < newer);
    }

    #[test]
    fn version_ordering_preserves_full_u64_segment_precision() {
        let penultimate = Version::parse("18446744073709551614").expect("valid version");
        let maximum = Version::parse("18446744073709551615").expect("valid version");

        assert!(penultimate < maximum);
    }

    #[test]
    fn trailing_zero_segments_are_insignificant_for_eq_and_ord() {
        let short = Version::parse("2.9.0").expect("valid");
        let long = Version::parse("2.9.0.0").expect("valid");
        let shorter = Version::parse("2.9").expect("valid");
        let patch = Version::parse("2.9.1").expect("valid");

        assert_eq!(short, long);
        assert_eq!(short, shorter);
        assert_eq!(short.as_str(), "2.9.0");
        assert_eq!(long.as_str(), "2.9.0.0");
        assert!(short < patch);
        assert!(long < patch);
        assert_eq!(short.cmp(&long), std::cmp::Ordering::Equal);
    }

    #[test]
    fn trailing_zero_equivalent_versions_share_hash_map_keys() {
        use std::collections::{HashMap, HashSet};

        let short = Version::parse("2.9.0").expect("valid");
        let long = Version::parse("2.9.0.0").expect("valid");
        let other = Version::parse("2.9.1").expect("valid");

        let mut set = HashSet::new();
        set.insert(short.clone());
        assert!(set.contains(&long));
        assert!(!set.contains(&other));

        let mut map = HashMap::new();
        map.insert(long, "label");
        assert_eq!(map.get(&short), Some(&"label"));
    }

    #[test]
    fn version_parse_rejects_segment_overflow() {
        let error =
            Version::parse("18446744073709551616").expect_err("overflowing segment should fail");

        assert_eq!(error, VersionParseError::SegmentOverflow);
    }

    #[test]
    fn version_serializes_as_canonical_string() {
        let version = Version::parse("03.007.20").expect("valid version");

        let json = serde_json::to_string(&version).expect("version should serialize");

        assert_eq!(json, r#""3.7.20""#);
    }

    #[test]
    fn version_deserialization_validates_input() {
        let error =
            serde_json::from_str::<Version>(r#""1.beta.2""#).expect_err("version should fail");

        assert!(
            error
                .to_string()
                .contains("version segments must be numeric")
        );
    }
}
