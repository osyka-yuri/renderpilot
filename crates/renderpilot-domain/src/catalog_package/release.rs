//! Catalog release identity and availability.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::PackageVersion;

/// Stability channel declared by a curated package release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseChannel {
    /// Production release.
    Stable,
    /// Upstream beta release.
    Beta,
    /// Upstream preview release.
    Preview,
    /// Debug build.
    Debug,
}

/// Whether a catalog package is still available from the active remote catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogPackageAvailability {
    /// Package is present in the active catalog.
    Available,
    /// Package exists only as a local downloaded receipt.
    LocalOnly,
}

/// Exact curated package release identity used for display and selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageRelease {
    /// Canonical full package version.
    pub version: PackageVersion,
    /// Release stability channel.
    pub channel: ReleaseChannel,
    /// Optional supplemental annotation.
    pub label: Option<String>,
    /// Versions of members in a composite release, keyed by stable component
    /// name and serialized in deterministic key order.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub components: BTreeMap<String, PackageVersion>,
}
