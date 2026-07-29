//! Transport contract for inspecting and confirming one explicit game root.

use std::path::{Path, PathBuf};

use renderpilot_orchestration::catalog;

use crate::utils::{JsonResult, to_json};

/// Inspects a manually chosen installation root without mutating the catalog.
pub fn inspect_game_install(
    context: &renderpilot_orchestration::Context,
    path: &Path,
) -> JsonResult {
    to_json(AddGameInspectionOutput::from(
        catalog::inspect_game_install(context, path)?,
    ))
}

/// Adds or refreshes exactly one confirmed installation root.
pub fn add_game(
    context: &renderpilot_orchestration::Context,
    selected_root: PathBuf,
    root_choice: &str,
    allow_root_correction: bool,
    chosen_executable: Option<PathBuf>,
    inspection_fingerprint: String,
) -> JsonResult {
    let root_choice = match root_choice {
        "selected" => catalog::AddGameRootChoice::Selected,
        "recommended" => catalog::AddGameRootChoice::Recommended,
        other => {
            return Err(crate::ApiError::Service(
                renderpilot_orchestration::ServiceError::invalid_input(format!(
                    "unknown add-game root choice: {other}"
                )),
            ));
        }
    };
    to_json(AddGameResultOutput::from(catalog::add_game(
        context,
        catalog::AddGameRequest {
            selected_root,
            root_choice,
            allow_root_correction,
            chosen_executable,
            inspection_fingerprint,
        },
    )?))
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AddGameInspectionOutput {
    selected_root: String,
    inspection_fingerprint: String,
    catalog_generation: u64,
    boundary: InstallBoundaryOutput,
    recommendation: Option<RootRecommendationOutput>,
    relationship: InstallRelationshipOutput,
    executables: Vec<ExecutableInspectionOutput>,
    requires_explicit_executable: bool,
    root_correction: Option<RootCorrectionAssessmentOutput>,
    decision: AddGameDecisionOutput,
    warnings: Vec<AddGameWarningOutput>,
}

impl From<catalog::AddGameInspection> for AddGameInspectionOutput {
    fn from(value: catalog::AddGameInspection) -> Self {
        Self {
            selected_root: value.selected_root.path().as_str().to_owned(),
            inspection_fingerprint: value.inspection_fingerprint,
            catalog_generation: value.catalog_generation,
            boundary: value.boundary.into(),
            recommendation: value.recommendation.map(Into::into),
            relationship: value.relationship.into(),
            executables: value.executables.into_iter().map(Into::into).collect(),
            requires_explicit_executable: value.requires_explicit_executable,
            root_correction: value.root_correction.map(Into::into),
            decision: value.decision.into(),
            warnings: value.warnings.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallBoundaryOutput {
    kind: &'static str,
    completeness: &'static str,
    candidate_roots: Vec<String>,
    evidence: Vec<&'static str>,
}

impl From<catalog::InstallBoundaryInspection> for InstallBoundaryOutput {
    fn from(value: catalog::InstallBoundaryInspection) -> Self {
        Self {
            kind: boundary_kind(value.kind),
            completeness: traversal_completeness(value.completeness),
            candidate_roots: value
                .candidate_roots
                .into_iter()
                .map(|root| root.path().as_str().to_owned())
                .collect(),
            evidence: value.evidence.into_iter().map(boundary_evidence).collect(),
        }
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RootRecommendationOutput {
    root: String,
    source: &'static str,
    confidence: &'static str,
    completeness: &'static str,
    evidence: Vec<&'static str>,
}

impl From<catalog::RootRecommendationInspection> for RootRecommendationOutput {
    fn from(value: catalog::RootRecommendationInspection) -> Self {
        Self {
            root: value.root.path().as_str().to_owned(),
            source: match value.source {
                catalog::RootRecommendationSource::LauncherManifest => "launcher_manifest",
                catalog::RootRecommendationSource::ExistingCatalog => "existing_catalog",
                catalog::RootRecommendationSource::EngineDistributionRoot => {
                    "engine_distribution_root"
                }
                catalog::RootRecommendationSource::RootExecutable => "root_executable",
                catalog::RootRecommendationSource::ComponentContext => "component_context",
            },
            confidence: match value.confidence {
                catalog::RootRecommendationConfidence::Authoritative => "authoritative",
                catalog::RootRecommendationConfidence::Suggested => "suggested",
            },
            completeness: traversal_completeness(value.completeness),
            evidence: value.evidence.into_iter().map(boundary_evidence).collect(),
        }
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallRelationshipOutput {
    kind: &'static str,
    game_ids: Vec<String>,
    proven_install_roots: Vec<String>,
}

impl From<catalog::InstallRelationship> for InstallRelationshipOutput {
    fn from(value: catalog::InstallRelationship) -> Self {
        Self {
            kind: match value.kind {
                catalog::InstallRelationshipKind::New => "new",
                catalog::InstallRelationshipKind::ExactExisting => "exact_existing",
                catalog::InstallRelationshipKind::InsideExisting => "inside_existing",
                catalog::InstallRelationshipKind::ExpandsExisting => "expands_existing",
                catalog::InstallRelationshipKind::NarrowsExisting => "narrows_existing",
                catalog::InstallRelationshipKind::ContainsProvenInstall => {
                    "contains_proven_install"
                }
                catalog::InstallRelationshipKind::ContainsMultiple => "contains_multiple",
            },
            game_ids: value.game_ids,
            proven_install_roots: value
                .proven_install_roots
                .into_iter()
                .map(|root| root.path().as_str().to_owned())
                .collect(),
        }
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ExecutableInspectionOutput {
    path: String,
    relative_path: String,
    size_bytes: u64,
    rank_score: i32,
    valid_windows_pe: bool,
    rejection_kind: Option<String>,
    rejection_token: Option<String>,
}

impl From<catalog::ExecutableInspection> for ExecutableInspectionOutput {
    fn from(value: catalog::ExecutableInspection) -> Self {
        Self {
            path: value.path,
            relative_path: value.relative_path,
            size_bytes: value.size_bytes,
            rank_score: value.rank_score,
            valid_windows_pe: value.valid_windows_pe,
            rejection_kind: value.rejection_kind,
            rejection_token: value.rejection_token,
        }
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RootCorrectionAssessmentOutput {
    game_id: String,
    status: &'static str,
    cleanup_actions: Vec<RootCorrectionCleanupActionOutput>,
    blockers: Vec<&'static str>,
}

#[derive(Debug, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum RootCorrectionCleanupActionOutput {
    RollbackComponent {
        #[serde(rename = "componentId")]
        component_id: String,
    },
}

impl From<catalog::RootCorrectionAssessment> for RootCorrectionAssessmentOutput {
    fn from(value: catalog::RootCorrectionAssessment) -> Self {
        Self {
            game_id: value.game_id,
            status: match value.status {
                catalog::RootCorrectionStatus::Ready => "ready",
                catalog::RootCorrectionStatus::CleanupRequired => "cleanup_required",
                catalog::RootCorrectionStatus::Blocked => "blocked",
            },
            cleanup_actions: value
                .cleanup_actions
                .into_iter()
                .map(|action| match action {
                    catalog::RootCorrectionCleanupAction::RollbackComponent { component_id } => {
                        RootCorrectionCleanupActionOutput::RollbackComponent { component_id }
                    }
                })
                .collect(),
            blockers: value
                .blockers
                .into_iter()
                .map(catalog::RootCorrectionBlockerKind::as_str)
                .collect(),
        }
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum AddGameDecisionOutput {
    Automatic {
        option: AddGameOptionOutput,
    },
    Review {
        #[serde(rename = "defaultOption")]
        default_option: AddGameOptionOutput,
        options: Vec<AddGameOptionOutput>,
    },
    Unavailable {
        reasons: Vec<&'static str>,
    },
}

impl From<catalog::AddGameDecision> for AddGameDecisionOutput {
    fn from(value: catalog::AddGameDecision) -> Self {
        match value {
            catalog::AddGameDecision::Automatic { option } => Self::Automatic {
                option: option.into(),
            },
            catalog::AddGameDecision::Review(review) => Self::Review {
                default_option: review.default_option().into(),
                options: review.options().iter().copied().map(Into::into).collect(),
            },
            catalog::AddGameDecision::Unavailable { reasons } => Self::Unavailable {
                reasons: reasons.into_iter().map(unavailable_reason).collect(),
            },
        }
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AddGameOptionOutput {
    root_choice: &'static str,
    catalog_action: &'static str,
}

impl From<catalog::AddGameOption> for AddGameOptionOutput {
    fn from(value: catalog::AddGameOption) -> Self {
        Self {
            root_choice: match value.root_choice {
                catalog::AddGameRootChoice::Selected => "selected",
                catalog::AddGameRootChoice::Recommended => "recommended",
            },
            catalog_action: match value.catalog_action {
                catalog::AddGameCatalogAction::Add => "add",
                catalog::AddGameCatalogAction::Rescan => "rescan",
                catalog::AddGameCatalogAction::CorrectExistingRoot => "correct_existing_root",
            },
        }
    }
}

const fn unavailable_reason(reason: catalog::AddGameUnavailableReason) -> &'static str {
    match reason {
        catalog::AddGameUnavailableReason::MultipleInstalls => "multiple_installs",
        catalog::AddGameUnavailableReason::ContainsProvenInstall => "contains_proven_install",
        catalog::AddGameUnavailableReason::ContainsMultipleCatalogInstalls => {
            "contains_multiple_catalog_installs"
        }
        catalog::AddGameUnavailableReason::InsideExistingInstall => "inside_existing_install",
        catalog::AddGameUnavailableReason::NoReadableExecutable => "no_readable_executable",
        catalog::AddGameUnavailableReason::RootCorrectionBlocked => "root_correction_blocked",
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AddGameWarningOutput {
    code: String,
    message: String,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    parameters: std::collections::BTreeMap<String, AddGameWarningParameterOutput>,
}

#[derive(Debug, serde::Serialize)]
#[serde(untagged)]
enum AddGameWarningParameterOutput {
    Text(String),
    Number(usize),
}

impl From<catalog::AddGameWarning> for AddGameWarningOutput {
    fn from(value: catalog::AddGameWarning) -> Self {
        let mut parameters = std::collections::BTreeMap::new();
        let (code, message) = match value {
            catalog::AddGameWarning::LegacyCardsConsolidated { count } => {
                parameters.insert(
                    "count".to_owned(),
                    AddGameWarningParameterOutput::Number(count),
                );
                (
                    "legacy_cards_consolidated",
                    format!("consolidated {count} proven false legacy game card(s)"),
                )
            }
            catalog::AddGameWarning::LegacyCardsRetained { count } => {
                parameters.insert(
                    "count".to_owned(),
                    AddGameWarningParameterOutput::Number(count),
                );
                (
                    "legacy_cards_retained",
                    format!(
                        "retained {count} legacy card(s) because independent-install evidence was inconclusive"
                    ),
                )
            }
            catalog::AddGameWarning::RecoveryBundleCreated { path } => {
                let message =
                    format!("catalog state excluded by root correction was preserved in {path}");
                parameters.insert(
                    "path".to_owned(),
                    AddGameWarningParameterOutput::Text(path),
                );
                ("recovery_bundle_created", message)
            }
            catalog::AddGameWarning::RootCorrectionHistoryArchived { path } => {
                let message =
                    format!("operation history excluded by root correction was preserved in {path}");
                parameters.insert(
                    "path".to_owned(),
                    AddGameWarningParameterOutput::Text(path),
                );
                ("root_correction_history_archived", message)
            }
            catalog::AddGameWarning::FilesystemProbeError => (
                "filesystem_probe_error",
                "the selected folder could not be inspected completely".to_owned(),
            ),
            catalog::AddGameWarning::InsideExistingInstall => (
                "inside_existing_install",
                "the selected folder belongs to an existing game; use that game root".to_owned(),
            ),
            catalog::AddGameWarning::NarrowsExistingInstall => (
                "narrows_existing_install",
                "the existing manual root appears to contain multiple game folders; confirming will correct that card to the selected folder".to_owned(),
            ),
            catalog::AddGameWarning::MultipleProvenInstalls => (
                "multiple_proven_installs",
                "the selected folder contains multiple proven game installations".to_owned(),
            ),
            catalog::AddGameWarning::ContainsProvenInstall => (
                "contains_proven_install",
                "the selected folder contains a proven game installation; use its exact root"
                    .to_owned(),
            ),
            catalog::AddGameWarning::ExplicitExecutableRequired => (
                "explicit_executable_required",
                "all valid executables look like launchers or helpers; choose one explicitly"
                    .to_owned(),
            ),
            catalog::AddGameWarning::NoReadableExecutable => (
                "no_readable_executable",
                "the selected folder cannot be added separately because it has no readable Windows PE executable".to_owned(),
            ),
        };
        Self {
            code: code.to_owned(),
            message,
            parameters,
        }
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AddGameResultOutput {
    game_id: String,
    effective_root: String,
    disposition: &'static str,
    root_authority: &'static str,
    detected_library_count: usize,
    consolidated_game_ids: Vec<String>,
    recovery_bundle_path: Option<String>,
    warnings: Vec<AddGameWarningOutput>,
}

impl From<catalog::AddGameResult> for AddGameResultOutput {
    fn from(value: catalog::AddGameResult) -> Self {
        let root_authority = value.root_authority_name();
        Self {
            game_id: value.game_id,
            effective_root: value.effective_root,
            disposition: match value.disposition {
                catalog::AddGameDisposition::Added => "added",
                catalog::AddGameDisposition::Unchanged => "unchanged",
                catalog::AddGameDisposition::Updated => "updated",
                catalog::AddGameDisposition::RootCorrected => "root_corrected",
            },
            root_authority,
            detected_library_count: value.detected_library_count,
            consolidated_game_ids: value.consolidated_game_ids,
            recovery_bundle_path: value.recovery_bundle_path,
            warnings: value.warnings.into_iter().map(Into::into).collect(),
        }
    }
}

const fn boundary_kind(kind: catalog::InstallBoundaryKind) -> &'static str {
    match kind {
        catalog::InstallBoundaryKind::SingleInstall => "single_install",
        catalog::InstallBoundaryKind::EngineProjectSubtree => "engine_project_subtree",
        catalog::InstallBoundaryKind::BinarySubtree => "binary_subtree",
        catalog::InstallBoundaryKind::SingleInstallContainer => "single_install_container",
        catalog::InstallBoundaryKind::MultipleInstallContainer => "multiple_install_container",
        catalog::InstallBoundaryKind::Ambiguous => "ambiguous",
        catalog::InstallBoundaryKind::Incomplete => "incomplete",
    }
}

const fn traversal_completeness(value: catalog::TraversalCompleteness) -> &'static str {
    match value {
        catalog::TraversalCompleteness::Complete => "complete",
        catalog::TraversalCompleteness::Incomplete => "incomplete",
    }
}

const fn boundary_evidence(value: catalog::InstallBoundaryEvidence) -> &'static str {
    match value {
        catalog::InstallBoundaryEvidence::LauncherManifest => "launcher_manifest",
        catalog::InstallBoundaryEvidence::EngineDistributionRoot => "engine_distribution_root",
        catalog::InstallBoundaryEvidence::RootExecutable => "root_executable",
        catalog::InstallBoundaryEvidence::EngineStructure => "engine_structure",
        catalog::InstallBoundaryEvidence::ComponentContext => "component_context",
        catalog::InstallBoundaryEvidence::ExecutableBranch => "executable_branch",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn add_game_decision_json_is_exact_and_discriminated() {
        let selected_add = catalog::AddGameOption {
            root_choice: catalog::AddGameRootChoice::Selected,
            catalog_action: catalog::AddGameCatalogAction::Add,
        };
        let recommended_rescan = catalog::AddGameOption {
            root_choice: catalog::AddGameRootChoice::Recommended,
            catalog_action: catalog::AddGameCatalogAction::Rescan,
        };

        let automatic = serde_json::to_value(AddGameDecisionOutput::from(
            catalog::AddGameDecision::Automatic {
                option: selected_add,
            },
        ))
        .expect("automatic JSON");
        let review = serde_json::to_value(AddGameDecisionOutput::from(
            catalog::AddGameDecision::Review(
                catalog::AddGameReview::new(
                    recommended_rescan,
                    vec![recommended_rescan, selected_add],
                )
                .expect("valid review"),
            ),
        ))
        .expect("review JSON");
        let unavailable = serde_json::to_value(AddGameDecisionOutput::from(
            catalog::AddGameDecision::Unavailable {
                reasons: vec![
                    catalog::AddGameUnavailableReason::MultipleInstalls,
                    catalog::AddGameUnavailableReason::RootCorrectionBlocked,
                ],
            },
        ))
        .expect("unavailable JSON");

        assert_eq!(
            automatic,
            json!({
                "kind": "automatic",
                "option": {"rootChoice": "selected", "catalogAction": "add"}
            })
        );
        assert_eq!(
            review,
            json!({
                "kind": "review",
                "defaultOption": {
                    "rootChoice": "recommended",
                    "catalogAction": "rescan"
                },
                "options": [
                    {"rootChoice": "recommended", "catalogAction": "rescan"},
                    {"rootChoice": "selected", "catalogAction": "add"}
                ]
            })
        );
        assert_eq!(
            unavailable,
            json!({
                "kind": "unavailable",
                "reasons": ["multiple_installs", "root_correction_blocked"]
            })
        );
    }
}
