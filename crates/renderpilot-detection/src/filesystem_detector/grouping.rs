//! Clusters detected library files into graphics components and locally-observed
//! artifact bundles, keyed by `(directory, grouping technology)`.

use std::collections::{HashMap, HashSet};

use renderpilot_application::AppResult;
use renderpilot_domain::{
    fsr, ArtifactId, ArtifactTrustLevel, ComponentFile, ComponentId, ComponentKind, GameId,
    GameInstallation, GraphicsComponent, GraphicsTechnology, LibraryArtifact, PathRef,
    Swappability,
};

use crate::error::detection_error;

use super::classification::component_file_from_detection;
use super::DetectedLibraryFile;

/// Groups detected library files into one [`GraphicsComponent`] per
/// `(directory, grouping technology)`.
///
/// Files normally group by technology family inside one directory. Native FSR 4
/// directories are the exception: when a directory contains an
/// [`GraphicsTechnology::AmdFsrLoader`] and no [`GraphicsTechnology::AmdFsr`]
/// entry point, each FSR DLL keeps its exact technology and becomes its own
/// single-file component.
pub fn group_into_components(
    game: &GameInstallation,
    libraries: &[DetectedLibraryFile],
) -> AppResult<Vec<GraphicsComponent>> {
    group_detected_files(libraries)
        .into_iter()
        .map(|group| build_grouped_component(game, &group))
        .collect()
}

/// Groups detected library files into one locally-observed [`LibraryArtifact`]
/// bundle per `(directory, grouping technology)`, mirroring
/// [`group_into_components`].
pub fn group_into_artifacts(
    game_id: &GameId,
    libraries: &[DetectedLibraryFile],
) -> AppResult<Vec<LibraryArtifact>> {
    group_detected_files(libraries)
        .into_iter()
        .map(|group| build_grouped_artifact(game_id, &group))
        .collect()
}

#[derive(Debug)]
struct GroupedDetectedFiles<'a> {
    technology: GraphicsTechnology,
    files: Vec<&'a DetectedLibraryFile>,
}

/// Partitions detected files into groups keyed by `(parent_dir, grouping
/// technology)`, preserving first-seen order so component/artifact ordering is
/// deterministic.
fn group_detected_files(libraries: &[DetectedLibraryFile]) -> Vec<GroupedDetectedFiles<'_>> {
    let native_fsr_directories = native_fsr_directories(libraries);
    let mut groups: Vec<GroupedDetectedFiles<'_>> = Vec::new();
    let mut index: HashMap<(String, &'static str), usize> = HashMap::new();

    for library in libraries {
        let parent_dir = parent_directory(library.file_path());
        let technology = grouping_technology(library, &parent_dir, &native_fsr_directories);
        let key = (parent_dir, technology.as_slug());

        if let Some(&existing) = index.get(&key) {
            groups[existing].files.push(library);
        } else {
            index.insert(key, groups.len());
            groups.push(GroupedDetectedFiles {
                technology,
                files: vec![library],
            });
        }
    }

    groups
}

fn build_grouped_component(
    game: &GameInstallation,
    group: &GroupedDetectedFiles<'_>,
) -> AppResult<GraphicsComponent> {
    let ordered = order_with_primary_first(&group.files);
    let parent_dir = parent_directory(ordered[0].file_path());
    let component_id = grouped_component_id(game, group.technology, &parent_dir)?;

    let mut component = GraphicsComponent::new(
        component_id,
        game.id().clone(),
        group_kind(&ordered),
        group.technology,
        group_swappability(&ordered),
    );

    for file in &ordered {
        component = component.with_file(component_file_from_detection(
            file.file_path().clone(),
            file.sha256().clone(),
            file.version().cloned(),
        ));
    }

    Ok(component)
}

fn build_grouped_artifact(
    game_id: &GameId,
    group: &GroupedDetectedFiles<'_>,
) -> AppResult<LibraryArtifact> {
    let ordered = order_with_primary_first(&group.files);
    let artifact_id = ArtifactId::for_bundle(ordered.iter().map(|file| file.sha256()));

    let files: Vec<ComponentFile> = ordered
        .iter()
        .map(|file| {
            component_file_from_detection(
                file.file_path().clone(),
                file.sha256().clone(),
                file.version().cloned(),
            )
        })
        .collect();

    LibraryArtifact::new(
        artifact_id,
        group.technology,
        ordered[0].file_name(),
        files,
        ArtifactTrustLevel::LocalObserved,
    )
    .map_err(detection_error)?
    .with_source("scan-folder")
    .map_err(detection_error)
    .map(|artifact| artifact.with_source_game_id(game_id.clone()))
}

/// Orders a group's files so the representative comes first, then alphabetically
/// for determinism. The first file becomes the bundle's primary/display file.
fn order_with_primary_first<'a>(group: &[&'a DetectedLibraryFile]) -> Vec<&'a DetectedLibraryFile> {
    let family = group
        .first()
        .map(|file| file.technology().family())
        .unwrap_or_default();

    // FSR sets arbitrate the representative by release-build cohesion: a leftover
    // upscaler next to a real unified FSR 3.1 must not hijack the version display.
    let fsr_upscaler_represents = family == GraphicsTechnology::AmdFsr
        && fsr::upscaler_represents_set(
            group.iter().map(|file| (file.file_name(), file.version())),
        );

    let mut ordered = group.to_vec();
    ordered.sort_by(|left, right| {
        primary_rank(left, family, fsr_upscaler_represents)
            .cmp(&primary_rank(right, family, fsr_upscaler_represents))
            .then_with(|| left.file_name().cmp(right.file_name()))
    });

    ordered
}

/// Lower rank = more representative. AMD FSR delegates to the shared
/// [`fsr::primary_rank`] (the upscaler carries the FSR version only in a
/// cohesive set); otherwise the file whose technology equals the family is.
fn primary_rank(
    file: &DetectedLibraryFile,
    family: GraphicsTechnology,
    fsr_upscaler_represents: bool,
) -> u8 {
    if family == GraphicsTechnology::AmdFsr {
        return fsr::primary_rank(file.file_name(), fsr_upscaler_represents);
    }

    if file.technology() == family {
        0
    } else {
        1
    }
}

fn group_kind(ordered: &[&DetectedLibraryFile]) -> ComponentKind {
    if ordered
        .iter()
        .any(|file| file.kind() == ComponentKind::StreamlineComponent)
    {
        ComponentKind::StreamlineComponent
    } else {
        ordered[0].kind()
    }
}

/// A multi-file bundle must be swapped as a unit ([`Swappability::BundleOnly`]);
/// a single file keeps its own detected policy. (A single restrictive sibling no
/// longer blocks an otherwise-swappable bundle.)
fn group_swappability(ordered: &[&DetectedLibraryFile]) -> Swappability {
    if ordered.len() > 1 {
        return Swappability::BundleOnly;
    }

    ordered
        .first()
        .map(|file| file.swappability())
        .unwrap_or(Swappability::Unknown)
}

fn grouped_component_id(
    game: &GameInstallation,
    technology: GraphicsTechnology,
    parent_dir: &str,
) -> AppResult<ComponentId> {
    ComponentId::new(format!(
        "component:{}:{}:{parent_dir}",
        game.id(),
        technology.as_slug()
    ))
    .map_err(detection_error)
}

fn native_fsr_directories(libraries: &[DetectedLibraryFile]) -> HashSet<String> {
    let mut directories = HashMap::<String, (bool, bool)>::new();

    for library in libraries {
        if library.technology().family() != GraphicsTechnology::AmdFsr {
            continue;
        }

        let summary = directories
            .entry(parent_directory(library.file_path()))
            .or_insert((false, false));
        match library.technology() {
            GraphicsTechnology::AmdFsrLoader => summary.0 = true,
            GraphicsTechnology::AmdFsr => summary.1 = true,
            _ => {}
        }
    }

    directories
        .into_iter()
        .filter_map(|(directory, (has_loader, has_anchor))| {
            (has_loader && !has_anchor).then_some(directory)
        })
        .collect()
}

fn grouping_technology(
    library: &DetectedLibraryFile,
    parent_dir: &str,
    native_fsr_directories: &HashSet<String>,
) -> GraphicsTechnology {
    if library.technology().family() == GraphicsTechnology::AmdFsr
        && native_fsr_directories.contains(parent_dir)
    {
        return library.technology();
    }

    library.technology().family()
}

/// Returns the normalized parent directory of a file path (forward slashes).
fn parent_directory(path: &PathRef) -> String {
    path.parent().unwrap_or_default().to_owned()
}
