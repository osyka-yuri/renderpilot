//! Swap execution: apply an artifact overlay and roll it back.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use renderpilot_application::{
    build_swap_operation_plan, fsr, AppError, AppResult, ComponentRepository, GameRepository,
};
use renderpilot_domain::{
    ArtifactId, ComponentFile, ComponentId, GameId, GraphicsComponent, GraphicsTechnology,
    LibraryArtifact, PathRef,
};
use renderpilot_storage_sqlite::SqliteStorage;
use serde::Serialize;

use crate::fs_sync;
use crate::ServiceError;

use crate::catalog::swap::{require_artifact, require_component_for_game, require_game};

const UNKNOWN_GAME_NAME: &str = "Unknown Game";
const UNKNOWN_VERSION: &str = "Unknown";
/// Label written to the journal when rolling back to the pre-swap baseline.
const ROLLBACK_TARGET_LABEL: &str = "Original";

/// Metadata recorded alongside a swap or rollback operation in the journal.
#[derive(Debug, Serialize)]
pub struct OperationMetadata {
    /// Human-readable game name at the time of the operation.
    pub game_name: String,
    /// Technology slug (e.g. `dlss`, `fsr`).
    pub library: String,
    /// Version string of the component before the operation, if known.
    pub from_version: Option<String>,
    /// Version string the component was swapped to.
    pub to_version: String,
}

/// Result of a successfully applied swap.
#[derive(Debug, Serialize)]
pub struct SwapResult {
    /// String form of the game id.
    pub game_id: String,
    /// String form of the component id.
    pub component_id: String,
    /// Install path of the primary applied file.
    pub applied_path: String,
    /// Source path of the artifact package that was installed.
    pub replacement_path: String,
}

/// Result of a successfully applied rollback.
#[derive(Debug, Serialize)]
pub struct RollbackResult {
    /// String form of the game id.
    pub game_id: String,
    /// String form of the component id.
    pub component_id: String,
    /// Path of the first restored baseline file.
    pub restored_path: String,
}

/// A single file affected by the operation.
struct JournalEntryItem<'a> {
    path: &'a PathRef,
    artifact_id: Option<ArtifactId>,
}

/// Parameters for recording a completed operation in the journal.
///
/// Passed as a single value to [`record_operation_journal_entry`] so that the
/// call sites remain readable without a 7-argument call.
struct JournalEntryParams<'a> {
    game_id: &'a GameId,
    component_id: &'a ComponentId,
    kind: renderpilot_application::OperationKind,
    component: &'a GraphicsComponent,
    /// The version the component is being swapped to.
    /// `None` falls back to [`UNKNOWN_VERSION`] in the stored metadata.
    to_version: Option<&'a str>,
    /// Files affected by the operation.
    items: Vec<JournalEntryItem<'a>>,
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
    /// FSR split members the (unified) target abandons and must delete — see
    /// [`fsr_members_to_remove`]. Empty for every non-downgrade swap.
    removed: Vec<ComponentFile>,
    next_components: Vec<GraphicsComponent>,
    first_swap: bool,
}

impl PreparedApplySwap {
    fn applied_path(&self) -> String {
        self.artifact
            .files()
            .iter()
            .zip(&self.planned)
            .find_map(|(artifact_file, plan)| {
                artifact_file
                    .install_as()
                    .map(|_| plan.file.path().as_str().to_owned())
            })
            .or_else(|| {
                self.planned
                    .first()
                    .map(|plan| plan.file.path().as_str().to_owned())
            })
            .unwrap_or_default()
    }

    fn replacement_path(&self) -> String {
        self.artifact.path().as_str().to_owned()
    }
}

/// Records a journal entry for the completed operation, best-effort.
///
/// Failures are logged as warnings and do **not** propagate — journal
/// persistence is informational and must never disrupt the main swap / rollback
/// flow.
fn record_operation_journal_entry(storage: &SqliteStorage, params: JournalEntryParams<'_>) {
    let JournalEntryParams {
        game_id,
        component_id,
        kind,
        component,
        to_version,
        items,
    } = params;

    let Ok(op_id) = renderpilot_domain::OperationId::new(ulid::Ulid::new().to_string()) else {
        log::warn!("Failed to generate operation id for journal");
        return;
    };
    let timestamp = renderpilot_application::UnixTimestampMillis::now()
        .unwrap_or_else(|_| renderpilot_application::UnixTimestampMillis::new(0).unwrap());

    let game_name = storage
        .find_game(game_id)
        .ok()
        .flatten()
        .map(|g| g.identity().title().to_string())
        .unwrap_or_else(|| UNKNOWN_GAME_NAME.to_owned());

    let metadata = OperationMetadata {
        game_name,
        library: component.technology().as_slug().to_string(),
        from_version: component
            .files()
            .first()
            .and_then(|f| f.version())
            .map(|v| v.to_string()),
        to_version: to_version.unwrap_or(UNKNOWN_VERSION).to_owned(),
    };
    let metadata_str = serde_json::to_string(&metadata).unwrap_or_else(|_| "{}".to_owned());
    let metadata_json =
        renderpilot_application::MetadataJson::new(metadata_str).unwrap_or_else(|_| {
            // SAFETY: a literal empty JSON object is always a valid MetadataJson.
            renderpilot_application::MetadataJson::new("{}")
                .expect("empty JSON object is always valid")
        });

    let operation_record = renderpilot_application::OperationRecord::new(
        op_id.clone(),
        game_id.clone(),
        kind,
        renderpilot_application::OperationStatus::Completed,
        timestamp,
    )
    .with_completed_at(timestamp)
    .with_metadata_json(metadata_json);

    let item_records: Vec<_> = items
        .into_iter()
        .map(|item| {
            let mut record = renderpilot_application::OperationItemRecord::new(
                op_id.clone(),
                component_id.clone(),
                item.path.clone(),
                renderpilot_application::OperationStatus::Completed,
            );
            if let Some(aid) = item.artifact_id {
                record = record.with_artifact_id(aid);
            }
            record
        })
        .collect();

    if let Ok(entry) =
        renderpilot_application::OperationJournalEntry::try_new(operation_record, item_records)
    {
        if let Err(e) =
            renderpilot_application::OperationRepository::save_operation_entry(storage, &entry)
        {
            log::warn!("Failed to save operation journal entry: {}", e);
        }
    }
}

/// Installs an artifact package over a component as an **additive overlay**.
pub fn apply_swap(
    context: &crate::Context,
    game_id: GameId,
    component_id: ComponentId,
    artifact_id: ArtifactId,
) -> Result<SwapResult, ServiceError> {
    let storage = context.storage();
    let prepared = prepare_apply_swap(storage, &game_id, &component_id, &artifact_id)?;

    let changes = perform_apply_fs(
        &prepared.component,
        &prepared.baseline,
        &prepared.planned,
        &prepared.removed,
        prepared.first_swap,
    )?;

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

    record_operation_journal_entry(
        storage,
        JournalEntryParams {
            game_id: &prepared.game_id,
            component_id: &prepared.component_id,
            kind: renderpilot_application::OperationKind::ReplaceComponent,
            component: &prepared.component,
            to_version: prepared.artifact.version().map(|v| v.as_str()),
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
}

/// Rolls a component back to its recorded baseline.
pub fn rollback_component(
    context: &crate::Context,
    game_id: GameId,
    component_id: ComponentId,
) -> Result<RollbackResult, ServiceError> {
    let storage = context.storage();
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

    record_operation_journal_entry(
        storage,
        JournalEntryParams {
            game_id: &game_id,
            component_id: &component_id,
            kind: renderpilot_application::OperationKind::RollbackComponent,
            component: &component,
            to_version: baseline
                .first()
                .and_then(|f| f.version())
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
    // the package's installed files, minus any FSR member a unified downgrade drops.
    // Computed before any FS/DB mutation.
    let removed = fsr_members_to_remove(&component, &artifact, &planned);
    let new_files = additive_active_files(&baseline, &planned, &removed);
    let rebuilt = rebuild_component(&component, new_files);
    let next_components = full_component_set(storage, game_id, rebuilt)?;

    Ok(PreparedApplySwap {
        game_id: game_id.clone(),
        component_id: component_id.clone(),
        component,
        artifact,
        baseline,
        planned,
        removed,
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
    ///
    /// Reversal is best-effort by necessity (the disk may be full, a file may be
    /// locked by anti-virus), but every failure is logged at error level: a swap
    /// whose rollback could not complete leaves the game folder in a mixed state,
    /// and that must be diagnosable rather than silently swallowed.
    fn undo(&self) {
        for copied in &self.copied {
            if let Err(error) = fs::remove_file(copied) {
                log::error!(
                    "swap rollback: failed to remove copied file {}: {error}",
                    copied.display()
                );
            }
        }
        for (target, bak) in &self.renamed_to_bak {
            if let Err(error) = fs::rename(bak, target) {
                log::error!(
                    "swap rollback: failed to restore backup {} to {}: {error}",
                    bak.display(),
                    target.display()
                );
            }
        }
    }
}

fn perform_apply_fs(
    component: &GraphicsComponent,
    baseline: &[ComponentFile],
    planned: &[PlannedFile],
    removed: &[ComponentFile],
    first_swap: bool,
) -> AppResult<AppliedFsChanges> {
    let mut changes = AppliedFsChanges::default();
    let result = apply_fs_steps(
        component,
        baseline,
        planned,
        removed,
        first_swap,
        &mut changes,
    );
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
    removed: &[ComponentFile],
    first_swap: bool,
    changes: &mut AppliedFsChanges,
) -> AppResult<()> {
    // On a re-swap, revert the current overlay back to the recorded baseline first
    // so the new package installs cleanly — no mixed versions, no stale leftovers.
    if !first_swap {
        revert_to_baseline_fs(component.files(), baseline)?;
    }

    // Downgrade cleanup: a unified FSR 3.x target drops the FSR 4 split members the
    // game held. Back each up to its `.bak` (so rollback can restore it) and remove
    // it, so the folder lands on a clean FSR 3.1 — never a mix. On a re-swap the
    // revert above already deleted them, so these are no-ops then.
    for file in removed {
        let target = real_path(file);
        if !target.exists() {
            continue;
        }
        let bak = bak_path(file);
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
                "failed to back up {} before removing it: {error}",
                target.display()
            ))
        })?;
        changes.renamed_to_bak.push((target, bak));
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
        // Track the copy before flushing so a `sync_file` failure still triggers
        // its removal during `undo()`.
        changes.copied.push(target.clone());
        // Flush the installed bytes to durable storage. A crash/power-loss after
        // a bare `fs::copy` can leave a torn or zero-length DLL on disk, which
        // the game would then load. Treat a flush failure as fatal so the swap
        // rolls back rather than committing an undurable file.
        fs_sync::sync_file(&target).map_err(|error| {
            AppError::provider_failed(format!(
                "failed to flush {} to disk: {error}",
                target.display()
            ))
        })?;
    }

    // Flush the directory entries (renames to `.bak`, new files) so the layout
    // is durable, not just the file contents above. Best-effort: the data is
    // already durable, and a directory-sync failure must not fail the swap.
    sync_touched_directories(changes);

    Ok(())
}

/// Fsyncs each distinct directory touched by the overlay so renames and new
/// files survive a crash. Best-effort; see [`fs_sync::sync_directory_best_effort`].
fn sync_touched_directories(changes: &AppliedFsChanges) {
    let mut synced: HashSet<PathBuf> = HashSet::new();
    let touched = changes
        .copied
        .iter()
        .chain(changes.renamed_to_bak.iter().map(|(target, _bak)| target));
    for path in touched {
        if let Some(parent) = path.parent() {
            if synced.insert(parent.to_path_buf()) {
                fs_sync::sync_directory_best_effort(parent);
            }
        }
    }
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

    // Flush the directory entries so the deletes/renames above survive a crash.
    // Without this, `rollback_component` could commit the rollback to SQLite while
    // the `.bak`→live rename never reaches disk, leaving the catalog claiming a
    // restore the filesystem lost. The restored content is the pre-existing `.bak`
    // (already durable), so a directory fsync is sufficient. Best-effort, mirroring
    // the apply path; on a re-swap the later overlay fsync of the same directory is
    // a cheap, idempotent repeat.
    sync_component_file_dirs(current.iter().chain(baseline));

    Ok(())
}

/// Fsyncs the distinct parent directories of `files` (best-effort), making the
/// deletes/renames performed against them durable.
fn sync_component_file_dirs<'a>(files: impl IntoIterator<Item = &'a ComponentFile>) {
    let mut synced: HashSet<PathBuf> = HashSet::new();
    for file in files {
        let path = real_path(file);
        if let Some(parent) = path.parent() {
            if synced.insert(parent.to_path_buf()) {
                fs_sync::sync_directory_best_effort(parent);
            }
        }
    }
}

/// New active component files after an additive overlay: baseline files that the
/// package neither overwrites nor removes (kept), plus the package's installed files.
fn additive_active_files(
    baseline: &[ComponentFile],
    planned: &[PlannedFile],
    removed: &[ComponentFile],
) -> Vec<ComponentFile> {
    let target_paths: HashSet<&str> = planned
        .iter()
        .map(|plan| plan.file.path().as_str())
        .collect();
    let removed_paths: HashSet<&str> = removed.iter().map(|file| file.path().as_str()).collect();

    let mut files: Vec<ComponentFile> = baseline
        .iter()
        .filter(|file| {
            !target_paths.contains(file.path().as_str())
                && !removed_paths.contains(file.path().as_str())
        })
        .cloned()
        .collect();
    files.extend(planned.iter().map(|plan| plan.file.clone()));
    files
}

/// FSR split members the unified target abandons on a downgrade — to be removed so
/// the folder ends on a clean FSR 3.1, never a mix.
///
/// Non-empty only when the artifact is a **unified** FSR backend (its primary is not
/// the split-marker upscaler) replacing a **dx12-lineage** component (one that loads
/// `amd_fidelityfx_dx12.dll`) that still holds FSR split members. The RenderPilot
/// upgrade path already cleans up via revert-to-baseline; this also covers a folder
/// upgraded to FSR 4 outside RenderPilot, where there is no FSR 3.1 baseline.
fn fsr_members_to_remove(
    component: &GraphicsComponent,
    artifact: &LibraryArtifact,
    planned: &[PlannedFile],
) -> Vec<ComponentFile> {
    let target_is_unified_fsr = artifact.technology().family() == GraphicsTechnology::AmdFsr
        && !fsr::is_split_marker(artifact.file_name());
    if !target_is_unified_fsr || !fsr::has_entry_point(component.files()) {
        return Vec::new();
    }

    let planned_names: HashSet<String> = planned
        .iter()
        .filter_map(|plan| plan.file.path().file_name().map(str::to_ascii_lowercase))
        .collect();

    component
        .files()
        .iter()
        .filter(|file| {
            file.path().file_name().is_some_and(|name| {
                fsr::is_split_member(name) && !planned_names.contains(&name.to_ascii_lowercase())
            })
        })
        .cloned()
        .collect()
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
    artifact
        .files()
        .iter()
        .map(|artifact_file| {
            let install_name =
                fsr::resolve_artifact_install_target(artifact_file, component.files());
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

#[cfg(test)]
mod tests {
    use super::*;
    use renderpilot_domain::{
        ArtifactTrustLevel, ComponentKind, PathRef, Sha256Hash, Swappability,
    };
    use std::fs;

    const HEX64: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn comp_file(path: &Path) -> ComponentFile {
        ComponentFile::new(PathRef::new(path.to_string_lossy().as_ref()).expect("valid path"))
    }

    fn comp_file_str(path: &str) -> ComponentFile {
        ComponentFile::new(PathRef::new(path).expect("valid path"))
    }

    fn bak_of(path: &Path) -> PathBuf {
        PathBuf::from(format!("{}.bak", path.display()))
    }

    fn write(path: &Path, bytes: &[u8]) {
        fs::write(path, bytes).expect("write fixture file");
    }

    fn planned_copy(source: &Path, target: &Path) -> PlannedFile {
        PlannedFile {
            source: source.to_path_buf(),
            file: comp_file(target),
        }
    }

    /// Minimal FSR component placeholder; `component` is only read on the
    /// re-swap (`first_swap == false`) revert path, so these tests pass it
    /// `first_swap = true` and never depend on its files.
    fn placeholder_component() -> GraphicsComponent {
        GraphicsComponent::new(
            ComponentId::new("component:test").expect("component id"),
            GameId::new("manual:C:/Games/Test").expect("game id"),
            ComponentKind::NativeLibrary,
            GraphicsTechnology::AmdFsr,
            Swappability::Swappable,
        )
    }

    #[test]
    fn overlay_backs_up_existing_target_and_installs_durably() {
        let dir = tempfile::tempdir().expect("temp dir");
        let target = dir.path().join("nvngx_dlss.dll");
        let source = dir.path().join("source.dll");
        write(&target, b"original");
        write(&source, b"new-version");

        let plans = vec![planned_copy(&source, &target)];
        let changes = perform_apply_fs(&placeholder_component(), &[], &plans, &[], true)
            .expect("apply should succeed");

        assert_eq!(fs::read(&target).expect("target readable"), b"new-version");
        assert_eq!(
            fs::read(bak_of(&target)).expect("bak readable"),
            b"original"
        );
        assert_eq!(changes.copied, vec![target.clone()]);
        assert_eq!(
            changes.renamed_to_bak,
            vec![(target.clone(), bak_of(&target))]
        );
    }

    #[test]
    fn overlay_adds_new_file_without_creating_backup() {
        let dir = tempfile::tempdir().expect("temp dir");
        let target = dir.path().join("amd_fidelityfx_upscaler_dx12.dll");
        let source = dir.path().join("source.dll");
        write(&source, b"fresh");

        let plans = vec![planned_copy(&source, &target)];
        let changes = perform_apply_fs(&placeholder_component(), &[], &plans, &[], true)
            .expect("apply should succeed");

        assert_eq!(fs::read(&target).expect("target readable"), b"fresh");
        assert!(
            !bak_of(&target).exists(),
            "no backup for a newly added file"
        );
        assert!(changes.renamed_to_bak.is_empty());
    }

    #[test]
    fn removed_member_is_backed_up_then_deleted() {
        let dir = tempfile::tempdir().expect("temp dir");
        let member = dir.path().join("amd_fidelityfx_framegeneration_dx12.dll");
        write(&member, b"fsr4-member");

        let removed = vec![comp_file(&member)];
        let changes = perform_apply_fs(&placeholder_component(), &[], &[], &removed, true)
            .expect("apply should succeed");

        assert!(!member.exists(), "removed member should be gone");
        assert_eq!(
            fs::read(bak_of(&member)).expect("bak readable"),
            b"fsr4-member",
            "removed member must be preserved as a .bak for rollback"
        );
        assert_eq!(
            changes.renamed_to_bak,
            vec![(member.clone(), bak_of(&member))]
        );
    }

    #[test]
    fn apply_failure_midway_rolls_back_every_change() {
        let dir = tempfile::tempdir().expect("temp dir");
        let target = dir.path().join("nvngx_dlss.dll");
        let good_source = dir.path().join("good.dll");
        let missing_source = dir.path().join("does-not-exist.dll");
        write(&target, b"original");
        write(&good_source, b"new-version");

        let plans = vec![
            planned_copy(&good_source, &target),
            planned_copy(&missing_source, &dir.path().join("second.dll")),
        ];
        let result = perform_apply_fs(&placeholder_component(), &[], &plans, &[], true);

        assert!(result.is_err(), "missing source must fail the apply");
        assert_eq!(
            fs::read(&target).expect("target readable"),
            b"original",
            "the first file must be restored to its original bytes"
        );
        assert!(
            !bak_of(&target).exists(),
            "backup must be consumed by the restore"
        );
        assert!(
            !dir.path().join("second.dll").exists(),
            "the failed file must not be left behind"
        );
    }

    #[test]
    fn revert_to_baseline_restores_backup_and_deletes_added_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        let replaced = dir.path().join("nvngx_dlss.dll");
        let added = dir.path().join("nvngx_dlssg.dll");
        write(&replaced, b"overlay");
        write(&bak_of(&replaced), b"original");
        write(&added, b"added-by-swap");

        let current = vec![comp_file(&replaced), comp_file(&added)];
        let baseline = vec![comp_file(&replaced)];
        revert_to_baseline_fs(&current, &baseline).expect("revert should succeed");

        assert_eq!(fs::read(&replaced).expect("readable"), b"original");
        assert!(!bak_of(&replaced).exists(), "backup consumed by restore");
        assert!(!added.exists(), "overlay-added file removed on revert");
    }

    #[test]
    fn undo_removes_copies_and_restores_backups() {
        let dir = tempfile::tempdir().expect("temp dir");
        let target = dir.path().join("nvngx_dlss.dll");
        write(&target, b"overlay");
        write(&bak_of(&target), b"original");

        let changes = AppliedFsChanges {
            renamed_to_bak: vec![(target.clone(), bak_of(&target))],
            copied: vec![target.clone()],
        };
        changes.undo();

        assert_eq!(fs::read(&target).expect("readable"), b"original");
        assert!(!bak_of(&target).exists());
    }

    #[test]
    fn fsr_downgrade_removes_unmatched_split_members() {
        let component = GraphicsComponent::new(
            ComponentId::new("component:fsr").expect("component id"),
            GameId::new("manual:C:/Games/Test").expect("game id"),
            ComponentKind::NativeLibrary,
            GraphicsTechnology::AmdFsr,
            Swappability::Swappable,
        )
        .with_file(comp_file_str("C:/game/amd_fidelityfx_dx12.dll"))
        .with_file(comp_file_str("C:/game/amd_fidelityfx_upscaler_dx12.dll"))
        .with_file(comp_file_str(
            "C:/game/amd_fidelityfx_framegeneration_dx12.dll",
        ));

        let artifact = LibraryArtifact::new(
            ArtifactId::new("artifact:fsr31").expect("artifact id"),
            GraphicsTechnology::AmdFsr,
            "amd_fidelityfx_dx12.dll",
            vec![comp_file_str("C:/lib/amd_fidelityfx_dx12.dll")
                .with_sha256(Sha256Hash::new(HEX64).expect("sha"))],
            ArtifactTrustLevel::ManifestDownloaded,
        )
        .expect("artifact");

        let planned = vec![planned_copy(
            Path::new("C:/lib/amd_fidelityfx_dx12.dll"),
            Path::new("C:/game/amd_fidelityfx_dx12.dll"),
        )];

        let removed = fsr_members_to_remove(&component, &artifact, &planned);
        let names: Vec<&str> = removed
            .iter()
            .filter_map(|file| file.path().file_name())
            .collect();
        assert_eq!(
            names,
            vec![
                "amd_fidelityfx_upscaler_dx12.dll",
                "amd_fidelityfx_framegeneration_dx12.dll",
            ],
            "a unified FSR 3.1 downgrade drops the split members it does not install"
        );
    }

    #[test]
    fn fsr_members_to_remove_is_empty_for_a_split_artifact() {
        let component = GraphicsComponent::new(
            ComponentId::new("component:fsr").expect("component id"),
            GameId::new("manual:C:/Games/Test").expect("game id"),
            ComponentKind::NativeLibrary,
            GraphicsTechnology::AmdFsr,
            Swappability::Swappable,
        )
        .with_file(comp_file_str("C:/game/amd_fidelityfx_dx12.dll"))
        .with_file(comp_file_str("C:/game/amd_fidelityfx_upscaler_dx12.dll"));

        // The artifact's primary file *is* the upscaler (split marker) → not a
        // unified downgrade, so nothing is removed.
        let artifact = LibraryArtifact::new(
            ArtifactId::new("artifact:fsr4").expect("artifact id"),
            GraphicsTechnology::AmdFsr,
            "amd_fidelityfx_upscaler_dx12.dll",
            vec![comp_file_str("C:/lib/amd_fidelityfx_upscaler_dx12.dll")
                .with_sha256(Sha256Hash::new(HEX64).expect("sha"))],
            ArtifactTrustLevel::ManifestDownloaded,
        )
        .expect("artifact");

        assert!(fsr_members_to_remove(&component, &artifact, &[]).is_empty());
    }
}
