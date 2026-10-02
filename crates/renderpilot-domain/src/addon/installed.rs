//! Install record model ([`InstalledAddon`]) and reconstruction parts.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{AddonKind, GameId, PathRef};

use super::engine_config::EngineConfigJournal;
use super::managed_file::{
    InstalledAddonInvariantError, ManagedAddonFile, ManagedFileBaseline, ManagedFileMode,
};
use super::renodx_config::RenoDxConfigReceipt;
use super::tracked::{InstalledAddonHostKind, TrackedSource, TrackedSourceRole};

/// Named fields for reconstructing an [`InstalledAddon`] from storage or rebuild paths.
#[derive(Debug, Clone)]
pub struct InstalledAddonParts {
    /// Game that owns the install.
    pub game_id: GameId,
    /// Add-on kind.
    pub kind: AddonKind,
    /// Primary payload path (must appear in `created_files`).
    pub addon_file: PathRef,
    /// Optional upstream version label.
    pub addon_version: Option<String>,
    /// Files created by the install.
    pub created_files: Vec<PathRef>,
    /// Pre-existing files backed up before overwrite.
    pub backed_up_files: Vec<PathRef>,
    /// Coordinated managed-file bindings.
    pub managed_files: Vec<ManagedAddonFile>,
    /// Upstream provenance for updates.
    pub tracked_sources: Vec<TrackedSource>,
    /// RenoDX-only provenance for managed typed ReShade.ini mutations.
    pub renodx_config_receipt: Option<RenoDxConfigReceipt>,
    /// Shared Unreal Engine.ini ownership journal for RenoDX/Luma.
    pub engine_config_journal: Option<EngineConfigJournal>,
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
    /// Files whose lifecycle is coordinated with another feature. These paths
    /// must not also be handled by the generic create/backup engine.
    #[serde(default)]
    managed_files: Vec<ManagedAddonFile>,
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
    /// RenoDX-only provenance for managed typed ReShade.ini mutations.
    #[serde(default)]
    renodx_config_receipt: Option<RenoDxConfigReceipt>,
    #[serde(default)]
    engine_config_journal: Option<EngineConfigJournal>,
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
            managed_files: Vec::new(),
            tracked_sources: Vec::new(),
            installed_at: None,
            updated_at: None,
            host_kind: None,
            reshade_channel: None,
            registered_exe_path: None,
            renodx_config_receipt: None,
            engine_config_journal: None,
        }
    }

    /// Reconstructs a record from its persisted fields (no managed files).
    ///
    /// Prefer [`from_parts_with_managed`](Self::from_parts_with_managed) when
    /// rehydrating a row that may carry coordinated file bindings.
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
        Self::from_parts_with_managed(InstalledAddonParts {
            game_id,
            kind,
            addon_file,
            addon_version,
            created_files,
            backed_up_files,
            managed_files: Vec::new(),
            tracked_sources,
            renodx_config_receipt: None,
            engine_config_journal: None,
        })
        .ok()
        .flatten()
    }

    /// Reconstructs a record from its persisted fields, including coordinated
    /// managed-file bindings, in one step so callers cannot forget them.
    ///
    /// Returns `Ok(None)` when `created_files` does not contain `addon_file`.
    /// Returns `Err` when managed-file invariants fail.
    pub fn from_parts_with_managed(
        parts: InstalledAddonParts,
    ) -> Result<Option<Self>, InstalledAddonInvariantError> {
        let InstalledAddonParts {
            game_id,
            kind,
            addon_file,
            addon_version,
            created_files,
            backed_up_files,
            managed_files,
            tracked_sources,
            renodx_config_receipt,
            engine_config_journal,
        } = parts;
        validate_renodx_config_receipt_invariants(kind, renodx_config_receipt.as_ref())?;
        if let Some(journal) = &engine_config_journal {
            journal
                .validate_for_kind(kind)
                .map_err(|_| InstalledAddonInvariantError::InvalidEngineConfigJournal)?;
        }
        if !created_files.contains(&addon_file) {
            return Ok(None);
        }

        let record = Self {
            game_id,
            kind,
            addon_file,
            addon_version,
            created_files,
            backed_up_files,
            managed_files: Vec::new(),
            tracked_sources,
            installed_at: None,
            updated_at: None,
            host_kind: None,
            reshade_channel: None,
            registered_exe_path: None,
            renodx_config_receipt,
            engine_config_journal,
        };
        if managed_files.is_empty() {
            return Ok(Some(record));
        }
        Ok(Some(record.try_with_managed_files(managed_files)?))
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

    /// Compares the complete durable record while ignoring database-managed
    /// persistence timestamps.
    ///
    /// The exhaustive destructuring is intentional: adding a new field to the
    /// record must make this comparison fail to compile until that field is
    /// classified explicitly.
    #[must_use]
    pub fn eq_ignoring_persistence_timestamps(&self, other: &Self) -> bool {
        self.eq_ignoring_timestamps_and_engine_config_journal(other)
            && self.engine_config_journal == other.engine_config_journal
    }

    /// Returns whether this record is the result of releasing the original
    /// Engine.ini journal without changing any other durable install data.
    /// Database-managed persistence timestamps may differ.
    #[must_use]
    pub fn is_engine_config_release_of(&self, original: &Self) -> bool {
        self.engine_config_journal.is_none()
            && self.eq_ignoring_timestamps_and_engine_config_journal(original)
    }

    fn eq_ignoring_timestamps_and_engine_config_journal(&self, other: &Self) -> bool {
        let Self {
            game_id,
            kind,
            addon_file,
            addon_version,
            created_files,
            backed_up_files,
            managed_files,
            tracked_sources,
            installed_at: _,
            updated_at: _,
            host_kind,
            reshade_channel,
            registered_exe_path,
            renodx_config_receipt,
            engine_config_journal: _,
        } = self;
        let Self {
            game_id: other_game_id,
            kind: other_kind,
            addon_file: other_addon_file,
            addon_version: other_addon_version,
            created_files: other_created_files,
            backed_up_files: other_backed_up_files,
            managed_files: other_managed_files,
            tracked_sources: other_tracked_sources,
            installed_at: _,
            updated_at: _,
            host_kind: other_host_kind,
            reshade_channel: other_reshade_channel,
            registered_exe_path: other_registered_exe_path,
            renodx_config_receipt: other_renodx_config_receipt,
            engine_config_journal: _,
        } = other;

        game_id == other_game_id
            && kind == other_kind
            && addon_file == other_addon_file
            && addon_version == other_addon_version
            && created_files == other_created_files
            && backed_up_files == other_backed_up_files
            && managed_files == other_managed_files
            && tracked_sources == other_tracked_sources
            && host_kind == other_host_kind
            && reshade_channel == other_reshade_channel
            && registered_exe_path == other_registered_exe_path
            && renodx_config_receipt == other_renodx_config_receipt
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

    /// Attaches or clears RenoDX's typed ReShade.ini configuration receipt.
    pub fn with_renodx_config_receipt(
        mut self,
        receipt: Option<RenoDxConfigReceipt>,
    ) -> Result<Self, InstalledAddonInvariantError> {
        validate_renodx_config_receipt_invariants(self.kind, receipt.as_ref())?;
        self.renodx_config_receipt = receipt;
        Ok(self)
    }

    /// Attaches or clears the shared Unreal Engine.ini journal.
    pub fn with_engine_config_journal(
        mut self,
        journal: Option<EngineConfigJournal>,
    ) -> Result<Self, InstalledAddonInvariantError> {
        if let Some(value) = &journal {
            value
                .validate_for_kind(self.kind)
                .map_err(|_| InstalledAddonInvariantError::InvalidEngineConfigJournal)?;
        }
        self.engine_config_journal = journal;
        Ok(self)
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

    /// Replaces the coordinated file bindings after enforcing path uniqueness
    /// and disjoint ownership from the generic add-on engine.
    pub fn try_with_managed_files(
        mut self,
        files: Vec<ManagedAddonFile>,
    ) -> Result<Self, InstalledAddonInvariantError> {
        let engine_paths: HashSet<String> = self
            .created_files
            .iter()
            .chain(&self.backed_up_files)
            .map(|path| crate::normalized_path_key(path.as_str()))
            .collect();
        let mut managed_paths = HashSet::new();
        for file in &files {
            if file.mode() == ManagedFileMode::Reused {
                match file.baseline() {
                    ManagedFileBaseline::Absent => {
                        return Err(InstalledAddonInvariantError::ReusedFileHasAbsentBaseline(
                            file.path().clone(),
                        ));
                    }
                    ManagedFileBaseline::Present { sha256 }
                        if sha256 != file.installed_sha256() =>
                    {
                        return Err(InstalledAddonInvariantError::ReusedFileHashMismatch(
                            file.path().clone(),
                        ));
                    }
                    ManagedFileBaseline::Present { .. } => {}
                }
            }
            let key = crate::normalized_path_key(file.path().as_str());
            if !managed_paths.insert(key.clone()) {
                return Err(InstalledAddonInvariantError::DuplicateManagedPath(
                    file.path().clone(),
                ));
            }
            if engine_paths.contains(&key) {
                return Err(InstalledAddonInvariantError::ManagedPathOwnedByEngine(
                    file.path().clone(),
                ));
            }
        }
        self.managed_files = files;
        Ok(self)
    }

    /// Removes a path from the generic engine-owned sets. Used when a legacy
    /// record is reconciled into [`ManagedAddonFile`] ownership.
    ///
    /// Comparison uses [`crate::normalized_path_key`] so case / separator /
    /// `\\?\` variants of the same path match.
    #[must_use]
    pub fn without_engine_managed_path(mut self, path: &PathRef) -> Self {
        let key = crate::normalized_path_key(path.as_str());
        self.created_files
            .retain(|candidate| crate::normalized_path_key(candidate.as_str()) != key);
        self.backed_up_files
            .retain(|candidate| crate::normalized_path_key(candidate.as_str()) != key);
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

    /// Returns files coordinated outside the generic add-on engine.
    #[must_use]
    pub fn managed_files(&self) -> &[ManagedAddonFile] {
        &self.managed_files
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

    /// Returns RenoDX's managed typed ReShade.ini provenance, if present.
    #[must_use]
    pub fn renodx_config_receipt(&self) -> Option<&RenoDxConfigReceipt> {
        self.renodx_config_receipt.as_ref()
    }

    /// Returns the shared Unreal Engine.ini ownership journal, if present.
    #[must_use]
    pub fn engine_config_journal(&self) -> Option<&EngineConfigJournal> {
        self.engine_config_journal.as_ref()
    }

    /// Returns whether the add-on payload has a checkable upstream identity. This
    /// includes an advisory source recovered from an exact manifest payload; only
    /// records with no source URL at all are displayed as installed from a file.
    #[must_use]
    pub fn has_addon_source(&self) -> bool {
        self.tracked_sources.iter().any(|source| {
            source.role() == TrackedSourceRole::AddonPayload && !source.url().is_empty()
        })
    }
}

fn validate_renodx_config_receipt_invariants(
    kind: AddonKind,
    receipt: Option<&RenoDxConfigReceipt>,
) -> Result<(), InstalledAddonInvariantError> {
    let Some(receipt) = receipt else {
        return Ok(());
    };
    if kind != AddonKind::RenoDx {
        return Err(InstalledAddonInvariantError::RenoDxConfigReceiptOnNonRenoDx);
    }
    if !receipt.is_supported() {
        return Err(InstalledAddonInvariantError::InvalidRenoDxConfigReceipt);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_agnostic_equality_ignores_only_persistence_timestamps() {
        let record = InstalledAddon::new(
            GameId::new("steam:1").expect("game id"),
            AddonKind::Luma,
            PathRef::new("C:/Games/Test/luma.addon64").expect("addon path"),
        )
        .with_timestamps(Some(10), Some(20));
        let retimestamped = record.clone().with_timestamps(Some(30), Some(40));
        assert!(record.eq_ignoring_persistence_timestamps(&retimestamped));

        let changed = retimestamped.with_addon_version("2.0.0");
        assert!(!record.eq_ignoring_persistence_timestamps(&changed));
    }

    #[test]
    fn engine_config_release_requires_a_cleared_journal_and_same_ownership() {
        let original = InstalledAddon::new(
            GameId::new("steam:1").expect("game id"),
            AddonKind::Luma,
            PathRef::new("C:/Games/Test/luma.addon64").expect("addon path"),
        )
        .with_engine_config_journal(Some(EngineConfigJournal::default()))
        .expect("valid journal");
        let released = original
            .clone()
            .with_engine_config_journal(None)
            .expect("release journal");

        assert!(released.is_engine_config_release_of(&original));
        assert!(!released.eq_ignoring_persistence_timestamps(&original));
        assert!(!original.is_engine_config_release_of(&original));
        let retimestamped_release = released.clone().with_timestamps(Some(30), Some(40));
        assert!(retimestamped_release.is_engine_config_release_of(&original));

        let changed_owner = released
            .with_created_file(PathRef::new("C:/Games/Test/extra.dll").expect("extra owned file"));
        assert!(!changed_owner.is_engine_config_release_of(&original));
    }
}
