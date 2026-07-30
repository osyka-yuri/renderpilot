use std::collections::HashSet;
use std::path::{Path, PathBuf};

use renderpilot_application::{AppResult, GameRepository, InstalledAddonRepository};
use renderpilot_detection::sha256_file;
use renderpilot_domain::{
    ComponentFile, ComponentRollbackBaseline, D3d12ExecutableBaseline, D3d12ExecutableIdentity,
    GameId, LibraryComponent, LibraryTechnology, PathRef, fsr,
};
use renderpilot_storage_sqlite::{
    ComponentBaselineMutation, GameMutationCommit, InstalledAddonMutation, SqliteStorage,
};

pub(super) fn recover_orphaned_backups(
    storage: &SqliteStorage,
    game_id: &GameId,
    components: &[LibraryComponent],
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
                    Some(baseline.files()),
                    crate::coordinated_files::managed_files_of(installed_addon.as_ref()),
                )?;
                if baseline.expected_active_files().is_empty() {
                    let current = crate::coordinated_files::current_component_snapshot(
                        component,
                        crate::coordinated_files::managed_files_of(installed_addon.as_ref()),
                    )
                    .map_err(renderpilot_application::AppError::from)?
                    .into_component();
                    storage.commit_game_mutation(GameMutationCommit {
                        game_id,
                        component_set: None,
                        baseline_mutations: &[
                            ComponentBaselineMutation::UpdateExpectedActiveFiles {
                                component_id: component.id(),
                                files: current.files(),
                            },
                        ],
                        addon: InstalledAddonMutation::Keep,
                        mutation_id: None,
                    })?;
                }
                if component.technology() == LibraryTechnology::D3D12Agility
                    && baseline.d3d12_executable().is_none()
                    && let Some(executable) =
                        recover_unique_d3d12_executable_baseline(storage, &game)?
                {
                    let executable_is_original =
                        executable.original() == executable.expected_active();
                    if executable_is_original || baseline_has_complete_sidecars(baseline.files()) {
                        storage.recover_component_d3d12_executable_baseline(
                            component.id(),
                            &executable,
                        )?;
                    } else {
                        log::info!(
                            "recovery: refusing to pair a patched executable with a DLL baseline that has no immutable sidecars for {}",
                            component.id()
                        );
                    }
                }
                continue;
            }
            crate::coordinated_files::ComponentBackupAvailability::Unavailable(_) => {
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
        if component.technology().family() == LibraryTechnology::AmdFsr
            && let Some(primary) = component.files().first()
            && let Some(parent) = primary.path().parent()
        {
            recover_orphaned_fsr_split_members(parent, &mut recovered_baseline)?;
        }

        if !recovered_baseline.is_empty() {
            let mut rollback_baseline = ComponentRollbackBaseline::new(recovered_baseline)
                .with_expected_active_files(component.files().to_vec());
            if component.technology() == LibraryTechnology::D3D12Agility
                && let Some(executable) = recover_unique_d3d12_executable_baseline(storage, &game)?
            {
                rollback_baseline = rollback_baseline.with_d3d12_executable(executable);
            }
            storage.recover_component_rollback_baseline(
                game_id,
                component.id(),
                &rollback_baseline,
            )?;
        }
    }

    Ok(())
}

fn baseline_has_complete_sidecars(files: &[ComponentFile]) -> bool {
    !files.is_empty()
        && files.iter().all(|file| {
            crate::fs::backup_path(Path::new(file.path().as_str()))
                .is_ok_and(|path| crate::fs::is_readable_non_empty_file(&path))
        })
}

fn recover_unique_d3d12_executable_baseline(
    storage: &SqliteStorage,
    game: &renderpilot_domain::GameInstallation,
) -> AppResult<Option<D3d12ExecutableBaseline>> {
    let override_path = storage
        .get_nvapi_executable_override(game.id().as_str())?
        .map(|row| PathBuf::from(row.selected_path));
    let resolved = crate::game_executable::resolve_primary_executable(
        Path::new(game.install_path().as_str()),
        override_path.as_deref(),
        true,
    );
    let mut candidates = Vec::new();
    if let Some(resolved) = resolved {
        candidates.push(PathBuf::from(resolved.path.as_str()));
    }
    candidates.extend(
        game.executable_candidates()
            .iter()
            .map(|path| PathBuf::from(path.as_str())),
    );
    let mut seen = HashSet::new();
    let recovered = candidates
        .into_iter()
        .filter(|path| seen.insert(crate::paths::normalized_key(path)))
        .filter_map(|path| recover_d3d12_executable_pair(&path).transpose())
        .collect::<AppResult<Vec<_>>>()?;
    Ok((recovered.len() == 1).then(|| recovered[0].clone()))
}

fn recover_d3d12_executable_pair(live_path: &Path) -> AppResult<Option<D3d12ExecutableBaseline>> {
    let backup_path = crate::fs::backup_path(live_path)
        .map_err(|error| renderpilot_application::AppError::invalid_input(error.to_string()))?;
    if !live_path.is_file() || !backup_path.is_file() {
        return Ok(None);
    }
    let live = std::fs::read(live_path).map_err(|error| {
        renderpilot_application::AppError::provider_failed(format!(
            "cannot read executable {} during backup recovery: {error}",
            live_path.display()
        ))
    })?;
    let original = std::fs::read(&backup_path).map_err(|error| {
        renderpilot_application::AppError::provider_failed(format!(
            "cannot read executable backup {} during recovery: {error}",
            backup_path.display()
        ))
    })?;
    if !crate::catalog::runtime_compatibility::differs_only_at_sdk_export(&original, &live) {
        return Ok(None);
    }
    let Some(original_export) = renderpilot_detection::pe_exported_u32_from_bytes(
        &original,
        crate::catalog::runtime_compatibility::D3D12_SDK_VERSION_EXPORT,
    ) else {
        return Ok(None);
    };
    let Some(current_export) = renderpilot_detection::pe_exported_u32_from_bytes(
        &live,
        crate::catalog::runtime_compatibility::D3D12_SDK_VERSION_EXPORT,
    ) else {
        return Ok(None);
    };
    let executable_path = PathRef::new(live_path.to_string_lossy().into_owned())
        .map_err(|error| renderpilot_application::AppError::invalid_input(error.to_string()))?;
    Ok(Some(D3d12ExecutableBaseline::new(
        executable_path,
        D3d12ExecutableIdentity::new(
            original_export.value,
            renderpilot_detection::sha256_bytes(&original)?,
        ),
        D3d12ExecutableIdentity::new(
            current_export.value,
            renderpilot_detection::sha256_bytes(&live)?,
        ),
    )))
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
    technology: LibraryTechnology,
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
            LibraryTechnology::AmdFsr,
        )? {
            recovered_baseline.push(recovered_file);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderpilot_application::ComponentRepository;
    use renderpilot_domain::{
        ComponentId, ComponentKind, GameIdentity, GameInstallation, GameRuntime, Launcher,
        Platform, Swappability,
    };
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
            LibraryTechnology::DlssSuperResolution,
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
            LibraryTechnology::DlssSuperResolution,
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
            LibraryTechnology::DlssSuperResolution,
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
    fn d3d12_executable_pair_recovers_only_a_single_field_transition() {
        let dir = tempfile::tempdir().expect("temp dir");
        let live_path = dir.path().join("game.exe");
        let backup_path = dir.path().join("game.exe.bak");
        let original = crate::catalog::runtime_compatibility::synthetic_d3d12_executable(606);
        let mut live = original.clone();
        renderpilot_detection::replace_pe_exported_u32_in_bytes(
            &mut live,
            crate::catalog::runtime_compatibility::D3D12_SDK_VERSION_EXPORT,
            606,
            619,
        )
        .expect("patch fixture");
        write(&live_path, &live);
        write(&backup_path, &original);

        let recovered = recover_d3d12_executable_pair(&live_path)
            .expect("recovery")
            .expect("valid pair");
        assert_eq!(recovered.original().sdk_version(), 606);
        assert_eq!(recovered.expected_active().sdk_version(), 619);

        live[2] ^= 1;
        write(&live_path, &live);
        assert!(
            recover_d3d12_executable_pair(&live_path)
                .expect("external-change assessment")
                .is_none(),
            "a change outside the SDK export must never be adopted"
        );

        write(&live_path, &original);
        write(&backup_path, b"not a PE");
        assert!(
            recover_d3d12_executable_pair(&live_path)
                .expect("corrupt-backup assessment")
                .is_none()
        );
    }

    #[test]
    fn recovery_enriches_a_dll_baseline_only_when_the_pair_is_coherent() {
        let dir = tempfile::tempdir().expect("temp dir");
        let live_dll = dir.path().join("D3D12Core.dll");
        let backup_dll = dir.path().join("D3D12Core.dll.bak");
        let executable = dir.path().join("game.exe");
        let executable_backup = dir.path().join("game.exe.bak");
        write(&live_dll, b"active-sdk-619");
        write(&backup_dll, b"original-sdk-606");
        let original_executable =
            crate::catalog::runtime_compatibility::synthetic_d3d12_executable(606);
        let active_executable =
            crate::catalog::runtime_compatibility::synthetic_d3d12_executable(619);
        write(&executable, &active_executable);
        write(&executable_backup, &original_executable);

        let game = recovery_game(dir.path(), &executable, "coherent");
        let component_id = ComponentId::new("component:recovery-coherent").expect("component");
        let component = LibraryComponent::new(
            component_id.clone(),
            game.id().clone(),
            ComponentKind::NativeLibrary,
            LibraryTechnology::D3D12Agility,
            Swappability::Swappable,
        )
        .with_file(
            ComponentFile::new(path_ref(&live_dll))
                .with_sha256(renderpilot_detection::sha256_file(&live_dll).expect("live hash")),
        );
        let original_file = ComponentFile::new(path_ref(&live_dll))
            .with_sha256(renderpilot_detection::sha256_file(&backup_dll).expect("backup hash"));
        let storage = SqliteStorage::in_memory().expect("storage");
        storage.upsert_game(&game).expect("game");
        storage
            .replace_components_for_game(game.id(), std::slice::from_ref(&component))
            .expect("component");
        storage
            .recover_component_rollback_baseline(
                game.id(),
                &component_id,
                &ComponentRollbackBaseline::new(vec![original_file]),
            )
            .expect("DLL-only baseline");

        recover_orphaned_backups(&storage, game.id(), &[component]).expect("recovery");

        let recovered = storage
            .get_component_backup(&component_id)
            .expect("query")
            .expect("baseline");
        let executable = recovered
            .d3d12_executable()
            .expect("EXE baseline must be attached");
        assert_eq!(executable.original().sdk_version(), 606);
        assert_eq!(executable.expected_active().sdk_version(), 619);
    }

    #[test]
    fn recovery_does_not_pair_a_patched_executable_without_immutable_dll_bytes() {
        let dir = tempfile::tempdir().expect("temp dir");
        let live_dll = dir.path().join("D3D12Core.dll");
        let executable = dir.path().join("game.exe");
        write(&live_dll, b"unbacked-live-runtime");
        let original_executable =
            crate::catalog::runtime_compatibility::synthetic_d3d12_executable(606);
        let active_executable =
            crate::catalog::runtime_compatibility::synthetic_d3d12_executable(619);
        write(&executable, &active_executable);
        write(&dir.path().join("game.exe.bak"), &original_executable);

        let game = recovery_game(dir.path(), &executable, "unpaired");
        let component_id = ComponentId::new("component:recovery-unpaired").expect("component");
        let live_file = ComponentFile::new(path_ref(&live_dll))
            .with_sha256(renderpilot_detection::sha256_file(&live_dll).expect("live hash"));
        let component = LibraryComponent::new(
            component_id.clone(),
            game.id().clone(),
            ComponentKind::NativeLibrary,
            LibraryTechnology::D3D12Agility,
            Swappability::Swappable,
        )
        .with_file(live_file.clone());
        let storage = SqliteStorage::in_memory().expect("storage");
        storage.upsert_game(&game).expect("game");
        storage
            .replace_components_for_game(game.id(), std::slice::from_ref(&component))
            .expect("component");
        storage
            .recover_component_rollback_baseline(
                game.id(),
                &component_id,
                &ComponentRollbackBaseline::new(vec![live_file]),
            )
            .expect("DLL-only baseline");

        recover_orphaned_backups(&storage, game.id(), &[component]).expect("recovery");

        assert!(
            storage
                .get_component_backup(&component_id)
                .expect("query")
                .expect("baseline")
                .d3d12_executable()
                .is_none(),
            "a patched EXE must not be paired with mutable DLL bytes"
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

    fn path_ref(path: &Path) -> PathRef {
        PathRef::new(path.to_string_lossy().into_owned()).expect("path")
    }

    fn recovery_game(root: &Path, executable: &Path, suffix: &str) -> GameInstallation {
        GameInstallation::new(
            GameIdentity::new(
                GameId::new(format!("manual:recovery-{suffix}")).expect("game id"),
                format!("Recovery {suffix}"),
                Launcher::Manual,
            )
            .expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            path_ref(root),
        )
        .with_executable_candidate(path_ref(executable))
    }
}
