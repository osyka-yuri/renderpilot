//! Stable catalog-package identities and download receipts.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{Architecture, ArtifactId, PackageVersion, RuntimeCompatibility, Sha256Hash};

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
}

/// V1 closed set of legal-document roles preserved in a local receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogLegalDocumentKind {
    /// Primary license terms for the package.
    License,
    /// Attribution or supplemental third-party terms.
    Notice,
}

impl CatalogLegalDocumentKind {
    /// Returns the stable wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::License => "license",
            Self::Notice => "notice",
        }
    }
}

/// V1 closed set of legal-document formats preserved in a local receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogLegalDocumentFormat {
    /// UTF-8-compatible text or Markdown.
    Text,
    /// Portable Document Format.
    Pdf,
}

impl CatalogLegalDocumentFormat {
    /// Returns the stable wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Pdf => "pdf",
        }
    }
}

/// Wire-schema marker for [`CatalogPackageReceiptV1`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CatalogReceiptSchemaV1;

impl Serialize for CatalogReceiptSchemaV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(1)
    }
}

impl<'de> Deserialize<'de> for CatalogReceiptSchemaV1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let version = u32::deserialize(deserializer)?;
        if version != 1 {
            return Err(serde::de::Error::custom(format!(
                "unsupported catalog package receipt schema {version}"
            )));
        }
        Ok(Self)
    }
}

/// V1 receipt persisted with a downloaded catalog artifact.
///
/// This snapshot is deliberately transport-free: it preserves enough immutable
/// identity and presentation data to manage a local package after its upstream
/// catalog entry is withdrawn, without retaining a stale download address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogPackageReceiptV1 {
    /// Constant V1 wire-schema marker.
    pub schema_version: CatalogReceiptSchemaV1,
    /// Stable catalog package identifier.
    pub package_id: String,
    /// Catalog vendor identifier.
    pub vendor: String,
    /// Graphics technology slug.
    pub technology: String,
    /// Package variant.
    pub variant: String,
    /// User-facing package name.
    pub display_name: String,
    /// Exact package release identity.
    pub release: PackageRelease,
    /// Runtime target snapshot.
    pub target: CatalogTargetReceipt,
    /// Optional immutable upstream provenance.
    pub provenance: Option<CatalogProvenanceReceipt>,
    /// Canonical package revision digest.
    pub revision_sha256: Sha256Hash,
    /// Primary member installation name.
    pub primary_file_name: String,
    /// Primary member DLL digest.
    pub primary_sha256: Sha256Hash,
    /// Primary member signature snapshot.
    pub primary_signature: CatalogSignatureReceipt,
    /// Legal-document links applicable at download time.
    pub legal_documents: Vec<CatalogLegalDocumentReceipt>,
    /// Total uncompressed package size.
    pub size_bytes: u64,
}

impl CatalogPackageReceiptV1 {
    /// Returns the only valid local artifact identity for this package revision.
    #[must_use]
    pub fn artifact_id(&self) -> ArtifactId {
        ArtifactId::for_package_revision(&self.revision_sha256)
    }
}

/// Runtime target persisted in a catalog package receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogTargetReceipt {
    /// Target operating system.
    pub os: String,
    /// Target architecture.
    pub architecture: Architecture,
    /// Optional runtime compatibility requirement.
    pub compatibility: Option<RuntimeCompatibility>,
}

/// Immutable upstream provenance persisted in a catalog package receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CatalogProvenanceReceipt {
    /// NuGet package identity and registry digest.
    Nuget {
        /// NuGet package identifier.
        package_id: String,
        /// Exact NuGet package version.
        version: PackageVersion,
        /// Base64 package digest supplied by NuGet.
        package_sha512: String,
    },
    /// GitHub release identity.
    GithubRelease {
        /// Repository in `owner/name` form.
        repository: String,
        /// Exact release tag.
        tag: String,
        /// Immutable Git commit SHA.
        commit_sha: String,
    },
}

/// Authenticode information persisted in a catalog package receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum CatalogSignatureReceipt {
    /// Signed DLL.
    Signed {
        /// Optional certificate subject.
        subject: Option<String>,
        /// Optional certificate thumbprint.
        thumbprint: Option<String>,
        /// Optional signing timestamp.
        signed_at: Option<String>,
    },
    /// Unsigned DLL.
    Unsigned,
}

/// Legal-document link persisted in a catalog package receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogLegalDocumentReceipt {
    /// Stable document identifier.
    pub legal_document_id: String,
    /// Document kind.
    pub kind: CatalogLegalDocumentKind,
    /// User-facing title.
    pub title: String,
    /// Document format.
    pub format: CatalogLegalDocumentFormat,
    /// Original safe filename.
    pub file_name: String,
    /// Validated public content URL.
    pub content_url: String,
}
