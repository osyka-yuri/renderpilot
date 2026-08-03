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
    code: &'static str,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    parameters: std::collections::BTreeMap<String, AddGameWarningParameterOutput>,
}

#[derive(Debug, serde::Serialize)]
#[serde(untagged)]
enum AddGameWarningParameterOutput {
    Text(String),
    Number(usize),
}

/// Defines the exhaustive domain-to-wire mapping and its contract samples together.
///
/// Adding a domain variant makes the generated `match` non-exhaustive; adding its
/// arm automatically adds it to the manifest parity test, so the test fixture
/// cannot silently drift behind the production serializer.
macro_rules! define_add_game_warning_wire_contract {
    ($(
        $pattern:pat => {
            sample: $sample:expr,
            code: $code:literal
            $(, $parameter_name:literal => $parameter_value:expr)* $(,)?
        }
    ),+ $(,)?) => {
        impl From<catalog::AddGameWarning> for AddGameWarningOutput {
            fn from(value: catalog::AddGameWarning) -> Self {
                match value {
                    $(
                        $pattern => {
                            let parameters: std::collections::BTreeMap<
                                String,
                                AddGameWarningParameterOutput,
                            > = [$(
                                ($parameter_name.to_owned(), $parameter_value),
                            )*]
                            .into_iter()
                            .collect();
                            Self { code: $code, parameters }
                        }
                    )+
                }
            }
        }

        #[cfg(test)]
        fn add_game_warning_contract_samples() -> Vec<catalog::AddGameWarning> {
            vec![$($sample),+]
        }
    };
}

define_add_game_warning_wire_contract!(
    catalog::AddGameWarning::LegacyCardsConsolidated { count } => {
        sample: catalog::AddGameWarning::LegacyCardsConsolidated { count: 2 },
        code: "legacy_cards_consolidated",
        "count" => AddGameWarningParameterOutput::Number(count),
    },
    catalog::AddGameWarning::LegacyCardsRetained { count } => {
        sample: catalog::AddGameWarning::LegacyCardsRetained { count: 3 },
        code: "legacy_cards_retained",
        "count" => AddGameWarningParameterOutput::Number(count),
    },
    catalog::AddGameWarning::RecoveryBundleCreated { path } => {
        sample: catalog::AddGameWarning::RecoveryBundleCreated { path: "C:/recovery".into() },
        code: "recovery_bundle_created",
        "path" => AddGameWarningParameterOutput::Text(path),
    },
    catalog::AddGameWarning::RootCorrectionHistoryArchived { path } => {
        sample: catalog::AddGameWarning::RootCorrectionHistoryArchived { path: "C:/history".into() },
        code: "root_correction_history_archived",
        "path" => AddGameWarningParameterOutput::Text(path),
    },
    catalog::AddGameWarning::FilesystemProbeError => {
        sample: catalog::AddGameWarning::FilesystemProbeError,
        code: "filesystem_probe_error",
    },
    catalog::AddGameWarning::InsideExistingInstall => {
        sample: catalog::AddGameWarning::InsideExistingInstall,
        code: "inside_existing_install",
    },
    catalog::AddGameWarning::NarrowsExistingInstall => {
        sample: catalog::AddGameWarning::NarrowsExistingInstall,
        code: "narrows_existing_install",
    },
    catalog::AddGameWarning::MultipleProvenInstalls => {
        sample: catalog::AddGameWarning::MultipleProvenInstalls,
        code: "multiple_proven_installs",
    },
    catalog::AddGameWarning::ContainsProvenInstall => {
        sample: catalog::AddGameWarning::ContainsProvenInstall,
        code: "contains_proven_install",
    },
    catalog::AddGameWarning::ExplicitExecutableRequired => {
        sample: catalog::AddGameWarning::ExplicitExecutableRequired,
        code: "explicit_executable_required",
    },
    catalog::AddGameWarning::NoReadableExecutable => {
        sample: catalog::AddGameWarning::NoReadableExecutable,
        code: "no_readable_executable",
    },
);

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
    use std::{
        collections::{BTreeMap, BTreeSet},
        fs,
        path::PathBuf,
    };

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

    #[test]
    fn add_game_warning_wire_contract_matches_the_shared_manifest() {
        let actual_entries = add_game_warning_contract_samples()
            .into_iter()
            .map(AddGameWarningOutput::from)
            .map(|warning| {
                let parameters = warning
                    .parameters
                    .into_iter()
                    .map(|(name, value)| {
                        let parameter_type = match value {
                            AddGameWarningParameterOutput::Text(value) => {
                                assert!(!value.trim().is_empty());
                                "non_blank_string"
                            }
                            AddGameWarningParameterOutput::Number(value) => {
                                assert!(value > 0);
                                "positive_integer"
                            }
                        };
                        (name, parameter_type)
                    })
                    .collect::<BTreeMap<_, _>>();
                (warning.code, parameters)
            })
            .collect::<Vec<_>>();
        let unique_actual_codes = actual_entries
            .iter()
            .map(|(code, _)| *code)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            unique_actual_codes.len(),
            actual_entries.len(),
            "warning samples must contain each domain variant exactly once"
        );
        let actual = actual_entries.into_iter().collect::<BTreeMap<_, _>>();

        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/contracts/add-game-warnings.json");
        let manifest: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(path).expect("read add-game warning contract"),
        )
        .expect("parse add-game warning contract");
        let expected_entries = manifest["addGameWarnings"]
            .as_array()
            .expect("addGameWarnings array")
            .iter()
            .map(|entry| {
                let code = entry["code"].as_str().expect("warning code");
                let parameters = entry["parameters"]
                    .as_object()
                    .expect("warning parameters")
                    .iter()
                    .map(|(name, parameter_type)| {
                        (
                            name.clone(),
                            parameter_type.as_str().expect("warning parameter type"),
                        )
                    })
                    .collect::<BTreeMap<_, _>>();
                (code, parameters)
            })
            .collect::<Vec<_>>();
        let unique_expected_codes = expected_entries
            .iter()
            .map(|(code, _)| *code)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            unique_expected_codes.len(),
            expected_entries.len(),
            "warning manifest must not contain duplicate codes"
        );
        let expected = expected_entries.into_iter().collect::<BTreeMap<_, _>>();

        assert_eq!(actual, expected);
        for warning in actual.keys() {
            let serialized = serde_json::to_value(AddGameWarningOutput {
                code: warning,
                parameters: BTreeMap::new(),
            })
            .expect("serialize add-game warning");
            assert!(serialized.get("message").is_none());
        }
    }
}
