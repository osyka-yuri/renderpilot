//! Domain types for injected game add-ons (initially RenoDX).
//!
//! Unlike a [`crate::GraphicsComponent`], which models a vendor library that
//! already exists in a game folder and is swapped between versions, an add-on is
//! a third-party runtime that RenderPilot *introduces* into a game (the RenoDX
//! ReShade add-on, plus the ReShade host when absent). These types describe what
//! a game's executable renders with and what an installed add-on left behind, so
//! an install can be fully and safely reversed.

use serde::{Deserialize, Serialize};

use crate::{AddonKind, Architecture, GameId, GraphicsApi, PathRef};

/// Graphics APIs and architecture inferred from a game executable.
///
/// Produced by the detection layer from the executable's PE import table: the
/// `apis` set lists every graphics API the binary imports (a detection fact,
/// with no product-specific ranking applied), and `architecture` is the CPU
/// bitness read from the COFF machine type. The orchestration layer applies
/// any tool-specific policy (e.g. "pick the most capable DirectX API for
/// RenoDX") on top of these facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExeGraphicsInfo {
    /// The set of graphics APIs the executable imports, deduplicated and without
    /// ranking. Empty when no known graphics import was found.
    apis: Vec<GraphicsApi>,
    architecture: Option<Architecture>,
    /// The actual graphics DLL basenames the executable imports (lowercased, e.g.
    /// `dxgi.dll`, `d3d12.dll`, `d3d9.dll`), in first-seen order. Unlike `apis`
    /// (which collapses `dxgi.dll` into `D3D11`), this preserves the exact DLL so
    /// the orchestration layer can pick the precise ReShade proxy the game loads
    /// instead of guessing. Empty when no known graphics import was found.
    #[serde(default)]
    graphics_dlls: Vec<String>,
}

impl ExeGraphicsInfo {
    /// Creates a new graphics-info record from the imported API set and
    /// architecture. The imported-DLL list is empty; use
    /// [`Self::with_graphics_dlls`] to attach it.
    #[must_use]
    pub fn new(apis: Vec<GraphicsApi>, architecture: Option<Architecture>) -> Self {
        Self {
            apis,
            architecture,
            graphics_dlls: Vec::new(),
        }
    }

    /// Attaches the exact imported graphics DLL basenames (lowercased).
    #[must_use]
    pub fn with_graphics_dlls(mut self, graphics_dlls: Vec<String>) -> Self {
        self.graphics_dlls = graphics_dlls;
        self
    }

    /// Returns the detected graphics API set, without ranking.
    #[must_use]
    pub fn apis(&self) -> &[GraphicsApi] {
        &self.apis
    }

    /// Returns the detected architecture, if it could be determined.
    #[must_use]
    pub const fn architecture(&self) -> Option<Architecture> {
        self.architecture
    }

    /// Returns the exact imported graphics DLL basenames (lowercased), in
    /// first-seen order. Empty when none were found or detection was inconclusive.
    #[must_use]
    pub fn graphics_dlls(&self) -> &[String] {
        &self.graphics_dlls
    }
}

/// Current RenoDX installation state for a game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RenoDxInstallState {
    /// RenoDX is not installed for the game.
    NotInstalled,
    /// RenoDX is installed.
    Installed {
        /// Host mechanism used by this install, mapped to a UI-facing stable
        /// vocabulary. `None` for legacy records that predate host metadata.
        #[serde(default)]
        host_kind: Option<RenoDxHostKind>,
        /// Installed add-on version label, when known (free-form, e.g.
        /// `snapshot-2026.06`). RenoDX add-ons are rolling snapshots with no version
        /// number, so this is effectively always `null`; the UI uses `addon_dated`
        /// as the concrete anchor instead.
        version: Option<String>,
        /// The add-on's upstream `Last-Modified` HTTP-date string (its publish-date
        /// proxy), when the host sent one — the UI's "Add-on dated …" anchor.
        #[serde(default)]
        addon_dated: Option<String>,
        /// When the add-on was first installed (Unix epoch milliseconds), when known.
        #[serde(default)]
        installed_at: Option<i64>,
        /// When the install record was last updated (Unix epoch milliseconds), when
        /// known — bumped by an add-on/host/DLSS-Fix update.
        #[serde(default)]
        updated_at: Option<i64>,
        /// Whether the install includes the DLSS-Fix companion add-on. Surfaced
        /// directly on the state so the UI does not have to infer it from the
        /// update report (which is `null` while the update probe is in flight or
        /// after a network failure).
        #[serde(default)]
        dlss_fix_installed: bool,
        /// Whether the add-on has a tracked upstream source (a normal install).
        /// `false` for a user-file install, which records no upstream URL.
        /// Surfaced directly on the state for the same reason as
        /// `dlss_fix_installed`, so the "installed from a file" hint stays correct
        /// while the update probe is in flight or after it fails (the report's
        /// `addon` is `null` in those cases too).
        #[serde(default)]
        addon_tracked: bool,
    },
}

/// UI-facing host mechanism used by an installed RenoDX add-on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenoDxHostKind {
    /// A per-game ReShade proxy DLL.
    Proxy,
    /// The shared ReShade Vulkan implicit layer.
    Vulkan,
}

impl From<InstalledAddonHostKind> for RenoDxHostKind {
    fn from(value: InstalledAddonHostKind) -> Self {
        match value {
            InstalledAddonHostKind::Proxy => Self::Proxy,
            InstalledAddonHostKind::SharedVulkanLayer => Self::Vulkan,
        }
    }
}

impl RenoDxInstallState {
    /// Returns whether this state is `Installed` **and** includes the DLSS-Fix
    /// companion add-on. A thin pattern-match helper so callers need not repeat
    /// the `match`/`if let` boilerplate.
    #[must_use]
    pub fn is_dlss_fix_installed(&self) -> bool {
        matches!(
            self,
            Self::Installed {
                dlss_fix_installed: true,
                ..
            }
        )
    }

    /// Returns whether this state is `Installed` and its add-on payload has a
    /// non-empty upstream source URL.
    #[must_use]
    pub fn is_addon_tracked(&self) -> bool {
        matches!(
            self,
            Self::Installed {
                addon_tracked: true,
                ..
            }
        )
    }
}

/// The role a [`TrackedSource`] plays in an install, so the update system knows
/// what each upstream URL points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackedSourceRole {
    /// The add-on payload itself (e.g. the RenoDX `.addon64`); its file is a member
    /// of [`InstalledAddon::created_files`].
    AddonPayload,
    /// The DLSS-Fix companion add-on (`renodx-dlssfix.addon64`), installed alongside
    /// the main add-on when the game has NVIDIA Frame Generation + DLSS + Streamline.
    DlssFix,
    /// A host binary artifact recorded for update/rollback provenance (e.g. the
    /// ReShade proxy binary).
    #[serde(rename = "host")]
    HostBinary,
}

/// Host mechanism used by an installed add-on.
///
/// This is persisted as per-game metadata so reversible flows do not have to
/// re-derive the host from mutable facts such as the currently selected
/// executable override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstalledAddonHostKind {
    /// A per-game ReShade proxy DLL, such as `dxgi.dll`.
    Proxy,
    /// The shared ReShade Vulkan implicit layer.
    SharedVulkanLayer,
}

/// One upstream source an install tracks for updates: where a placed file came
/// from and the identity needed to tell whether it changed.
///
/// Tool-agnostic by design — an install records one entry per managed download
/// (RenoDX tracks the add-on and, when it installs one, the ReShade host; a future
/// tool tracks whatever it fetches), so the update system is generic over the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackedSource {
    role: TrackedSourceRole,
    /// Upstream URL the file was fetched from.
    url: String,
    /// HTTP cache validator (ETag/Last-Modified) for a cheap update pre-check.
    etag: Option<String>,
    /// SHA-256 of the installed bytes — the durable change-detection digest.
    digest: String,
    /// The raw `Last-Modified` HTTP-date string from the download response, when
    /// the host sent one. Surfaced to the UI as the upstream "dated" anchor (a
    /// rolling-snapshot add-on has no version number). Optional and
    /// `#[serde(default)]` so records persisted before this field deserialize as
    /// `None`.
    #[serde(default)]
    last_modified: Option<String>,
    /// Tool-owned source variant/provenance tag. Generic at the domain layer:
    /// RenoDX uses `stable` / `nightly` for ReShade Host sources.
    #[serde(default)]
    channel: Option<String>,
    /// Whether this source was reconstructed from on-disk facts (e.g. adopting an
    /// install RenderPilot did not create) rather than recorded from an actual
    /// download. An advisory source's URL/digest are a best-effort guess, not
    /// proof of what is on disk. `#[serde(default)]` so records persisted before
    /// this field deserialize as `false`.
    #[serde(default)]
    advisory: bool,
}

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

impl TrackedSource {
    /// Creates a tracked source for a managed download. The upstream `Last-Modified`
    /// date is attached separately via [`with_last_modified`](Self::with_last_modified).
    #[must_use]
    pub fn new(
        role: TrackedSourceRole,
        url: impl Into<String>,
        etag: Option<String>,
        digest: impl Into<String>,
    ) -> Self {
        Self {
            role,
            url: url.into(),
            etag,
            digest: digest.into(),
            last_modified: None,
            channel: None,
            advisory: false,
        }
    }

    /// Attaches the upstream `Last-Modified` HTTP-date string (the file's publish
    /// date proxy), shown by the UI as the source's "dated" anchor.
    #[must_use]
    pub fn with_last_modified(mut self, last_modified: Option<String>) -> Self {
        self.last_modified = last_modified;
        self
    }

    /// Attaches a tool-owned source variant/provenance tag.
    #[must_use]
    pub fn with_channel(mut self, channel: impl Into<String>) -> Self {
        self.channel = Some(channel.into());
        self
    }

    /// Marks this source as reconstructed from on-disk facts rather than a
    /// recorded download.
    #[must_use]
    pub fn with_advisory(mut self) -> Self {
        self.advisory = true;
        self
    }

    /// Returns the role this source plays in the install.
    #[must_use]
    pub fn role(&self) -> TrackedSourceRole {
        self.role
    }

    /// Returns the upstream URL the file was fetched from.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Returns the HTTP cache validator for a cheap update pre-check.
    #[must_use]
    pub fn etag(&self) -> Option<&str> {
        self.etag.as_deref()
    }

    /// Returns the change-detection digest (SHA-256 of the installed bytes).
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// Returns the upstream `Last-Modified` HTTP-date string, when the host sent one.
    #[must_use]
    pub fn last_modified(&self) -> Option<&str> {
        self.last_modified.as_deref()
    }

    /// Returns the tool-owned source variant/provenance tag, when present.
    #[must_use]
    pub fn channel(&self) -> Option<&str> {
        self.channel.as_deref()
    }

    /// Returns whether this source was reconstructed from on-disk facts rather
    /// than recorded from an actual download.
    #[must_use]
    pub fn is_advisory(&self) -> bool {
        self.advisory
    }
}

/// Record of an installed add-on: the source of truth for reversing an install.
///
/// Tracks every file RenderPilot *created* (removed on uninstall) and every
/// pre-existing file it *backed up* before overwriting (restored on uninstall), so
/// a game folder can be returned to its prior state, plus the upstream
/// [`TrackedSource`]s the update system compares against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledAddon {
    game_id: GameId,
    kind: AddonKind,
    addon_file: PathRef,
    addon_version: Option<String>,
    created_files: Vec<PathRef>,
    backed_up_files: Vec<PathRef>,
    /// Upstream artifacts to check for updates — one per fetched file whose
    /// identity is needed by the private update/rollback flow.
    tracked_sources: Vec<TrackedSource>,
    /// When the add-on was first installed (Unix epoch ms). Set from the persisted
    /// `created_at` column when a record is rehydrated; `None` for a freshly built
    /// (not-yet-persisted) record.
    #[serde(default)]
    installed_at: Option<i64>,
    /// When the record was last persisted (Unix epoch ms). Set from the persisted
    /// `updated_at` column on rehydrate; `None` for a freshly built record.
    #[serde(default)]
    updated_at: Option<i64>,
    /// Host mechanism used by this install. Optional for records created before
    /// host metadata existed.
    #[serde(default)]
    host_kind: Option<InstalledAddonHostKind>,
    /// Effective ReShade channel used for the host, when known.
    #[serde(default)]
    reshade_channel: Option<String>,
    /// Executable registered with a shared host, when applicable. Persisted so
    /// uninstall does not depend on the current executable override.
    #[serde(default)]
    registered_exe_path: Option<PathRef>,
}

impl InstalledAddon {
    /// Creates a record for a newly installed add-on.
    ///
    /// `addon_file` is the add-on payload RenderPilot placed in the game folder
    /// (for example `renodx-<game>.addon64`); it is always treated as a created
    /// file, so [`created_files`](Self::created_files) is never empty.
    #[must_use]
    pub fn new(game_id: GameId, kind: AddonKind, addon_file: PathRef) -> Self {
        Self {
            game_id,
            kind,
            created_files: vec![addon_file.clone()],
            addon_file,
            addon_version: None,
            backed_up_files: Vec::new(),
            tracked_sources: Vec::new(),
            installed_at: None,
            updated_at: None,
            host_kind: None,
            reshade_channel: None,
            registered_exe_path: None,
        }
    }

    /// Reconstructs a record from its persisted fields.
    ///
    /// The all-fields counterpart to [`new`](Self::new) plus the builders, used
    /// by the storage layer to rehydrate a record without replaying the install.
    ///
    /// Returns `None` if the persisted data violates the invariant that
    /// `created_files` must contain `addon_file`; such rows are treated as
    /// corrupt by the storage layer.
    #[must_use]
    pub fn from_parts(
        game_id: GameId,
        kind: AddonKind,
        addon_file: PathRef,
        addon_version: Option<String>,
        created_files: Vec<PathRef>,
        backed_up_files: Vec<PathRef>,
        tracked_sources: Vec<TrackedSource>,
    ) -> Option<Self> {
        if !created_files.contains(&addon_file) {
            return None;
        }

        Some(Self {
            game_id,
            kind,
            addon_file,
            addon_version,
            created_files,
            backed_up_files,
            tracked_sources,
            installed_at: None,
            updated_at: None,
            host_kind: None,
            reshade_channel: None,
            registered_exe_path: None,
        })
    }

    /// Sets the installed add-on version label.
    #[must_use]
    pub fn with_addon_version(mut self, version: impl Into<String>) -> Self {
        self.addon_version = Some(version.into());
        self
    }

    /// Attaches the persisted install/update timestamps (Unix epoch ms). Used by the
    /// storage layer when rehydrating a record from its row; a freshly built record
    /// leaves both `None` until it is persisted and read back.
    #[must_use]
    pub fn with_timestamps(mut self, installed_at: Option<i64>, updated_at: Option<i64>) -> Self {
        self.installed_at = installed_at;
        self.updated_at = updated_at;
        self
    }

    /// Attaches host metadata to the install.
    #[must_use]
    pub fn with_host_kind(mut self, host_kind: InstalledAddonHostKind) -> Self {
        self.host_kind = Some(host_kind);
        self
    }

    /// Attaches the effective ReShade channel to the install.
    #[must_use]
    pub fn with_reshade_channel(mut self, channel: impl Into<String>) -> Self {
        self.reshade_channel = Some(channel.into());
        self
    }

    /// Attaches the executable registered with a shared host.
    #[must_use]
    pub fn with_registered_exe_path(mut self, path: PathRef) -> Self {
        self.registered_exe_path = Some(path);
        self
    }

    /// Records an additional file created by the install (removed on uninstall).
    #[must_use]
    pub fn with_created_file(mut self, path: PathRef) -> Self {
        self.created_files.push(path);
        self
    }

    /// Records a pre-existing file backed up before being overwritten (restored
    /// on uninstall).
    #[must_use]
    pub fn with_backed_up_file(mut self, path: PathRef) -> Self {
        self.backed_up_files.push(path);
        self
    }

    /// Records an upstream source to track for updates.
    #[must_use]
    pub fn with_tracked_source(mut self, source: TrackedSource) -> Self {
        self.tracked_sources.push(source);
        self
    }

    /// Replaces the tracked sources wholesale — used when an update refreshes the
    /// recorded digests/validators after re-fetching.
    #[must_use]
    pub fn with_tracked_sources(mut self, sources: Vec<TrackedSource>) -> Self {
        self.tracked_sources = sources;
        self
    }

    /// Returns the owning game identifier.
    #[must_use]
    pub fn game_id(&self) -> &GameId {
        &self.game_id
    }

    /// Returns the add-on kind.
    #[must_use]
    pub fn kind(&self) -> AddonKind {
        self.kind
    }

    /// Returns the add-on payload file placed in the game folder.
    #[must_use]
    pub fn addon_file(&self) -> &PathRef {
        &self.addon_file
    }

    /// Returns the installed add-on version label, if known.
    #[must_use]
    pub fn addon_version(&self) -> Option<&str> {
        self.addon_version.as_deref()
    }

    /// Returns whether this record carries private update provenance for a host
    /// binary.
    #[must_use]
    pub fn has_host_binary_provenance(&self) -> bool {
        self.tracked_sources
            .iter()
            .any(|source| source.role() == TrackedSourceRole::HostBinary)
    }

    /// Returns the files created by the install (removed on uninstall).
    #[must_use]
    pub fn created_files(&self) -> &[PathRef] {
        &self.created_files
    }

    /// Returns the pre-existing files backed up by the install (restored on
    /// uninstall).
    #[must_use]
    pub fn backed_up_files(&self) -> &[PathRef] {
        &self.backed_up_files
    }

    /// Returns the upstream sources the update system tracks for this install.
    #[must_use]
    pub fn tracked_sources(&self) -> &[TrackedSource] {
        &self.tracked_sources
    }

    /// Returns the upstream `Last-Modified` date of the add-on payload source, when
    /// recorded — the UI's "Add-on dated …" anchor.
    #[must_use]
    pub fn addon_dated(&self) -> Option<&str> {
        self.tracked_sources
            .iter()
            .find(|source| source.role() == TrackedSourceRole::AddonPayload)
            .and_then(TrackedSource::last_modified)
    }

    /// Returns the install/update timestamps (Unix epoch ms), when this record was
    /// rehydrated from storage.
    #[must_use]
    pub fn installed_at(&self) -> Option<i64> {
        self.installed_at
    }

    /// Returns when the record was last persisted (Unix epoch ms), when known.
    #[must_use]
    pub fn updated_at(&self) -> Option<i64> {
        self.updated_at
    }

    /// Returns the persisted host kind, if known.
    #[must_use]
    pub fn host_kind(&self) -> Option<InstalledAddonHostKind> {
        self.host_kind
    }

    /// Returns the persisted ReShade channel, if known.
    #[must_use]
    pub fn reshade_channel(&self) -> Option<&str> {
        self.reshade_channel.as_deref()
    }

    /// Returns the executable registered with a shared host, if known.
    #[must_use]
    pub fn registered_exe_path(&self) -> Option<&PathRef> {
        self.registered_exe_path.as_ref()
    }

    /// Returns whether the install includes the DLSS-Fix companion add-on (a
    /// `DlssFix` tracked source is present).
    #[must_use]
    pub fn has_dlss_fix(&self) -> bool {
        self.tracked_sources
            .iter()
            .any(|source| source.role() == TrackedSourceRole::DlssFix)
    }

    /// Returns whether the add-on payload has a tracked upstream source (a normal
    /// upstream install). A user-file install may keep a local-date placeholder
    /// with an empty URL, so this stays `false` and the UI shows the "installed
    /// from a file" note.
    #[must_use]
    pub fn has_addon_source(&self) -> bool {
        self.tracked_sources.iter().any(|source| {
            source.role() == TrackedSourceRole::AddonPayload && !source.url().is_empty()
        })
    }

    /// Returns the current install state described by this record.
    #[must_use]
    pub fn install_state(&self) -> RenoDxInstallState {
        RenoDxInstallState::Installed {
            host_kind: self.host_kind.map(RenoDxHostKind::from),
            version: self.addon_version.clone(),
            addon_dated: self.addon_dated().map(str::to_owned),
            installed_at: self.installed_at,
            updated_at: self.updated_at,
            dlss_fix_installed: self.has_dlss_fix(),
            addon_tracked: self.has_addon_source(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game_id() -> GameId {
        GameId::new("steam:1091500").expect("valid id")
    }

    fn addon_path() -> PathRef {
        PathRef::new(r"C:\Games\CP2077\renodx-cp2077.addon64").expect("valid path")
    }

    #[test]
    fn installed_addon_always_tracks_addon_file_as_created() {
        let installed = InstalledAddon::new(game_id(), AddonKind::RenoDx, addon_path());

        assert_eq!(installed.created_files(), &[addon_path()]);
        assert!(installed.backed_up_files().is_empty());
        assert!(!installed.has_host_binary_provenance());
    }

    #[test]
    fn installed_addon_records_host_binary_artifact_and_files() {
        let proxy = PathRef::new(r"C:\Games\CP2077\dxgi.dll").expect("valid path");
        let ini_backup = PathRef::new(r"C:\Games\CP2077\reshade.ini").expect("valid path");

        let installed = InstalledAddon::new(game_id(), AddonKind::RenoDx, addon_path())
            .with_tracked_source(TrackedSource::new(
                TrackedSourceRole::HostBinary,
                "https://nightly.link/x64.zip",
                None,
                "host-digest",
            ))
            .with_created_file(proxy.clone())
            .with_backed_up_file(ini_backup.clone());

        assert!(installed.has_host_binary_provenance());
        assert_eq!(installed.created_files(), &[addon_path(), proxy]);
        assert_eq!(installed.backed_up_files(), &[ini_backup]);
    }

    #[test]
    fn host_binary_provenance_is_derived_from_a_host_artifact() {
        let base = InstalledAddon::new(game_id(), AddonKind::RenoDx, addon_path())
            .with_tracked_source(TrackedSource::new(
                TrackedSourceRole::AddonPayload,
                "https://example/addon",
                None,
                "addon-digest",
            ));
        assert!(!base.has_host_binary_provenance());

        let with_host = base.with_tracked_source(TrackedSource::new(
            TrackedSourceRole::HostBinary,
            "https://example/host",
            None,
            "host-digest",
        ));
        assert!(with_host.has_host_binary_provenance());
    }

    #[test]
    fn tracked_source_is_not_advisory_by_default() {
        let source = TrackedSource::new(
            TrackedSourceRole::HostBinary,
            "https://example/host",
            None,
            "digest",
        );
        assert!(!source.is_advisory());
    }

    #[test]
    fn tracked_source_with_advisory_round_trips_through_json() {
        let source = TrackedSource::new(
            TrackedSourceRole::HostBinary,
            "https://example/host",
            None,
            "digest",
        )
        .with_advisory();
        assert!(source.is_advisory());

        let json = serde_json::to_string(&source).expect("serializes");
        let round_tripped: TrackedSource = serde_json::from_str(&json).expect("deserializes");
        assert!(round_tripped.is_advisory());
    }

    #[test]
    fn tracked_source_json_without_advisory_field_deserializes_as_false() {
        let legacy_json = r#"{
            "role": "host",
            "url": "https://example/host",
            "etag": null,
            "digest": "digest"
        }"#;
        let source: TrackedSource = serde_json::from_str(legacy_json).expect("deserializes");
        assert!(!source.is_advisory());
    }

    #[test]
    fn from_parts_rejects_rows_that_omit_addon_file() {
        let addon = addon_path();
        let other = PathRef::new(r"C:\Games\CP2077\dxgi.dll").expect("valid path");
        assert!(
            InstalledAddon::from_parts(
                game_id(),
                AddonKind::RenoDx,
                addon.clone(),
                None,
                vec![other],
                Vec::new(),
                Vec::new(),
            )
            .is_none()
        );

        assert!(
            InstalledAddon::from_parts(
                game_id(),
                AddonKind::RenoDx,
                addon.clone(),
                None,
                vec![addon],
                Vec::new(),
                Vec::new(),
            )
            .is_some()
        );
    }

    #[test]
    fn installed_addon_install_state_reflects_fields() {
        let installed = InstalledAddon::new(game_id(), AddonKind::RenoDx, addon_path())
            .with_addon_version("snapshot-2026.06");

        assert_eq!(
            installed.install_state(),
            RenoDxInstallState::Installed {
                host_kind: None,
                version: Some("snapshot-2026.06".to_owned()),
                addon_dated: None,
                installed_at: None,
                updated_at: None,
                dlss_fix_installed: false,
                addon_tracked: false,
            }
        );
    }

    #[test]
    fn install_state_surfaces_addon_date_and_timestamps() {
        let installed = InstalledAddon::new(game_id(), AddonKind::RenoDx, addon_path())
            .with_tracked_source(
                TrackedSource::new(
                    TrackedSourceRole::AddonPayload,
                    "https://clshortfuse.github.io/renodx/renodx-cp2077.addon64",
                    Some("\"etag\"".to_owned()),
                    "addon-digest",
                )
                .with_last_modified(Some("Wed, 18 Jun 2026 12:00:00 GMT".to_owned())),
            )
            .with_timestamps(Some(1_700_000_000_000), Some(1_700_000_500_000));

        assert_eq!(
            installed.install_state(),
            RenoDxInstallState::Installed {
                host_kind: None,
                version: None,
                addon_dated: Some("Wed, 18 Jun 2026 12:00:00 GMT".to_owned()),
                installed_at: Some(1_700_000_000_000),
                updated_at: Some(1_700_000_500_000),
                dlss_fix_installed: false,
                // The test record has an AddonPayload tracked source.
                addon_tracked: true,
            }
        );
    }

    #[test]
    fn install_state_maps_persisted_host_kind_to_ui_host_kind() {
        let proxy = InstalledAddon::new(game_id(), AddonKind::RenoDx, addon_path())
            .with_host_kind(InstalledAddonHostKind::Proxy);
        let vulkan = InstalledAddon::new(game_id(), AddonKind::RenoDx, addon_path())
            .with_host_kind(InstalledAddonHostKind::SharedVulkanLayer);

        assert_eq!(
            proxy.install_state(),
            RenoDxInstallState::Installed {
                host_kind: Some(RenoDxHostKind::Proxy),
                version: None,
                addon_dated: None,
                installed_at: None,
                updated_at: None,
                dlss_fix_installed: false,
                addon_tracked: false,
            }
        );
        assert_eq!(
            vulkan.install_state(),
            RenoDxInstallState::Installed {
                host_kind: Some(RenoDxHostKind::Vulkan),
                version: None,
                addon_dated: None,
                installed_at: None,
                updated_at: None,
                dlss_fix_installed: false,
                addon_tracked: false,
            }
        );
    }

    #[test]
    fn local_addon_date_placeholder_is_not_a_tracked_upstream_source() {
        let installed = InstalledAddon::new(game_id(), AddonKind::RenoDx, addon_path())
            .with_tracked_source(
                TrackedSource::new(TrackedSourceRole::AddonPayload, "", None, "addon-digest")
                    .with_last_modified(Some("Wed, 18 Jun 2026 12:00:00 GMT".to_owned())),
            );

        assert_eq!(
            installed.addon_dated(),
            Some("Wed, 18 Jun 2026 12:00:00 GMT")
        );
        assert!(!installed.has_addon_source());
        assert!(!installed.install_state().is_addon_tracked());
    }

    #[test]
    fn has_dlss_fix_and_install_state_reflect_dlss_fix_source() {
        let base = InstalledAddon::new(game_id(), AddonKind::RenoDx, addon_path());
        assert!(!base.has_dlss_fix());

        let with_fix = base.with_tracked_source(TrackedSource::new(
            TrackedSourceRole::DlssFix,
            "https://example/renodx-dlssfix.addon64",
            None,
            "dlss-fix-digest",
        ));
        assert!(with_fix.has_dlss_fix());
        assert!(with_fix.install_state().is_dlss_fix_installed());
    }

    #[test]
    fn install_state_serializes_with_status_tag() {
        let json = serde_json::to_string(&RenoDxInstallState::NotInstalled).expect("serialize");
        assert_eq!(json, r#"{"status":"not_installed"}"#);

        let installed = RenoDxInstallState::Installed {
            host_kind: Some(RenoDxHostKind::Proxy),
            version: None,
            addon_dated: None,
            installed_at: None,
            updated_at: None,
            dlss_fix_installed: false,
            addon_tracked: true,
        };
        let json = serde_json::to_string(&installed).expect("serialize");
        assert_eq!(
            json,
            r#"{"status":"installed","host_kind":"proxy","version":null,"addon_dated":null,"installed_at":null,"updated_at":null,"dlss_fix_installed":false,"addon_tracked":true}"#
        );
    }

    #[test]
    fn tracked_source_last_modified_round_trips_and_defaults() {
        let source = TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            "https://example/addon",
            None,
            "digest",
        )
        .with_last_modified(Some("Wed, 18 Jun 2026 12:00:00 GMT".to_owned()));
        let json = serde_json::to_string(&source).expect("serialize");
        assert_eq!(
            serde_json::from_str::<TrackedSource>(&json).expect("round-trip"),
            source
        );

        // A record persisted before `last_modified` existed (field absent) defaults
        // to `None` rather than failing to deserialize.
        let legacy = r#"{"role":"addon_payload","url":"https://example/addon","etag":null,"digest":"digest"}"#;
        let parsed: TrackedSource = serde_json::from_str(legacy).expect("legacy parse");
        assert_eq!(parsed.last_modified(), None);
        assert_eq!(parsed.channel(), None);
    }

    #[test]
    fn tracked_source_channel_round_trips_and_defaults() {
        let source = TrackedSource::new(
            TrackedSourceRole::HostBinary,
            "https://example/host.zip",
            None,
            "digest",
        )
        .with_channel("stable");
        let json = serde_json::to_string(&source).expect("serialize");
        assert_eq!(
            serde_json::from_str::<TrackedSource>(&json).expect("round-trip"),
            source
        );

        let legacy =
            r#"{"role":"host","url":"https://example/host.zip","etag":null,"digest":"digest"}"#;
        let parsed: TrackedSource = serde_json::from_str(legacy).expect("legacy parse");
        assert_eq!(parsed.channel(), None);
    }
}
