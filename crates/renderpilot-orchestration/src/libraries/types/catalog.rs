use serde::{Deserialize, Serialize};

use super::{LibraryArtifactRecord, LibraryLegalDocument, LibraryPackage};

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
    /// Deduplicated legal documents referenced by packages.
    #[serde(default)]
    pub legal_documents: Vec<LibraryLegalDocument>,
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LibraryIndex {
    pub(crate) schema_version: u32,
    pub(crate) generated_at: String,
    pub(crate) vendors: Vec<LibraryVendorReference>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LibraryVendorReference {
    pub(crate) vendor_id: String,
    pub(crate) display_name: String,
    pub(crate) snapshot_key: String,
    pub(crate) snapshot_sha256: String,
    pub(crate) snapshot_size_bytes: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LibraryVendorSnapshot {
    pub(crate) schema_version: u32,
    pub(crate) vendor: LibraryVendor,
    pub(crate) generated_at: String,
    #[serde(default)]
    pub(crate) legal_documents: Vec<LibraryLegalDocument>,
    pub(crate) artifacts: Vec<LibraryArtifactRecord>,
    pub(crate) packages: Vec<LibraryPackage>,
}
