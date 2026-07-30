//! Immutable package provenance, signatures, targets, and legal documents.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Architecture, PackageVersion, RuntimeCompatibility, Sha256Hash};

/// Closed set of legal-document roles preserved in a local receipt.
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

/// Closed set of legal-document formats preserved in a local receipt.
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

/// One immutable upstream source used by a reproducible source build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogSourceReceipt {
    /// Source repository in `owner/name` form.
    pub repository: String,
    /// Exact upstream release version used in archive and package identities.
    pub version: String,
    /// Exact annotated or lightweight tag.
    pub tag: Option<String>,
    /// Immutable tag-object SHA (equal to `commit_sha` for lightweight tags).
    pub tag_object_sha: Option<String>,
    /// Immutable peeled commit SHA.
    pub commit_sha: Option<String>,
    /// Canonical source archive URL.
    pub archive_url: String,
    /// Verified source archive digest.
    pub archive_sha256: Sha256Hash,
}

/// One deterministic source transformation used by a reproducible build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogSourcePatchReceipt {
    /// Source input whose temporary tree was transformed.
    pub source: String,
    /// Normalized path inside the source tree.
    pub target: String,
    /// Digest of the declarative patch descriptor.
    pub descriptor_sha256: Sha256Hash,
    /// Digest of the pristine target file.
    pub original_sha256: Sha256Hash,
    /// Digest of the transformed target file.
    pub patched_sha256: Sha256Hash,
}

/// Exact toolchain used by a reproducible source build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogSourceBuildToolchainReceipt {
    /// Exact GitHub-hosted or self-hosted runner image.
    pub runner_image: String,
    /// Exact compiler identity and version.
    pub compiler: String,
    /// Exact linker identity and version.
    pub linker: String,
    /// Exact Windows SDK version.
    pub windows_sdk: String,
    /// Exact CMake version.
    pub cmake: String,
}

/// Immutable provenance recorded by a composite receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CatalogPackageProvenanceReceipt {
    /// NuGet package identity and registry digest.
    Nuget {
        /// NuGet package identifier.
        package_id: String,
        /// Exact NuGet package version.
        version: PackageVersion,
        /// Base64 package digest supplied by NuGet.
        package_sha512: String,
    },
    /// GitHub release identity pinned to an immutable commit.
    GithubRelease {
        /// Repository in `owner/name` form.
        repository: String,
        /// Exact release tag.
        tag: String,
        /// Immutable Git commit SHA.
        commit_sha: String,
    },
    /// Reproducible build from immutable source archives.
    SourceBuild {
        /// Named source inputs in deterministic key order.
        sources: BTreeMap<String, CatalogSourceReceipt>,
        /// Monotonic revision for the same upstream source tuple.
        build_revision: u32,
        /// Digest of the complete build recipe.
        recipe_sha256: Sha256Hash,
        /// Digest of the binary verification policy.
        verification_policy_sha256: Sha256Hash,
        /// Applied compatibility/test patches keyed by stable patch id.
        patches: BTreeMap<String, CatalogSourcePatchReceipt>,
        /// Exact nested toolchain identity.
        toolchain: CatalogSourceBuildToolchainReceipt,
    },
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

/// Immutable upstream provenance persisted in a V1 receipt.
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
