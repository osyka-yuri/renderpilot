//! Versioned contracts for the graphics-library catalog.

use renderpilot_domain::{Architecture, RuntimeCompatibility};
use serde::{Deserialize, Serialize};

/// Complete, validated catalog snapshot exposed to callers.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryCatalog {
    /// Catalog schema version.
    pub schema_version: u32,
    /// Generation timestamp of the activating index.
    pub generated_at: String,
    /// Supported vendor snapshots activated by the index.
    pub vendors: Vec<LibraryVendorCatalog>,
}

/// One vendor's immutable catalog snapshot.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryVendorCatalog {
    /// Vendor identity and display metadata.
    pub vendor: LibraryVendor,
    /// Generation timestamp of this immutable vendor snapshot.
    pub generated_at: String,
    /// Physical DLL artifacts addressable by package members.
    pub artifacts: Vec<LibraryArtifactRecord>,
    /// Explicit install units. Consumers never infer packages from artifacts.
    pub packages: Vec<LibraryPackage>,
}

/// Catalog vendor metadata.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryVendor {
    /// Stable vendor identifier.
    pub id: String,
    /// User-facing vendor name.
    pub display_name: String,
}

/// Physical DLL and its transport object.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryArtifactRecord {
    /// Content-addressed artifact identifier (`sha256:<digest>`).
    pub artifact_id: String,
    /// Stable upstream library family identifier.
    pub library_id: String,
    /// Original DLL file name.
    pub file_name: String,
    /// PE file version.
    pub file_version: String,
    /// DLL architecture.
    pub architecture: Architecture,
    /// Uncompressed DLL content metadata.
    pub dll: LibraryContent,
    /// Compressed CDN transport metadata.
    pub transport: LibraryTransport,
    /// Authenticode status.
    pub signature: SignatureInfo,
    /// Forward-compatible vendor metadata ignored by core behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Size and digest of uncompressed content.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryContent {
    /// Lowercase SHA-256 digest.
    pub sha256: String,
    /// Exact content length.
    pub size_bytes: u64,
}

/// Address of a compressed artifact in the catalog CDN.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryTransport {
    /// Compression format (v1 supports `zstd`).
    pub compression: String,
    /// CDN-relative, content-addressed object key.
    pub object_key: String,
    /// Lowercase SHA-256 digest of the compressed bytes.
    pub sha256: String,
    /// Exact compressed content length.
    pub size_bytes: u64,
}

/// Explicit package/install-unit definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryPackage {
    /// Stable package identifier used by UI actions and local state.
    pub package_id: String,
    /// Hash of the canonical package contract.
    pub revision_sha256: String,
    /// Stable graphics-technology slug.
    pub technology: String,
    /// Package variant within the technology.
    pub variant: String,
    /// User-facing package name.
    pub display_name: String,
    /// Release metadata.
    pub release: LibraryRelease,
    /// Runtime target and compatibility constraints.
    pub target: LibraryTarget,
    /// Optional upstream registry provenance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<LibraryProvenance>,
    /// Ordered package members; the primary member must be first.
    pub members: Vec<LibraryPackageMember>,
    /// Forward-compatible vendor metadata ignored by core behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<serde_json::Map<String, serde_json::Value>>,
}

/// User-facing package release metadata.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryRelease {
    /// Canonical package version used for presentation, ordering, and selection.
    pub version: String,
    /// Release stability channel.
    pub channel: LibraryReleaseChannel,
    /// Optional supplemental annotation displayed verbatim after the version.
    pub label: Option<String>,
}

/// Release stability channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryReleaseChannel {
    /// Production release.
    Stable,
    /// Preview release.
    Beta,
    /// Debug build.
    Debug,
}

/// Windows runtime target for a package.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryTarget {
    /// Operating system identifier (v1 supports `windows`).
    pub os: String,
    /// Required executable architecture.
    pub architecture: Architecture,
    /// Optional ABI compatibility requirement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<RuntimeCompatibility>,
}

/// Upstream package provenance.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LibraryProvenance {
    /// NuGet package identity and registry digest.
    Nuget {
        /// NuGet package identifier.
        package_id: String,
        /// NuGet package version.
        version: String,
        /// Base64-encoded SHA-512 supplied by NuGet registration metadata.
        package_sha512: String,
    },
}

/// One physical artifact installed as part of a package.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryPackageMember {
    /// Referenced physical artifact identifier.
    pub artifact_id: String,
    /// Semantic role within the package.
    pub role: String,
    /// Target DLL basename at installation time.
    pub install_as: String,
}

/// Authenticode signing status.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum SignatureInfo {
    /// Signed DLL.
    Signed {
        /// Optional certificate subject.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subject: Option<String>,
        /// Optional certificate thumbprint.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thumbprint: Option<String>,
        /// Timestamp indicating when the artifact was signed.
        signed_at: Option<String>,
    },
    /// Unsigned DLL.
    Unsigned,
}

/// Local download state of a package.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryPackageState {
    /// Stable package identifier.
    pub package_id: String,
    /// User-facing package version.
    pub version: String,
    /// Whether the complete package is locally materialized and verified.
    pub is_downloaded: bool,
    /// Registered domain artifact id.
    pub artifact_id: Option<String>,
}

/// UI-facing package projection with all member resolution performed by the
/// orchestration layer.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryPackageSummary {
    /// Stable package identifier.
    pub package_id: String,
    /// Domain artifact identity derived from the package revision.
    pub artifact_id: String,
    /// Stable vendor identifier.
    pub vendor: String,
    /// Graphics technology slug.
    pub technology: String,
    /// Package variant within the technology.
    pub variant: String,
    /// User-facing package name.
    pub display_name: String,
    /// Release metadata.
    pub release: LibraryRelease,
    /// Runtime target metadata.
    pub target: LibraryTarget,
    /// Canonical package revision digest.
    pub revision_sha256: String,
    /// Primary member installation name.
    pub primary_file_name: String,
    /// Primary member's uncompressed SHA-256 digest.
    pub primary_sha256: String,
    /// Primary member signature.
    pub primary_signature: SignatureInfo,
    /// Sum of all member DLL sizes.
    pub size_bytes: u64,
    /// Whether the verified package is materialized locally.
    pub is_downloaded: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LibraryIndex {
    pub(super) schema_version: u32,
    pub(super) generated_at: String,
    pub(super) vendors: Vec<LibraryVendorReference>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LibraryVendorReference {
    pub(super) vendor_id: String,
    pub(super) display_name: String,
    pub(super) snapshot_key: String,
    pub(super) snapshot_sha256: String,
    pub(super) snapshot_size_bytes: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LibraryVendorSnapshot {
    pub(super) schema_version: u32,
    pub(super) vendor: LibraryVendor,
    pub(super) generated_at: String,
    pub(super) artifacts: Vec<LibraryArtifactRecord>,
    pub(super) packages: Vec<LibraryPackage>,
}
