//! Complete partitioning of current, baseline, and candidate transition paths.

use std::collections::{BTreeMap, BTreeSet};

use renderpilot_domain::{
    ComponentFile, LibraryArtifact, LibraryComponent, LibraryTechnology, normalized_path_key, xiph,
};

use crate::{AppError, AppResult};

use super::{
    model::{
        ArchiveMode, ExternalAliasRequirements, ResolvedArchiveAndRemove, ResolvedPathDisposition,
        ResolvedRemove, ResolvedTransition, ResolvedUntouchedBaseline, ResolvedWrite,
        ResolvedXiphTransition,
    },
    paths::{TransitionWrite, file_map, insert_transition_write, join_target},
    projection::{
        resolve_transition_install_target, resolve_transition_members, resolve_transition_removals,
    },
    xiph::{runtime_name, xiph_member_for_component_file, xiph_transition_writes},
};

/// Resolves one complete component transition without filesystem access.
///
/// `baseline` must be the immutable original files captured before the first
/// managed mutation. `external_aliases` is a proof result from orchestration,
/// never an inference performed here.
pub fn resolve_transition(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
    baseline: &[ComponentFile],
    external_aliases: &ExternalAliasRequirements,
) -> AppResult<ResolvedTransition> {
    if component.technology() != artifact.technology() {
        return Err(AppError::invalid_input(
            "component and artifact technologies do not match",
        ));
    }

    let primary = component.files().first().ok_or_else(|| {
        AppError::invalid_input("component does not contain a primary transition target")
    })?;
    let target_directory = primary
        .path()
        .parent()
        .ok_or_else(|| AppError::invalid_input("component target has no parent directory"))?
        .to_owned();

    let xiph_component = component.technology() == LibraryTechnology::XiphVorbis;
    let current_by_path = file_map(
        component.files(),
        &target_directory,
        "component",
        xiph_component,
    )?;
    let baseline_by_path = file_map(baseline, &target_directory, "baseline", xiph_component)?;
    let (writes, xiph) =
        transition_writes(component, artifact, external_aliases, &target_directory)?;

    let mut all_paths = BTreeSet::new();
    all_paths.extend(current_by_path.keys().map(String::as_str));
    all_paths.extend(baseline_by_path.keys().map(String::as_str));
    all_paths.extend(writes.keys().map(String::as_str));

    let mut explicit_removals = resolve_transition_removals(
        baseline,
        artifact,
        writes
            .values()
            .map(|write| write.target.file_name().unwrap_or_default()),
    )
    .into_iter()
    .map(|file| normalized_path_key(file.path().as_str()))
    .collect::<BTreeSet<_>>();
    if xiph.is_some() {
        // Only aliases the external importer proof selected remain live. Every
        // other vendor member is replaced by its canonical candidate target
        // and therefore becomes a rollback-reserved original.
        for file in component.files() {
            let is_vendor = runtime_name(file)
                .ok()
                .and_then(|name| xiph::parse_runtime_file_name(name).ok().flatten())
                .is_some_and(|parsed| parsed.is_vendor());
            if !is_vendor {
                continue;
            }
            let key = normalized_path_key(file.path().as_str());
            if !writes.contains_key(&key) {
                explicit_removals.insert(key);
            }
        }
    }

    let mut paths = Vec::with_capacity(all_paths.len());
    for key in all_paths {
        let current_file = current_by_path.get(key).copied();
        let baseline_file = baseline_by_path.get(key).copied();
        if let Some(write) = writes.get(key) {
            paths.push(ResolvedPathDisposition::Write(ResolvedWrite {
                target: write.target.clone(),
                source: write.source.clone(),
                current: current_file.cloned(),
                baseline: baseline_file.cloned(),
                member: write.member,
            }));
            continue;
        }

        let member = if xiph_component {
            current_file
                .or(baseline_file)
                .and_then(xiph_member_for_component_file)
        } else {
            None
        };
        match (current_file, baseline_file) {
            (Some(current), Some(baseline)) if explicit_removals.contains(key) => {
                paths.push(ResolvedPathDisposition::ArchiveAndRemove(
                    ResolvedArchiveAndRemove {
                        target: current.path().clone(),
                        baseline: baseline.clone(),
                        current: Some(current.clone()),
                        mode: ArchiveMode::Create,
                        member,
                    },
                ));
            }
            (Some(current), Some(baseline)) => {
                paths.push(ResolvedPathDisposition::UntouchedBaseline(
                    ResolvedUntouchedBaseline {
                        target: current.path().clone(),
                        baseline: baseline.clone(),
                        current: current.clone(),
                        member,
                    },
                ));
            }
            (None, Some(baseline)) => {
                paths.push(ResolvedPathDisposition::ArchiveAndRemove(
                    ResolvedArchiveAndRemove {
                        target: baseline.path().clone(),
                        baseline: baseline.clone(),
                        current: None,
                        mode: ArchiveMode::RequireOwnedArchive,
                        member,
                    },
                ));
            }
            (Some(current), None) => {
                paths.push(ResolvedPathDisposition::Remove(ResolvedRemove {
                    target: current.path().clone(),
                    current: current.clone(),
                    member,
                }));
            }
            (None, None) => {
                return Err(AppError::invalid_input(
                    "transition path partition was incomplete",
                ));
            }
        }
    }

    let primary_target = xiph_component
        .then_some(primary)
        .and_then(xiph_member_for_component_file)
        .and_then(|primary_member| {
            writes
                .values()
                .find(|write| write.member == Some(primary_member))
                .map(|write| write.target.clone())
        })
        .or_else(|| {
            writes
                .get(&normalized_path_key(primary.path().as_str()))
                .map(|write| write.target.clone())
        })
        .or_else(|| {
            paths.iter().find_map(|path| match path {
                ResolvedPathDisposition::Write(write) => Some(write.target.clone()),
                ResolvedPathDisposition::ArchiveAndRemove(_)
                | ResolvedPathDisposition::Remove(_)
                | ResolvedPathDisposition::UntouchedBaseline(_) => None,
            })
        })
        .ok_or_else(|| AppError::invalid_input("transition has no resolved write target"))?;

    Ok(ResolvedTransition {
        component_id: component.id().clone(),
        artifact_id: artifact.id().clone(),
        target_directory,
        primary_target,
        paths,
        xiph,
    })
}

fn transition_writes(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
    external_aliases: &ExternalAliasRequirements,
    target_directory: &str,
) -> AppResult<(
    BTreeMap<String, TransitionWrite>,
    Option<ResolvedXiphTransition>,
)> {
    if component.technology() == LibraryTechnology::XiphVorbis {
        crate::validate_runtime_artifact(artifact)
            .map_err(|error| AppError::invalid_input(error.to_string()))?;
        crate::compatibility::ensure_transition_compatible_with_external_aliases(
            component,
            artifact,
            external_aliases,
        )
        .map_err(|error| AppError::invalid_input(error.to_string()))?;
        return xiph_transition_writes(component, artifact, external_aliases);
    }

    let members = resolve_transition_members(component, artifact)?;
    let mut writes = BTreeMap::new();
    for source in members {
        let target_name = resolve_transition_install_target(component, source);
        let target = join_target(target_directory, &target_name)?;
        insert_transition_write(
            &mut writes,
            TransitionWrite {
                target,
                source: source.clone(),
                member: None,
            },
        )?;
    }
    Ok((writes, None))
}
