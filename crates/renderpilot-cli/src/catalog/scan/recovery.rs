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
            let bak_path = PathBuf::from(format!("{}.bak", file.path().as_str()));
            if let Some(recovered_file) = recover_bak_file_if_exists(&bak_path)? {
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

/// Attempts to recover a `.bak` file as a `ComponentFile` referencing the original path.
/// Returns `None` if the `.bak` file does not exist.
fn recover_bak_file_if_exists(bak_path: &PathBuf) -> AppResult<Option<ComponentFile>> {
    if !bak_path.exists() {
        return Ok(None);
    }

    let bak_str = bak_path.to_string_lossy();
    let original_path_str = bak_str
        .strip_suffix(".bak")
        .unwrap_or(bak_str.as_ref());
    let path_ref = PathRef::new(original_path_str)
        .map_err(|e| renderpilot_application::AppError::invalid_input(e.to_string()))?;

    let mut component_file = ComponentFile::new(path_ref);

    // Best-effort: compute SHA-256 from the backup bytes.
    if let Ok(hash) = sha256_file(bak_path) {
        component_file = component_file.with_sha256(hash);
    }

    // Best-effort: read the PE file version from the backup.
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
        Err(_) => return Ok(()),
    };

    // Build a set of already-recovered original paths (lower-case) to avoid duplicates.
    let already_recovered: std::collections::HashSet<String> = recovered_baseline
        .iter()
        .map(|f| f.path().as_str().to_ascii_lowercase())
        .collect();

    for entry in read_dir.flatten() {
        let bak_path = entry.path();

        // Only consider `.bak` files.
        if !bak_path.extension().is_some_and(|ext| ext == "bak") {
            continue;
        }

        // Only consider FSR split members / markers.
        let Some(stem) = bak_path.file_stem() else { continue };
        let stem_str = stem.to_string_lossy();
        if !fsr::is_split_member(&stem_str) && !fsr::is_split_marker(&stem_str) {
            continue;
        }

        // Derive the original (non-.bak) path and skip if already recovered.
        let bak_str = bak_path.to_string_lossy();
        let original_path = bak_str
            .strip_suffix(".bak")
            .unwrap_or("");
        if already_recovered.contains(&original_path.to_ascii_lowercase()) {
            continue;
        }

        if let Some(recovered_file) = recover_bak_file_if_exists(&bak_path)? {
            recovered_baseline.push(recovered_file);
        }
    }

    Ok(())
}
