//! Typed Xiph release dimensions kept separate from physical deployments.

use std::collections::{BTreeMap, BTreeSet};

use crate::{ComponentFile, PackageVersion};

use super::naming::{XiphMember, parse_runtime_file_name};

/// One independently versioned Xiph upstream release coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum XiphReleaseAxis {
    /// The Ogg container library.
    Ogg,
    /// The Vorbis codec family.
    Vorbis,
}

impl XiphReleaseAxis {
    /// Returns the stable catalog component key for this axis.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ogg => "ogg",
            Self::Vorbis => "vorbis",
        }
    }

    /// Maps one physical member to its upstream release coordinate.
    #[must_use]
    pub const fn for_member(member: XiphMember) -> Self {
        match member {
            XiphMember::Ogg => Self::Ogg,
            XiphMember::Vorbis | XiphMember::VorbisFile | XiphMember::VorbisEnc => Self::Vorbis,
        }
    }
}

/// Upstream coordinates required by one physical Xiph deployment.
///
/// The axes deliberately say nothing about concrete versions. A Vorbis-family
/// deployment always requires the Ogg coordinate, including an embedded-Ogg
/// build whose Ogg DLL is not installed separately.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct XiphReleaseAxes(BTreeSet<XiphReleaseAxis>);

impl XiphReleaseAxes {
    /// Derives release axes from the semantic physical members of a layout.
    #[must_use]
    pub fn from_members(members: impl IntoIterator<Item = XiphMember>) -> Self {
        let mut axes = BTreeSet::new();
        let mut has_vorbis = false;
        for member in members {
            let axis = XiphReleaseAxis::for_member(member);
            has_vorbis |= axis == XiphReleaseAxis::Vorbis;
            axes.insert(axis);
        }
        if has_vorbis {
            axes.insert(XiphReleaseAxis::Ogg);
        }
        Self(axes)
    }

    /// Derives axes from classified component files before a PE layout is
    /// available. Callers that validate replacement topology use the axes
    /// retained by [`super::XiphLayout`] instead.
    #[must_use]
    pub fn from_component_files(files: &[ComponentFile]) -> Option<Self> {
        if files.is_empty() {
            return None;
        }
        let members = files
            .iter()
            .map(|file| {
                parse_runtime_file_name(file.path().file_name()?)
                    .ok()?
                    .map(|runtime| runtime.member())
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self::from_members(members))
    }

    /// Returns axes in deterministic catalog-key order.
    pub fn iter(&self) -> impl Iterator<Item = XiphReleaseAxis> + '_ {
        self.0.iter().copied()
    }

    /// Returns whether the coordinate is mandatory for this deployment.
    #[must_use]
    pub fn contains(&self, axis: XiphReleaseAxis) -> bool {
        self.0.contains(&axis)
    }
}

/// Concrete upstream versions for a declared [`XiphReleaseAxes`] set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XiphReleaseVersions(BTreeMap<XiphReleaseAxis, PackageVersion>);

impl XiphReleaseVersions {
    /// Decodes required Xiph versions from a catalog component map.
    ///
    /// Unknown catalog keys are ignored here because catalog validation owns
    /// technology-specific schema validation. Missing required axes fail
    /// closed.
    #[must_use]
    pub fn from_catalog_components(
        axes: &XiphReleaseAxes,
        components: &BTreeMap<String, PackageVersion>,
    ) -> Option<Self> {
        let versions = axes
            .iter()
            .map(|axis| {
                components
                    .get(axis.as_str())
                    .cloned()
                    .map(|version| (axis, version))
            })
            .collect::<Option<BTreeMap<_, _>>>()?;
        Some(Self(versions))
    }

    /// Returns the version for one required coordinate.
    #[must_use]
    pub fn get(&self, axis: XiphReleaseAxis) -> Option<&PackageVersion> {
        self.0.get(&axis)
    }

    /// Serializes versions at the catalog boundary using stable component keys.
    #[must_use]
    pub fn to_catalog_components(&self) -> BTreeMap<String, PackageVersion> {
        self.0
            .iter()
            .map(|(axis, version)| (axis.as_str().to_owned(), version.clone()))
            .collect()
    }
}
