//! Record lookups and cascade composition for Luma-managed DLSS.

use std::path::{Path, PathBuf};

use renderpilot_domain::{
    InstalledAddon, ManagedAddonFile, ManagedFileBaseline, ManagedFileMode, PathRef, Sha256Hash,
};

use crate::ServiceError;
use crate::addons::luma::errors;
use crate::addons::luma::fetch::types::LumaPayloadFile;

pub(crate) use renderpilot_detection::NVNGX_DLSS_FILE_NAME;

pub(crate) fn bundled_file(payload: &[LumaPayloadFile]) -> Option<&LumaPayloadFile> {
    payload.iter().find(|file| {
        file.relative_path
            .eq_ignore_ascii_case(NVNGX_DLSS_FILE_NAME)
    })
}

pub(crate) fn is_dlss_relative_path(path: &str) -> bool {
    path.eq_ignore_ascii_case(NVNGX_DLSS_FILE_NAME)
}

pub(crate) fn find_managed_dlss_binding(record: &InstalledAddon) -> Option<&ManagedAddonFile> {
    record.managed_files().iter().find(|managed| {
        Path::new(managed.path().as_str())
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case(NVNGX_DLSS_FILE_NAME))
    })
}

pub(crate) fn find_created_dlss(record: &InstalledAddon) -> Option<&PathRef> {
    record.created_files().iter().find(|path| {
        Path::new(path.as_str())
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case(NVNGX_DLSS_FILE_NAME))
    })
}

/// Owned DLSS binding path when the update payload no longer ships DLSS.
///
/// Empty when the payload still includes a bundled DLSS file, or when there is
/// no owned managed DLSS binding to release.
///
/// Full uninstall and mutation-path snapshotting use all owned managed paths
/// instead; only apply-time "payload dropped DLSS" composition uses this set
/// (see [`cascade_for_disappearing_owned`]).
fn disappearing_owned_dlss_paths(
    record: &InstalledAddon,
    payload: &[LumaPayloadFile],
) -> Vec<PathBuf> {
    if bundled_file(payload).is_some() {
        return Vec::new();
    }
    find_managed_dlss_binding(record)
        .filter(|managed| managed.mode() == ManagedFileMode::Owned)
        .map(|managed| vec![PathBuf::from(managed.path().as_str())])
        .unwrap_or_default()
}

/// Catalog cascade plan for an owned DLSS binding that the update payload no
/// longer ships. Thin composition of [`disappearing_owned_dlss_paths`] +
/// [`crate::catalog::cascade::cascade_for_owned_paths`].
pub(crate) fn cascade_for_disappearing_owned(
    storage: &renderpilot_storage_sqlite::SqliteStorage,
    record: &InstalledAddon,
    payload: &[LumaPayloadFile],
) -> renderpilot_application::AppResult<crate::catalog::cascade::CascadeResult> {
    let owned_paths = disappearing_owned_dlss_paths(record, payload);
    crate::catalog::cascade::cascade_for_owned_paths(storage, record.game_id(), &owned_paths)
}

pub(super) fn owned_binding(
    path: &Path,
    baseline: ManagedFileBaseline,
    installed_sha256: Sha256Hash,
) -> Result<ManagedAddonFile, ServiceError> {
    Ok(ManagedAddonFile::owned(
        path_ref(path)?,
        baseline,
        installed_sha256,
    ))
}

pub(super) fn path_ref(path: &Path) -> Result<PathRef, ServiceError> {
    PathRef::new(path.to_string_lossy().as_ref())
        .map_err(|error| errors::invalid(error.to_string()))
}
