use renderpilot_domain::{
    ComponentFile, GraphicsComponent, GraphicsTechnology, LibraryArtifact, PathRef, Swappability,
};

use crate::{AppError, AppResult};

use super::{OperationPlanBlocker, OperationPlanRiskLevel, OperationPlanWarning};

const PROTECTED_WINDOWS_ROOTS: [&str; 3] = ["program files", "program files (x86)", "windows"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OperationPlanAssessment {
    pub(crate) blockers: Vec<OperationPlanBlocker>,
    pub(crate) warnings: Vec<OperationPlanWarning>,
    pub(crate) risk_level: OperationPlanRiskLevel,
    pub(crate) requires_elevation: bool,
}

impl OperationPlanAssessment {
    pub(crate) fn assess(
        component: &GraphicsComponent,
        artifact: &LibraryArtifact,
        target_file: &ComponentFile,
    ) -> Self {
        let blockers = collect_blockers(component, artifact, target_file);
        let warnings = collect_warnings(component, artifact, target_file);
        let requires_elevation = path_requires_elevation(target_file.path());

        let risk_level =
            OperationPlanRiskLevel::from_findings(&blockers, &warnings, requires_elevation);

        Self {
            blockers,
            warnings,
            risk_level,
            requires_elevation,
        }
    }
}

fn collect_blockers(
    component: &GraphicsComponent,
    artifact: &LibraryArtifact,
    target_file: &ComponentFile,
) -> Vec<OperationPlanBlocker> {
    let mut blockers = Vec::new();

    if component.technology() != artifact.technology() {
        blockers.push(OperationPlanBlocker::TechnologyMismatch);
    }

    if target_file
        .sha256()
        .is_some_and(|sha256| sha256 == artifact.sha256())
    {
        blockers.push(OperationPlanBlocker::ArtifactMatchesCurrentFile);
    }

    if let Some(blocker) = swappability_blocker(component.swappability()) {
        blockers.push(blocker);
    }

    blockers
}

fn collect_warnings(
    component: &GraphicsComponent,
    artifact: &LibraryArtifact,
    target_file: &ComponentFile,
) -> Vec<OperationPlanWarning> {
    let mut warnings = Vec::new();

    if let Some(warning) = swappability_warning(component.swappability()) {
        warnings.push(warning);
    }

    if is_streamline_partial_swap(component) {
        warnings.push(OperationPlanWarning::StreamlinePartialSwap);
    }

    if target_file.version().is_none() || artifact.version().is_none() {
        warnings.push(OperationPlanWarning::ManualVersionComparisonRequired);
    }

    warnings
}

pub(crate) fn primary_component_file(component: &GraphicsComponent) -> AppResult<&ComponentFile> {
    component.files().first().ok_or_else(|| {
        AppError::invalid_input(format!(
            "component {} does not contain a target file",
            component.id().as_str()
        ))
    })
}

fn path_requires_elevation(path: &PathRef) -> bool {
    let Some(tail) = normalized_windows_drive_tail(path) else {
        return false;
    };

    PROTECTED_WINDOWS_ROOTS
        .iter()
        .any(|root| is_same_or_child_path(&tail, root))
}

fn normalized_windows_drive_tail(path: &PathRef) -> Option<String> {
    let normalized = path.as_str().replace('\\', "/").to_ascii_lowercase();
    let (drive, tail) = normalized.split_once(":/")?;

    let is_drive_letter = drive.len() == 1
        && drive
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_alphabetic());

    if !is_drive_letter {
        return None;
    }

    Some(tail.trim_start_matches('/').to_owned())
}

fn is_same_or_child_path(path_tail: &str, root: &str) -> bool {
    path_tail
        .strip_prefix(root)
        .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with('/'))
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

fn is_streamline_partial_swap(component: &GraphicsComponent) -> bool {
    component.swappability() == Swappability::BundleOnly
        && component.technology() == GraphicsTechnology::NvidiaStreamline
}
