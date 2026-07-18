use std::fs;
use std::path::{Path, PathBuf};

use renderpilot_detection::NVNGX_DLSS_FILE_NAME;
use renderpilot_domain::AddonKind;

use crate::ServiceError;
use crate::addons::engine;
use crate::addons::errors::io;
use crate::addons::luma::tool::{is_luma_addon_backup_file_name, is_luma_addon_file_name};
use crate::addons::tool::unmanaged_files_present_in_dirs;

/// Bare file names Luma may shadow via `BackupAndReplace` / `MergeText` during a
/// managed dependency install. On a torn install these leave `{name}` (Luma
/// bytes) + `{name}.bak` (game original). Recovery must restore the bak the same
/// way it restores a shadowed `nvngx_dlss.dll`.
///
/// Deliberately an allowlist — never restore arbitrary `*.bak` siblings. Torn
/// recovery may replace live with bak for these names.
pub(super) fn recover_torn_install(scan_dirs: &[&Path]) {
    // Payload (including optional root `nvngx_dlss.dll`) may live on any scan
    // root — unified installs use `game_dir` only; split AddonPath puts the
    // payload tree in `addon_dir`. Restore `.bak` siblings in every root, not
    // just the sentinel directory.
    let mut recovery_complete = true;
    for dir in scan_dirs {
        recovery_complete &= recover_torn_install_in_dir(dir);
        if let Err(error) = restore_managed_backup_siblings(dir) {
            recovery_complete = false;
            log::warn!("Luma torn-install recovery: managed bak restore failed: {error}");
        }
    }

    if let Some(game_dir) = scan_dirs.first()
        && recovery_complete
        && !recovery_debris_present(scan_dirs)
        && !unmanaged_files_present_in_dirs(scan_dirs, AddonKind::Luma)
    {
        engine::clear_torn_install_marker(game_dir, AddonKind::Luma);
    }
}

pub(super) fn recover_torn_install_in_dir(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    let mut complete = true;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        let Ok(file_type) = entry.file_type() else {
            complete = false;
            continue;
        };
        if file_type.is_file()
            && (is_luma_addon_file_name(&name) || is_luma_addon_backup_file_name(&name))
        {
            complete &= remove_file_best_effort(&entry.path());
        } else if file_type.is_dir() && name == "luma" {
            complete &= remove_dir_best_effort(&entry.path());
        }
    }
    complete
}

/// Restores game-owned files shadowed via `{name}.bak` for allowlisted slots.
/// Never deletes `{name}` without a surviving bak — that would be the game's own.
///
/// Torn recovery may replace an existing live file with bak (live is often Luma
/// debris). Uses atomic copy-then-remove so a failed restore cannot leave an
/// empty slot.
pub(super) fn restore_managed_backup_siblings(dir: &Path) -> Result<(), ServiceError> {
    for name in std::iter::once(NVNGX_DLSS_FILE_NAME).chain(
        crate::addons::luma::dgvoodoo::historical_dependency_basenames()
            .iter()
            .copied(),
    ) {
        restore_backup_sibling(dir, name)?;
    }
    Ok(())
}

/// If `{file_name}.bak` exists under `dir`, restore it onto `{file_name}`.
/// Case-insensitive match for live/bak when the bak uses the allowlist
/// spelling (Windows game folders vary in casing).
fn restore_backup_sibling(dir: &Path, file_name: &str) -> Result<(), ServiceError> {
    let Some(bak) = find_case_insensitive_file(dir, &format!("{file_name}.bak")) else {
        return Ok(());
    };
    let live = find_case_insensitive_file(dir, file_name);
    let target = live.unwrap_or_else(|| dir.join(file_name));
    restore_bak_onto_live(&bak, &target)
}

/// Atomically put bak content at live, then remove the bak sibling.
///
/// Uses [`crate::fs::copy_file_atomically`] (temp + replace rename) so a failure
/// never leaves an empty slot after deleting live. After a successful replace,
/// the bak must be removed or `place_file` remains hard-blocked.
fn restore_bak_onto_live(bak: &Path, live: &Path) -> Result<(), ServiceError> {
    crate::fs::copy_file_atomically(bak, live)?;
    fs::remove_file(bak).map_err(|error| io("remove restored backup", bak, &error))?;
    Ok(())
}

fn find_case_insensitive_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|kind| kind.is_file())
            && entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(name)
        {
            return Some(entry.path());
        }
    }
    None
}

fn recovery_debris_present(scan_dirs: &[&Path]) -> bool {
    scan_dirs.iter().any(|dir| {
        let Ok(entries) = fs::read_dir(dir) else {
            return true;
        };
        entries.flatten().any(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let lower = name.to_ascii_lowercase();
            let is_luma_payload = entry.file_type().is_ok_and(|kind| {
                (kind.is_file()
                    && (is_luma_addon_file_name(&lower) || is_luma_addon_backup_file_name(&lower)))
                    || (kind.is_dir() && lower == "luma")
            });
            is_luma_payload
                || std::iter::once(NVNGX_DLSS_FILE_NAME)
                    .chain(
                        crate::addons::luma::dgvoodoo::historical_dependency_basenames()
                            .iter()
                            .copied(),
                    )
                    .any(|managed| name.eq_ignore_ascii_case(&format!("{managed}.bak")))
        })
    })
}

pub(super) fn remove_file_best_effort(path: &Path) -> bool {
    if let Err(error) = fs::remove_file(path) {
        log::warn!(
            "Luma torn-install recovery: failed to remove `{}`: {error}",
            path.display()
        );
        return false;
    }
    true
}

pub(super) fn remove_dir_best_effort(path: &Path) -> bool {
    if let Err(error) = fs::remove_dir_all(path) {
        log::warn!(
            "Luma torn-install recovery: failed to remove directory `{}`: {error}",
            path.display()
        );
        return false;
    }
    true
}
