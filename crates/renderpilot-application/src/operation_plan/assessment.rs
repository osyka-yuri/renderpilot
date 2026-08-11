use renderpilot_domain::{
    ArtifactId, ComponentFile, LibraryArtifact, LibraryComponent, Swappability,
};

use crate::{AppError, AppResult};

use super::{OperationPlanBlocker, OperationPlanRiskLevel, OperationPlanWarning};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OperationPlanAssessment {
    pub(crate) blockers: Vec<OperationPlanBlocker>,
    pub(crate) warnings: Vec<OperationPlanWarning>,
    pub(crate) risk_level: OperationPlanRiskLevel,
}

impl OperationPlanAssessment {
    pub(crate) fn assess(component: &LibraryComponent, artifact: &LibraryArtifact) -> Self {
        let blockers = collect_blockers(component, artifact);
        let warnings = collect_warnings(component, artifact);
        let risk_level = OperationPlanRiskLevel::from_findings(&blockers, &warnings);

        Self {
            blockers,
            warnings,
            risk_level,
        }
    }
}

fn collect_blockers(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
) -> Vec<OperationPlanBlocker> {
    let mut blockers = Vec::new();

    if component.technology() != artifact.technology() {
        blockers.push(OperationPlanBlocker::TechnologyMismatch);
    }

    if artifact_matches_active_bundle(component, artifact) {
        blockers.push(OperationPlanBlocker::ArtifactMatchesCurrentFile);
    }

    if let Some(blocker) = swappability_blocker(component.swappability()) {
        blockers.push(blocker);
    }

    blockers
}

fn collect_warnings(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
) -> Vec<OperationPlanWarning> {
    let mut warnings = Vec::new();

    if let Some(warning) = swappability_warning(component.swappability()) {
        warnings.push(warning);
    }

    if primary_version_unknown(component) || artifact.version().is_none() {
        warnings.push(OperationPlanWarning::ManualVersionComparisonRequired);
    }

    warnings
}

/// Returns true when the artifact bundle is byte-identical to the component's
/// currently active file set, i.e. applying it would be a no-op. Compared by
/// content identity (the bundle id of each file set).
fn artifact_matches_active_bundle(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
) -> bool {
    match (
        ArtifactId::for_component_files(component.files()),
        ArtifactId::for_component_files(artifact.files()),
    ) {
        (Some(active), Some(candidate)) => active == candidate,
        _ => false,
    }
}

fn primary_version_unknown(component: &LibraryComponent) -> bool {
    component
        .files()
        .first()
        .is_none_or(|file| file.version().is_none())
}

pub(crate) fn primary_component_file(component: &LibraryComponent) -> AppResult<&ComponentFile> {
    component.files().first().ok_or_else(|| {
        AppError::invalid_input(format!(
            "component {} does not contain a target file",
            component.id().as_str()
        ))
    })
}

fn swappability_blocker(swappability: Swappability) -> Option<OperationPlanBlocker> {
    match swappability {
        Swappability::ReadOnly => Some(OperationPlanBlocker::ComponentReadOnly),
        Swappability::IntegratedIntoEngine => {
            Some(OperationPlanBlocker::ComponentIntegratedIntoEngine)
        }
        Swappability::Unsafe => Some(OperationPlanBlocker::ComponentUnsafe),
        Swappability::Swappable | Swappability::BundleOnly | Swappability::Unknown => None,
    }
}

fn swappability_warning(swappability: Swappability) -> Option<OperationPlanWarning> {
    match swappability {
        Swappability::BundleOnly | Swappability::Unknown => {
            Some(OperationPlanWarning::ConfirmationRequiredForSwappability)
        }
        Swappability::Swappable
        | Swappability::ReadOnly
        | Swappability::IntegratedIntoEngine
        | Swappability::Unsafe => None,
    }
}
