use std::path::PathBuf;

use renderpilot_application::{fsr, AppResult};
use renderpilot_detection::{read_windows_file_version, sha256_file};
use renderpilot_domain::{ComponentFile, GameId, GraphicsComponent, GraphicsTechnology, PathRef};
use renderpilot_storage_sqlite::SqliteStorage;

pub(super) fn recover_orphaned_backups(
    storage: &SqliteStorage,
    game_id: &GameId,
    components: &[GraphicsComponent],
) -> AppResult<()> {
    for component in components {
        // If the database already knows about a backup for this component, skip.
        if storage.get_component_backup(component.id())?.is_some() {
            continue;
        }

        let mut recovered_baseline = Vec::new();

        // 1. Recover `.bak` files directly corresponding to the component's current files.
        for file in component.files() {
            let original_path = file.path().as_str();
            let bak_path = PathBuf::from(format!("{original_path}.bak"));
            if let Some(recovered_file) = recover_bak_file(&bak_path, original_path)? {
                recovered_baseline.push(recovered_file);
            }
        }

        // 2. FSR-specific: recover orphaned split-member backups from a previous downgrade.
        //    When an FSR 4 package is replaced by a single-file FSR 3 package, the split
        //    member `.bak` files remain on disk but are no longer tracked by the component.
        if component.technology().family() == GraphicsTechnology::AmdFsr {
            if let Some(primary) = component.files().first() {
                if let Some(parent) = primary.path().parent() {
                    recover_orphaned_fsr_split_members(parent, &mut recovered_baseline)?;
                }
            }
        }

        if !recovered_baseline.is_empty() {
            storage.recover_component_backup(game_id, component.id(), &recovered_baseline)?;
        }
    }

    Ok(())
}

/// Recovers a `.bak` file as a `ComponentFile` referencing `original_path` (the
/// live path the backup would be restored to).
///
/// Returns `None` when the backup does not exist, is empty, or cannot be read in
/// full. A backup we cannot verify is worse than no backup: restoring it on
/// rollback would overwrite the live file with truncated or corrupt bytes, so we
/// skip (and log) it rather than record an unverifiable baseline.
fn recover_bak_file(
    bak_path: &std::path::Path,
    original_path: &str,
) -> AppResult<Option<ComponentFile>> {
    match std::fs::metadata(bak_path) {
        Ok(meta) if meta.len() == 0 => {
            log::warn!("recovery: skipping empty backup {}", bak_path.display());
            return Ok(None);
        }
        Ok(_) => {}
        // No `.bak` for this file is the normal case (most files were never
        // swapped), not a corruption to warn about.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            log::warn!(
                "recovery: skipping backup with unreadable metadata {}: {error}",
                bak_path.display()
            );
            return Ok(None);
        }
    }

    // Require a full content hash — this both records the integrity digest and
    // proves the backup is readable end-to-end before we trust it as a baseline.
    let sha256 = match sha256_file(bak_path) {
        Ok(hash) => hash,
        Err(error) => {
            log::warn!(
                "recovery: skipping unreadable backup {}: {error}",
                bak_path.display()
            );
            return Ok(None);
        }
    };

    let path_ref = PathRef::new(original_path)
        .map_err(|e| renderpilot_application::AppError::invalid_input(e.to_string()))?;

    let mut component_file = ComponentFile::new(path_ref).with_sha256(sha256);

    // Best-effort: read the PE file version from the backup (non-PE backups have none).
    if let Some(version) = read_windows_file_version(bak_path) {
        component_file = component_file.with_version(version);
    }

    Ok(Some(component_file))
}

fn recover_orphaned_fsr_split_members(
    directory: &str,
    recovered_baseline: &mut Vec<ComponentFile>,
) -> AppResult<()> {
    let dir_path = PathBuf::from(directory);
    if !dir_path.is_dir() {
        return Ok(());
    }

    let read_dir = match std::fs::read_dir(&dir_path) {
        Ok(d) => d,
        Err(error) => {
            log::warn!(
                "recovery: cannot read directory {}: {error}",
                dir_path.display()
            );
            return Ok(());
        }
    };

    // Build a set of already-recovered original paths (lower-case) to avoid duplicates.
    let already_recovered: std::collections::HashSet<String> = recovered_baseline
        .iter()
        .map(|f| f.path().as_str().to_ascii_lowercase())
        .collect();

    for entry in read_dir.flatten() {
        let bak_path = entry.path();

        // Only consider `.bak` files.
        if bak_path.extension() != Some(std::ffi::OsStr::new("bak")) {
            continue;
        }

        // Only consider FSR split members / markers.
        let Some(stem) = bak_path.file_stem() else {
            continue;
        };
        let stem_str = stem.to_string_lossy();
        if !fsr::is_split_member(&stem_str) && !fsr::is_split_marker(&stem_str) {
            continue;
        }

        // Derive the original (non-`.bak`) path. The extension check above
        // guarantees the suffix is present; skip defensively if it is not.
        let bak_str = bak_path.to_string_lossy();
        let Some(original_path) = bak_str.strip_suffix(".bak") else {
            continue;
        };
        if already_recovered.contains(&original_path.to_ascii_lowercase()) {
            continue;
        }

        if let Some(recovered_file) = recover_bak_file(&bak_path, original_path)? {
            recovered_baseline.push(recovered_file);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &std::path::Path, bytes: &[u8]) {
        fs::write(path, bytes).expect("write fixture file");
    }

    #[test]
    fn missing_backup_is_not_recovered() {
        let dir = tempfile::tempdir().expect("temp dir");
        let bak = dir.path().join("nvngx_dlss.dll.bak");
        let original = dir.path().join("nvngx_dlss.dll");
        let recovered =
            recover_bak_file(&bak, original.to_string_lossy().as_ref()).expect("no error");
        assert!(recovered.is_none());
    }

    #[test]
    fn empty_backup_is_rejected() {
        let dir = tempfile::tempdir().expect("temp dir");
        let bak = dir.path().join("nvngx_dlss.dll.bak");
        write(&bak, b"");
        let original = dir.path().join("nvngx_dlss.dll");
        let recovered =
            recover_bak_file(&bak, original.to_string_lossy().as_ref()).expect("no error");
        assert!(
            recovered.is_none(),
            "a zero-byte backup must never be recovered as a baseline"
        );
    }

    #[test]
    fn readable_backup_is_recovered_with_content_hash() {
        let dir = tempfile::tempdir().expect("temp dir");
        let bak = dir.path().join("nvngx_dlss.dll.bak");
        write(&bak, b"original-bytes");
        let original = dir.path().join("nvngx_dlss.dll");
        let recovered = recover_bak_file(&bak, original.to_string_lossy().as_ref())
            .expect("no error")
            .expect("a readable backup should be recovered");
        assert!(
            recovered.path().as_str().ends_with("nvngx_dlss.dll"),
            "recovered file points at the live path, not the .bak"
        );
        assert!(
            recovered.sha256().is_some(),
            "a verified backup carries its content hash"
        );
    }

    #[test]
    fn orphaned_split_member_baks_are_recovered_and_others_ignored() {
        let dir = tempfile::tempdir().expect("temp dir");
        write(
            &dir.path().join("amd_fidelityfx_upscaler_dx12.dll.bak"),
            b"upscaler",
        );
        // A non-split component backup must not be swept up by FSR recovery.
        write(&dir.path().join("nvngx_dlss.dll.bak"), b"dlss");
        // A live (non-.bak) file must be ignored.
        write(
            &dir.path().join("amd_fidelityfx_upscaler_dx12.dll"),
            b"live",
        );

        let mut baseline = Vec::new();
        recover_orphaned_fsr_split_members(dir.path().to_string_lossy().as_ref(), &mut baseline)
            .expect("recovery should succeed");

        let names: Vec<String> = baseline
            .iter()
            .filter_map(|file| file.path().file_name().map(str::to_owned))
            .collect();
        assert_eq!(names, vec!["amd_fidelityfx_upscaler_dx12.dll"]);
    }
}
