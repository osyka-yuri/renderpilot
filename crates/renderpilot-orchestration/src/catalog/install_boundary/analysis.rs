//! Traversal session and recommendation analysis.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use renderpilot_detection::InstallTreeCompleteness;
use renderpilot_domain::InstallKey;
use renderpilot_platform_windows::{
    EngineLayoutRequest, analyze_engine_layout, inspect_executable_candidates_bounded,
    is_readable_windows_pe_executable,
};

use super::classification::*;
use super::recommendation::choose_best_recommendation;
use super::types::*;
use crate::catalog::install_paths;

const DEFAULT_INSPECTION_ENTRY_BUDGET: usize = 100_000;

struct BoundaryInspectionSession<'a> {
    launcher_roots: &'a [PathBuf],
    cancellation: Option<&'a AtomicBool>,
    remaining_entries: usize,
    assessments: BTreeMap<InstallKey, CandidateBoundaryAssessment>,
}

impl<'a> BoundaryInspectionSession<'a> {
    fn new(launcher_roots: &'a [PathBuf], cancellation: Option<&'a AtomicBool>) -> Self {
        Self {
            launcher_roots,
            cancellation,
            remaining_entries: DEFAULT_INSPECTION_ENTRY_BUDGET,
            assessments: BTreeMap::new(),
        }
    }

    fn assess(&mut self, candidate: &Path) -> CandidateBoundaryAssessment {
        let key = install_paths::install_path_match_key(&candidate.to_string_lossy());
        if let Some(assessment) = key.as_ref().and_then(|key| self.assessments.get(key)) {
            return assessment.clone();
        }

        let assessment = if self.remaining_entries == 0 {
            incomplete_budget_assessment(candidate)
        } else {
            assess_candidate_uncached(
                candidate,
                self.launcher_roots,
                self.remaining_entries,
                self.cancellation,
            )
        };
        self.remaining_entries = self
            .remaining_entries
            .saturating_sub(assessment.visited_entries);
        if let Some(key) = key {
            self.assessments.insert(key, assessment.clone());
        }
        assessment
    }
}

pub(super) fn inspect_candidate(
    candidate: &Path,
    launcher_install_roots: &[PathBuf],
) -> CandidateBoundaryAssessment {
    let mut session = BoundaryInspectionSession::new(launcher_install_roots, None);
    session.assess(candidate)
}

pub(super) fn inspect(request: &InstallBoundaryRequest<'_>) -> InstallBoundaryAssessment {
    let mut session =
        BoundaryInspectionSession::new(request.launcher_install_roots, request.cancellation);
    let selected = session.assess(request.selected_root);
    let authoritative =
        containing_launcher_root(request.selected_root, request.launcher_install_roots)
            .filter(|root| {
                !install_paths::same_install_path(
                    &root.to_string_lossy(),
                    &request.selected_root.to_string_lossy(),
                )
            })
            .and_then(|root| {
                recommendation_for_root(
                    &mut session,
                    &root,
                    RootRecommendationSource::LauncherManifest,
                )
            });

    if let Some(recommendation) = authoritative {
        return InstallBoundaryAssessment {
            selected,
            recommendation: Some(recommendation),
        };
    }

    let mut recommendations = Vec::new();
    if selected.kind == InstallBoundaryKind::SingleInstallContainer
        && let Some(contained) = selected.candidate_roots.first()
        && let Some(recommendation) = recommendation_for_root(
            &mut session,
            contained,
            RootRecommendationSource::RootExecutable,
        )
    {
        recommendations.push((0_usize, recommendation));
    }

    let engine_distribution = selected
        .engine_layout
        .iter()
        .filter_map(|evidence| evidence.distribution_root.as_ref())
        .find(|root| {
            !install_paths::same_install_path(
                &root.to_string_lossy(),
                &request.selected_root.to_string_lossy(),
            )
        })
        .and_then(|root| {
            recommendation_for_root(
                &mut session,
                root,
                RootRecommendationSource::EngineDistributionRoot,
            )
        });

    if let Some(recommendation) = engine_distribution {
        recommendations.push((0, recommendation));
    }

    // A valid selected installation is the user's boundary. Searching above
    // it can only turn a library folder into a false recommendation and makes
    // inspection cost proportional to unrelated sibling games.
    let selected_contains_readable_executable = selected
        .executables
        .iter()
        .any(|executable| executable.valid_windows_pe);
    if selected.kind == InstallBoundaryKind::SingleInstall
        || selected.completeness == BoundaryCompleteness::Incomplete
        || selected_contains_readable_executable
    {
        return InstallBoundaryAssessment {
            selected,
            recommendation: choose_best_recommendation(recommendations),
        };
    }

    let mut parent = request.selected_root.parent();
    let mut distance = 1_usize;
    while let Some(candidate) = parent {
        if candidate.parent().is_none()
            || is_same_path(candidate, request.selected_root)
            || is_launcher_library_root(candidate, request.launcher_library_roots)
            || is_forbidden_ancestor(candidate)
        {
            break;
        }

        let assessment = session.assess(candidate);

        if assessment.kind == InstallBoundaryKind::MultipleInstallContainer
            || assessment.completeness == BoundaryCompleteness::Incomplete
        {
            break;
        }
        if assessment.kind == InstallBoundaryKind::SingleInstall
            && assessment.is_complete()
            && let Some(source) = assessment.recommendation_source()
        {
            recommendations.push((
                distance,
                recommendation_from_assessment(&assessment, source),
            ));
        }
        parent = candidate.parent();
        distance += 1;
    }

    InstallBoundaryAssessment {
        selected,
        recommendation: choose_best_recommendation(recommendations),
    }
}

fn recommendation_for_root(
    session: &mut BoundaryInspectionSession<'_>,
    root: &Path,
    source: RootRecommendationSource,
) -> Option<RootRecommendation> {
    let assessment = session.assess(root);
    if assessment.kind != InstallBoundaryKind::SingleInstall || !assessment.is_complete() {
        return None;
    }
    Some(recommendation_from_assessment(&assessment, source))
}

fn recommendation_from_assessment(
    assessment: &CandidateBoundaryAssessment,
    source: RootRecommendationSource,
) -> RootRecommendation {
    RootRecommendation {
        root: assessment.root.clone(),
        source,
        completeness: assessment.completeness,
        evidence: assessment.evidence.clone(),
    }
}

fn assess_candidate_uncached(
    candidate: &Path,
    launcher_roots: &[PathBuf],
    max_entries: usize,
    cancellation: Option<&AtomicBool>,
) -> CandidateBoundaryAssessment {
    let report = inspect_executable_candidates_bounded(candidate, max_entries, || {
        cancellation.is_some_and(|flag| flag.load(Ordering::Relaxed))
    });
    let executables = report
        .candidates()
        .iter()
        .map(|executable| {
            let valid_windows_pe = is_readable_windows_pe_executable(&executable.absolute_path);
            BoundaryExecutableEvidence {
                absolute_path: executable.absolute_path.clone(),
                relative_path: executable.relative_path.clone(),
                size_bytes: executable.size_bytes,
                depth: executable.depth,
                rank_score: executable.rank_score,
                valid_windows_pe,
                rejection_kind: executable
                    .rejection
                    .as_ref()
                    .map(|reason| reason.kind().to_owned()),
                rejection_token: executable
                    .rejection
                    .as_ref()
                    .map(|reason| reason.token().to_owned()),
            }
        })
        .collect::<Vec<_>>();
    let accepted_executables = executables
        .iter()
        .filter(|executable| executable.rejection_kind.is_none())
        .filter(|executable| executable.valid_windows_pe)
        .map(|executable| executable.absolute_path.clone())
        .collect::<Vec<_>>();
    let root_executable = executables.iter().any(|executable| {
        executable.depth == 0 && executable.rejection_kind.is_none() && executable.valid_windows_pe
    });
    let candidate_roots =
        executable_installation_anchors(candidate, &executables, report.structural_files());
    let component_context = report.structural_files().iter().any(|path| {
        let relative = path.strip_prefix(candidate).unwrap_or(path);
        relative.components().count() == 1
            || !candidate_roots
                .iter()
                .any(|installation| path_within(path, installation))
    });
    let distribution_context = report.structural_files().iter().any(|path| {
        let relative = path.strip_prefix(candidate).unwrap_or(path);
        is_distribution_payload(path)
            && (relative.components().count() == 1
                || !candidate_roots
                    .iter()
                    .any(|installation| path_within(path, installation)))
    });
    let completeness = if report.completeness() == InstallTreeCompleteness::Complete
        && report.diagnostics().is_empty()
    {
        BoundaryCompleteness::Complete
    } else {
        BoundaryCompleteness::Incomplete
    };
    let engine_layout = analyze_engine_layout(&EngineLayoutRequest {
        candidate,
        accepted_executables: &accepted_executables,
    })
    .iter()
    .map(project_engine_evidence)
    .collect::<Vec<_>>();
    let launcher_proven = launcher_roots
        .iter()
        .any(|root| is_same_path(root, candidate));

    let mut evidence = BTreeSet::new();
    if launcher_proven {
        evidence.insert(InstallBoundaryEvidenceKind::LauncherManifest);
    }
    if root_executable {
        evidence.insert(InstallBoundaryEvidenceKind::RootExecutable);
    }
    if !candidate_roots.is_empty() {
        evidence.insert(InstallBoundaryEvidenceKind::ExecutableBranch);
    }
    if !engine_layout.is_empty() {
        evidence.insert(InstallBoundaryEvidenceKind::EngineStructure);
    }
    if component_context {
        evidence.insert(InstallBoundaryEvidenceKind::ComponentContext);
    }
    if engine_layout
        .iter()
        .any(|item| item.role == EngineBoundaryRole::DistributionRoot)
    {
        evidence.insert(InstallBoundaryEvidenceKind::EngineDistributionRoot);
    }

    let kind = classify_candidate(CandidateClassificationEvidence {
        completeness,
        launcher_proven,
        root_executable,
        executable_anchor_count: candidate_roots.len(),
        engine_layout: &engine_layout,
        no_accepted_executable: accepted_executables.is_empty(),
        component_context,
        distribution_context,
    });

    CandidateBoundaryAssessment {
        root: candidate.to_path_buf(),
        kind,
        completeness,
        has_accepted_executable: !accepted_executables.is_empty(),
        candidate_roots,
        evidence,
        diagnostics: report.diagnostics().to_vec(),
        engine_layout,
        launcher_proven,
        executables,
        visited_entries: report.visited_entries(),
    }
}

fn incomplete_budget_assessment(candidate: &Path) -> CandidateBoundaryAssessment {
    CandidateBoundaryAssessment {
        root: candidate.to_path_buf(),
        kind: InstallBoundaryKind::Incomplete,
        completeness: BoundaryCompleteness::Incomplete,
        has_accepted_executable: false,
        candidate_roots: Vec::new(),
        evidence: BTreeSet::new(),
        diagnostics: vec!["installation inspection exhausted its traversal budget".to_owned()],
        engine_layout: Vec::new(),
        launcher_proven: false,
        executables: Vec::new(),
        visited_entries: 0,
    }
}
