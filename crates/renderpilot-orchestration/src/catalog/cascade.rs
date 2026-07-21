//! Compound catalog rollback driven by disappearing owned managed paths.
//!
//! Feature-neutral: any tool that records owned [`ManagedAddonFile`] paths can
//! request whole-component restore when those paths leave the game tree.
//! Tool-specific composition (which owned paths disappear) stays in the tool.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use renderpilot_application::{
    AppError, AppResult, ComponentRepository, InstalledAddonRepository, OperationKind,
};
use renderpilot_domain::{
    ComponentFile, ComponentId, GameId, GraphicsComponent, component_version_report, fsr,
};
use renderpilot_storage_sqlite::SqliteStorage;

use crate::catalog::execute::{
    JournalEntryItem, JournalEntryParams, ROLLBACK_TARGET_LABEL, record_operation_journal_entry,
    revert_to_baseline_fs,
};

/// Full catalog component rollback selected by an owned managed-file intersection.
pub(crate) struct ValidatedRollbackPlan {
    component: GraphicsComponent,
    baseline: Vec<ComponentFile>,
}

/// Named result of [`cascade_for_owned_paths`].
pub(crate) struct CascadeResult {
    pub(crate) rollback_specs: Vec<ValidatedRollbackPlan>,
    pub(crate) next_components: Vec<GraphicsComponent>,
    pub(crate) mutation_paths: Vec<PathBuf>,
}

impl ValidatedRollbackPlan {
    pub(crate) fn component_id(&self) -> &ComponentId {
        self.component.id()
    }

    pub(crate) fn contains_path(&self, path: &Path) -> bool {
        let path = crate::paths::normalized_key(path);
        self.component
            .files()
            .iter()
            .chain(&self.baseline)
            .any(|file| crate::paths::normalized_key(Path::new(file.path().as_str())) == path)
    }
}

/// Selects whole component bundles when any active member intersects an owned path.
pub(crate) fn cascade_rollback_specs(
    storage: &SqliteStorage,
    game_id: &GameId,
    owned_paths: &[PathBuf],
) -> AppResult<Vec<ValidatedRollbackPlan>> {
    if owned_paths.is_empty() {
        return Ok(Vec::new());
    }
    let game_root = crate::catalog::game_root_for_mutation(
        storage,
        game_id,
        owned_paths
            .first()
            .and_then(|path| path.parent().map(Path::to_path_buf)),
    )?;
    let managed_files =
        crate::coordinated_files::managed_files_of(storage.get_installed_addon(game_id)?.as_ref())
            .to_vec();
    let owned: HashSet<String> = owned_paths
        .iter()
        .map(|path| crate::paths::normalized_key(path))
        .collect();
    let mut specs = Vec::new();
    for component in storage.list_components_for_game(game_id)? {
        let Some(baseline) =
            crate::coordinated_files::load_component_backup_availability(storage, &component)?
                .into_available()
        else {
            continue;
        };
        let intersects = component
            .files()
            .iter()
            .chain(&baseline)
            .map(|file| crate::paths::normalized_key(Path::new(file.path().as_str())))
            .any(|path| owned.contains(&path));
        if intersects {
            let component =
                crate::coordinated_files::current_component_snapshot(&component, &managed_files)
                    .map_err(|error| {
                        AppError::invalid_input(format!(
                            "cannot validate active component {} for cascade rollback: {error}",
                            component.id().as_str()
                        ))
                    })?
                    .into_component();
            let baseline = crate::coordinated_files::resolve_component_baseline(
                &game_root,
                component.files(),
                Some(&baseline),
                &managed_files,
            )
            .map_err(|error| {
                AppError::invalid_input(format!(
                    "cannot validate baseline for cascade rollback {}: {error}",
                    component.id().as_str()
                ))
            })?;
            specs.push(ValidatedRollbackPlan {
                component,
                baseline,
            });
        }
    }
    Ok(specs)
}

pub(crate) fn cascade_next_components(
    storage: &SqliteStorage,
    game_id: &GameId,
    specs: &[ValidatedRollbackPlan],
) -> AppResult<Vec<GraphicsComponent>> {
    let mut components = storage.list_components_for_game(game_id)?;
    for spec in specs {
        if spec.baseline.is_empty() {
            components.retain(|component| component.id() != spec.component.id());
            continue;
        }
        let mut restored = spec.baseline.clone();
        fsr::sort_representative_first(&mut restored);
        let rebuilt = spec.component.rebuild_with_files(restored);
        if let Some(component) = components
            .iter_mut()
            .find(|component| component.id() == rebuilt.id())
        {
            *component = rebuilt;
        }
    }
    Ok(components)
}

pub(crate) fn cascade_mutation_paths(specs: &[ValidatedRollbackPlan]) -> Vec<PathBuf> {
    crate::catalog::execute::mutation_paths_from_component_files(
        specs
            .iter()
            .flat_map(|spec| spec.component.files().iter().chain(&spec.baseline)),
    )
}

/// Plans cascade rollback for any owned managed paths that are about to leave
/// the game tree: validated specs, the post-cascade component set, and
/// live/sidecar mutation paths.
///
/// Path selection is the caller's responsibility:
/// - full uninstall → all [`crate::addons::records::owned_managed_paths`]
/// - Luma update when payload drops DLSS →
///   [`crate::addons::luma::dlss::cascade_for_disappearing_owned`]
/// - mutation-path snapshotting may intentionally use a wider owned set than
///   the apply-time cascade plan
pub(crate) fn cascade_for_owned_paths(
    storage: &SqliteStorage,
    game_id: &GameId,
    owned_paths: &[PathBuf],
) -> AppResult<CascadeResult> {
    let rollback_specs = cascade_rollback_specs(storage, game_id, owned_paths)?;
    let next_components = cascade_next_components(storage, game_id, &rollback_specs)?;
    let mutation_paths = cascade_mutation_paths(&rollback_specs);
    Ok(CascadeResult {
        rollback_specs,
        next_components,
        mutation_paths,
    })
}

pub(crate) fn apply_cascade_rollback_fs(specs: &[ValidatedRollbackPlan]) -> AppResult<()> {
    for spec in specs {
        revert_to_baseline_fs(spec.component.files(), &spec.baseline)?;
    }
    Ok(())
}

pub(crate) fn record_cascade_rollback_journal(
    storage: &SqliteStorage,
    game_id: &GameId,
    specs: &[ValidatedRollbackPlan],
) {
    for spec in specs {
        // Owned so the version string outlives the temporary version report.
        let to_version = cascade_rollback_to_version(spec);
        record_operation_journal_entry(
            storage,
            JournalEntryParams {
                game_id,
                component_id: spec.component.id(),
                kind: OperationKind::RollbackComponent,
                component: &spec.component,
                to_version: Some(to_version.as_str()),
                items: cascade_rollback_journal_items(spec),
            },
        );
    }
}

fn cascade_rollback_to_version(spec: &ValidatedRollbackPlan) -> String {
    component_version_report(&spec.baseline, spec.component.technology())
        .known_version()
        .map(|version| version.as_str().to_owned())
        .unwrap_or_else(|| ROLLBACK_TARGET_LABEL.to_owned())
}

fn cascade_rollback_journal_items(spec: &ValidatedRollbackPlan) -> Vec<JournalEntryItem<'_>> {
    spec.baseline
        .iter()
        .map(|file| JournalEntryItem {
            path: file.path(),
            artifact_id: None,
        })
        .collect()
}
