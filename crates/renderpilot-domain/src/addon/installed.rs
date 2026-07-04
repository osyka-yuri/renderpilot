use serde::{Deserialize, Serialize};

use crate::{AddonKind, GameId, PathRef};

use super::tracked::{InstalledAddonHostKind, TrackedSource, TrackedSourceRole};

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
}
