//! Candidate classification and path-topology policy.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use renderpilot_domain::InstallKey;
use renderpilot_platform_windows::{EngineKind, EngineLayoutEvidence, EngineLayoutRole};

use super::types::*;
use crate::catalog::install_paths;

#[derive(Clone, Copy)]
pub(super) struct CandidateClassificationEvidence<'a> {
    pub(super) completeness: BoundaryCompleteness,
    pub(super) launcher_proven: bool,
    pub(super) root_executable: bool,
    pub(super) executable_anchor_count: usize,
    pub(super) engine_layout: &'a [EngineBoundaryEvidence],
    pub(super) no_accepted_executable: bool,
    pub(super) component_context: bool,
    pub(super) distribution_context: bool,
}

pub(super) fn classify_candidate(
    evidence: CandidateClassificationEvidence<'_>,
) -> InstallBoundaryKind {
    let CandidateClassificationEvidence {
        completeness,
        launcher_proven,
        root_executable,
        executable_anchor_count,
        engine_layout,
        no_accepted_executable,
        component_context,
        distribution_context,
    } = evidence;
    if completeness == BoundaryCompleteness::Incomplete {
        return InstallBoundaryKind::Incomplete;
    }
    if launcher_proven {
        return InstallBoundaryKind::SingleInstall;
    }
    if let Some(engine) = engine_layout.first() {
        match engine.role {
            EngineBoundaryRole::DistributionRoot => {
                return if executable_anchor_count <= 1 {
                    InstallBoundaryKind::SingleInstall
                } else {
                    InstallBoundaryKind::MultipleInstallContainer
                };
            }
            EngineBoundaryRole::ProjectSubtree if engine.distribution_root.is_some() => {
                return InstallBoundaryKind::EngineProjectSubtree;
            }
            EngineBoundaryRole::BinarySubtree if engine.distribution_root.is_some() => {
                return InstallBoundaryKind::BinarySubtree;
            }
            EngineBoundaryRole::ProjectSubtree => {
                return if no_accepted_executable {
                    InstallBoundaryKind::Ambiguous
                } else {
                    InstallBoundaryKind::SingleInstall
                };
            }
            EngineBoundaryRole::SharedEngineSubtree => {
                return InstallBoundaryKind::EngineProjectSubtree;
            }
            EngineBoundaryRole::BinarySubtree => {}
        }
    }
    if no_accepted_executable {
        return InstallBoundaryKind::Ambiguous;
    }
    if executable_anchor_count > 1 {
        return InstallBoundaryKind::MultipleInstallContainer;
    }
    if root_executable && executable_anchor_count == 0 {
        return InstallBoundaryKind::SingleInstall;
    }
    if executable_anchor_count == 1 {
        if (root_executable && component_context) || (!root_executable && distribution_context) {
            InstallBoundaryKind::SingleInstall
        } else if root_executable {
            InstallBoundaryKind::Ambiguous
        } else {
            InstallBoundaryKind::SingleInstallContainer
        }
    } else {
        InstallBoundaryKind::Ambiguous
    }
}

/// Derives installation anchors from the whole executable topology.
///
/// Grouping only by the first relative segment loses independent games below a
/// shared container (`Bundle/Games/A` and `Bundle/Games/B`). Conversely, using
/// every executable directory as an installation would split one game with
/// multiple binary folders. This routine finds the first divergent executable
/// branches and keeps their common ancestor only when that ancestor has its own
/// distribution payload (or directly contains an accepted executable).
pub(super) fn executable_installation_anchors(
    candidate: &Path,
    executables: &[BoundaryExecutableEvidence],
    structural_files: &[PathBuf],
) -> Vec<PathBuf> {
    let mut parent_segments = executables
        .iter()
        .filter(|executable| executable.rejection_kind.is_none())
        .filter(|executable| executable.valid_windows_pe)
        .filter_map(|executable| {
            let parent = executable.absolute_path.parent()?;
            let relative = parent.strip_prefix(candidate).ok()?;
            let segments = relative
                .components()
                .map(|component| component.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            (!segments.is_empty()).then_some(segments)
        })
        .collect::<Vec<_>>();
    parent_segments.sort_by_key(|segments| {
        segments
            .iter()
            .map(|segment| segment.to_ascii_lowercase())
            .collect::<Vec<_>>()
    });
    parent_segments.dedup_by(|left, right| {
        left.len() == right.len()
            && left
                .iter()
                .zip(right.iter())
                .all(|(left, right)| left.eq_ignore_ascii_case(right))
    });
    if parent_segments.is_empty() {
        return Vec::new();
    }
    if parent_segments.len() == 1 {
        return vec![candidate.join(&parent_segments[0][0])];
    }

    let common_len =
        parent_segments
            .iter()
            .skip(1)
            .fold(parent_segments[0].len(), |common, segments| {
                parent_segments[0]
                    .iter()
                    .zip(segments.iter())
                    .take(common)
                    .take_while(|(left, right)| left.eq_ignore_ascii_case(right))
                    .count()
            });
    let common_root = parent_segments[0]
        .iter()
        .take(common_len)
        .fold(candidate.to_path_buf(), |path, segment| path.join(segment));

    let branch_roots = parent_segments
        .iter()
        .filter_map(|segments| {
            segments.get(common_len).map(|branch| {
                if common_len == 0 {
                    candidate.join(branch)
                } else {
                    common_root.join(branch)
                }
            })
        })
        .fold(BTreeMap::<InstallKey, PathBuf>::new(), |mut roots, root| {
            if let Some(key) = install_paths::install_path_match_key(&root.to_string_lossy()) {
                roots.entry(key).or_insert(root);
            }
            roots
        })
        .into_values()
        .collect::<Vec<_>>();

    // A directly contained executable may unify one consistent child branch,
    // but it must never hide two independent installations below the common
    // directory.
    let has_direct_common_executable = common_len > 0
        && parent_segments
            .iter()
            .any(|segments| segments.len() == common_len);
    if has_direct_common_executable && branch_roots.len() <= 1 {
        return vec![common_root];
    }

    if common_len > 0
        && structural_files.iter().any(|path| {
            path_within(path, &common_root)
                && is_distribution_payload(path)
                && !branch_roots.iter().any(|branch| path_within(path, branch))
        })
    {
        vec![common_root]
    } else {
        branch_roots
    }
}

pub(super) fn is_distribution_payload(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        ["pak", "utoc", "ucas", "archive", "bundle"]
            .iter()
            .any(|candidate| extension.eq_ignore_ascii_case(candidate))
    })
}

pub(super) fn project_engine_evidence(evidence: &EngineLayoutEvidence) -> EngineBoundaryEvidence {
    let kind = match evidence.kind() {
        EngineKind::Unreal => EngineBoundaryKind::Unreal,
    };
    let role = match evidence.role() {
        EngineLayoutRole::DistributionRoot => EngineBoundaryRole::DistributionRoot,
        EngineLayoutRole::ProjectSubtree => EngineBoundaryRole::ProjectSubtree,
        EngineLayoutRole::SharedEngineSubtree => EngineBoundaryRole::SharedEngineSubtree,
        EngineLayoutRole::BinarySubtree => EngineBoundaryRole::BinarySubtree,
    };
    EngineBoundaryEvidence {
        kind,
        role,
        distribution_root: evidence.distribution_root().map(Path::to_path_buf),
        project_root: evidence.project_root().map(Path::to_path_buf),
    }
}

pub(super) fn containing_launcher_root(
    selected: &Path,
    launcher_roots: &[PathBuf],
) -> Option<PathBuf> {
    launcher_roots
        .iter()
        .filter(|root| path_within(selected, root))
        .max_by_key(|root| root.components().count())
        .cloned()
}

pub(super) fn is_launcher_library_root(candidate: &Path, library_roots: &[PathBuf]) -> bool {
    library_roots
        .iter()
        .any(|root| is_same_path(root, candidate))
}

pub(super) fn is_forbidden_ancestor(candidate: &Path) -> bool {
    candidate.file_name().is_some_and(|name| {
        [
            "Windows",
            "System32",
            "SysWOW64",
            "Program Files",
            "Program Files (x86)",
            "ProgramData",
            "System Volume Information",
            "$Recycle.Bin",
        ]
        .iter()
        .any(|forbidden| name.eq_ignore_ascii_case(forbidden))
    })
}

pub(super) fn path_within(path: &Path, root: &Path) -> bool {
    crate::paths::is_within(path, root)
}

pub(super) fn is_same_path(left: &Path, right: &Path) -> bool {
    crate::paths::same_path(left, right)
}
