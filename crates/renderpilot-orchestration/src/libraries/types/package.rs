use std::collections::BTreeMap;

use renderpilot_domain::{
    Architecture, CatalogPackageAvailability, CatalogSignatureReceipt, PackageRelease,
    PackageVersion, PeExportSet, ReleaseChannel, RuntimeCompatibility,
};
use serde::{Deserialize, Deserializer, Serialize};

use super::LibraryLegalDocumentLink;

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
    pub file_version: Option<String>,
    /// Sorted, unique named PE exports when the package contract relies on them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pe_named_exports: Option<PeExportSet>,
    /// Strict regular and delay-load imports when the package contract relies
    /// on them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pe_imports: Option<renderpilot_domain::PeImportProfile>,
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
    /// Applicable legal documents resolved within the vendor snapshot.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub legal_document_ids: Vec<String>,
    /// Ordered package members; the primary member must be first.
    pub members: Vec<LibraryPackageMember>,
    /// Forward-compatible vendor metadata ignored by core behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<serde_json::Map<String, serde_json::Value>>,
}

/// User-facing package release metadata.
///
/// `revision_version` is retained only for authenticating legacy catalog
/// revisions whose producer emitted an equivalent four-segment version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryRelease {
    /// Canonical package version used by application code and UI.
    pub version: PackageVersion,
    /// Release stability channel.
    pub channel: ReleaseChannel,
    /// Optional supplemental annotation.
    pub label: Option<String>,
    /// Composite component versions in deterministic key order.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub components: BTreeMap<String, PackageVersion>,
    #[serde(skip)]
    pub(crate) revision_version: String,
}

impl LibraryRelease {
    /// Creates a release with canonical revision spelling.
    pub fn new(version: PackageVersion, channel: ReleaseChannel, label: Option<String>) -> Self {
        Self {
            revision_version: version.as_str().to_owned(),
            version,
            channel,
            label,
            components: BTreeMap::new(),
        }
    }

    /// Returns the exact version spelling used by the catalog revision.
    pub(crate) fn revision_version(&self) -> &str {
        &self.revision_version
    }

    /// Converts the orchestration release to its domain presentation value.
    pub(crate) fn to_package_release(&self) -> PackageRelease {
        PackageRelease {
            version: self.version.clone(),
            channel: self.channel,
            label: self.label.clone(),
            components: self.components.clone(),
        }
    }
}

impl<'de> Deserialize<'de> for LibraryRelease {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireRelease {
            version: String,
            channel: ReleaseChannel,
            label: Option<String>,
            #[serde(default)]
            components: BTreeMap<String, PackageVersion>,
        }

        let wire = WireRelease::deserialize(deserializer)?;
        let version = PackageVersion::parse(&wire.version).map_err(serde::de::Error::custom)?;
        if wire.version != version.as_str() && !is_supported_legacy_spelling(&wire.version) {
            return Err(serde::de::Error::custom(
                "catalog package version must use its canonical spelling",
            ));
        }
        Ok(Self {
            version,
            channel: wire.channel,
            label: wire.label,
            components: wire.components,
            revision_version: wire.version,
        })
    }
}

impl From<PackageRelease> for LibraryRelease {
    fn from(release: PackageRelease) -> Self {
        let mut value = Self::new(release.version, release.channel, release.label);
        value.components = release.components;
        value
    }
}

fn is_supported_legacy_spelling(value: &str) -> bool {
    let segments = value.split('.').collect::<Vec<_>>();
    let legacy_shape =
        (segments.len() < 3) || (segments.len() == 4 && segments.last() == Some(&"0"));
    legacy_shape
        && segments.iter().all(|segment| {
            !segment.is_empty()
                && segment.bytes().all(|byte| byte.is_ascii_digit())
                && (segment == &"0" || !segment.starts_with('0'))
        })
}

/// Release stability channel.
pub type LibraryReleaseChannel = ReleaseChannel;

/// Windows runtime target for a package.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryTarget {
    /// Operating system identifier (v1 supports `windows`).
    pub os: String,
    /// Architecture interpreted by the package's technology policy.
    pub architecture: Architecture,
    /// Optional technology-specific runtime compatibility requirement.
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
        version: PackageVersion,
        /// Base64-encoded SHA-512 supplied by NuGet registration metadata.
        package_sha512: String,
    },
    /// GitHub release identity pinned to an immutable commit.
    GithubRelease {
        /// GitHub repository in `owner/name` form.
        repository: String,
        /// Exact release tag.
        tag: String,
        /// Lowercase immutable Git commit SHA.
        commit_sha: String,
    },
    /// Reproducible build from immutable source archives.
    SourceBuild {
        /// Named source inputs in deterministic key order.
        sources: BTreeMap<String, LibrarySourceInput>,
        /// Monotonic recipe rebuild revision.
        build_revision: u32,
        /// Complete build-recipe digest.
        recipe_sha256: String,
        /// Binary verification-policy digest.
        verification_policy_sha256: String,
        /// Applied source transformations keyed by stable patch id.
        patches: BTreeMap<String, LibrarySourcePatch>,
        /// Exact nested toolchain identity.
        toolchain: LibrarySourceBuildToolchain,
    },
}

/// Exact toolchain used by a reproducible source build.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LibrarySourceBuildToolchain {
    /// Exact runner image.
    pub runner_image: String,
    /// Exact compiler identity.
    pub compiler: String,
    /// Exact linker identity.
    pub linker: String,
    /// Exact Windows SDK.
    pub windows_sdk: String,
    /// Exact CMake.
    pub cmake: String,
}

/// Content-addressed transformation of one temporary source-tree file.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LibrarySourcePatch {
    /// Source input owning the target file.
    pub source: String,
    /// Normalized source-relative target path.
    pub target: String,
    /// Declarative patch descriptor SHA-256.
    pub descriptor_sha256: String,
    /// Pristine target SHA-256.
    pub original_sha256: String,
    /// Transformed target SHA-256.
    pub patched_sha256: String,
}

/// Immutable source input for a reproducible package build.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LibrarySourceInput {
    /// Repository in `owner/name` form.
    pub repository: String,
    /// Exact upstream release version, without catalog normalization.
    pub version: String,
    /// Exact source tag.
    pub tag: Option<String>,
    /// Immutable tag object SHA.
    pub tag_object_sha: Option<String>,
    /// Immutable peeled commit SHA.
    pub commit_sha: Option<String>,
    /// Canonical archive URL.
    pub archive_url: String,
    /// Verified archive SHA-256.
    pub archive_sha256: String,
}

/// One physical artifact installed as part of a package.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryPackageMember {
    /// Referenced physical artifact identifier.
    pub artifact_id: String,
    /// Stable semantic component name for composite V2 packages.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub component: String,
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

impl SignatureInfo {
    pub(crate) fn to_receipt(&self) -> CatalogSignatureReceipt {
        match self {
            Self::Signed {
                subject,
                thumbprint,
                signed_at,
            } => CatalogSignatureReceipt::Signed {
                subject: subject.clone(),
                thumbprint: thumbprint.clone(),
                signed_at: signed_at.clone(),
            },
            Self::Unsigned => CatalogSignatureReceipt::Unsigned,
        }
    }

    pub(crate) fn from_receipt(receipt: &CatalogSignatureReceipt) -> Self {
        match receipt {
            CatalogSignatureReceipt::Signed {
                subject,
                thumbprint,
                signed_at,
            } => Self::Signed {
                subject: subject.clone(),
                thumbprint: thumbprint.clone(),
                signed_at: signed_at.clone(),
            },
            CatalogSignatureReceipt::Unsigned => Self::Unsigned,
        }
    }
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
    /// Primary member installation name.
    pub primary_file_name: String,
    /// Primary member's uncompressed SHA-256 digest.
    pub primary_sha256: String,
    /// Primary member signature.
    pub primary_signature: SignatureInfo,
    /// Applicable legal documents with validated public links.
    pub legal_documents: Vec<LibraryLegalDocumentLink>,
    /// Sum of all member DLL sizes.
    pub size_bytes: u64,
    /// Whether the package remains present in the active remote catalog.
    pub availability: LibraryPackageAvailability,
    /// Integrity state of local package content, independent of availability.
    pub local_state: LibraryLocalState,
    /// Backend capability used by unattended selection; never persisted.
    pub automatic_selection_allowed: bool,
}

/// Availability of a package shown in the Libraries page.
pub type LibraryPackageAvailability = CatalogPackageAvailability;

/// Integrity state of a locally registered package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryLocalState {
    /// No local receipt is registered.
    Absent,
    /// Every registered member exists and matches its digest.
    Verified,
    /// At least one registered member is absent.
    Missing,
    /// A member exists but cannot be read or does not match its digest.
    Corrupt,
}

/// Whether a complete active catalog was available for a Libraries query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryCatalogStatus {
    /// A validated active or last-known-good catalog was projected.
    Active,
    /// Only receipt-backed local registrations could be returned.
    LocalFallback,
}

/// Envelope that prevents a receipt-only fallback from masquerading as a full catalog.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryPackagesOutput {
    /// Reconciled package rows.
    pub packages: Vec<LibraryPackageSummary>,
    /// Completeness of the catalog portion of this response.
    pub catalog_status: LibraryCatalogStatus,
}

/// Atomic row replacement returned by Libraries mutations.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryPackageMutation {
    /// Logical package identity affected by the mutation.
    pub package_id: String,
    /// Current row, or `None` when no active or local registration remains.
    pub package: Option<LibraryPackageSummary>,
}
