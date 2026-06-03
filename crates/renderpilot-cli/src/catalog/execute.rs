use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use renderpilot_application::{
    build_swap_operation_plan, fsr, AppError, AppResult, ComponentRepository,
};
use renderpilot_domain::{
    ArtifactId, ComponentFile, ComponentId, GameId, GraphicsComponent, LibraryArtifact, PathRef,
};
use renderpilot_storage_sqlite::SqliteStorage;
use serde::Serialize;

use crate::{
    catalog::{
        self,
        swap::{require_artifact, require_component_for_game, require_game},
    },
    error::CliError,
};

#[derive(Debug, Serialize)]
pub(crate) struct SwapResult {
    pub(crate) game_id: String,
    pub(crate) component_id: String,
    pub(crate) applied_path: String,
    pub(crate) replacement_path: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct RollbackResult {
    pub(crate) game_id: String,
    pub(crate) component_id: String,
    pub(crate) restored_path: String,
}

struct LoadedApplySwap {
    component: GraphicsComponent,
    artifact: LibraryArtifact,
    baseline: Vec<ComponentFile>,
    first_swap: bool,
}

struct PreparedApplySwap {
    game_id: GameId,
    component_id: ComponentId,
    component: GraphicsComponent,
    artifact: LibraryArtifact,
    baseline: Vec<ComponentFile>,
    planned: Vec<PlannedFile>,
    next_components: Vec<GraphicsComponent>,
    first_swap: bool,
}

impl PreparedApplySwap {
    fn applied_path(&self) -> String {
        self.planned
            .first()
            .map(|plan| plan.file.path().as_str().to_owned())
            .unwrap_or_default()
    }

    fn replacement_path(&self) -> String {
        self.artifact.path().as_str().to_owned()
    }
}

pub(crate) fn build_swap_plan(
    game_id: GameId,
    component_id: ComponentId,
    artifact_id: ArtifactId,
) -> Result<renderpilot_application::OperationPlan, CliError> {
    Ok(catalog::build_swap_plan(game_id, component_id, artifact_id)?.plan)
}

/// Installs an artifact package over a component as an **additive overlay**.
///
/// Each artifact file is placed at its install target (its `install_as` name, or
/// its own name). A file already at a target is backed up to a `.bak` sidecar and
/// overwritten (replace); a target with nothing there is simply added. The
/// component's other files are left untouched — so e.g. the FSR 4 upgrade installs
/// the loader as `amd_fidelityfx_dx12.dll`, adds the upscaler + framegen, and never
/// removes the game's other DLLs.
///
/// All filesystem work happens first and is unwound on any failure; the database
/// (the new active set, plus the original baseline on the first swap) is committed
/// last, in one transaction, so the catalog and disk never diverge.
pub(crate) fn apply_swap(
    game_id: GameId,
    component_id: ComponentId,
    artifact_id: ArtifactId,
) -> Result<SwapResult, CliError> {
    catalog::with_catalog_storage(|storage| {
        let prepared = prepare_apply_swap(storage, &game_id, &component_id, &artifact_id)?;

        // --- filesystem work (unwound on failure) ---
        let changes = perform_apply_fs(
            &prepared.component,
            &prepared.baseline,
            &prepared.planned,
            prepared.first_swap,
        )?;

        // --- persist; unwind the filesystem if the commit fails ---
        let commit = if prepared.first_swap {
            storage.commit_bundle_apply(
                &prepared.game_id,
                &prepared.next_components,
                Some((&prepared.component_id, &prepared.baseline)),
            )
        } else {
            storage.commit_bundle_apply(&prepared.game_id, &prepared.next_components, None)
        };
        if let Err(error) = commit {
            changes.undo();
            return Err(error.into());
        }

        Ok(SwapResult {
            game_id: prepared.game_id.as_str().to_owned(),
            component_id: prepared.component_id.as_str().to_owned(),
            applied_path: prepared.applied_path(),
            replacement_path: prepared.replacement_path(),
        })
    })
}

/// Rolls a component back to its recorded baseline: restore every replaced
/// original from its `.bak`, delete the files the swap added, leave the rest. The
/// DB read happens before any mutation and the baseline row is cleared in the same
/// transaction that restores the component set.
pub(crate) fn rollback_component(
    game_id: GameId,
    component_id: ComponentId,
) -> Result<RollbackResult, CliError> {
    catalog::with_catalog_storage(|storage| {
        require_game(storage, &game_id)?;
        let component = require_component_for_game(storage, &game_id, &component_id)?;

        let Some(baseline) = storage.get_component_backup(&component_id)? else {
            return Err(AppError::invalid_input(format!(
                "no swap to roll back for component {}",
                component_id.as_str()
            ))
            .into());
        };

        let restored_path = baseline
            .first()
            .map(|file| file.path().as_str().to_owned())
            .unwrap_or_default();

        let rebuilt = rebuild_component(&component, baseline.clone());
        let next_components = full_component_set(storage, &game_id, rebuilt)?;

        revert_to_baseline_fs(component.files(), &baseline)?;

        storage.commit_bundle_rollback(&game_id, &next_components, &component_id)?;

        Ok(RollbackResult {
            game_id: game_id.as_str().to_owned(),
            component_id: component_id.as_str().to_owned(),
            restored_path,
        })
    })
}

// --- private helpers ---

fn prepare_apply_swap(
    storage: &SqliteStorage,
    game_id: &GameId,
    component_id: &ComponentId,
    artifact_id: &ArtifactId,
) -> AppResult<PreparedApplySwap> {
    let LoadedApplySwap {
        component,
        artifact,
        baseline,
        first_swap,
    } = load_apply_swap(storage, game_id, component_id, artifact_id)?;

    validate_artifact_sources_exist(&artifact)?;
    validate_apply_is_allowed(&component, &artifact)?;

    let target_dir = resolve_target_dir(&component)?;
    let planned = planned_target_files(&artifact, &target_dir, &component)?;

    // New active set = baseline files not overwritten by the package (kept) +

    fn validate_apply_is_allowed(
        component: &GraphicsComponent,
        artifact: &LibraryArtifact,
    ) -> AppResult<()> {
        let plan = build_swap_operation_plan(component, artifact)?;

        if plan.blockers().is_empty() {
            return Ok(());
        }

        let blockers = plan
            .blockers()
            .iter()
            .map(|blocker| blocker.as_str())
            .collect::<Vec<_>>()
            .join(", ");

        Err(AppError::invalid_input(format!(
            "cannot apply blocked swap: {blockers}"
        )))
    }
    // the package's installed files. Computed before any FS/DB mutation.
    let new_files = additive_active_files(&baseline, &planned);
    let rebuilt = rebuild_component(&component, new_files);
    let next_components = full_component_set(storage, game_id, rebuilt)?;

    Ok(PreparedApplySwap {
        game_id: game_id.clone(),
        component_id: component_id.clone(),
        component,
        artifact,
        baseline,
        planned,
        next_components,
        first_swap,
    })
}

fn load_apply_swap(
    storage: &SqliteStorage,
    game_id: &GameId,
    component_id: &ComponentId,
    artifact_id: &ArtifactId,
) -> AppResult<LoadedApplySwap> {
    require_game(storage, game_id)?;
    let component = require_component_for_game(storage, game_id, component_id)?;
    let artifact = require_artifact(storage, artifact_id)?;

    let recorded_baseline = storage.get_component_backup(component_id)?;
    let first_swap = recorded_baseline.is_none();
    // The baseline is the *original* file set: the recorded one on a re-swap,
    // or the current files on the very first swap.
    let baseline = recorded_baseline.unwrap_or_else(|| component.files().to_vec());

    Ok(LoadedApplySwap {
        component,
        artifact,
        baseline,
        first_swap,
    })
}

/// One artifact file resolved to where it will be installed.
struct PlannedFile {
    /// Source artifact file on disk to copy from.
    source: PathBuf,
    /// The component file the install target becomes (its path is the install
    /// target; `install_as` is cleared because it is now in place).
    file: ComponentFile,
}

impl PlannedFile {
    fn target(&self) -> PathBuf {
        PathBuf::from(self.file.path().as_str())
    }

    fn target_bak(&self) -> PathBuf {
        PathBuf::from(format!("{}.bak", self.file.path().as_str()))
    }
}

/// Records the filesystem mutations of an overlay so they can be undone on failure.
#[derive(Default)]
struct AppliedFsChanges {
    /// Files renamed to `.bak` (target, bak) before being overwritten.
    renamed_to_bak: Vec<(PathBuf, PathBuf)>,
    /// Files copied into place.
    copied: Vec<PathBuf>,
}

impl AppliedFsChanges {
    /// Best-effort reversal of the overlay: remove copies, restore backups.
    ///
    /// A re-swap's revert-to-baseline (run before the overlay) is intentionally
    /// not tracked here — the recorded baseline `.bak` files remain intact, so a
    /// later `rollback` always recovers the original.
    fn undo(&self) {
        for copied in &self.copied {
            let _ = fs::remove_file(copied);
        }
        for (target, bak) in &self.renamed_to_bak {
            let _ = fs::rename(bak, target);
        }
    }
}

fn perform_apply_fs(
    component: &GraphicsComponent,
    baseline: &[ComponentFile],
    planned: &[PlannedFile],
    first_swap: bool,
) -> AppResult<AppliedFsChanges> {
    let mut changes = AppliedFsChanges::default();
    let result = apply_fs_steps(component, baseline, planned, first_swap, &mut changes);
    if let Err(error) = result {
        changes.undo();
        return Err(error);
    }
    Ok(changes)
}

fn apply_fs_steps(
    component: &GraphicsComponent,
    baseline: &[ComponentFile],
    planned: &[PlannedFile],
    first_swap: bool,
    changes: &mut AppliedFsChanges,
) -> AppResult<()> {
    // On a re-swap, revert the current overlay back to the recorded baseline first
    // so the new package installs cleanly — no mixed versions, no stale leftovers.
    if !first_swap {
        revert_to_baseline_fs(component.files(), baseline)?;
    }

    // Overlay the package: back up + replace any file already at a target, add the
    // rest. Untouched files stay where they are.
    for plan in planned {
        let target = plan.target();
        if target.exists() {
            let bak = plan.target_bak();
            // Drop any stale leftover backup so `.bak` holds the current original.
            if bak.exists() {
                fs::remove_file(&bak).map_err(|error| {
                    AppError::provider_failed(format!(
                        "failed to clear stale backup {}: {error}",
                        bak.display()
                    ))
                })?;
            }
            fs::rename(&target, &bak).map_err(|error| {
                AppError::provider_failed(format!(
                    "failed to back up {} before replacing it: {error}",
                    target.display()
                ))
            })?;
            changes.renamed_to_bak.push((target.clone(), bak));
        }

        fs::copy(&plan.source, &target).map_err(|error| {
            AppError::provider_failed(format!(
                "failed to install file to {}: {error}",
                target.display()
            ))
        })?;
        changes.copied.push(target);
    }

    Ok(())
}

/// Reverts the directory to `baseline`: delete files the overlay added (current
/// files whose path is not a baseline path) and restore each baseline file that
/// has a `.bak`. Used by both rollback and the re-swap path. Retry-safe: it only
/// touches files that still have a `.bak`, so re-running after a partial failure
/// completes cleanly.
fn revert_to_baseline_fs(current: &[ComponentFile], baseline: &[ComponentFile]) -> AppResult<()> {
    let baseline_paths: HashSet<&str> = baseline.iter().map(|f| f.path().as_str()).collect();

    // 1. Delete files the swap added (not part of the baseline).
    for file in current {
        if !baseline_paths.contains(file.path().as_str()) {
            let path = real_path(file);
            if path.exists() {
                fs::remove_file(&path).map_err(|error| {
                    AppError::provider_failed(format!(
                        "failed to remove added file {}: {error}",
                        path.display()
                    ))
                })?;
            }
        }
    }

    // 2. Restore each baseline file that was replaced (has a `.bak`). Files that
    //    were merely kept have no `.bak` and are left as-is.
    for file in baseline {
        let real = real_path(file);
        let bak = bak_path(file);
        if !bak.exists() {
            continue;
        }
        if real.exists() {
            fs::remove_file(&real).map_err(|error| {
                AppError::provider_failed(format!(
                    "failed to clear {} before restore: {error}",
                    real.display()
                ))
            })?;
        }
        fs::rename(&bak, &real).map_err(|error| {
            AppError::provider_failed(format!(
                "failed to restore backup to {}: {error}",
                real.display()
            ))
        })?;
    }

    Ok(())
}

/// New active component files after an additive overlay: baseline files that the
/// package does not overwrite (kept), plus the package's installed files.
fn additive_active_files(
    baseline: &[ComponentFile],
    planned: &[PlannedFile],
) -> Vec<ComponentFile> {
    let target_paths: HashSet<&str> = planned
        .iter()
        .map(|plan| plan.file.path().as_str())
        .collect();

    let mut files: Vec<ComponentFile> = baseline
        .iter()
        .filter(|file| !target_paths.contains(file.path().as_str()))
        .cloned()
        .collect();
    files.extend(planned.iter().map(|plan| plan.file.clone()));
    files
}

fn resolve_target_dir(component: &GraphicsComponent) -> AppResult<PathBuf> {
    let primary = component
        .files()
        .first()
        .ok_or_else(|| AppError::invalid_input("component has no files"))?;

    let parent = primary.path().parent().ok_or_else(|| {
        AppError::invalid_input(format!(
            "cannot determine target directory for {}",
            primary.path().as_str()
        ))
    })?;

    Ok(PathBuf::from(parent))
}

fn validate_artifact_sources_exist(artifact: &LibraryArtifact) -> AppResult<()> {
    for file in artifact.files() {
        let path = Path::new(file.path().as_str());
        if !path.exists() {
            return Err(AppError::invalid_input(format!(
                "artifact file does not exist: {}",
                file.path().as_str()
            )));
        }
    }
    Ok(())
}

fn planned_target_files(
    artifact: &LibraryArtifact,
    target_dir: &Path,
    component: &GraphicsComponent,
) -> AppResult<Vec<PlannedFile>> {
    let component_file_names: Vec<&str> = component
        .files()
        .iter()
        .filter_map(|file| file.path().file_name())
        .collect();

    artifact
        .files()
        .iter()
        .map(|artifact_file| {
            // Install under the file's `install_as` target (e.g. the FSR 4 loader
            // becomes `amd_fidelityfx_dx12.dll`), else under its own name. The FSR
            // loader's target then adapts to the game's real entry point — a natively
            // split game loads the loader as `amd_fidelityfx_loader_dx12.dll`.
            let default_name = artifact_file
                .install_as()
                .or_else(|| artifact_file.path().file_name())
                .unwrap_or("");
            let install_name = fsr::resolve_loader_install_target(
                default_name,
                component_file_names.iter().copied(),
            );
            let destination = target_dir.join(&install_name);
            let target_ref =
                PathRef::new(destination.to_string_lossy().as_ref()).map_err(|error| {
                    AppError::invalid_input(format!("invalid target path: {error}"))
                })?;

            let mut file = ComponentFile::new(target_ref);
            if let Some(sha256) = artifact_file.sha256() {
                file = file.with_sha256(sha256.clone());
            }
            if let Some(version) = artifact_file.version() {
                file = file.with_version(version.clone());
            }

            Ok(PlannedFile {
                source: PathBuf::from(artifact_file.path().as_str()),
                file,
            })
        })
        .collect()
}

fn rebuild_component(
    component: &GraphicsComponent,
    files: Vec<ComponentFile>,
) -> GraphicsComponent {
    let mut rebuilt = GraphicsComponent::new(
        component.id().clone(),
        component.game_id().clone(),
        component.kind(),
        component.technology(),
        component.swappability(),
    );
    for file in files {
        rebuilt = rebuilt.with_file(file);
    }
    rebuilt
}

/// Returns the game's full component set with `rebuilt` substituted in.
///
/// `replace_components_for_game` rewrites the entire set, so the swap must pass
/// every sibling component too; otherwise applying one swap would wipe the rest
/// of the game's components until the next full rescan.
fn full_component_set(
    storage: &SqliteStorage,
    game_id: &GameId,
    rebuilt: GraphicsComponent,
) -> AppResult<Vec<GraphicsComponent>> {
    let existing = storage.list_components_for_game(game_id)?;

    let mut next: Vec<GraphicsComponent> = Vec::with_capacity(existing.len().max(1));
    let mut replaced = false;
    for current in existing {
        if current.id() == rebuilt.id() {
            next.push(rebuilt.clone());
            replaced = true;
        } else {
            next.push(current);
        }
    }
    if !replaced {
        next.push(rebuilt);
    }
    Ok(next)
}

fn real_path(file: &ComponentFile) -> PathBuf {
    PathBuf::from(file.path().as_str())
}

fn bak_path(file: &ComponentFile) -> PathBuf {
    PathBuf::from(format!("{}.bak", file.path().as_str()))
}
