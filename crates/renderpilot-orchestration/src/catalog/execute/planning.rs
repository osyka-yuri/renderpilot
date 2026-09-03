//! Pure planning for swap apply: baseline / artifact / component inspection.
//!
//! Filesystem integrity checks and post-install PE rebind live in
//! [`super::source_integrity`].

#[cfg(test)]
use std::path::{Path, PathBuf};

#[cfg(test)]
use renderpilot_application::AppError;
use renderpilot_application::{AppResult, ComponentRepository, ResolvedTransition};
#[cfg(test)]
use renderpilot_domain::{ComponentFile, LibraryArtifact, PathRef};
use renderpilot_domain::{ComponentId, GameId, LibraryComponent, component_version_report, fsr};
use renderpilot_storage_sqlite::SqliteStorage;

use super::types::TransitionWrite;

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
#[cfg(test)]
pub(super) fn fsr_members_to_remove(
    baseline: &[ComponentFile],
    artifact: &LibraryArtifact,
    planned: &[TransitionWrite],
) -> Vec<ComponentFile> {
    let planned_names: Vec<&str> = planned
        .iter()
        .filter_map(|plan| plan.file.path().file_name())
        .collect();
    renderpilot_application::resolve_transition_removals(
        baseline,
        artifact,
        planned_names.iter().copied(),
    )
    .into_iter()
    .cloned()
    .collect()
}

#[cfg(test)]
pub(super) fn planned_target_files(
    artifact: &LibraryArtifact,
    target_dir: &Path,
    component: &LibraryComponent,
) -> AppResult<Vec<TransitionWrite>> {
    let artifact_files = renderpilot_application::resolve_transition_members(component, artifact)?;

    let planned: AppResult<Vec<TransitionWrite>> = artifact_files
        .iter()
        .map(|artifact_file| {
            let install_name = renderpilot_application::resolve_transition_install_target(
                component,
                artifact_file,
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
            if let Some(profile) = artifact_file.pe_compatibility() {
                file = file.with_pe_compatibility(profile.clone());
            }

            Ok(TransitionWrite {
                source: PathBuf::from(artifact_file.path().as_str()),
                file,
            })
        })
        .collect();

    planned
}

/// Returns the game's full component set with `rebuilt` substituted in. An
/// empty rebuilt component means its pre-overlay path was absent and therefore
/// removes the component rather than persisting a file-less catalog entry.
///
/// `replace_components_for_game` rewrites the entire set, so the swap must pass
/// every sibling component too; otherwise applying one swap would wipe the rest
/// of the game's components until the next full rescan.
pub(super) fn full_component_set(
    storage: &SqliteStorage,
    game_id: &GameId,
    rebuilt: LibraryComponent,
) -> AppResult<Vec<LibraryComponent>> {
    let mut components = storage.list_components_for_game(game_id)?;

    if rebuilt.files().is_empty() {
        components.retain(|component| component.id() != rebuilt.id());
    } else if let Some(component) = components.iter_mut().find(|c| c.id() == rebuilt.id()) {
        *component = rebuilt;
    } else {
        components.push(rebuilt);
    }

    Ok(components)
}

/// Rebuilds the persisted active component strictly from the resolved path
/// partition.  Rebound write metadata replaces only the matching write rows;
/// archive/remove rows can never be resurrected by a filesystem planner.
pub(super) fn rebuild_component_set_after_transition(
    storage: &SqliteStorage,
    game_id: &GameId,
    component: &LibraryComponent,
    component_id: &ComponentId,
    transition: &ResolvedTransition,
    rebound_writes: &[TransitionWrite],
) -> AppResult<(Vec<LibraryComponent>, Option<String>)> {
    let mut files = transition.expected_active();
    for file in &mut files {
        if let Some(rebound) = rebound_writes.iter().find(|write| {
            crate::paths::same_path(
                std::path::Path::new(write.file.path().as_str()),
                std::path::Path::new(file.path().as_str()),
            )
        }) {
            *file = rebound.file.clone();
        }
    }
    fsr::sort_representative_first(&mut files);
    let rebuilt = component.rebuild_with_files(files);
    let next_components = full_component_set(storage, game_id, rebuilt)?;
    let applied_files = next_components
        .iter()
        .find(|entry| entry.id() == component_id)
        .map(|entry| entry.files())
        .unwrap_or(&[]);
    let to_version = component_version_report(applied_files, component.technology())
        .known_version()
        .map(|version| version.as_str().to_owned());
    Ok((next_components, to_version))
}
