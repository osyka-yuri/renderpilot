//! Rebuilds the refreshed [`InstalledAddon`] record's tracked paths after a
//! successful set-diff update.

use std::path::{Path, PathBuf};

use renderpilot_domain::{InstalledAddon, PathRef, TrackedSource, TrackedSourceRole};

use super::diff::surviving_payload_paths;
use super::rollback::SetDiffRollback;
use crate::ServiceError;
use crate::addons::luma::fetch::types::LumaPayload;
use crate::addons::luma::source;
use crate::paths::same_path;

/// Membership via [`same_path`] so Windows path-form drift cannot duplicate entries.
pub(super) fn path_refs_contain(list: &[PathRef], path: &Path) -> bool {
    list.iter()
        .any(|tracked| same_path(Path::new(tracked.as_str()), path))
}

pub(super) fn tracked_addon_source(asset: &str, payload: &LumaPayload) -> TrackedSource {
    TrackedSource::new(
        TrackedSourceRole::AddonPayload,
        source::asset_url(asset),
        payload.etag.clone(),
        payload.zip_digest.clone(),
    )
    .with_last_modified(payload.last_modified.clone())
}

/// Rebuilds the full `created_files`/`backed_up_files` lists for the refreshed
/// record: surviving recorded payload paths, freshly added paths (as
/// engine-confirmed by `rollback.added`, the authoritative source for what was
/// actually written), the active host slot -- which may be a path this record
/// never tracked before (A.3) -- host-adjacent ownership such as adopted
/// `ReShade.ini` (never on the payload set-diff surface), and the old
/// `backed_up_files` minus any `.bak` a removal just consumed (A.4), plus
/// whatever an addition shadowed (A.1).
pub(super) fn rebuild_record_paths(
    record: &InstalledAddon,
    removed: &[PathBuf],
    rollback: &SetDiffRollback,
    host_path: Option<&Path>,
    dependency_paths: &[PathBuf],
) -> Result<(Vec<PathRef>, Vec<PathRef>), ServiceError> {
    let mut created_files = Vec::new();
    // Survivors must not depend on strip_prefix/key success -- only same_path
    // removal / host / dependency exclusion (see diff::surviving_payload_paths).
    for path in surviving_payload_paths(record, dependency_paths)
        .into_iter()
        .filter(|path| !removed.iter().any(|r| same_path(r, path)))
    {
        created_files.push(crate::addons::record::to_path_ref(&path)?);
    }
    for path in dependency_paths {
        if record
            .created_files()
            .iter()
            .any(|tracked| same_path(Path::new(tracked.as_str()), path))
            && !path_refs_contain(&created_files, path)
        {
            created_files.push(crate::addons::record::to_path_ref(path)?);
        }
    }
    for path in &rollback.added.created_files {
        created_files.push(crate::addons::record::to_path_ref(path)?);
    }
    if let Some(host_path) = host_path
        && !path_refs_contain(&created_files, host_path)
    {
        created_files.push(crate::addons::record::to_path_ref(host_path)?);
    }
    // Adopted ReShade.ini (and other host-adjacent non-payload files) stay owned
    // for uninstall but are excluded from the ZIP set-diff -- re-attach them.
    for path in crate::addons::luma::tracking::owned_host_adjacent_paths(record) {
        if !path_refs_contain(&created_files, &path) {
            created_files.push(crate::addons::record::to_path_ref(&path)?);
        }
    }

    let consumed_backups: Vec<&Path> = rollback
        .removed
        .iter()
        .filter(|undo| undo.restored_original.is_some())
        .map(|undo| undo.path.as_path())
        .collect();
    let mut backed_up_files: Vec<PathRef> = record
        .backed_up_files()
        .iter()
        .filter(|path_ref| {
            let tracked = Path::new(path_ref.as_str());
            !consumed_backups
                .iter()
                .any(|consumed| same_path(tracked, consumed))
        })
        .cloned()
        .collect();
    for path in &rollback.added.backed_up_files {
        if !path_refs_contain(&backed_up_files, path) {
            backed_up_files.push(crate::addons::record::to_path_ref(path)?);
        }
    }

    Ok((created_files, backed_up_files))
}
