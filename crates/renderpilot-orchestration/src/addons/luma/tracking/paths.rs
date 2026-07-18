use std::path::{Path, PathBuf};

use renderpilot_domain::InstalledAddon;

use crate::addons::reshade::scan::{RESHADE_INI_FILE_NAME, is_proxy_slot};
use crate::addons::tracking;
use crate::paths::same_path;

/// Whether `path` is a concrete file this Luma record is entitled to replace
/// and remove. This deliberately reads only `created_files`: a legacy backup
/// is not ownership of a foreign runtime.
///
/// Equality uses [`same_path`] (best-effort canonicalize) so Windows path
/// casing / equivalent forms still count as ownership.
#[must_use]
pub(crate) fn owns_path(record: &InstalledAddon, path: &Path) -> bool {
    record
        .created_files()
        .iter()
        .any(|owned| same_path(Path::new(owned.as_str()), path))
}

/// Whether `path` is a host-adjacent file Luma may own (via empty-host adoption)
/// but never ships in the release ZIP — so it must stay off the payload set-diff
/// surface while remaining in `created_files` for uninstall.
#[must_use]
pub(crate) fn is_host_adjacent_non_payload(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(RESHADE_INI_FILE_NAME))
}

/// Absolute paths from `created_files` that belong to the Luma payload set-diff
/// surface (main `.addon`, `Luma/**`, optional root payload files).
///
/// Always excludes owned host proxy slots, any path whose file name is a
/// ReShade proxy slot, and host-adjacent sidecars such as `ReShade.ini` (Luma
/// never extracts those from the release ZIP). Callers pass `extra_excluded`
/// for managed dependencies (dgVoodoo) that must not be deleted as "removed"
/// payload.
#[must_use]
pub(crate) fn payload_owned_paths(
    record: &InstalledAddon,
    extra_excluded: &[PathBuf],
) -> Vec<PathBuf> {
    let owned_host = tracking::owned_proxy_host_path(record);
    // Also drop any recorded host proxy path (defensive; usually equals owned).
    let host = tracking::host_proxy_path(record);
    record
        .created_files()
        .iter()
        .map(|path_ref| PathBuf::from(path_ref.as_str()))
        .filter(|path| {
            !owned_host
                .as_deref()
                .is_some_and(|host_path| same_path(host_path, path.as_path()))
        })
        .filter(|path| {
            !host
                .as_ref()
                .is_some_and(|host_path| same_path(host_path, path.as_path()))
        })
        .filter(|path| {
            !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(is_proxy_slot)
        })
        .filter(|path| !is_host_adjacent_non_payload(path))
        .filter(|path| {
            !extra_excluded
                .iter()
                .any(|excluded| same_path(excluded, path.as_path()))
        })
        .collect()
}

/// Owned host-adjacent paths that are not part of the release ZIP payload
/// (currently adopted `ReShade.ini`). Kept in `created_files` for uninstall
/// but excluded from set-diff removals; rebuild must re-inject them.
#[must_use]
pub(crate) fn owned_host_adjacent_paths(record: &InstalledAddon) -> Vec<PathBuf> {
    record
        .created_files()
        .iter()
        .map(|path_ref| PathBuf::from(path_ref.as_str()))
        .filter(|path| is_host_adjacent_non_payload(path))
        .collect()
}

/// Owned paths that look like managed dgVoodoo dependencies (current or
/// historical). Excluded from payload set-diff removals and payload-intact
/// checks so a missing wrapper forces dependency rewrite, not full ZIP reconverge.
#[must_use]
pub(crate) fn owned_dependency_paths(record: &InstalledAddon) -> Vec<PathBuf> {
    record
        .created_files()
        .iter()
        .map(|path_ref| PathBuf::from(path_ref.as_str()))
        .filter(|path| crate::addons::luma::dgvoodoo::is_dependency_basename(path))
        .collect()
}

/// Payload membership excluding managed dependency wrappers (intact-check surface).
#[must_use]
fn payload_tracked_paths(record: &InstalledAddon) -> Vec<PathBuf> {
    payload_owned_paths(record, &owned_dependency_paths(record))
}

/// Cheap on-disk invariant for a Luma payload: the main `.addon` and every other
/// tracked payload path must still exist **and be readable**. Missing or
/// unreadable payload forces a full ZIP reconverge even when upstream ETags
/// still match the install record.
///
/// This does **not** re-hash bytes against the recorded digest — corruption of
/// present, readable files is the Repair (`force_full`) path. Managed dgVoodoo
/// wrappers are excluded — they are repaired via the dependency path, not the
/// release ZIP.
#[must_use]
pub(crate) fn payload_disk_intact(record: &InstalledAddon) -> bool {
    if !path_is_readable_file(Path::new(record.addon_file().as_str())) {
        return false;
    }
    payload_tracked_paths(record)
        .into_iter()
        .all(|path| path_is_readable_file(&path))
}

/// True when `path` is a regular file that can be opened for read (presence alone
/// is not enough: a directory at the expected path, or a sharing violation, must
/// force reconverge rather than report "current").
#[must_use]
fn path_is_readable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    std::fs::File::open(path).is_ok()
}
