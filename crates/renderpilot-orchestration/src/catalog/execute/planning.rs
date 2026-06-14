//! Computes the new component file set and install plan before any mutation.
//!
//! Everything here is pure planning: it inspects the baseline, artifact and
//! component to decide what will be installed, what an FSR downgrade must drop,
//! and how the resulting component set looks — all without touching the
//! filesystem or the database.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use renderpilot_application::{AppError, AppResult, ComponentRepository};
use renderpilot_domain::{
    fsr, ComponentFile, GameId, GraphicsComponent, GraphicsTechnology, LibraryArtifact, PathRef,
};
use renderpilot_storage_sqlite::SqliteStorage;

use super::types::PlannedFile;

/// New active component files after an additive overlay: baseline files that the
/// package neither overwrites nor removes (kept), plus the package's installed files.
pub(super) fn additive_active_files(
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

/// FSR **upscaling-stack** members (upscaler, frame generation) the unified
/// target supersedes on a downgrade — to be removed so the folder ends on a
/// clean FSR 3.1, never a mix of upscaling releases.
///
/// Non-empty only when the artifact is a **unified** FSR backend (its primary is not
/// the split-marker upscaler) replacing a **dx12/vk-lineage** component (one that loads
/// `amd_fidelityfx_dx12.dll` or `amd_fidelityfx_vk.dll`) that still holds upscaling members. The RenderPilot
/// upgrade path already cleans up via revert-to-baseline; this also covers a folder
/// upgraded to FSR 4 outside RenderPilot, where there is no FSR 3.1 baseline.
///
/// Two deliberate boundaries:
/// * Only [`fsr::is_upscaling_member`] files are removed. A loader under its
///   own name and the optional effects (denoiser, radiance cache) form the
///   game's own effect stack (e.g. a loader+denoiser Ray Regeneration
///   pair) — an upscaling swap must leave them in place.
/// * Removals are computed from the **baseline**, not the live component: a
///   re-swap first reverts to the baseline, which restores baseline-owned
///   split members from their `.bak`s — computing from the already-cleaned
///   component would resurrect them. On a first swap the baseline IS the
///   component's current file set, so both views agree.
pub(super) fn fsr_members_to_remove(
    baseline: &[ComponentFile],
    artifact: &LibraryArtifact,
    planned: &[PlannedFile],
) -> Vec<ComponentFile> {
    let target_is_unified_fsr = artifact.technology().family() == GraphicsTechnology::AmdFsr
        && !fsr::is_split_marker(artifact.file_name());
    if !target_is_unified_fsr || !fsr::has_entry_point(baseline) {
        return Vec::new();
    }

    let planned_names: HashSet<String> = planned
        .iter()
        .filter_map(|plan| plan.file.path().file_name().map(str::to_ascii_lowercase))
        .collect();

    baseline
        .iter()
        .filter(|file| {
            file.path().file_name().is_some_and(|name| {
                fsr::is_upscaling_member(name)
                    && !planned_names.contains(&name.to_ascii_lowercase())
            })
        })
        .cloned()
        .collect()
}

pub(super) fn resolve_target_dir(component: &GraphicsComponent) -> AppResult<PathBuf> {
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

pub(super) fn validate_artifact_sources_exist(artifact: &LibraryArtifact) -> AppResult<()> {
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

pub(super) fn planned_target_files(
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

pub(super) fn rebuild_component(
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
pub(super) fn full_component_set(
    storage: &SqliteStorage,
    game_id: &GameId,
    rebuilt: GraphicsComponent,
) -> AppResult<Vec<GraphicsComponent>> {
    let mut components = storage.list_components_for_game(game_id)?;

    if let Some(component) = components.iter_mut().find(|c| c.id() == rebuilt.id()) {
        *component = rebuilt;
    } else {
        components.push(rebuilt);
    }

    Ok(components)
}
