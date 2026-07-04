use serde::{Deserialize, Serialize};

use crate::PathRef;

/// Shared artifact kind tracked outside any single game install.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SharedArtifactKind {
    /// The global ReShade Vulkan implicit layer used by RenoDX Vulkan games.
    RenoDxVulkanLayer,
}

/// Audit/provenance classification for a shared artifact record.
///
/// This must never be the sole source of truth for lifecycle decisions. Shared
/// resources are reconciled from filesystem/registry facts first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SharedArtifactOrigin {
    /// RenderPilot created or fully replaced the artifact.
    RenderPilotCreated,
    /// RenderPilot adopted an official-compatible artifact already on disk.
    AdoptedOfficial,
    /// The artifact was discovered without enough provenance to classify it.
    Unknown,
}

/// Advisory provenance record for a shared artifact.
///
/// The row is deliberately optional from a behavior standpoint: callers must be
/// able to reconstruct facts from disk/registry if this record is missing or
/// stale. Optional source fields allow adopting an already-installed official
/// artifact before RenderPilot has refreshed it from a known upstream source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedArtifactRecord {
    kind: SharedArtifactKind,
    install_dir: PathRef,
    manifest_path: PathRef,
    dll_path: PathRef,
    source_url: Option<String>,
    source_etag: Option<String>,
    source_digest: Option<String>,
    source_last_modified: Option<String>,
    channel: Option<String>,
    origin: SharedArtifactOrigin,
    created_files: Vec<PathRef>,
    #[serde(default)]
    installed_at: Option<i64>,
    #[serde(default)]
    updated_at: Option<i64>,
}

impl SharedArtifactRecord {
    /// Creates a shared artifact provenance record.
    #[must_use]
    pub fn new(
        kind: SharedArtifactKind,
        install_dir: PathRef,
        manifest_path: PathRef,
        dll_path: PathRef,
        origin: SharedArtifactOrigin,
    ) -> Self {
        Self {
            kind,
            install_dir,
            manifest_path,
            dll_path,
            source_url: None,
            source_etag: None,
            source_digest: None,
            source_last_modified: None,
            channel: None,
            origin,
            created_files: Vec::new(),
            installed_at: None,
            updated_at: None,
        }
    }

    /// Reconstructs a record from all persisted fields.
    #[must_use]
    pub fn from_parts(
        kind: SharedArtifactKind,
        install_dir: PathRef,
        manifest_path: PathRef,
        dll_path: PathRef,
        source: SharedArtifactSource,
        origin: SharedArtifactOrigin,
        created_files: Vec<PathRef>,
    ) -> Self {
        Self {
            kind,
            install_dir,
            manifest_path,
            dll_path,
            source_url: source.url,
            source_etag: source.etag,
            source_digest: source.digest,
            source_last_modified: source.last_modified,
            channel: source.channel,
            origin,
            created_files,
            installed_at: None,
            updated_at: None,
        }
    }

    /// Sets source identity/provenance fields.
    #[must_use]
    pub fn with_source(mut self, source: SharedArtifactSource) -> Self {
        self.source_url = source.url;
        self.source_etag = source.etag;
        self.source_digest = source.digest;
        self.source_last_modified = source.last_modified;
        self.channel = source.channel;
        self
    }

    /// Sets files RenderPilot created or replaced for this shared artifact.
    #[must_use]
    pub fn with_created_files(mut self, created_files: Vec<PathRef>) -> Self {
        self.created_files = created_files;
        self
    }

    /// Sets persisted timestamps.
    #[must_use]
    pub fn with_timestamps(mut self, installed_at: Option<i64>, updated_at: Option<i64>) -> Self {
        self.installed_at = installed_at;
        self.updated_at = updated_at;
        self
    }

    /// Returns the shared artifact kind.
    #[must_use]
    pub fn kind(&self) -> SharedArtifactKind {
        self.kind
    }

    /// Returns the install directory.
    #[must_use]
    pub fn install_dir(&self) -> &PathRef {
        &self.install_dir
    }

    /// Returns the manifest path.
    #[must_use]
    pub fn manifest_path(&self) -> &PathRef {
        &self.manifest_path
    }

    /// Returns the layer DLL path.
    #[must_use]
    pub fn dll_path(&self) -> &PathRef {
        &self.dll_path
    }

    /// Returns the source URL, if known.
    #[must_use]
    pub fn source_url(&self) -> Option<&str> {
        self.source_url.as_deref()
    }

    /// Returns the HTTP cache validator, if known.
    #[must_use]
    pub fn source_etag(&self) -> Option<&str> {
        self.source_etag.as_deref()
    }

    /// Returns the source digest, if known.
    #[must_use]
    pub fn source_digest(&self) -> Option<&str> {
        self.source_digest.as_deref()
    }

    /// Returns the source last-modified date, if known.
    #[must_use]
    pub fn source_last_modified(&self) -> Option<&str> {
        self.source_last_modified.as_deref()
    }

    /// Returns the source channel, if known.
    #[must_use]
    pub fn channel(&self) -> Option<&str> {
        self.channel.as_deref()
    }

    /// Returns the advisory origin.
    #[must_use]
    pub fn origin(&self) -> SharedArtifactOrigin {
        self.origin
    }

    /// Returns files RenderPilot created or replaced for this shared artifact.
    #[must_use]
    pub fn created_files(&self) -> &[PathRef] {
        &self.created_files
    }

    /// Returns the persisted creation timestamp.
    #[must_use]
    pub fn installed_at(&self) -> Option<i64> {
        self.installed_at
    }

    /// Returns the persisted update timestamp.
    #[must_use]
    pub fn updated_at(&self) -> Option<i64> {
        self.updated_at
    }
}

/// Optional source identity for a shared artifact.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SharedArtifactSource {
    /// Upstream URL, if RenderPilot knows it.
    pub url: Option<String>,
    /// HTTP cache validator, if known.
    pub etag: Option<String>,
    /// SHA-256 digest of the artifact bytes, if known.
    pub digest: Option<String>,
    /// Raw upstream Last-Modified value, if known.
    pub last_modified: Option<String>,
    /// Tool-owned source channel/provenance, if known.
    pub channel: Option<String>,
}

impl SharedArtifactSource {
    /// Creates a source record from known upstream download identity.
    #[must_use]
    pub fn known(
        url: impl Into<String>,
        etag: Option<String>,
        digest: impl Into<String>,
        last_modified: Option<String>,
        channel: impl Into<String>,
    ) -> Self {
        Self {
            url: Some(url.into()),
            etag,
            digest: Some(digest.into()),
            last_modified,
            channel: Some(channel.into()),
        }
    }
}
