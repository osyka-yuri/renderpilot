use std::{cmp::Ordering, error::Error, fmt, hash::Hash, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{Version, VersionParseError};

/// Canonical package version with an optional SemVer-compatible prerelease suffix.
///
/// Unlike [`Version`], this type represents package-registry identity rather than a
/// PE file version. Numeric core ordering remains trailing-zero-insensitive, while
/// prerelease identifiers follow NuGet/SemVer precedence.
#[derive(Debug, Clone)]
pub struct PackageVersion {
    text: String,
    numeric_core: Version,
    prerelease: Option<String>,
}

impl PackageVersion {
    /// Parses and normalizes a package version.
    pub fn parse(value: impl Into<String>) -> Result<Self, PackageVersionParseError> {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(PackageVersionParseError::Empty);
        }
        if trimmed.contains('+') {
            return Err(PackageVersionParseError::UnsupportedBuildMetadata);
        }

        let (numeric, prerelease) = match trimmed.split_once('-') {
            Some((numeric, prerelease)) => (numeric, Some(prerelease)),
            None => (trimmed, None),
        };
        let parsed_numeric_core =
            Version::parse(numeric).map_err(PackageVersionParseError::InvalidNumericCore)?;
        if parsed_numeric_core.segments().len() > 4 {
            return Err(PackageVersionParseError::TooManyNumericSegments);
        }
        let mut numeric_segments = parsed_numeric_core.segments().to_vec();
        while numeric_segments.len() < 3 {
            numeric_segments.push(0);
        }
        if numeric_segments.len() == 4 && numeric_segments[3] == 0 {
            numeric_segments.pop();
        }
        let numeric_core = Version::parse(
            numeric_segments
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join("."),
        )
        .expect("normalized package numeric core must remain valid");

        let prerelease = prerelease.map(normalize_prerelease).transpose()?;
        let text = match &prerelease {
            Some(prerelease) => format!("{}-{prerelease}", numeric_core.as_str()),
            None => numeric_core.as_str().to_owned(),
        };
        Ok(Self {
            text,
            numeric_core,
            prerelease,
        })
    }

    /// Returns the canonical full package version.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Returns the numeric package-version core.
    pub const fn numeric_core(&self) -> &Version {
        &self.numeric_core
    }

    /// Returns the normalized prerelease suffix without the leading hyphen.
    pub fn prerelease(&self) -> Option<&str> {
        self.prerelease.as_deref()
    }

    /// Returns whether this package version is a prerelease.
    pub const fn is_prerelease(&self) -> bool {
        self.prerelease.is_some()
    }
}

fn normalize_prerelease(value: &str) -> Result<String, PackageVersionParseError> {
    if value.is_empty() {
        return Err(PackageVersionParseError::EmptyPrerelease);
    }
    for identifier in value.split('.') {
        if identifier.is_empty() {
            return Err(PackageVersionParseError::EmptyPrereleaseIdentifier);
        }
        if !identifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(PackageVersionParseError::InvalidPrereleaseIdentifier);
        }
        if identifier.len() > 1
            && identifier.starts_with('0')
            && identifier.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(PackageVersionParseError::NonCanonicalNumericPrerelease);
        }
    }
    Ok(value.to_ascii_lowercase())
}

impl fmt::Display for PackageVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl PartialEq for PackageVersion {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for PackageVersion {}

impl Hash for PackageVersion {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.numeric_core.hash(state);
        self.prerelease.hash(state);
    }
}

impl Ord for PackageVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        self.numeric_core
            .cmp(&other.numeric_core)
            .then_with(|| compare_prerelease(self.prerelease(), other.prerelease()))
    }
}

impl PartialOrd for PackageVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn compare_prerelease(left: Option<&str>, right: Option<&str>) -> Ordering {
    match (left, right) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(left), Some(right)) => {
            let mut left = left.split('.');
            let mut right = right.split('.');
            loop {
                match (left.next(), right.next()) {
                    (Some(left), Some(right)) => {
                        let ordering = compare_prerelease_identifier(left, right);
                        if ordering != Ordering::Equal {
                            return ordering;
                        }
                    }
                    (Some(_), None) => return Ordering::Greater,
                    (None, Some(_)) => return Ordering::Less,
                    (None, None) => return Ordering::Equal,
                }
            }
        }
    }
}

fn compare_prerelease_identifier(left: &str, right: &str) -> Ordering {
    let left_numeric = left.bytes().all(|byte| byte.is_ascii_digit());
    let right_numeric = right.bytes().all(|byte| byte.is_ascii_digit());
    match (left_numeric, right_numeric) {
        (true, true) => compare_decimal_text(left, right),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => left.cmp(right),
    }
}

fn compare_decimal_text(left: &str, right: &str) -> Ordering {
    let left = left.trim_start_matches('0');
    let right = right.trim_start_matches('0');
    let left = if left.is_empty() { "0" } else { left };
    let right = if right.is_empty() { "0" } else { right };
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

impl FromStr for PackageVersion {
    type Err = PackageVersionParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl Serialize for PackageVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PackageVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        let parsed = Self::parse(&value).map_err(serde::de::Error::custom)?;
        if value != parsed.as_str() {
            return Err(serde::de::Error::custom(
                PackageVersionParseError::NonCanonicalWireValue,
            ));
        }
        Ok(parsed)
    }
}

/// Error returned when a package version is malformed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageVersionParseError {
    /// Version text is empty.
    Empty,
    /// Numeric core is not a valid dotted numeric version.
    InvalidNumericCore(VersionParseError),
    /// NuGet package versions support at most four numeric core segments.
    TooManyNumericSegments,
    /// A hyphen is present without a prerelease suffix.
    EmptyPrerelease,
    /// A prerelease dot separates an empty identifier.
    EmptyPrereleaseIdentifier,
    /// A prerelease identifier contains unsupported characters.
    InvalidPrereleaseIdentifier,
    /// A numeric prerelease identifier contains a leading zero.
    NonCanonicalNumericPrerelease,
    /// Build metadata is intentionally unsupported by the V1 package identity.
    UnsupportedBuildMetadata,
    /// A wire value is not already in canonical package-version form.
    NonCanonicalWireValue,
}

impl fmt::Display for PackageVersionParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("package version cannot be empty"),
            Self::InvalidNumericCore(error) => write!(formatter, "invalid numeric core: {error}"),
            Self::TooManyNumericSegments => {
                formatter.write_str("package version has more than four numeric segments")
            }
            Self::EmptyPrerelease => formatter.write_str("prerelease suffix cannot be empty"),
            Self::EmptyPrereleaseIdentifier => {
                formatter.write_str("prerelease identifiers cannot be empty")
            }
            Self::InvalidPrereleaseIdentifier => formatter.write_str(
                "prerelease identifiers must contain only ASCII letters, digits, or hyphens",
            ),
            Self::NonCanonicalNumericPrerelease => {
                formatter.write_str("numeric prerelease identifiers cannot contain leading zeros")
            }
            Self::UnsupportedBuildMetadata => formatter
                .write_str("package build metadata is unsupported by catalog package version V1"),
            Self::NonCanonicalWireValue => formatter
                .write_str("catalogue package version must use its canonical NuGet spelling"),
        }
    }
}

impl Error for PackageVersionParseError {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct PackageVersionCases {
        parse: Vec<ParseCase>,
        rejected: Vec<RejectedCase>,
        order: Vec<OrderCase>,
    }

    #[derive(Deserialize)]
    struct ParseCase {
        input: String,
        canonical: String,
    }

    #[derive(Deserialize)]
    struct RejectedCase {
        input: String,
    }

    #[derive(Deserialize)]
    struct OrderCase {
        lower: String,
        higher: String,
    }

    fn shared_cases() -> PackageVersionCases {
        serde_json::from_str(include_str!("../../../testdata/package-version-cases.json"))
            .expect("shared package-version corpus must be valid")
    }

    #[test]
    fn matches_shared_package_version_corpus() {
        let cases = shared_cases();
        for case in cases.parse {
            assert_eq!(
                PackageVersion::parse(&case.input)
                    .unwrap_or_else(|error| panic!("failed to parse `{}`: {error}", case.input))
                    .as_str(),
                case.canonical
            );
        }
        for case in cases.rejected {
            assert!(
                PackageVersion::parse(&case.input).is_err(),
                "`{}` must be rejected",
                case.input
            );
        }
        for case in cases.order {
            let lower = PackageVersion::parse(&case.lower).expect("lower version must parse");
            let higher = PackageVersion::parse(&case.higher).expect("higher version must parse");
            assert!(
                lower < higher,
                "{} must sort before {}",
                case.lower,
                case.higher
            );
        }
    }

    #[test]
    fn parses_and_normalizes_microsoft_preview_versions() {
        let version = PackageVersion::parse(" 01.0721.002-PREVIEW ").expect("valid");
        assert_eq!(version.as_str(), "1.721.2-preview");
        assert_eq!(version.numeric_core().as_str(), "1.721.2");
        assert_eq!(version.prerelease(), Some("preview"));

        assert_eq!(
            PackageVersion::parse("1.8.2404.55-mesh-nodes-preview")
                .expect("valid")
                .as_str(),
            "1.8.2404.55-mesh-nodes-preview"
        );
        assert_eq!(
            PackageVersion::parse("1.4.0-preview2-2606.904")
                .expect("valid")
                .as_str(),
            "1.4.0-preview2-2606.904"
        );
    }

    #[test]
    fn orders_stable_and_prerelease_with_nuget_precedence() {
        let preview1 = PackageVersion::parse("1.4.0-preview1-2603.504").unwrap();
        let preview2 = PackageVersion::parse("1.4.0-preview2-2606.904").unwrap();
        let stable = PackageVersion::parse("1.4.0").unwrap();
        assert!(preview1 < preview2);
        assert!(preview2 < stable);
    }

    #[test]
    fn numeric_prerelease_identifiers_compare_without_integer_limits() {
        let left = PackageVersion::parse("1.0.0-preview.99999999999999999999").unwrap();
        let right = PackageVersion::parse("1.0.0-preview.100000000000000000000").unwrap();
        assert!(left < right);
    }

    #[test]
    fn normalizes_nuget_equivalent_versions() {
        for value in ["1", "1.0", "1.0.0", "1.0.0.0"] {
            assert_eq!(
                PackageVersion::parse(value).expect("valid").as_str(),
                "1.0.0"
            );
        }
    }

    #[test]
    fn rejects_malformed_versions() {
        assert!(matches!(
            PackageVersion::parse("1.2.3.4.5"),
            Err(PackageVersionParseError::TooManyNumericSegments)
        ));
        assert!(matches!(
            PackageVersion::parse("1.2.3-preview..1"),
            Err(PackageVersionParseError::EmptyPrereleaseIdentifier)
        ));
        assert!(matches!(
            PackageVersion::parse("1.2.3+"),
            Err(PackageVersionParseError::UnsupportedBuildMetadata)
        ));
        assert!(matches!(
            PackageVersion::parse("1.2.3+build.7"),
            Err(PackageVersionParseError::UnsupportedBuildMetadata)
        ));
        assert!(matches!(
            PackageVersion::parse("1.2.3-preview.01"),
            Err(PackageVersionParseError::NonCanonicalNumericPrerelease)
        ));
    }

    #[test]
    fn deserialization_accepts_only_canonical_wire_values() {
        for value in [
            "1",
            "1.0",
            "1.0.0.0",
            "01.0.0",
            "1.0.0+build.7",
            "1.0.0-PREVIEW",
        ] {
            let encoded = format!("\"{value}\"");
            assert!(serde_json::from_str::<PackageVersion>(&encoded).is_err());
        }
    }
}
