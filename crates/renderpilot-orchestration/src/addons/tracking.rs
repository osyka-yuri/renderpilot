//! Pure, shared helpers for deriving UI-facing and update-related facts from a
//! persisted [`renderpilot_domain::InstalledAddon`], plus the shared record-rebuild
//! path both tools use after updates.
//!
//! These functions are the canonical implementation so that RenoDX and future
//! tools produce identical "dated" display values and host-proxy discovery without
//! copy-paste. Tool-specific tracking modules delegate to (or re-export) these where
//! possible.

use std::path::{Path, PathBuf};

use renderpilot_domain::{InstalledAddon, InstalledAddonHostKind, PathRef, TrackedSource};

use crate::ServiceError;
use crate::addons::errors;
use crate::addons::reshade::scan::is_proxy_slot;
use crate::fs::{format_http_date, is_reasonable_file_mtime};

/// How an update should treat the record's `addon_version` field.
#[derive(Debug, Clone)]
pub(crate) enum AddonVersionUpdate {
    /// Keep the source record's version unchanged.
    Keep,
}

/// Parts of an install record that an update may rewrite.
#[derive(Debug)]
pub(crate) struct RebuildParts {
    /// Main add-on path (may change when a tool renames a rolling-release asset).
    pub addon_file: PathRef,
    /// Version field policy for the rebuild.
    pub addon_version: AddonVersionUpdate,
    pub created_files: Vec<PathRef>,
    pub backed_up_files: Vec<PathRef>,
    pub tracked_sources: Vec<TrackedSource>,
    pub label: String,
}

/// Which metadata fields to carry onto the rebuilt record.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PreserveMetadata {
    /// When set, force this host kind; otherwise preserve the source record's.
    pub force_host_kind: Option<InstalledAddonHostKind>,
    pub reshade_channel: bool,
    pub registered_exe: bool,
}

impl PreserveMetadata {
    /// RenoDX: preserve host kind (Proxy/Vulkan), channel, and registered exe.
    #[must_use]
    pub(crate) const fn renodx() -> Self {
        Self {
            force_host_kind: None,
            reshade_channel: true,
            registered_exe: true,
        }
    }
}

/// Rebuilds an install record from file/source lists while preserving identity
/// timestamps and selected metadata. Canonical home for both tools' update paths.
pub(crate) fn rebuild_install_record(
    record: &InstalledAddon,
    parts: RebuildParts,
    preserve: PreserveMetadata,
) -> Result<InstalledAddon, ServiceError> {
    let installed_at = record.installed_at();
    let updated_at = record.updated_at();
    let version = match &parts.addon_version {
        AddonVersionUpdate::Keep => record.addon_version().map(str::to_owned),
    };
    let addon_file = parts.addon_file.clone();
    InstalledAddon::from_parts(
        record.game_id().clone(),
        record.kind(),
        parts.addon_file,
        version,
        parts.created_files,
        parts.backed_up_files,
        parts.tracked_sources,
    )
    .map(|rebuilt| {
        let mut rebuilt = apply_preserve(record, rebuilt, preserve);
        rebuilt = rebuilt.with_timestamps(installed_at, updated_at);
        rebuilt
    })
    .ok_or_else(|| {
        errors::failed(format!(
            "{} violated the addon_file invariant: `{}` is missing from the rebuilt created_files list",
            parts.label,
            addon_file.as_str()
        ))
    })
}

fn apply_preserve(
    source: &InstalledAddon,
    mut rebuilt: InstalledAddon,
    preserve: PreserveMetadata,
) -> InstalledAddon {
    if let Some(kind) = preserve.force_host_kind {
        rebuilt = rebuilt.with_host_kind(kind);
    } else if let Some(host_kind) = source.host_kind() {
        rebuilt = rebuilt.with_host_kind(host_kind);
    }
    if preserve.reshade_channel
        && let Some(channel) = source.reshade_channel()
    {
        rebuilt = rebuilt.with_reshade_channel(channel.to_owned());
    }
    if preserve.registered_exe
        && let Some(path) = source.registered_exe_path()
    {
        rebuilt = rebuilt.with_registered_exe_path(path.clone());
    }
    rebuilt
}

/// Returns the effective "add-on dated" string for UI display.
///
/// Prefers a fresh on-disk mtime of the main `addon_file` (when it looks reasonable)
/// over the `Last-Modified` recorded at install time. This gives users the most
/// useful "dated …" anchor even if the asset was re-published under the same URL.
#[must_use]
pub(crate) fn effective_addon_dated(record: &InstalledAddon) -> Option<String> {
    let path = Path::new(record.addon_file().as_str());
    if let Ok(modified) = std::fs::metadata(path).and_then(|metadata| metadata.modified())
        && is_reasonable_file_mtime(modified)
    {
        return Some(format_http_date(modified));
    }
    record.addon_dated().map(str::to_owned)
}

/// Returns the path of the ReShade host proxy DLL (dxgi/d3d11/etc.) that was
/// created or backed up by this install, if any can be identified from the record.
///
/// Used by rollback/uninstall and by update flows that must replace the host in place.
#[must_use]
pub(crate) fn host_proxy_path(record: &InstalledAddon) -> Option<PathBuf> {
    record
        .created_files()
        .iter()
        .chain(record.backed_up_files())
        .map(|path| PathBuf::from(path.as_str()))
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(is_proxy_slot)
        })
}

/// Returns a proxy DLL this record is entitled to remove. Unlike
/// [`host_proxy_path`], this deliberately ignores legacy backup references: an
/// adopted/managed ReShade runtime is represented by a created-file entry.
#[must_use]
pub(crate) fn owned_proxy_host_path(record: &InstalledAddon) -> Option<PathBuf> {
    record
        .created_files()
        .iter()
        .map(|path| PathBuf::from(path.as_str()))
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(is_proxy_slot)
        })
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{
        AddonKind, GameId, InstalledAddonHostKind, PathRef, TrackedSource, TrackedSourceRole,
    };

    use super::*;

    fn game_id() -> GameId {
        GameId::new("steam:42").expect("id")
    }

    fn path(value: &str) -> PathRef {
        PathRef::new(value).expect("path")
    }

    fn base_renodx() -> InstalledAddon {
        InstalledAddon::from_parts(
            game_id(),
            AddonKind::RenoDx,
            path(r"C:\Games\Test\renodx-test.addon64"),
            Some("1.0".to_owned()),
            vec![
                path(r"C:\Games\Test\renodx-test.addon64"),
                path(r"C:\Games\Test\dxgi.dll"),
            ],
            Vec::new(),
            vec![TrackedSource::new(
                TrackedSourceRole::AddonPayload,
                "https://example/a",
                None,
                "d",
            )],
        )
        .expect("record")
        .with_host_kind(InstalledAddonHostKind::Proxy)
        .with_reshade_channel("stable".to_owned())
        .with_registered_exe_path(path(r"C:\Games\Test\game.exe"))
        .with_timestamps(Some(10), Some(20))
    }

    #[test]
    fn effective_addon_dated_prefers_tracked_when_no_file_or_unreasonable_mtime() {
        let rec = InstalledAddon::new(
            game_id(),
            AddonKind::RenoDx,
            path(r"C:\nonexistent\renodx-test.addon64"),
        )
        .with_tracked_source(
            TrackedSource::new(
                TrackedSourceRole::AddonPayload,
                "https://example/a",
                None,
                "d",
            )
            .with_last_modified(Some("Wed, 18 Jun 2026 12:00:00 GMT".to_owned())),
        );

        // No on-disk file → falls back to tracked
        assert_eq!(
            effective_addon_dated(&rec),
            Some("Wed, 18 Jun 2026 12:00:00 GMT".to_owned())
        );
    }

    #[test]
    fn host_proxy_path_finds_in_created_or_backed_up() {
        let rec = base_renodx();

        assert_eq!(
            host_proxy_path(&rec).as_deref(),
            Some(Path::new("C:/Games/Test/dxgi.dll"))
        );
    }

    #[test]
    fn rebuild_renodx_preserves_metadata_and_version() {
        let source = base_renodx();
        let rebuilt = rebuild_install_record(
            &source,
            RebuildParts {
                addon_file: source.addon_file().clone(),
                addon_version: AddonVersionUpdate::Keep,
                created_files: source.created_files().to_vec(),
                backed_up_files: source.backed_up_files().to_vec(),
                tracked_sources: source.tracked_sources().to_vec(),
                label: "renodx test".to_owned(),
            },
            PreserveMetadata::renodx(),
        )
        .expect("rebuild");

        assert_eq!(rebuilt.addon_version(), Some("1.0"));
        assert_eq!(rebuilt.host_kind(), Some(InstalledAddonHostKind::Proxy));
        assert_eq!(rebuilt.reshade_channel(), Some("stable"));
        assert_eq!(
            rebuilt.registered_exe_path().map(PathRef::as_str),
            Some("C:/Games/Test/game.exe")
        );
        assert_eq!(rebuilt.installed_at(), Some(10));
        assert_eq!(rebuilt.updated_at(), Some(20));
    }

    #[test]
    fn rebuild_rejects_missing_addon_file_in_created_list() {
        let source = base_renodx();
        let err = rebuild_install_record(
            &source,
            RebuildParts {
                addon_file: path(r"C:\Games\Test\missing.addon64"),
                addon_version: AddonVersionUpdate::Keep,
                created_files: vec![path(r"C:\Games\Test\dxgi.dll")],
                backed_up_files: Vec::new(),
                tracked_sources: Vec::new(),
                label: "invariant test".to_owned(),
            },
            PreserveMetadata::renodx(),
        )
        .expect_err("must fail when addon_file not in created_files");

        let msg = err.to_string();
        assert!(
            msg.contains("invariant test"),
            "error should include label, got: {msg}"
        );
    }
}
