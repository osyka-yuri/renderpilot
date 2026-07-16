//! Swap execution: apply an artifact overlay and roll it back.

use renderpilot_application::{
    AppError, AppErrorKind, AppResult, ArtifactRepository, GameRepository,
    InstalledAddonRepository, OperationKind,
};
use renderpilot_domain::{ArtifactId, ComponentId, GameId, component_version_report, fsr};
use renderpilot_storage_sqlite::{
    ComponentBaselineInsert, GameMutationCommit, InstalledAddonMutation, SqliteStorage,
};

use crate::ServiceError;
use crate::catalog::swap::require_component_for_game;

mod fs_ops;
mod journal;
mod planning;
mod prepare;
mod source_integrity;
mod streamline_install;
mod types;

#[cfg(test)]
mod tests;

pub use self::types::{OperationMetadata, RollbackResult, SwapResult};

use self::fs_ops::perform_apply_fs;
pub(crate) use self::fs_ops::revert_to_baseline_fs;
pub(crate) use self::journal::{
    JournalEntryItem, JournalEntryParams, record_operation_journal_entry,
};
use self::planning::rebuild_component_set_after_overlay;
use self::prepare::{load_apply_swap, prepare_apply_swap};
use self::source_integrity::{
    ArtifactSourceCheck, rebind_planned_files_from_disk, validate_artifact_sources,
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
    let guard = crate::game_mutation_lock::enter_game_mutation_boundary(context, game_id)?;
    let storage = context.storage();
    let game = storage.require_game(game_id)?;
    let game_root = std::path::Path::new(game.install_path().as_str());
    let loaded = load_apply_swap(storage, game_id, component_id, artifact_id)?;
    ensure_artifact_sources_usable(storage, &loaded.artifact)?;
    let mut prepared = prepare_apply_swap(game_id, component_id, loaded)?;
    let scope = crate::file_mutation::MutationScope::single(game_root)?;
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
            perform_apply_fs(
                &prepared.component,
                &prepared.baseline,
                &prepared.planned,
                &prepared.removed,
            )?;

            if let Err(error) = rebind_planned_files_from_disk(&mut prepared.planned) {
                if matches!(error.kind(), AppErrorKind::StaleReplacementSource) {
                    invalidate_stale_artifact(
                        storage,
                        prepared.artifact.id(),
                        "installed target hash mismatch",
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

            let baseline_inserts = prepared
                .first_swap
                .then_some(ComponentBaselineInsert {
                    component_id: &prepared.component_id,
                    files: &prepared.baseline,
                })
                .into_iter()
                .collect::<Vec<_>>();
            storage.commit_game_mutation(GameMutationCommit {
                game_id: &prepared.game_id,
                component_set: Some(&next_components),
                baseline_inserts: &baseline_inserts,
                baseline_deletes: &[],
                addon: InstalledAddonMutation::Keep,
                mutation_id: Some(mutation_id),
            })?;

            record_operation_journal_entry(
                storage,
                JournalEntryParams {
                    game_id: &prepared.game_id,
                    component_id: &prepared.component_id,
                    kind: OperationKind::ReplaceComponent,
                    component: &prepared.component,
                    to_version: to_version.as_deref(),
                    items: prepared
                        .planned
                        .iter()
                        .map(|plan| JournalEntryItem {
                            path: plan.file.path(),
                            artifact_id: Some(prepared.artifact.id().clone()),
                        })
                        .collect(),
                },
            );

            Ok(SwapResult {
                game_id: prepared.game_id.as_str().to_owned(),
                component_id: prepared.component_id.as_str().to_owned(),
                applied_path: prepared.applied_path(),
                replacement_path: prepared.replacement_path(),
            })
        },
        |_| {},
        || {},
    )
}

/// Rolls a component back to its recorded baseline.
pub fn rollback_component(
    context: &crate::Context,
    game_id: &GameId,
    component_id: &ComponentId,
) -> Result<RollbackResult, ServiceError> {
    let guard = crate::game_mutation_lock::enter_game_mutation_boundary(context, game_id)?;
    let storage = context.storage();
    let game = storage.require_game(game_id)?;
    let game_root = std::path::Path::new(game.install_path().as_str());
    let component = require_component_for_game(storage, game_id, component_id)?;

    let Some(baseline) = storage.get_component_backup(component_id)? else {
        return Err(AppError::invalid_input(format!(
            "no swap to roll back for component {}",
            component_id.as_str()
        ))
        .into());
    };

    let managed_files =
        crate::coordinated_files::managed_files_of(storage.get_installed_addon(game_id)?.as_ref())
            .to_vec();
    let component =
        crate::coordinated_files::current_component_snapshot(&component, &managed_files)
            .map_err(|error| {
                AppError::invalid_input(format!(
                    "component {} changed on disk since it was scanned: {error}",
                    component_id.as_str()
                ))
            })?
            .into_component();
    let baseline = crate::coordinated_files::resolve_component_baseline(
        game_root,
        component.files(),
        Some(&baseline),
        &managed_files,
    )
    .map_err(|error| {
        AppError::invalid_input(format!(
            "cannot validate rollback baseline for component {}: {error}",
            component_id.as_str()
        ))
    })?;

    let restored_path = baseline
        .first()
        .map(|file| file.path().as_str().to_owned())
        .unwrap_or_default();

    let mut restored_files = baseline.clone();
    fsr::sort_representative_first(&mut restored_files);
    let rebuilt = component.rebuild_with_files(restored_files);
    let next_components = planning::full_component_set(storage, game_id, rebuilt)?;
    let scope = crate::file_mutation::MutationScope::single(game_root)?;
    crate::file_mutation::run_durable_mutation(
        crate::file_mutation::DurableMutation {
            context,
            guard: &guard,
            scope: &scope,
            feature: crate::addons::mutation_features::CATALOG_ROLLBACK,
            subject_id: Some(component_id.as_str()),
            paths: rollback_mutation_paths(component.files(), &baseline),
        },
        |mutation_id| -> AppResult<RollbackResult> {
            revert_to_baseline_fs(component.files(), &baseline)?;

            let coordinated_addon =
                addon_after_catalog_rollback(storage, game_id, &component, &baseline)?;
            let addon_mutation = coordinated_addon
                .as_ref()
                .map_or(InstalledAddonMutation::Keep, InstalledAddonMutation::Upsert);
            storage.commit_game_mutation(GameMutationCommit {
                game_id,
                component_set: Some(&next_components),
                baseline_inserts: &[],
                baseline_deletes: std::slice::from_ref(component_id),
                addon: addon_mutation,
                mutation_id: Some(mutation_id),
            })?;

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
                    items: baseline
                        .iter()
                        .map(|file| JournalEntryItem {
                            path: file.path(),
                            artifact_id: None,
                        })
                        .collect(),
                },
            );

            Ok(RollbackResult {
                game_id: game_id.as_str().to_owned(),
                component_id: component_id.as_str().to_owned(),
                restored_path,
            })
        },
        |_| {},
        || {},
    )
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
    apply_mutation_paths_set(
        prepared.component.files(),
        &prepared.baseline,
        &prepared.planned,
        &prepared.removed,
    )
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

/// Performs read-only source validation and owns best-effort stale-row recovery.
///
/// The planning module never mutates the catalog. Once apply proves that a
/// source is unusable, this boundary removes the stale row so a details reload
/// can offer a healthy downloaded or re-scanned candidate.
fn ensure_artifact_sources_usable(
    storage: &SqliteStorage,
    artifact: &renderpilot_domain::LibraryArtifact,
) -> AppResult<()> {
    match validate_artifact_sources(artifact)? {
        ArtifactSourceCheck::Ok => Ok(()),
        ArtifactSourceCheck::Unusable(issue) => {
            invalidate_stale_artifact(storage, artifact.id(), &issue.to_string());
            Err(AppError::stale_replacement_source())
        }
    }
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
