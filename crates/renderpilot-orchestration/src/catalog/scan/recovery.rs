use std::path::PathBuf;

use renderpilot_application::{AppResult, GameRepository, InstalledAddonRepository};
use renderpilot_detection::sha256_file;
use renderpilot_domain::{
    ComponentFile, GameId, GraphicsComponent, GraphicsTechnology, PathRef, fsr,
};
use renderpilot_storage_sqlite::SqliteStorage;

pub(super) fn recover_orphaned_backups(
    storage: &SqliteStorage,
    game_id: &GameId,
    components: &[GraphicsComponent],
) -> AppResult<()> {
    let installed_addon = storage.get_installed_addon(game_id)?;
    let game = storage.require_game(game_id)?;
    let game_root = std::path::Path::new(game.install_path().as_str());
    for component in components {
        // A recorded rollback claim must still match the immutable sidecars on
        // disk. Merely having a DB row is not enough to skip validation.
        match crate::coordinated_files::load_component_backup_availability(storage, component)? {
            crate::coordinated_files::ComponentBackupAvailability::Available(baseline) => {
                crate::coordinated_files::resolve_component_baseline(
                    game_root,
                    component.technology(),
                    component.files(),
                    Some(&baseline),
                    crate::coordinated_files::managed_files_of(installed_addon.as_ref()),
                )?;
                continue;
            }
            crate::coordinated_files::ComponentBackupAvailability::Unavailable => {
                log::info!(
                    "recovery: recorded backup for {} is no longer available on disk",
                    component.id()
                );
                continue;
            }
            crate::coordinated_files::ComponentBackupAvailability::NotRecorded => {}
        }

        let mut recovered_baseline = Vec::new();

        // 1. Recover `.bak` files directly corresponding to the component's current files.
        for file in component.files() {
            let original_path = file.path().as_str();
            let original_path_on_disk = std::path::Path::new(original_path);
            if installed_addon.as_ref().is_some_and(|record| {
                record.managed_files().iter().any(|binding| {
                    binding.mode() == renderpilot_domain::ManagedFileMode::Owned
                        && crate::paths::same_path(
                            std::path::Path::new(binding.path().as_str()),
                            original_path_on_disk,
                        )
                }) || record.backed_up_files().iter().any(|path| {
                    crate::paths::same_path(
                        std::path::Path::new(path.as_str()),
                        original_path_on_disk,
                    )
                })
            }) {
                continue;
            }
            let Ok(bak_path) = crate::fs::backup_path(std::path::Path::new(original_path)) else {
                log::warn!("recovery: cannot derive backup path for {original_path}, skipping");
                continue;
            };
            if let Some(recovered_file) =
                recover_bak_file_for_technology(&bak_path, original_path, component.technology())?
            {
                recovered_baseline.push(recovered_file);
            }
        }

        // 2. FSR-specific: recover orphaned split-member backups from a previous downgrade.
        //    When an FSR 4 package is replaced by a single-file FSR 3 package, the split
        //    member `.bak` files remain on disk but are no longer tracked by the component.
        if component.technology().family() == GraphicsTechnology::AmdFsr
            && let Some(primary) = component.files().first()
            && let Some(parent) = primary.path().parent()
        {
            recover_orphaned_fsr_split_members(parent, &mut recovered_baseline)?;
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
/// Returns `None` only when the backup does not exist. Once a classic sidecar
/// exists it is a baseline claim: invalid, empty or unreadable bytes block the
/// scan instead of being silently ignored and later overwritten by a mutator.
fn recover_bak_file_for_technology(
    bak_path: &std::path::Path,
    original_path: &str,
    technology: GraphicsTechnology,
) -> AppResult<Option<ComponentFile>> {
    match std::fs::metadata(bak_path) {
        Ok(meta) if !meta.is_file() || meta.len() == 0 => {
            return Err(renderpilot_application::AppError::invalid_input(format!(
                "classic backup is not a readable non-empty file: {}",
                bak_path.display()
            )));
        }
        Ok(_) => {}
        // No `.bak` for this file is the normal case (most files were never
        // swapped), not a corruption to warn about.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(renderpilot_application::AppError::provider_failed(format!(
                "cannot inspect classic backup {}: {error}",
                bak_path.display()
            )));
        }
    }

    // Require a full content hash — this both records the integrity digest and
    // proves the backup is readable end-to-end before we trust it as a baseline.
    let sha256 = sha256_file(bak_path).map_err(|error| {
        renderpilot_application::AppError::provider_failed(format!(
            "cannot read classic backup {}: {error}",
            bak_path.display()
        ))
    })?;

    let path_ref = PathRef::new(original_path)
        .map_err(|e| renderpilot_application::AppError::invalid_input(e.to_string()))?;

    let mut component_file = ComponentFile::new(path_ref).with_sha256(sha256);

    // Best-effort: observe version and technology-specific PE metadata.
    component_file =
        crate::coordinated_files::with_observed_metadata(component_file, technology, bak_path);

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

        // Only consider lowercase `.bak` sidecars produced by our backup helper.
        let Some(original) = crate::fs::original_path_from_backup(&bak_path) else {
            continue;
        };

        // Only consider FSR split members (the upscaler marker is one of them).
        let Some(stem) = original.file_name() else {
            continue;
        };
        let stem_str = stem.to_string_lossy();
        if !fsr::is_split_member(&stem_str) {
            continue;
        }

        let original_path = original.to_string_lossy();
        if already_recovered.contains(&original_path.to_ascii_lowercase()) {
            continue;
        }

        if let Some(recovered_file) = recover_bak_file_for_technology(
            &bak_path,
            original_path.as_ref(),
            GraphicsTechnology::AmdFsr,
        )? {
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
        let recovered = recover_bak_file_for_technology(
            &bak,
            original.to_string_lossy().as_ref(),
            GraphicsTechnology::DlssSuperResolution,
        )
        .expect("no error");
        assert!(recovered.is_none());
    }

    #[test]
    fn empty_backup_blocks_recovery() {
        let dir = tempfile::tempdir().expect("temp dir");
        let bak = dir.path().join("nvngx_dlss.dll.bak");
        write(&bak, b"");
        let original = dir.path().join("nvngx_dlss.dll");
        let error = recover_bak_file_for_technology(
            &bak,
            original.to_string_lossy().as_ref(),
            GraphicsTechnology::DlssSuperResolution,
        )
        .expect_err("invalid sidecar must block");
        assert!(error.message().contains("non-empty"));
    }

    #[test]
    fn readable_backup_is_recovered_with_content_hash() {
        let dir = tempfile::tempdir().expect("temp dir");
        let bak = dir.path().join("nvngx_dlss.dll.bak");
        write(&bak, b"original-bytes");
        let original = dir.path().join("nvngx_dlss.dll");
        let recovered = recover_bak_file_for_technology(
            &bak,
            original.to_string_lossy().as_ref(),
            GraphicsTechnology::DlssSuperResolution,
        )
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
