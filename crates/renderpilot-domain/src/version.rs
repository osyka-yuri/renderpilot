use std::{cmp::Ordering, error::Error, fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Normalized dotted numeric version, for example `3.7.20`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.segments.cmp(&other.segments)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
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

        assert!(error
            .to_string()
            .contains("version segments must be numeric"));
    }
}
