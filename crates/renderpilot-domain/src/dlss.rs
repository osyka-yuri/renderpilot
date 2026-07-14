//! Pure compatibility rules for NVIDIA DLSS replacement binaries.

use crate::Version;

/// Returns whether two verified DLSS Super Resolution versions belong to a
/// mutually replaceable generation. DLSS 1.x is isolated from DLSS 2.x and
/// later; transitions inside either side are permitted.
#[must_use]
pub fn versions_are_compatible(current: &Version, candidate: &Version) -> bool {
    (current.major() == 1) == (candidate.major() == 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dlss_one_is_isolated_from_later_generations() {
        let one = Version::parse("1.5").expect("version");
        let two = Version::parse("2.5").expect("version");
        let four = Version::parse("4.0").expect("version");

        assert!(versions_are_compatible(&one, &one));
        assert!(versions_are_compatible(&two, &four));
        assert!(!versions_are_compatible(&one, &two));
        assert!(!versions_are_compatible(&four, &one));
    }
}
