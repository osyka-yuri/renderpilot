//! Rollback-side orchestration for catalog components.

use super::*;

pub(crate) const ROLLBACK_TARGET_LABEL: &str = "Original";

/// Rolls a component back to its recorded baseline.
pub fn rollback_component(
    context: &crate::Context,
    game_id: &GameId,
    component_id: &ComponentId,
) -> Result<RollbackResult, ServiceError> {
    let guard = crate::game_mutation_lock::enter_game_mutation_boundary(context, game_id)?;
    rollback_component_locked(context, &guard, game_id, component_id)
}

/// Rolls a component back while the caller owns the game's mutation boundary.
///
/// Compound operations such as catalog removal use this entry point so the
/// rollback and the subsequent metadata change cannot race with another
/// command. Public callers must use [`rollback_component`].
pub(crate) fn rollback_component_locked(
    context: &crate::Context,
    guard: &crate::game_mutation_lock::GameMutationGuard,
    game_id: &GameId,
    component_id: &ComponentId,
) -> Result<RollbackResult, ServiceError> {
    if guard.game_id() != game_id {
        return Err(ServiceError::invalid_input(
            "component rollback guard does not match the requested game",
        ));
    }
    let storage = context.storage();
    let prepared = prepare_rollback(context, game_id, component_id)?;
    let PreparedRollback {
        game,
        component,
        rollback_baseline: _,
        baseline,
        d3d12_state,
        d3d12_action,
    } = prepared;
    let game_root = std::path::Path::new(game.install_path().as_str());
    let restored_path = baseline
        .first()
        .map(|file| file.path().as_str().to_owned())
        .unwrap_or_default();

    let mut restored_files = baseline.clone();
    fsr::sort_representative_first(&mut restored_files);
    let rebuilt = component.rebuild_with_files(restored_files);
    let next_components = planning::full_component_set(storage, game_id, rebuilt)?;
    let scope = crate::file_mutation::MutationScope::single(game_root)?;
    let mut mutation_paths = rollback_mutation_paths(component.files(), &baseline);
    if let Some(state) = d3d12_state.as_ref() {
        mutation_paths.push(state.executable_path.clone());
        mutation_paths.push(state.backup_path.clone());
    }
    let mut executable_guard = d3d12_state
        .as_ref()
        .map(mutation_guard::D3d12ExecutableMutationGuard::acquire)
        .transpose()?;
    crate::file_mutation::run_durable_mutation(
        crate::file_mutation::DurableMutation {
            context,
            guard,
            scope: &scope,
            feature: crate::addons::mutation_features::CATALOG_ROLLBACK,
            subject_id: Some(component_id.as_str()),
            paths: mutation_paths,
        },
        |mutation_id| -> AppResult<RollbackResult> {
            let result = (|| -> AppResult<RollbackResult> {
                restore_baseline_preserving_sidecars(component.files(), &baseline)?;
                #[cfg(test)]
                inject_d3d12_rollback_failure(D3d12RollbackFailurePoint::AfterDllRestore)?;
                if let (Some(guard), Some(state)) =
                    (executable_guard.as_mut(), d3d12_state.as_ref())
                {
                    guard.restore_for_rollback(state)?;
                    // Windows cannot remove the sidecar while its deny-delete
                    // handle is open. Keep the live EXE locked until the
                    // database commit, and release only the backup handle.
                    guard.release_backup_lock();
                }
                #[cfg(test)]
                inject_d3d12_rollback_failure(D3d12RollbackFailurePoint::AfterExecutableRestore)?;
                release_baseline_sidecars(component.files(), &baseline)?;
                #[cfg(test)]
                inject_d3d12_rollback_failure(D3d12RollbackFailurePoint::AfterDllSidecarRelease)?;
                if let Some(state) = d3d12_state.as_ref() {
                    mutation_guard::release_rollback_backup(state)?;
                }
                #[cfg(test)]
                inject_d3d12_rollback_failure(
                    D3d12RollbackFailurePoint::AfterExecutableSidecarRelease,
                )?;

                let coordinated_addon =
                    addon_after_catalog_rollback(storage, game_id, &component, &baseline)?;
                let addon_mutation = coordinated_addon
                    .as_ref()
                    .map_or(InstalledAddonMutation::Keep, InstalledAddonMutation::Upsert);
                let baseline_mutations = [ComponentBaselineMutation::Delete { component_id }];
                #[cfg(test)]
                inject_d3d12_rollback_failure(D3d12RollbackFailurePoint::BeforeDatabaseCommit)?;
                storage.commit_game_mutation(GameMutationCommit {
                    game_id,
                    component_set: Some(&next_components),
                    baseline_mutations: &baseline_mutations,
                    addon: addon_mutation,
                    mutation_id: Some(mutation_id),
                })?;

                let d3d12_executable_action = d3d12_action
                    .as_ref()
                    .and_then(types::D3d12ExecutableActionResult::from_action);
                let mut journal_items = baseline
                    .iter()
                    .map(|file| JournalEntryItem::component_file(file.path(), None))
                    .collect::<Vec<_>>();
                if let Some(action) = d3d12_action
                    .as_ref()
                    .filter(|action| action.changes_executable())
                {
                    journal_items.push(JournalEntryItem::d3d12_executable(action));
                }
                record_operation_journal_entry(
                    storage,
                    JournalEntryParams {
                        game_id,
                        component_id,
                        kind: OperationKind::RollbackComponent,
                        component: &component,
                        to_version: component_version_report(&baseline, component.technology())
                            .known_version()
                            .map(|v| v.as_str())
                            .or(Some(ROLLBACK_TARGET_LABEL)),
                        items: journal_items,
                        d3d12_executable_action: d3d12_executable_action.clone(),
                    },
                );

                Ok(RollbackResult {
                    game_id: game_id.as_str().to_owned(),
                    component_id: component_id.as_str().to_owned(),
                    restored_path,
                    restored_file_count: baseline.len(),
                    d3d12_executable_action,
                })
            })();
            // Release locks before durable rollback restores any before-snapshots.
            drop(executable_guard.take());
            result
        },
        |_| {},
        || {},
    )
}

/// Builds a fresh, non-mutating rollback plan including the managed EXE action.
pub fn build_rollback_plan(
    context: &crate::Context,
    game_id: &GameId,
    component_id: &ComponentId,
) -> Result<RollbackPlan, ServiceError> {
    let guard = crate::game_mutation_lock::enter_game_mutation_boundary(context, game_id)?;
    build_rollback_plan_locked(context, &guard, game_id, component_id)
}

/// Builds a rollback preflight while a compound operation owns the game lock.
pub(crate) fn build_rollback_plan_locked(
    context: &crate::Context,
    guard: &crate::game_mutation_lock::GameMutationGuard,
    game_id: &GameId,
    component_id: &ComponentId,
) -> Result<RollbackPlan, ServiceError> {
    if guard.game_id() != game_id {
        return Err(ServiceError::invalid_input(
            "component rollback-plan guard does not match the requested game",
        ));
    }
    let PreparedRollback {
        component,
        rollback_baseline,
        d3d12_action: d3d12_executable_action,
        ..
    } = prepare_rollback(context, game_id, component_id)?;
    let live_and_baseline = component
        .files()
        .iter()
        .chain(rollback_baseline.files())
        .map(|file| file.path().clone())
        .collect::<Vec<_>>();
    let mut affected_files = live_and_baseline.clone();
    affected_files.extend(live_and_baseline.iter().filter_map(|path| {
        crate::fs::backup_path(std::path::Path::new(path.as_str()))
            .ok()
            .and_then(|backup| PathRef::new(backup.to_string_lossy().into_owned()).ok())
    }));
    if let Some(executable) = rollback_baseline.d3d12_executable() {
        affected_files.push(executable.executable_path().clone());
        if let Ok(backup) =
            crate::fs::backup_path(std::path::Path::new(executable.executable_path().as_str()))
            && let Ok(backup) = PathRef::new(backup.to_string_lossy().into_owned())
        {
            affected_files.push(backup);
        }
    }
    affected_files.sort();
    affected_files.dedup();
    Ok(RollbackPlan {
        game_id: game_id.clone(),
        component_id: component_id.clone(),
        affected_files,
        d3d12_executable_action,
    })
}

struct PreparedRollback {
    game: renderpilot_domain::GameInstallation,
    component: renderpilot_domain::LibraryComponent,
    rollback_baseline: ComponentRollbackBaseline,
    baseline: Vec<renderpilot_domain::ComponentFile>,
    d3d12_state: Option<crate::catalog::runtime_compatibility::D3d12ExecutableState>,
    d3d12_action: Option<renderpilot_application::D3d12ExecutableAction>,
}

/// Shared authoritative rollback preparation used by preview and mutation.
fn prepare_rollback(
    context: &crate::Context,
    game_id: &GameId,
    component_id: &ComponentId,
) -> Result<PreparedRollback, ServiceError> {
    let storage = context.storage();
    let game = storage.require_game(game_id)?;
    let scanned_component = require_component_for_game(storage, game_id, component_id)?;
    let rollback_baseline = match crate::coordinated_files::load_component_backup_availability(
        storage,
        &scanned_component,
    )? {
        crate::coordinated_files::ComponentBackupAvailability::NotRecorded => {
            return Err(AppError::invalid_input(format!(
                "no swap to roll back for component {}",
                component_id.as_str()
            ))
            .into());
        }
        crate::coordinated_files::ComponentBackupAvailability::Available(baseline) => baseline,
        crate::coordinated_files::ComponentBackupAvailability::Unavailable(_) => {
            return Err(AppError::invalid_input(format!(
                "rollback baseline for component {} is incomplete; verify game files and scan again",
                component_id.as_str()
            ))
            .into());
        }
    };
    let managed_files =
        crate::coordinated_files::managed_files_of(storage.get_installed_addon(game_id)?.as_ref())
            .to_vec();
    let component =
        crate::coordinated_files::current_component_snapshot(&scanned_component, &managed_files)
            .map_err(|error| {
                AppError::invalid_input(format!(
                    "component {} changed on disk since it was scanned: {error}",
                    component_id.as_str()
                ))
            })?
            .into_component();
    validate_expected_active_component(
        &component,
        rollback_baseline.expected_active_files(),
        &managed_files,
    )?;
    let baseline = crate::coordinated_files::resolve_component_baseline(
        std::path::Path::new(game.install_path().as_str()),
        component.technology(),
        component.files(),
        Some(rollback_baseline.files()),
        &managed_files,
    )
    .map_err(|error| {
        AppError::invalid_input(format!(
            "cannot validate rollback baseline for component {}: {error}",
            component_id.as_str()
        ))
    })?;
    let (d3d12_state, d3d12_action) = rollback_executable_assessment(&rollback_baseline)?;
    if d3d12_action.as_ref().is_some_and(|action| {
        action.kind() == renderpilot_application::D3d12ExecutableActionKind::RepairRequired
    }) {
        return Err(AppError::invalid_input(
            "D3D12 executable changed outside D3D12SDKVersion; verify game files and scan again",
        )
        .into());
    }
    Ok(PreparedRollback {
        game,
        component,
        rollback_baseline,
        baseline,
        d3d12_state,
        d3d12_action,
    })
}

fn validate_expected_active_component(
    component: &renderpilot_domain::LibraryComponent,
    expected_active: &[renderpilot_domain::ComponentFile],
    managed_files: &[renderpilot_domain::ManagedAddonFile],
) -> AppResult<()> {
    if expected_active.is_empty() {
        return Ok(());
    }
    let current_paths = component
        .files()
        .iter()
        .map(|file| crate::paths::normalized_key(std::path::Path::new(file.path().as_str())))
        .collect::<std::collections::BTreeSet<_>>();
    let expected_paths = expected_active
        .iter()
        .map(|file| crate::paths::normalized_key(std::path::Path::new(file.path().as_str())))
        .collect::<std::collections::BTreeSet<_>>();
    if current_paths != expected_paths {
        return Err(AppError::invalid_input(format!(
            "component {} no longer matches the active file set recorded by its rollback baseline",
            component.id().as_str()
        )));
    }

    for current in component.files() {
        let path = std::path::Path::new(current.path().as_str());
        let actual = current.sha256().ok_or_else(|| {
            AppError::invalid_input(format!(
                "current component file has no hash for {}",
                path.display()
            ))
        })?;
        let expected = expected_active
            .iter()
            .find(|candidate| {
                crate::paths::same_path(std::path::Path::new(candidate.path().as_str()), path)
            })
            .and_then(renderpilot_domain::ComponentFile::sha256)
            .ok_or_else(|| {
                AppError::invalid_input(format!(
                    "rollback baseline has no active hash for {}",
                    path.display()
                ))
            })?;
        let accepted_by_addon = managed_files.iter().any(|managed| {
            crate::paths::same_path(std::path::Path::new(managed.path().as_str()), path)
                && managed.installed_sha256() == actual
        });
        if actual != expected && !accepted_by_addon {
            return Err(AppError::invalid_input(format!(
                "component {} no longer matches the active bytes recorded by its rollback baseline at {}",
                component.id().as_str(),
                path.display()
            )));
        }
    }
    Ok(())
}

pub(super) fn rollback_executable_assessment(
    rollback_baseline: &ComponentRollbackBaseline,
) -> AppResult<(
    Option<crate::catalog::runtime_compatibility::D3d12ExecutableState>,
    Option<renderpilot_application::D3d12ExecutableAction>,
)> {
    let Some(executable) = rollback_baseline.d3d12_executable() else {
        return Ok((None, None));
    };
    let state = crate::catalog::runtime_compatibility::assess_d3d12_executable(
        std::path::Path::new(executable.executable_path().as_str()),
        Some(rollback_baseline),
    )?;
    let executable_path = PathRef::new(state.executable_path.to_string_lossy().into_owned())
        .map_err(|error| AppError::invalid_input(error.to_string()))?;
    let backup_path = PathRef::new(state.backup_path.to_string_lossy().into_owned())
        .map_err(|error| AppError::invalid_input(error.to_string()))?;
    let profile = renderpilot_application::D3d12ExecutableProfile::new(
        executable_path,
        backup_path,
        state.original_sdk_version,
        state.current_sdk_version,
        state.backup_exists,
        state.repair_required,
    );
    let action = renderpilot_application::D3d12ExecutableAction::for_managed_rollback(&profile)
        .map_err(|error| AppError::invalid_input(error.to_string()))?;
    Ok((Some(state), Some(action)))
}

/// Expands component-file live paths with their classic `.bak` sidecars.
///
/// Shared by apply, rollback, and cascade so snapshot-set and validated-set
/// cannot diverge on sidecar expansion rules.
pub(crate) fn mutation_paths_from_component_files<'a>(
    files: impl IntoIterator<Item = &'a renderpilot_domain::ComponentFile>,
) -> Vec<std::path::PathBuf> {
    crate::fs::expand_with_sidecars(
        files
            .into_iter()
            .map(|file| std::path::PathBuf::from(file.path().as_str())),
    )
}

pub(super) fn rollback_mutation_paths(
    current: &[renderpilot_domain::ComponentFile],
    baseline: &[renderpilot_domain::ComponentFile],
) -> Vec<std::path::PathBuf> {
    mutation_paths_from_component_files(current.iter().chain(baseline))
}

/// A direct catalog rollback also unwinds any owned add-on overlay on the same
/// path. Remove that binding in the same SQLite commit so later scan, update,
/// and uninstall operations do not claim a sidecar that rollback consumed.
fn addon_after_catalog_rollback(
    storage: &SqliteStorage,
    game_id: &GameId,
    component: &renderpilot_domain::LibraryComponent,
    baseline: &[renderpilot_domain::ComponentFile],
) -> AppResult<Option<renderpilot_domain::InstalledAddon>> {
    let Some(record) = storage.get_installed_addon(game_id)? else {
        return Ok(None);
    };
    crate::coordinated_files::record_after_component_rollback(&record, component, baseline)
}
