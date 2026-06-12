//! Data types for the graphics-library download subsystem.

use serde::{Deserialize, Serialize};

/// Manifest describing all graphics libraries available for download.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LibraryManifest {
    /// Manifest schema version used to interpret the manifest payload.
    pub schema_version: u32,
    /// Timestamp indicating when the manifest was generated.
    pub generated_at: String,
    /// Downloadable library entries listed in the manifest.
    pub entries: Vec<LibraryManifestEntry>,
}

/// Single downloadable graphics library entry from the manifest.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LibraryManifestEntry {
    /// Stable unique identifier of this manifest entry.
    pub entry_id: String,
    /// Library metadata shared by all builds and versions of this library.
    pub library: LibraryInfo,
    /// Version metadata for this entry.
    pub version: VersionInfo,
    /// Build metadata for this entry.
    pub build: BuildInfo,
    /// File metadata for downloadable and extracted artifacts.
    pub files: FilesInfo,
    /// Signing status metadata for this entry.
    pub signature: SignatureInfo,
}

impl LibraryManifestEntry {
    /// Local cache filename for the compressed archive.
    ///
    /// Version is omitted from the name because each `entry_id` in the
    /// manifest maps to exactly one version.  If the manifest is updated
    /// with a new size for the same entry the old cached file is detected
    /// as stale and replaced.
    pub(crate) fn archive_file_name(&self) -> String {
        format!(
            "{}.dll.zst",
            super::storage::sanitize_path_component(&self.entry_id)
        )
    }
}

/// Metadata that identifies a graphics library artifact.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LibraryInfo {
    /// Library identifier from the upstream manifest.
    pub id: String,
    /// Expected file name of the library artifact.
    pub file_name: String,
}

/// Human-readable and sortable version metadata.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VersionInfo {
    /// Human-readable version value.
    pub value: String,
    /// Sort key used by the UI to order versions deterministically.
    pub sort_key: String,
}

/// Build metadata for a library entry.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BuildInfo {
    /// Build type value from the manifest.
    #[serde(rename = "type")]
    pub build_type: String,
    /// Optional user-facing build label.
    pub label: Option<String>,
}

/// File metadata for a library entry.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FilesInfo {
    /// Metadata for the DLL file inside the downloaded archive.
    pub dll: DllFileInfo,
    /// Metadata for the downloadable zstd-compressed archive.
    pub zst: ZstFileInfo,
}

/// Metadata for a DLL file contained in a library archive.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DllFileInfo {
    /// Expected DLL size in bytes.
    pub size_bytes: u64,
    /// Hash metadata for the DLL file.
    pub hashes: HashesInfo,
}

/// Hash metadata for a library artifact.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HashesInfo {
    /// SHA-256 checksum published by the manifest.
    ///
    /// Normalized to lowercase at parse time so it can be compared with
    /// the lowercase output of `hex::encode` using plain `==`.
    #[serde(deserialize_with = "deserialize_lowercase")]
    pub sha256: String,
}

fn deserialize_lowercase<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    String::deserialize(d).map(|s| s.to_ascii_lowercase())
}

/// Metadata for a downloadable zstd-compressed DLL archive.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ZstFileInfo {
    /// Expected compressed archive size in bytes.
    pub size_bytes: u64,
    /// URL used to download the compressed archive.
    pub download_url: String,
}

/// Signing status metadata for a library entry.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "status")]
pub enum SignatureInfo {
    /// Entry whose DLL artifact is signed.
    #[serde(rename = "signed")]
    Signed {
        /// Timestamp indicating when the artifact was signed.
        signed_at: String,
    },
    /// Entry whose DLL artifact is not signed.
    #[serde(rename = "unsigned")]
    Unsigned,
}

/// Local download state of a library entry.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryState {
    /// Stable unique identifier of the manifest entry.
    pub id: String,
    /// Human-readable library version.
    pub version: String,
    /// Whether the library archive exists in local storage.
    pub is_downloaded: bool,
    /// Absolute local archive path when the library is downloaded.
    pub local_path: Option<String>,
    /// Registered artifact id when the library has been materialized.
    pub artifact_id: Option<String>,
}
