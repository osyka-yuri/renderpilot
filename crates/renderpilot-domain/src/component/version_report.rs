//! Technology-aware version state for a component file set.
//!
//! FSR-specific representative selection lives in [`crate::fsr`]. This module
//! owns the state exposed to candidate matching, operation history, and clients
//! so Streamline's multi-file truth is not recomputed in each layer.

use crate::{ComponentFile, LibraryTechnology, Version, fsr};

/// Honest version state for one installed library component.
///
/// `Mixed` is emitted only when known file versions prove the set is not
/// uniform. A missing PE version alongside otherwise matching files is
/// `Unknown`, not an invented uniform release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentVersionReport {
    /// The component has one trustworthy display version.
    Known(Version),
    /// At least two known files prove the component contains different releases.
    Mixed {
        /// Lowest known version in the installed set.
        min: Version,
        /// Highest known version in the installed set.
        max: Version,
    },
    /// The component does not have enough metadata for a trustworthy version.
    Unknown,
}

impl ComponentVersionReport {
    /// Returns the comparison baseline when the component is known to be uniform.
    #[must_use]
    pub fn known_version(&self) -> Option<&Version> {
        match self {
            Self::Known(version) => Some(version),
            Self::Mixed { .. } | Self::Unknown => None,
        }
    }
}

/// Builds the honest version state for one installed component.
///
/// Streamline is a matched plugin set. Other technologies preserve the existing
/// FSR-aware representative policy because their component version is defined by
/// that representative rather than by every member agreeing.
#[must_use]
pub fn component_version_report(
    files: &[ComponentFile],
    technology: LibraryTechnology,
) -> ComponentVersionReport {
    if technology == LibraryTechnology::NvidiaStreamline {
        streamline_version_report(files)
    } else {
        fsr::version_representative(files)
            .and_then(ComponentFile::version)
            .cloned()
            .map_or(
                ComponentVersionReport::Unknown,
                ComponentVersionReport::Known,
            )
    }
}

fn streamline_version_report(files: &[ComponentFile]) -> ComponentVersionReport {
    let mut first_known: Option<&Version> = None;
    let mut min: Option<&Version> = None;
    let mut max: Option<&Version> = None;
    let mut has_unknown = false;

    for file in files {
        let Some(version) = file.version() else {
            has_unknown = true;
            continue;
        };

        first_known.get_or_insert(version);
        if min.is_none_or(|current| version < current) {
            min = Some(version);
        }
        if max.is_none_or(|current| version > current) {
            max = Some(version);
        }
    }

    let (Some(first), Some(min), Some(max)) = (first_known, min, max) else {
        return ComponentVersionReport::Unknown;
    };

    if min != max {
        return ComponentVersionReport::Mixed {
            min: min.clone(),
            max: max.clone(),
        };
    }

    if has_unknown {
        ComponentVersionReport::Unknown
    } else {
        ComponentVersionReport::Known(first.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PathRef, Version};

    fn versioned_file(path: &str, version: &str) -> ComponentFile {
        ComponentFile::new(PathRef::new(path).expect("path"))
            .with_version(Version::parse(version).expect("version"))
    }

    #[test]
    fn streamline_report_is_known_when_every_plugin_agrees() {
        let uniform = vec![
            versioned_file("C:/game/sl.common.dll", "2.9.0"),
            versioned_file("C:/game/sl.interposer.dll", "2.9.0"),
        ];
        assert_eq!(
            component_version_report(&uniform, LibraryTechnology::NvidiaStreamline),
            ComponentVersionReport::Known(Version::parse("2.9.0").expect("version")),
        );
    }

    #[test]
    fn streamline_report_is_mixed_when_known_plugins_disagree() {
        let mixed = vec![
            versioned_file("C:/game/sl.common.dll", "2.9.0"),
            versioned_file("C:/game/sl.interposer.dll", "2.4.0"),
        ];
        assert_eq!(
            component_version_report(&mixed, LibraryTechnology::NvidiaStreamline),
            ComponentVersionReport::Mixed {
                min: Version::parse("2.4.0").expect("version"),
                max: Version::parse("2.9.0").expect("version"),
            }
        );
    }

    #[test]
    fn streamline_report_treats_trailing_zero_versions_as_uniform() {
        let files = vec![
            versioned_file("C:/game/sl.common.dll", "2.9.0.0"),
            versioned_file("C:/game/sl.interposer.dll", "2.9.0"),
        ];
        assert_eq!(
            component_version_report(&files, LibraryTechnology::NvidiaStreamline),
            ComponentVersionReport::Known(Version::parse("2.9.0.0").expect("version")),
        );
    }

    #[test]
    fn streamline_report_is_unknown_when_a_plugin_lacks_version_metadata() {
        let files = vec![
            versioned_file("C:/game/sl.common.dll", "2.9.0"),
            ComponentFile::new(PathRef::new("C:/game/sl.interposer.dll").expect("path")),
        ];
        assert_eq!(
            component_version_report(&files, LibraryTechnology::NvidiaStreamline),
            ComponentVersionReport::Unknown,
        );
    }

    #[test]
    fn streamline_empty_set_is_unknown() {
        assert_eq!(
            component_version_report(&[], LibraryTechnology::NvidiaStreamline),
            ComponentVersionReport::Unknown,
        );
    }

    #[test]
    fn streamline_mixed_known_versions_win_over_missing_pe_metadata() {
        // Divergent known PE versions already prove mixed state; a third file
        // without version metadata must not collapse that to Unknown.
        let files = vec![
            versioned_file("C:/game/sl.common.dll", "2.9.0"),
            versioned_file("C:/game/sl.interposer.dll", "2.4.0"),
            ComponentFile::new(PathRef::new("C:/game/sl.dlss.dll").expect("path")),
        ];
        assert_eq!(
            component_version_report(&files, LibraryTechnology::NvidiaStreamline),
            ComponentVersionReport::Mixed {
                min: Version::parse("2.4.0").expect("version"),
                max: Version::parse("2.9.0").expect("version"),
            }
        );
    }

    #[test]
    fn non_streamline_uses_representative_or_unknown() {
        // Name-min representative for non-FSR picks nvngx_dlss.dll over a sibling.
        let files = vec![
            versioned_file("C:/game/nvngx_dlssg.dll", "3.1.0"),
            versioned_file("C:/game/nvngx_dlss.dll", "3.7.0"),
        ];
        assert_eq!(
            component_version_report(&files, LibraryTechnology::DlssSuperResolution),
            ComponentVersionReport::Known(Version::parse("3.7.0").expect("version")),
        );

        assert_eq!(
            component_version_report(&[], LibraryTechnology::DlssSuperResolution),
            ComponentVersionReport::Unknown,
        );
    }
}
