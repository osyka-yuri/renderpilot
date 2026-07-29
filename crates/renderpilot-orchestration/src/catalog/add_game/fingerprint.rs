//! Stable, presentation-independent fingerprints for add-game inspections.

use renderpilot_domain::{InstallRoot, normalized_path_key};
use serde::Serialize;

use super::*;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InspectionFingerprintV1 {
    version: &'static str,
    selected_root: String,
    catalog_generation: u64,
    boundary: FingerprintBoundary,
    recommendation: Option<FingerprintRecommendation>,
    relationship: FingerprintRelationship,
    decision_options: Vec<(&'static str, &'static str)>,
    root_correction: Option<FingerprintRootCorrection>,
    executables: Vec<FingerprintExecutable>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EffectiveRootFingerprintV1 {
    version: &'static str,
    root: String,
    catalog_generation: u64,
    boundary: FingerprintBoundary,
    relationship: FingerprintRelationship,
    root_correction: Option<FingerprintRootCorrection>,
    executables: Vec<FingerprintExecutable>,
}

#[derive(Serialize)]
struct FingerprintBoundary {
    kind: &'static str,
    completeness: &'static str,
    candidate_roots: Vec<String>,
    evidence: Vec<&'static str>,
}

#[derive(Serialize)]
struct FingerprintRecommendation {
    root: String,
    source: &'static str,
    confidence: &'static str,
    completeness: &'static str,
    evidence: Vec<&'static str>,
    effective_fingerprint: String,
}

#[derive(Serialize)]
struct FingerprintRelationship {
    kind: &'static str,
    game_ids: Vec<String>,
    proven_install_roots: Vec<String>,
}

#[derive(Serialize)]
struct FingerprintRootCorrection {
    game_id: String,
    status: &'static str,
    cleanup_component_ids: Vec<String>,
    blockers: Vec<&'static str>,
}

#[derive(Serialize)]
struct FingerprintExecutable {
    path: String,
    size_bytes: u64,
    valid_windows_pe: bool,
    rank_score: i32,
    rejection_kind: Option<String>,
    rejection_token: Option<String>,
}

pub(super) fn compute_inspection_fingerprint(
    inspection: &AddGameInspection,
) -> Result<String, ServiceError> {
    let recommendation = inspection.recommendation.as_ref().map(|recommendation| {
        let mut evidence = recommendation
            .evidence
            .iter()
            .copied()
            .map(boundary_evidence_name)
            .collect::<Vec<_>>();
        evidence.sort();
        evidence.dedup();
        FingerprintRecommendation {
            root: recommendation.root.key().as_str().to_owned(),
            source: recommendation_source_name(recommendation.source),
            confidence: recommendation_confidence_name(recommendation.confidence),
            completeness: completeness_name(recommendation.completeness),
            evidence,
            effective_fingerprint: recommendation.effective_fingerprint.clone(),
        }
    });

    let facts = InspectionFingerprintV1 {
        version: "add-game-inspection/v1",
        selected_root: inspection.selected_root.key().as_str().to_owned(),
        catalog_generation: inspection.catalog_generation,
        boundary: fingerprint_boundary(&inspection.boundary),
        recommendation,
        relationship: fingerprint_relationship(&inspection.relationship),
        decision_options: fingerprint_decision_options(&inspection.decision),
        root_correction: fingerprint_root_correction(inspection.root_correction.as_ref()),
        executables: fingerprint_executables(&inspection.executables),
    };
    hash_fingerprint(&facts)
}

pub(super) fn compute_effective_root_fingerprint(
    root: &InstallRoot,
    catalog_generation: u64,
    boundary: &InstallBoundaryInspection,
    relationship: &InstallRelationship,
    root_correction: Option<&RootCorrectionAssessment>,
    executables: &[ExecutableInspection],
) -> Result<String, ServiceError> {
    hash_fingerprint(&EffectiveRootFingerprintV1 {
        version: "add-game-effective-root/v1",
        root: root.key().as_str().to_owned(),
        catalog_generation,
        boundary: fingerprint_boundary(boundary),
        relationship: fingerprint_relationship(relationship),
        root_correction: fingerprint_root_correction(root_correction),
        executables: fingerprint_executables(executables),
    })
}

fn fingerprint_boundary(boundary: &InstallBoundaryInspection) -> FingerprintBoundary {
    let mut candidate_roots = boundary
        .candidate_roots
        .iter()
        .map(|root| root.key().as_str().to_owned())
        .collect::<Vec<_>>();
    candidate_roots.sort();
    candidate_roots.dedup();
    let mut evidence = boundary
        .evidence
        .iter()
        .copied()
        .map(boundary_evidence_name)
        .collect::<Vec<_>>();
    evidence.sort();
    evidence.dedup();
    FingerprintBoundary {
        kind: boundary_kind_name(boundary.kind),
        completeness: completeness_name(boundary.completeness),
        candidate_roots,
        evidence,
    }
}

fn fingerprint_relationship(relationship: &InstallRelationship) -> FingerprintRelationship {
    let mut game_ids = relationship.game_ids.clone();
    game_ids.sort();
    game_ids.dedup();
    let mut proven_install_roots = relationship
        .proven_install_roots
        .iter()
        .map(|root| root.key().as_str().to_owned())
        .collect::<Vec<_>>();
    proven_install_roots.sort();
    proven_install_roots.dedup();
    FingerprintRelationship {
        kind: relationship_kind_name(relationship.kind),
        game_ids,
        proven_install_roots,
    }
}

fn fingerprint_decision_options(decision: &AddGameDecision) -> Vec<(&'static str, &'static str)> {
    let mut options = match decision {
        AddGameDecision::Automatic { option } => vec![*option],
        AddGameDecision::Review(review) => review.options().to_vec(),
        AddGameDecision::Unavailable { .. } => Vec::new(),
    };
    options.sort();
    options.dedup();
    options
        .into_iter()
        .map(|option| {
            (
                root_choice_name(option.root_choice),
                catalog_action_name(option.catalog_action),
            )
        })
        .collect()
}

fn fingerprint_root_correction(
    assessment: Option<&RootCorrectionAssessment>,
) -> Option<FingerprintRootCorrection> {
    assessment.map(|assessment| {
        let mut cleanup_component_ids = assessment
            .cleanup_actions
            .iter()
            .map(|action| match action {
                crate::catalog::RootCorrectionCleanupAction::RollbackComponent { component_id } => {
                    component_id.clone()
                }
            })
            .collect::<Vec<_>>();
        cleanup_component_ids.sort();
        cleanup_component_ids.dedup();
        let mut blockers = assessment
            .blockers
            .iter()
            .copied()
            .map(crate::catalog::RootCorrectionBlockerKind::as_str)
            .collect::<Vec<_>>();
        blockers.sort();
        blockers.dedup();
        FingerprintRootCorrection {
            game_id: assessment.game_id.clone(),
            status: root_correction_status_name(assessment.status),
            cleanup_component_ids,
            blockers,
        }
    })
}

fn fingerprint_executables(executables: &[ExecutableInspection]) -> Vec<FingerprintExecutable> {
    let mut executables = executables
        .iter()
        .map(|executable| FingerprintExecutable {
            path: normalized_path_key(&executable.path),
            size_bytes: executable.size_bytes,
            valid_windows_pe: executable.valid_windows_pe,
            rank_score: executable.rank_score,
            rejection_kind: executable.rejection_kind.clone(),
            rejection_token: executable.rejection_token.clone(),
        })
        .collect::<Vec<_>>();
    executables.sort_by(|left, right| {
        (
            &left.path,
            left.size_bytes,
            left.valid_windows_pe,
            left.rank_score,
            &left.rejection_kind,
            &left.rejection_token,
        )
            .cmp(&(
                &right.path,
                right.size_bytes,
                right.valid_windows_pe,
                right.rank_score,
                &right.rejection_kind,
                &right.rejection_token,
            ))
    });
    executables
}

fn hash_fingerprint(value: &impl Serialize) -> Result<String, ServiceError> {
    let encoded = serde_json::to_vec(value).map_err(|error| {
        ServiceError::command_failed(format!("could not encode add-game fingerprint: {error}"))
    })?;
    renderpilot_detection::sha256_bytes(&encoded)
        .map(|hash| hash.as_str().to_owned())
        .map_err(ServiceError::from)
}

const fn boundary_kind_name(kind: InstallBoundaryKind) -> &'static str {
    match kind {
        InstallBoundaryKind::SingleInstall => "single_install",
        InstallBoundaryKind::EngineProjectSubtree => "engine_project_subtree",
        InstallBoundaryKind::BinarySubtree => "binary_subtree",
        InstallBoundaryKind::SingleInstallContainer => "single_install_container",
        InstallBoundaryKind::MultipleInstallContainer => "multiple_install_container",
        InstallBoundaryKind::Ambiguous => "ambiguous",
        InstallBoundaryKind::Incomplete => "incomplete",
    }
}

const fn completeness_name(completeness: TraversalCompleteness) -> &'static str {
    match completeness {
        TraversalCompleteness::Complete => "complete",
        TraversalCompleteness::Incomplete => "incomplete",
    }
}

const fn boundary_evidence_name(evidence: InstallBoundaryEvidence) -> &'static str {
    match evidence {
        InstallBoundaryEvidence::LauncherManifest => "launcher_manifest",
        InstallBoundaryEvidence::EngineDistributionRoot => "engine_distribution_root",
        InstallBoundaryEvidence::RootExecutable => "root_executable",
        InstallBoundaryEvidence::EngineStructure => "engine_structure",
        InstallBoundaryEvidence::ComponentContext => "component_context",
        InstallBoundaryEvidence::ExecutableBranch => "executable_branch",
    }
}

const fn recommendation_source_name(source: RootRecommendationSource) -> &'static str {
    match source {
        RootRecommendationSource::LauncherManifest => "launcher_manifest",
        RootRecommendationSource::ExistingCatalog => "existing_catalog",
        RootRecommendationSource::EngineDistributionRoot => "engine_distribution_root",
        RootRecommendationSource::RootExecutable => "root_executable",
        RootRecommendationSource::ComponentContext => "component_context",
    }
}

const fn recommendation_confidence_name(confidence: RootRecommendationConfidence) -> &'static str {
    match confidence {
        RootRecommendationConfidence::Authoritative => "authoritative",
        RootRecommendationConfidence::Suggested => "suggested",
    }
}

const fn relationship_kind_name(kind: InstallRelationshipKind) -> &'static str {
    match kind {
        InstallRelationshipKind::New => "new",
        InstallRelationshipKind::ExactExisting => "exact_existing",
        InstallRelationshipKind::InsideExisting => "inside_existing",
        InstallRelationshipKind::ExpandsExisting => "expands_existing",
        InstallRelationshipKind::NarrowsExisting => "narrows_existing",
        InstallRelationshipKind::ContainsProvenInstall => "contains_proven_install",
        InstallRelationshipKind::ContainsMultiple => "contains_multiple",
    }
}

const fn root_choice_name(choice: AddGameRootChoice) -> &'static str {
    match choice {
        AddGameRootChoice::Selected => "selected",
        AddGameRootChoice::Recommended => "recommended",
    }
}

const fn catalog_action_name(action: AddGameCatalogAction) -> &'static str {
    match action {
        AddGameCatalogAction::Add => "add",
        AddGameCatalogAction::Rescan => "rescan",
        AddGameCatalogAction::CorrectExistingRoot => "correct_existing_root",
    }
}

const fn root_correction_status_name(status: RootCorrectionStatus) -> &'static str {
    match status {
        RootCorrectionStatus::Ready => "ready",
        RootCorrectionStatus::CleanupRequired => "cleanup_required",
        RootCorrectionStatus::Blocked => "blocked",
    }
}
