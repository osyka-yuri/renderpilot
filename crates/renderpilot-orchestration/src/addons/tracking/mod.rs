//! Pure, shared helpers for deriving UI-facing and update-related facts from a
//! persisted [`renderpilot_domain::InstalledAddon`], plus the shared record-rebuild
//! path both tools use after updates.
//!
//! These functions are the canonical implementation so that RenoDX, Luma, and
//! future tools produce identical "dated" display values and host-proxy discovery
//! without copy-paste. Tool-specific tracking modules (renodx/tracking, luma/tracking)
//! delegate to (or re-export) these where possible.

use std::path::{Path, PathBuf};

use renderpilot_domain::{
    InstalledAddon, InstalledAddonHostKind, InstalledAddonParts, ManagedAddonFile, PathRef,
    TrackedSource,
};

use crate::ServiceError;
use crate::addons::errors;
use crate::addons::reshade::scan::is_proxy_slot;
use crate::fs::{format_http_date, is_reasonable_file_mtime};

/// How an update should treat the record's `addon_version` field.
#[derive(Debug, Clone)]
pub(crate) enum AddonVersionUpdate {
    /// Keep the source record's version unchanged.
    Keep,
    /// Replace with this value (`None` clears the version label).
    Set(Option<String>),
}

/// How an update should treat coordinated `managed_files` ownership.
#[derive(Debug, Clone)]
pub(crate) enum ManagedFilesUpdate {
    /// Keep the source record's managed bindings (default for most rebuilds).
    Keep,
    /// Replace with this set (Luma set-diff may drop or re-bind DLSS).
    Replace(Vec<ManagedAddonFile>),
}

/// Parts of an install record that an update may rewrite.
#[derive(Debug)]
pub(crate) struct RebuildParts {
    /// Main add-on path (may change when Luma renames the rolling-release asset).
    pub addon_file: PathRef,
    /// Version field policy for the rebuild.
    pub addon_version: AddonVersionUpdate,
    /// Managed-file ownership policy for the rebuild.
    pub managed_files: ManagedFilesUpdate,
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

    /// Luma: always Proxy host and preserve its channel. The registered
    /// executable field remains reserved for shared-host registration.
    #[must_use]
    pub(crate) const fn luma() -> Self {
        Self {
            force_host_kind: Some(InstalledAddonHostKind::Proxy),
            reshade_channel: true,
            registered_exe: false,
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
        AddonVersionUpdate::Set(v) => v.clone(),
    };
    let addon_file = parts.addon_file.clone();
    let managed_files = match &parts.managed_files {
        ManagedFilesUpdate::Keep => record.managed_files().to_vec(),
        ManagedFilesUpdate::Replace(files) => files.clone(),
    };
    let rebuilt = InstalledAddon::from_parts_with_managed(InstalledAddonParts {
        game_id: record.game_id().clone(),
        kind: record.kind(),
        addon_file: parts.addon_file,
        addon_version: version,
        created_files: parts.created_files,
        backed_up_files: parts.backed_up_files,
        managed_files,
        tracked_sources: parts.tracked_sources,
    })
    .map_err(|error| errors::failed(error.to_string()))?
    .ok_or_else(|| {
        errors::failed(format!(
            "{} violated the addon_file invariant: `{}` is missing from the rebuilt created_files list",
            parts.label,
            addon_file.as_str()
        ))
    })?;
    let mut rebuilt = apply_preserve(record, rebuilt, preserve);
    rebuilt = rebuilt.with_timestamps(installed_at, updated_at);
    Ok(rebuilt)
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
/// useful "dated --" anchor even if the asset was re-published under the same URL.
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
mod tests;
