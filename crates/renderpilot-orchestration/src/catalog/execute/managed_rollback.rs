//! Managed and orphaned rollback paths used by catalog cleanup.

use super::*;

/// Cleanup-facing rollback plan that also represents a baseline whose
/// component row disappeared during a later scan.
#[derive(Debug, Clone)]
pub(crate) struct ManagedComponentRollbackPlan {
    component_id: ComponentId,
    affected_files: Vec<PathRef>,
}

impl ManagedComponentRollbackPlan {
    pub(crate) const fn component_id(&self) -> &ComponentId {
        &self.component_id
    }

    pub(crate) fn affected_files(&self) -> &[PathRef] {
        &self.affected_files
    }
}

/// Builds a rollback plan for removal without assuming that a current
/// component row still exists. The immutable baseline deliberately survives
/// rescans; its expected-active provenance is therefore the authority for an
/// orphaned preflight.
pub(crate) fn build_managed_rollback_plan_locked(
    context: &crate::Context,
    guard: &crate::game_mutation_lock::GameMutationGuard,
    game_id: &GameId,
    component_id: &ComponentId,
) -> Result<ManagedComponentRollbackPlan, ServiceError> {
    if context
        .storage()
        .list_components_for_game(game_id)?
        .iter()
        .any(|component| component.id() == component_id)
    {
        let plan = build_rollback_plan_locked(context, guard, game_id, component_id)?;
        return Ok(ManagedComponentRollbackPlan {
            component_id: plan.component_id().clone(),
            affected_files: plan.affected_files().to_vec(),
        });
    }

    let prepared = prepare_orphaned_rollback(context, guard, game_id, component_id)?;
    Ok(ManagedComponentRollbackPlan {
        component_id: component_id.clone(),
        affected_files: orphaned_rollback_affected_files(&prepared.rollback_baseline),
    })
}

/// Executes a removal-owned rollback against fresh locked state.
pub(crate) fn rollback_managed_component_locked(
    context: &crate::Context,
    guard: &crate::game_mutation_lock::GameMutationGuard,
    game_id: &GameId,
    component_id: &ComponentId,
) -> Result<(), ServiceError> {
    if context
        .storage()
        .list_components_for_game(game_id)?
        .iter()
        .any(|component| component.id() == component_id)
    {
        return rollback_component_locked(context, guard, game_id, component_id).map(|_| ());
    }
    rollback_orphaned_component_locked(context, guard, game_id, component_id)
}

struct PreparedOrphanedRollback {
    game: renderpilot_domain::GameInstallation,
    rollback_baseline: ComponentRollbackBaseline,
    d3d12_state: Option<crate::catalog::runtime_compatibility::D3d12ExecutableState>,
}

fn prepare_orphaned_rollback(
    context: &crate::Context,
    guard: &crate::game_mutation_lock::GameMutationGuard,
    game_id: &GameId,
    component_id: &ComponentId,
) -> Result<PreparedOrphanedRollback, ServiceError> {
    if guard.game_id() != game_id {
        return Err(ServiceError::invalid_input(
            "orphaned component rollback guard does not match the requested game",
        ));
    }
    let storage = context.storage();
    let game = storage.require_game(game_id)?;
    if storage
        .list_components_for_game(game_id)?
        .iter()
        .any(|component| component.id() == component_id)
    {
        return Err(ServiceError::invalid_input(
            "orphaned component rollback received a current component",
        ));
    }
    let rollback_baseline = storage.get_component_backup(component_id)?.ok_or_else(|| {
        ServiceError::invalid_input(format!(
            "no rollback baseline exists for orphaned component {}",
            component_id.as_str()
        ))
    })?;
    validate_orphaned_component_state(
        &game,
        &rollback_baseline,
        storage.get_installed_addon(game_id)?.as_ref(),
    )?;
    let (d3d12_state, d3d12_action) = rollback_executable_assessment(&rollback_baseline)?;
    if d3d12_action.as_ref().is_some_and(|action| {
        action.kind() == renderpilot_application::D3d12ExecutableActionKind::RepairRequired
    }) {
        return Err(AppError::invalid_input(
            "orphaned D3D12 executable changed outside its recorded active identity",
        )
        .into());
    }
    Ok(PreparedOrphanedRollback {
        game,
        rollback_baseline,
        d3d12_state,
    })
}

fn validate_orphaned_component_state(
    game: &renderpilot_domain::GameInstallation,
    baseline: &ComponentRollbackBaseline,
    addon: Option<&renderpilot_domain::InstalledAddon>,
) -> Result<(), ServiceError> {
    let scope = crate::file_mutation::MutationScope::single(std::path::Path::new(
        game.install_path().as_str(),
    ))?;
    let mut live_paths = baseline
        .files()
        .iter()
        .chain(baseline.expected_active_files())
        .map(|file| file.path().clone())
        .collect::<Vec<_>>();
    live_paths.sort();
    live_paths.dedup();

    for path in &live_paths {
        let live = std::path::Path::new(path.as_str());
        if !scope.contains_reachable(live) {
            return Err(crate::failed(format!(
                "orphaned component path is outside the game root: {}",
                live.display()
            )));
        }
        let metadata = match std::fs::symlink_metadata(live) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(crate::failed(format!(
                    "cannot inspect orphaned component path {}: {error}",
                    live.display()
                )));
            }
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(crate::failed(format!(
                "orphaned component path is not a regular file: {}",
                live.display()
            )));
        }
        let actual = renderpilot_detection::sha256_file(live)?;
        let accepted_by_catalog = baseline
            .files()
            .iter()
            .chain(baseline.expected_active_files())
            .filter(|file| {
                crate::paths::same_path(std::path::Path::new(file.path().as_str()), live)
            })
            .filter_map(renderpilot_domain::ComponentFile::sha256)
            .any(|expected| expected == &actual);
        let accepted_by_addon = addon.is_some_and(|record| {
            record.managed_files().iter().any(|managed| {
                crate::paths::same_path(std::path::Path::new(managed.path().as_str()), live)
                    && managed.installed_sha256() == &actual
            })
        });
        if !accepted_by_catalog && !accepted_by_addon {
            return Err(crate::failed(format!(
                "orphaned component live bytes no longer match recorded provenance at {}",
                live.display()
            )));
        }
    }

    for file in baseline.files() {
        let live = std::path::Path::new(file.path().as_str());
        let expected = file.sha256().ok_or_else(|| {
            crate::failed(format!(
                "orphaned component baseline has no hash for {}",
                live.display()
            ))
        })?;
        let sidecar =
            crate::fs::backup_path(live).map_err(|error| crate::failed(error.to_string()))?;
        if !scope.contains_reachable(&sidecar) {
            return Err(crate::failed(format!(
                "orphaned component sidecar is outside the game root: {}",
                sidecar.display()
            )));
        }
        if sidecar.exists() {
            crate::fs::verify_sidecar(&sidecar, expected)
                .map_err(|error| crate::failed(error.to_string()))?;
        } else if !live.exists() {
            return Err(crate::failed(format!(
                "orphaned component baseline bytes are missing for {}",
                live.display()
            )));
        } else if renderpilot_detection::sha256_file(live)? != *expected {
            return Err(crate::failed(format!(
                "orphaned component baseline sidecar is missing for {}",
                live.display()
            )));
        }
    }
    Ok(())
}

pub(crate) fn orphaned_rollback_affected_files(
    baseline: &ComponentRollbackBaseline,
) -> Vec<PathRef> {
    let mut paths = rollback_mutation_paths(baseline.expected_active_files(), baseline.files())
        .into_iter()
        .filter_map(|path| PathRef::new(path.to_string_lossy().into_owned()).ok())
        .collect::<Vec<_>>();
    if let Some(executable) = baseline.d3d12_executable() {
        paths.push(executable.executable_path().clone());
        if let Ok(sidecar) =
            crate::fs::backup_path(std::path::Path::new(executable.executable_path().as_str()))
            && let Ok(sidecar) = PathRef::new(sidecar.to_string_lossy().into_owned())
        {
            paths.push(sidecar);
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

fn rollback_orphaned_component_locked(
    context: &crate::Context,
    guard: &crate::game_mutation_lock::GameMutationGuard,
    game_id: &GameId,
    component_id: &ComponentId,
) -> Result<(), ServiceError> {
    let storage = context.storage();
    let prepared = prepare_orphaned_rollback(context, guard, game_id, component_id)?;
    let baseline = prepared.rollback_baseline.files().to_vec();
    let current = prepared.rollback_baseline.expected_active_files().to_vec();
    let game_root = std::path::Path::new(prepared.game.install_path().as_str());
    let scope = crate::file_mutation::MutationScope::single(game_root)?;
    let mut mutation_paths = rollback_mutation_paths(&current, &baseline);
    if let Some(state) = prepared.d3d12_state.as_ref() {
        mutation_paths.push(state.executable_path.clone());
        mutation_paths.push(state.backup_path.clone());
    }
    let mut executable_guard = prepared
        .d3d12_state
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
        |mutation_id| -> AppResult<()> {
            let result = (|| -> AppResult<()> {
                restore_baseline_preserving_sidecars(&current, &baseline)?;
                if let (Some(guard), Some(state)) =
                    (executable_guard.as_mut(), prepared.d3d12_state.as_ref())
                {
                    guard.restore_for_rollback(state)?;
                    guard.release_backup_lock();
                }
                release_baseline_sidecars(&current, &baseline)?;
                if let Some(state) = prepared.d3d12_state.as_ref() {
                    mutation_guard::release_rollback_backup(state)?;
                }

                let rolled_back_paths = current
                    .iter()
                    .chain(&baseline)
                    .map(|file| file.path().clone())
                    .collect::<Vec<_>>();
                let coordinated_addon = storage
                    .get_installed_addon(game_id)?
                    .map(|record| {
                        crate::coordinated_files::record_after_paths_rollback(
                            &record,
                            &rolled_back_paths,
                        )
                    })
                    .transpose()?
                    .flatten();
                let addon_mutation = coordinated_addon
                    .as_ref()
                    .map_or(InstalledAddonMutation::Keep, InstalledAddonMutation::Upsert);
                let baseline_mutations = [ComponentBaselineMutation::Delete { component_id }];
                storage.commit_game_mutation(GameMutationCommit {
                    game_id,
                    component_set: None,
                    baseline_mutations: &baseline_mutations,
                    addon: addon_mutation,
                    mutation_id: Some(mutation_id),
                })
            })();
            drop(executable_guard.take());
            result
        },
        |_| {},
        || {},
    )
}

/// Consumes a duplicate rollback aggregate after an equivalent inverse action
/// already restored the shared files. No filesystem mutation occurs here: all
/// original bytes and released sidecars are revalidated before the metadata is
/// deleted.
pub(crate) fn release_redundant_component_baseline_locked(
    context: &crate::Context,
    guard: &crate::game_mutation_lock::GameMutationGuard,
    game_id: &GameId,
    component_id: &ComponentId,
) -> Result<(), ServiceError> {
    if guard.game_id() != game_id {
        return Err(ServiceError::invalid_input(
            "redundant baseline guard does not match the requested game",
        ));
    }
    let storage = context.storage();
    let game = storage.require_game(game_id)?;
    let baseline = storage.get_component_backup(component_id)?.ok_or_else(|| {
        ServiceError::invalid_input(format!(
            "redundant rollback baseline no longer exists for {}",
            component_id.as_str()
        ))
    })?;
    let scope = crate::file_mutation::MutationScope::single(std::path::Path::new(
        game.install_path().as_str(),
    ))?;

    for file in baseline.files() {
        let path = std::path::Path::new(file.path().as_str());
        if !scope.contains_reachable(path) {
            return Err(crate::failed(format!(
                "redundant baseline path is outside the game root: {}",
                path.display()
            )));
        }
        let expected = file.sha256().ok_or_else(|| {
            crate::failed(format!(
                "redundant baseline has no hash for {}",
                path.display()
            ))
        })?;
        if renderpilot_detection::sha256_file(path)? != *expected {
            return Err(crate::failed(format!(
                "shared rollback did not restore the expected bytes at {}",
                path.display()
            )));
        }
        let sidecar =
            crate::fs::backup_path(path).map_err(|error| crate::failed(error.to_string()))?;
        if sidecar.exists() {
            return Err(crate::failed(format!(
                "shared rollback did not release the baseline sidecar {}",
                sidecar.display()
            )));
        }
    }

    for active in baseline.expected_active_files() {
        let path = std::path::Path::new(active.path().as_str());
        if !scope.contains_reachable(path) {
            return Err(crate::failed(format!(
                "redundant active path is outside the game root: {}",
                path.display()
            )));
        }
        if baseline.files().iter().any(|original| {
            crate::paths::same_path(std::path::Path::new(original.path().as_str()), path)
        }) {
            continue;
        }
        if path.exists() {
            return Err(crate::failed(format!(
                "shared rollback did not remove overlay-added file {}",
                path.display()
            )));
        }
    }

    if let Some(executable) = baseline.d3d12_executable() {
        let path = std::path::Path::new(executable.executable_path().as_str());
        if !scope.contains_reachable(path) {
            return Err(crate::failed(format!(
                "redundant executable path is outside the game root: {}",
                path.display()
            )));
        }
        if renderpilot_detection::sha256_file(path)? != *executable.original().sha256() {
            return Err(crate::failed(format!(
                "shared rollback did not restore the expected executable bytes at {}",
                path.display()
            )));
        }
        let sidecar =
            crate::fs::backup_path(path).map_err(|error| crate::failed(error.to_string()))?;
        if sidecar.exists() {
            return Err(crate::failed(format!(
                "shared rollback did not release the executable sidecar {}",
                sidecar.display()
            )));
        }
    }

    if storage.get_installed_addon(game_id)?.is_some_and(|addon| {
        addon.managed_files().iter().any(|managed| {
            baseline
                .files()
                .iter()
                .chain(baseline.expected_active_files())
                .any(|file| {
                    crate::paths::same_path(
                        std::path::Path::new(file.path().as_str()),
                        std::path::Path::new(managed.path().as_str()),
                    )
                })
        })
    }) {
        return Err(crate::failed(
            "shared rollback left an add-on ownership binding on the redundant baseline",
        ));
    }

    let baseline_mutations = [ComponentBaselineMutation::Delete { component_id }];
    storage.commit_game_mutation(GameMutationCommit {
        game_id,
        component_set: None,
        baseline_mutations: &baseline_mutations,
        addon: InstalledAddonMutation::Keep,
        mutation_id: None,
    })?;
    Ok(())
}
