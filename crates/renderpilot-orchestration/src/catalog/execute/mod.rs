//! Swap execution: apply an artifact overlay and roll it back.

use renderpilot_application::{
    AppError, AppErrorKind, AppResult, ArtifactRepository, GameRepository,
    InstalledAddonRepository, OperationKind,
};
use renderpilot_domain::{
    ArtifactId, ComponentId, ComponentRollbackBaseline, D3d12ExecutableBaseline,
    D3d12ExecutableIdentity, GameId, PathRef, component_version_report, fsr,
};
use renderpilot_storage_sqlite::{
    ComponentBaselineMutation, GameMutationCommit, InstalledAddonMutation, SqliteStorage,
};

use crate::ServiceError;
use crate::catalog::swap::require_component_for_game;

mod fs_ops;
mod journal;
mod mutation_guard;
mod planning;
mod prepare;
mod source_integrity;
#[cfg(test)]
mod test_hooks;
mod types;

#[cfg(test)]
mod tests;

pub use self::types::{
    D3d12ExecutableActionResult, D3d12ExecutableActionResultKind, OperationMetadata, RollbackPlan,
    RollbackResult, SwapResult,
};

pub(crate) use self::fs_ops::revert_to_baseline_fs;
use self::fs_ops::{
    perform_apply_fs, release_baseline_sidecars, restore_baseline_preserving_sidecars,
};
pub(crate) use self::journal::{
    JournalEntryItem, JournalEntryParams, component_file_item_count,
    journal_item_is_component_file, record_operation_journal_entry,
};
use self::planning::rebuild_component_set_after_overlay;
use self::prepare::prepare_apply_swap;
use self::source_integrity::rebind_planned_files_for_technology;
#[cfg(test)]
use self::test_hooks::{
    D3d12ApplyFailurePoint, D3d12RollbackFailurePoint, inject_d3d12_apply_failure,
    inject_d3d12_rollback_failure, run_before_copy_hook, set_before_copy_hook,
    set_d3d12_apply_failure_point, set_d3d12_rollback_failure_point,
};

/// Label written to the journal when rolling back to the pre-swap baseline.
pub(crate) const ROLLBACK_TARGET_LABEL: &str = "Original";

/// Installs an artifact package over a component as an **additive overlay**.
pub fn apply_swap(
    context: &crate::Context,
    game_id: &GameId,
    component_id: &ComponentId,
    artifact_id: &ArtifactId,
) -> Result<SwapResult, ServiceError> {
    apply_swap_confirmed(context, game_id, component_id, artifact_id, None)
}

/// Installs an artifact and validates a fresh token when the planned EXE action
/// requires explicit user confirmation.
pub fn apply_swap_confirmed(
    context: &crate::Context,
    game_id: &GameId,
    component_id: &ComponentId,
    artifact_id: &ArtifactId,
    confirmation_token: Option<&str>,
) -> Result<SwapResult, ServiceError> {
    let guard = crate::game_mutation_lock::enter_game_mutation_boundary(context, game_id)?;
    let storage = context.storage();
    let preflight = match super::swap::load_swap_preflight(
        context,
        game_id,
        component_id,
        artifact_id,
        super::swap::SwapPreflightMode::Apply {
            confirmation_supplied: confirmation_token.is_some(),
        },
    )? {
        super::swap::SwapPreflight::Ready(preflight) => *preflight,
        super::swap::SwapPreflight::UnusableSource { artifact_id, issue } => {
            invalidate_stale_artifact(storage, &artifact_id, &issue.to_string());
            return Err(AppError::stale_replacement_source().into());
        }
    };
    if confirmation_token.is_some()
        && preflight
            .operation_plan
            .d3d12_executable_action()
            .is_some_and(|action| {
                action.kind() == renderpilot_application::D3d12ExecutableActionKind::RepairRequired
            })
    {
        return Err(AppError::confirmation_token_mismatch().into());
    }
    let game_root = std::path::PathBuf::from(preflight.game.install_path().as_str());
    let mut prepared = prepare_apply_swap(game_id, component_id, preflight)?;
    validate_executable_confirmation(&prepared, confirmation_token)?;
    let mut executable_guard = prepared
        .d3d12
        .as_ref()
        .filter(|d3d12| d3d12.action.changes_executable())
        .map(|d3d12| mutation_guard::D3d12ExecutableMutationGuard::acquire(&d3d12.state))
        .transpose()?;
    let scope = crate::file_mutation::MutationScope::single(&game_root)?;
    crate::file_mutation::run_durable_mutation(
        crate::file_mutation::DurableMutation {
            context,
            guard: &guard,
            scope: &scope,
            feature: crate::addons::mutation_features::CATALOG_SWAP,
            subject_id: Some(component_id.as_str()),
            paths: apply_mutation_paths(&prepared),
        },
        |mutation_id| -> AppResult<SwapResult> {
            let result = (|| -> AppResult<SwapResult> {
                #[cfg(test)]
                run_before_copy_hook();

                if let (Some(guard), Some(d3d12)) =
                    (executable_guard.as_mut(), prepared.d3d12.as_ref())
                {
                    guard.ensure_backup(&d3d12.state, &d3d12.action)?;
                }
                #[cfg(test)]
                inject_d3d12_apply_failure(D3d12ApplyFailurePoint::AfterExecutableBackup)?;

                perform_apply_fs(
                    &prepared.component,
                    &prepared.baseline,
                    &prepared.planned,
                    &prepared.removed,
                )?;
                #[cfg(test)]
                inject_d3d12_apply_failure(D3d12ApplyFailurePoint::AfterDllMutation)?;

                let active_executable_sha256 =
                    match (executable_guard.as_mut(), prepared.d3d12.as_ref()) {
                        (Some(guard), Some(d3d12)) => {
                            Some(guard.apply_action(&d3d12.state, &d3d12.action)?)
                        }
                        _ => None,
                    };
                #[cfg(test)]
                inject_d3d12_apply_failure(D3d12ApplyFailurePoint::AfterExecutableMutation)?;

                if let Err(error) = rebind_planned_files_for_technology(
                    &mut prepared.planned,
                    prepared.component.technology(),
                ) {
                    if matches!(error.kind(), AppErrorKind::StaleReplacementSource) {
                        invalidate_stale_artifact(
                            storage,
                            prepared.artifact.id(),
                            "installed target content or PE metadata mismatch",
                        );
                    }
                    return Err(error);
                }

                let (next_components, to_version) = rebuild_component_set_after_overlay(
                    storage,
                    &prepared.game_id,
                    &prepared.component,
                    &prepared.component_id,
                    &prepared.baseline,
                    &prepared.planned,
                    &prepared.removed,
                )?;

                let executable_baseline =
                    build_executable_baseline(&prepared, active_executable_sha256.as_ref())?;
                let rollback_baseline = match executable_baseline.clone() {
                    Some(executable) => ComponentRollbackBaseline::new(prepared.baseline.clone())
                        .with_d3d12_executable(executable),
                    None => ComponentRollbackBaseline::new(prepared.baseline.clone()),
                };
                let expected_active = expected_active_executable_identity(
                    &prepared,
                    active_executable_sha256.as_ref(),
                );
                let baseline_mutations = if prepared.first_swap {
                    vec![ComponentBaselineMutation::Capture {
                        component_id: &prepared.component_id,
                        baseline: &rollback_baseline,
                    }]
                } else if prepared
                    .rollback_baseline
                    .as_ref()
                    .is_some_and(|baseline| baseline.d3d12_executable().is_none())
                    && executable_baseline.is_some()
                {
                    vec![ComponentBaselineMutation::CaptureD3d12Executable {
                        component_id: &prepared.component_id,
                        baseline: executable_baseline
                            .as_ref()
                            .expect("checked executable baseline"),
                    }]
                } else if let Some(expected_active) = expected_active.as_ref() {
                    vec![ComponentBaselineMutation::UpdateD3d12ExecutableState {
                        component_id: &prepared.component_id,
                        expected_active,
                    }]
                } else {
                    Vec::new()
                };
                #[cfg(test)]
                inject_d3d12_apply_failure(D3d12ApplyFailurePoint::BeforeDatabaseCommit)?;
                storage.commit_game_mutation(GameMutationCommit {
                    game_id: &prepared.game_id,
                    component_set: Some(&next_components),
                    baseline_mutations: &baseline_mutations,
                    addon: InstalledAddonMutation::Keep,
                    mutation_id: Some(mutation_id),
                })?;

                let d3d12_executable_action = prepared.d3d12.as_ref().and_then(|d3d12| {
                    types::D3d12ExecutableActionResult::from_action(&d3d12.action)
                });
                let mut journal_items = prepared
                    .planned
                    .iter()
                    .map(|plan| {
                        JournalEntryItem::component_file(
                            plan.file.path(),
                            Some(prepared.artifact.id().clone()),
                        )
                    })
                    .collect::<Vec<_>>();
                if let Some(action) = prepared
                    .d3d12
                    .as_ref()
                    .map(|d3d12| &d3d12.action)
                    .filter(|action| action.changes_executable())
                {
                    journal_items.push(JournalEntryItem::d3d12_executable(action));
                }
                record_operation_journal_entry(
                    storage,
                    JournalEntryParams {
                        game_id: &prepared.game_id,
                        component_id: &prepared.component_id,
                        kind: OperationKind::ReplaceComponent,
                        component: &prepared.component,
                        to_version: to_version.as_deref(),
                        items: journal_items,
                        d3d12_executable_action: d3d12_executable_action.clone(),
                    },
                );

                Ok(SwapResult {
                    game_id: prepared.game_id.as_str().to_owned(),
                    component_id: prepared.component_id.as_str().to_owned(),
                    applied_path: prepared.applied_path(),
                    replacement_path: prepared.replacement_path(),
                    updated_file_count: prepared.planned.len(),
                    d3d12_executable_action,
                })
            })();
            // The transaction may need to restore the snapshotted EXE on error.
            // Release our deny-write handle before control reaches that rollback.
            drop(executable_guard.take());
            result
        },
        |_| {},
        || {},
    )
}

fn validate_executable_confirmation(
    prepared: &types::PreparedApplySwap,
    provided: Option<&str>,
) -> AppResult<()> {
    let Some(d3d12) = prepared.d3d12.as_ref() else {
        return Ok(());
    };
    if d3d12.action.requires_confirmation() && Some(d3d12.confirmation_token.as_str()) != provided {
        return Err(AppError::confirmation_token_mismatch());
    }
    Ok(())
}

fn build_executable_baseline(
    prepared: &types::PreparedApplySwap,
    active_sha256: Option<&renderpilot_domain::Sha256Hash>,
) -> AppResult<Option<D3d12ExecutableBaseline>> {
    let (Some(d3d12), Some(active_sha256)) = (prepared.d3d12.as_ref(), active_sha256) else {
        return Ok(None);
    };
    let state = &d3d12.state;
    let action = &d3d12.action;
    if !action.changes_executable() {
        return Ok(None);
    }
    let executable_path = PathRef::new(state.executable_path.to_string_lossy().into_owned())
        .map_err(|error| AppError::invalid_input(error.to_string()))?;
    Ok(Some(D3d12ExecutableBaseline::new(
        executable_path,
        D3d12ExecutableIdentity::new(state.original_sdk_version, state.original_sha256.clone()),
        D3d12ExecutableIdentity::new(action.target_sdk_version(), active_sha256.clone()),
    )))
}

fn expected_active_executable_identity(
    prepared: &types::PreparedApplySwap,
    active_sha256: Option<&renderpilot_domain::Sha256Hash>,
) -> Option<D3d12ExecutableIdentity> {
    let (Some(recorded), Some(d3d12), Some(active_sha256)) = (
        prepared.rollback_baseline.as_ref(),
        prepared.d3d12.as_ref(),
        active_sha256,
    ) else {
        return None;
    };
    if !d3d12.action.changes_executable() || recorded.d3d12_executable().is_none() {
        return None;
    }
    Some(D3d12ExecutableIdentity::new(
        d3d12.action.target_sdk_version(),
        active_sha256.clone(),
    ))
}

/// Rolls a component back to its recorded baseline.
pub fn rollback_component(
    context: &crate::Context,
    game_id: &GameId,
    component_id: &ComponentId,
) -> Result<RollbackResult, ServiceError> {
    let guard = crate::game_mutation_lock::enter_game_mutation_boundary(context, game_id)?;
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
            guard: &guard,
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
    let _guard = crate::game_mutation_lock::blocking_lock(game_id);
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
    component: renderpilot_domain::GraphicsComponent,
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

fn rollback_executable_assessment(
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

/// Computes the single canonical set of filesystem paths that an apply will
/// touch (live files + their `.bak` sidecars), expanded with sidecars.
///
/// This is the **only** path-set computation for a catalog apply. It feeds
/// `DurableFileTransaction::prepare` for snapshotting/validation. The FS executor
/// (`perform_apply_fs`) does not recompute or re-validate this set — it relies
/// on the transaction manifest for durability and validation, eliminating the
/// double-validation that an independent `capture` pass would introduce.
fn apply_mutation_paths_set(
    current: &[renderpilot_domain::ComponentFile],
    baseline: &[renderpilot_domain::ComponentFile],
    planned: &[self::types::PlannedFile],
    removed: &[renderpilot_domain::ComponentFile],
) -> Vec<std::path::PathBuf> {
    let mut live: Vec<std::path::PathBuf> = current
        .iter()
        .chain(baseline)
        .chain(removed)
        .map(|file| std::path::PathBuf::from(file.path().as_str()))
        .collect();
    live.extend(planned.iter().map(|plan| plan.target()));
    crate::fs::expand_with_sidecars(live)
}

fn apply_mutation_paths(prepared: &types::PreparedApplySwap) -> Vec<std::path::PathBuf> {
    let mut paths = apply_mutation_paths_set(
        prepared.component.files(),
        &prepared.baseline,
        &prepared.planned,
        &prepared.removed,
    );
    if let Some(d3d12) = prepared
        .d3d12
        .as_ref()
        .filter(|d3d12| d3d12.action.changes_executable())
    {
        let state = &d3d12.state;
        paths.push(state.executable_path.clone());
        // An existing immutable backup is opened read-only and never changed by
        // apply. Snapshot it only on the first patch, when the transaction may
        // create the sidecar and must be able to remove it during recovery.
        if !state.backup_exists {
            paths.push(state.backup_path.clone());
        }
    }
    paths
}

fn rollback_mutation_paths(
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
    component: &renderpilot_domain::GraphicsComponent,
    baseline: &[renderpilot_domain::ComponentFile],
) -> AppResult<Option<renderpilot_domain::InstalledAddon>> {
    let Some(record) = storage.get_installed_addon(game_id)? else {
        return Ok(None);
    };
    crate::coordinated_files::record_after_component_rollback(&record, component, baseline)
}

fn invalidate_stale_artifact(storage: &SqliteStorage, artifact_id: &ArtifactId, reason: &str) {
    if let Err(error) = storage.delete_artifact(artifact_id) {
        log::warn!(
            "failed to invalidate stale artifact {} ({reason}): {error}",
            artifact_id.as_str()
        );
    } else {
        log::info!(
            "invalidated stale artifact {} ({reason})",
            artifact_id.as_str()
        );
    }
}
