//! Value types shared by installation-boundary analysis stages.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

/// Physical role of a candidate directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InstallBoundaryKind {
    SingleInstall,
    EngineProjectSubtree,
    BinarySubtree,
    SingleInstallContainer,
    MultipleInstallContainer,
    Ambiguous,
    Incomplete,
}

/// Completeness relevant to a boundary decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BoundaryCompleteness {
    Complete,
    Incomplete,
}

/// Stable evidence categories kept independent from UI strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum InstallBoundaryEvidenceKind {
    LauncherManifest,
    EngineDistributionRoot,
    RootExecutable,
    EngineStructure,
    ComponentContext,
    ExecutableBranch,
}

/// Source and strength of a root recommendation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RootRecommendationSource {
    LauncherManifest,
    EngineDistributionRoot,
    RootExecutable,
    ComponentContext,
}

/// Internal recommendation returned by the boundary analyzer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RootRecommendation {
    pub root: PathBuf,
    pub source: RootRecommendationSource,
    pub completeness: BoundaryCompleteness,
    pub evidence: BTreeSet<InstallBoundaryEvidenceKind>,
}

impl RootRecommendation {
    pub const fn authoritative(&self) -> bool {
        matches!(self.source, RootRecommendationSource::LauncherManifest)
    }
}

/// One candidate directory and the evidence observed below it.
#[derive(Debug, Clone)]
pub(crate) struct CandidateBoundaryAssessment {
    pub root: PathBuf,
    pub kind: InstallBoundaryKind,
    pub completeness: BoundaryCompleteness,
    pub has_accepted_executable: bool,
    pub candidate_roots: Vec<PathBuf>,
    pub evidence: BTreeSet<InstallBoundaryEvidenceKind>,
    pub diagnostics: Vec<String>,
    pub engine_layout: Vec<EngineBoundaryEvidence>,
    pub launcher_proven: bool,
    pub executables: Vec<BoundaryExecutableEvidence>,
    pub(super) visited_entries: usize,
}

impl CandidateBoundaryAssessment {
    pub(super) fn is_complete(&self) -> bool {
        self.completeness == BoundaryCompleteness::Complete
    }

    pub(super) fn recommendation_source(&self) -> Option<RootRecommendationSource> {
        if self.launcher_proven {
            Some(RootRecommendationSource::LauncherManifest)
        } else if self
            .engine_layout
            .iter()
            .any(|evidence| evidence.role == EngineBoundaryRole::DistributionRoot)
        {
            Some(RootRecommendationSource::EngineDistributionRoot)
        } else if self
            .evidence
            .contains(&InstallBoundaryEvidenceKind::RootExecutable)
        {
            Some(RootRecommendationSource::RootExecutable)
        } else if self
            .evidence
            .contains(&InstallBoundaryEvidenceKind::ComponentContext)
        {
            Some(RootRecommendationSource::ComponentContext)
        } else {
            None
        }
    }
}

/// Executable facts collected by the same traversal as boundary evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BoundaryExecutableEvidence {
    pub absolute_path: PathBuf,
    pub relative_path: String,
    pub size_bytes: u64,
    pub depth: u32,
    pub rank_score: i32,
    pub valid_windows_pe: bool,
    pub rejection_kind: Option<String>,
    pub rejection_token: Option<String>,
}

/// Engine evidence projected into orchestration-owned types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EngineBoundaryEvidence {
    pub kind: EngineBoundaryKind,
    pub role: EngineBoundaryRole,
    pub distribution_root: Option<PathBuf>,
    pub project_root: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EngineBoundaryKind {
    Unreal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EngineBoundaryRole {
    DistributionRoot,
    ProjectSubtree,
    SharedEngineSubtree,
    BinarySubtree,
}

/// Complete boundary result for a selected directory.
#[derive(Debug, Clone)]
pub(crate) struct InstallBoundaryAssessment {
    pub selected: CandidateBoundaryAssessment,
    pub recommendation: Option<RootRecommendation>,
}

/// Immutable evidence used by one analysis operation.
#[derive(Clone, Copy)]
pub(crate) struct InstallBoundaryRequest<'a> {
    pub selected_root: &'a Path,
    pub launcher_install_roots: &'a [PathBuf],
    pub launcher_library_roots: &'a [PathBuf],
    pub cancellation: Option<&'a AtomicBool>,
}
