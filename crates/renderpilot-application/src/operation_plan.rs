use std::time::{SystemTime, UNIX_EPOCH};

use renderpilot_domain::{
    ArtifactId, ComponentFile, GraphicsComponent, GraphicsTechnology, LibraryArtifact, OperationId,
    PathRef, Sha256Hash, Swappability, Version,
};

use crate::{AppError, AppResult, OperationKind};

const PROTECTED_WINDOWS_ROOTS: [&str; 3] = ["program files", "program files (x86)", "windows"];

/// Builds a swap operation plan without applying any filesystem changes.
pub fn build_swap_operation_plan(
    component: &GraphicsComponent,
    artifact: &LibraryArtifact,
) -> AppResult<OperationPlan> {
    let target_file = component_target_file(component)?;

    Ok(OperationPlan::new(
        component,
        artifact,
        target_file,
        OperationPlanAssessment::assess(component, artifact, target_file),
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OperationPlanAssessment {
    blockers: Vec<OperationPlanBlocker>,
    warnings: Vec<OperationPlanWarning>,
    risk_level: OperationPlanRiskLevel,
    requires_elevation: bool,
}

impl OperationPlanAssessment {
    fn assess(
        component: &GraphicsComponent,
        artifact: &LibraryArtifact,
        target_file: &ComponentFile,
    ) -> Self {
        let mut blockers = Vec::new();
        let mut warnings = Vec::new();

        if component.technology() != artifact.technology() {
            blockers.push(OperationPlanBlocker::TechnologyMismatch);
        }

        if target_file.sha256() == Some(artifact.sha256()) {
            blockers.push(OperationPlanBlocker::ArtifactMatchesCurrentFile);
        }

        if let Some(blocker) = swappability_blocker(component.swappability()) {
            blockers.push(blocker);
        }

        if let Some(warning) = swappability_warning(component.swappability()) {
            warnings.push(warning);
        }

        if is_streamline_partial_swap(component) {
            warnings.push(OperationPlanWarning::StreamlinePartialSwap);
        }

        if target_file.version().is_none() || artifact.version().is_none() {
            warnings.push(OperationPlanWarning::ManualVersionComparisonRequired);
        }

        let requires_elevation = path_requires_elevation(target_file.path());
        let risk_level = derive_risk_level(&blockers, &warnings, requires_elevation);

        Self {
            blockers,
            warnings,
            risk_level,
            requires_elevation,
        }
    }
}

/// Read-only operation plan for a DLL replacement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationPlan {
    operation_id: OperationId,
    game_id: renderpilot_domain::GameId,
    operation_type: OperationKind,
    target_path: PathRef,
    replacement_path: PathRef,
    original_version: Option<Version>,
    replacement_version: Option<Version>,
    original_sha256: Option<Sha256Hash>,
    replacement_sha256: Option<Sha256Hash>,
    risk_level: OperationPlanRiskLevel,
    requires_backup: bool,
    requires_elevation: bool,
    artifact_id: ArtifactId,
    blockers: Vec<OperationPlanBlocker>,
    warnings: Vec<OperationPlanWarning>,
}

impl OperationPlan {
    fn new(
        component: &GraphicsComponent,
        artifact: &LibraryArtifact,
        target_file: &ComponentFile,
        assessment: OperationPlanAssessment,
    ) -> Self {
        Self {
            operation_id: generated_operation_id(component, artifact),
            game_id: component.game_id().clone(),
            operation_type: OperationKind::ReplaceComponent,
            target_path: target_file.path().clone(),
            replacement_path: artifact.path().clone(),
            original_version: target_file.version().cloned(),
            replacement_version: artifact.version().cloned(),
            original_sha256: target_file.sha256().cloned(),
            replacement_sha256: Some(artifact.sha256().clone()),
            risk_level: assessment.risk_level,
            requires_backup: true,
            requires_elevation: assessment.requires_elevation,
            artifact_id: artifact.id().clone(),
            blockers: assessment.blockers,
            warnings: assessment.warnings,
        }
    }

    /// Returns the generated operation identifier.
    pub fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }

    /// Returns the affected game identifier.
    pub fn game_id(&self) -> &renderpilot_domain::GameId {
        &self.game_id
    }

    /// Returns the planned operation type.
    pub fn operation_type(&self) -> &OperationKind {
        &self.operation_type
    }

    /// Returns the file path that would be replaced.
    pub fn target_path(&self) -> &PathRef {
        &self.target_path
    }

    /// Returns the artifact path that would be copied into place.
    pub fn replacement_path(&self) -> &PathRef {
        &self.replacement_path
    }

    /// Returns the currently detected version, when known.
    pub fn original_version(&self) -> Option<&Version> {
        self.original_version.as_ref()
    }

    /// Returns the selected artifact version, when known.
    pub fn replacement_version(&self) -> Option<&Version> {
        self.replacement_version.as_ref()
    }

    /// Returns the currently detected file hash, when known.
    pub fn original_sha256(&self) -> Option<&Sha256Hash> {
        self.original_sha256.as_ref()
    }

    /// Returns the selected artifact hash, when known.
    pub fn replacement_sha256(&self) -> Option<&Sha256Hash> {
        self.replacement_sha256.as_ref()
    }

    /// Returns the derived risk level of this plan.
    pub fn risk_level(&self) -> OperationPlanRiskLevel {
        self.risk_level
    }

    /// Returns whether a backup is required before execution.
    pub const fn requires_backup(&self) -> bool {
        self.requires_backup
    }

    /// Returns whether the target path likely requires elevation.
    pub const fn requires_elevation(&self) -> bool {
        self.requires_elevation
    }

    /// Returns the selected artifact identifier.
    pub fn artifact_id(&self) -> &ArtifactId {
        &self.artifact_id
    }

    /// Returns blockers that prevent execution.
    pub fn blockers(&self) -> &[OperationPlanBlocker] {
        &self.blockers
    }

    /// Returns warnings that should be shown before execution.
    pub fn warnings(&self) -> &[OperationPlanWarning] {
        &self.warnings
    }
}

/// Risk level assigned to a swap operation plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationPlanRiskLevel {
    /// No special risk beyond the standard backup flow.
    Low,
    /// User should review the plan before execution.
    Medium,
    /// User confirmation is strongly required.
    High,
    /// The plan is blocked and should not be executed.
    Blocked,
}

impl OperationPlanRiskLevel {
    /// Returns the stable text form used by CLI output.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Blocked => "blocked",
        }
    }
}

/// Condition that blocks a planned swap operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationPlanBlocker {
    /// Component and artifact technologies do not match.
    TechnologyMismatch,
    /// Component is marked read-only.
    ComponentReadOnly,
    /// Component is integrated into the engine and should not be swapped directly.
    ComponentIntegratedIntoEngine,
    /// Component is explicitly marked unsafe.
    ComponentUnsafe,
    /// Selected artifact matches the currently installed file hash.
    ArtifactMatchesCurrentFile,
}

impl OperationPlanBlocker {
    /// Returns the stable text form used by CLI output.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TechnologyMismatch => "technology_mismatch",
            Self::ComponentReadOnly => "component_read_only",
            Self::ComponentIntegratedIntoEngine => "component_integrated_into_engine",
            Self::ComponentUnsafe => "component_unsafe",
            Self::ArtifactMatchesCurrentFile => "artifact_matches_current_file",
        }
    }
}

/// Warning that should be surfaced before executing a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationPlanWarning {
    /// Component swap requires explicit confirmation because it is not independently swappable.
    ConfirmationRequiredForSwappability,
    /// Streamline single-file replacement is a partial swap and needs an explicit warning.
    StreamlinePartialSwap,
    /// One or both versions are unknown and the user must compare manually.
    ManualVersionComparisonRequired,
}

impl OperationPlanWarning {
    /// Returns the stable text form used by CLI output.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConfirmationRequiredForSwappability => "confirmation_required_for_swappability",
            Self::StreamlinePartialSwap => "streamline_partial_swap",
            Self::ManualVersionComparisonRequired => "manual_version_comparison_required",
        }
    }

    const fn raises_risk_to_high(self) -> bool {
        matches!(
            self,
            Self::ConfirmationRequiredForSwappability | Self::StreamlinePartialSwap
        )
    }
}

fn component_target_file(component: &GraphicsComponent) -> AppResult<&ComponentFile> {
    component.files().first().ok_or_else(|| {
        AppError::invalid_input(format!(
            "component {} does not contain a target file",
            component.id().as_str()
        ))
    })
}

fn generated_operation_id(
    component: &GraphicsComponent,
    artifact: &LibraryArtifact,
) -> OperationId {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);

    OperationId::new(format!(
        "operation:{}:{}:{}:{}",
        OperationKind::REPLACE_COMPONENT,
        timestamp,
        component.id().as_str(),
        artifact.id().as_str()
    ))
    .expect("generated operation id should never be empty")
}

fn derive_risk_level(
    blockers: &[OperationPlanBlocker],
    warnings: &[OperationPlanWarning],
    requires_elevation: bool,
) -> OperationPlanRiskLevel {
    if !blockers.is_empty() {
        return OperationPlanRiskLevel::Blocked;
    }

    if warnings
        .iter()
        .copied()
        .any(OperationPlanWarning::raises_risk_to_high)
    {
        return OperationPlanRiskLevel::High;
    }

    if requires_elevation || !warnings.is_empty() {
        return OperationPlanRiskLevel::Medium;
    }

    OperationPlanRiskLevel::Low
}

fn path_requires_elevation(path: &PathRef) -> bool {
    let lower = path.as_str().to_ascii_lowercase();
    let Some((_, tail)) = lower.split_once(":/") else {
        return false;
    };

    PROTECTED_WINDOWS_ROOTS.iter().any(|root| {
        tail.strip_prefix(root)
            .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with('/'))
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

fn is_streamline_partial_swap(component: &GraphicsComponent) -> bool {
    component.swappability() == Swappability::BundleOnly
        && component.technology() == GraphicsTechnology::NvidiaStreamline
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{
        ArtifactId, ArtifactTrustLevel, ComponentFile, ComponentId, ComponentKind, GameId,
        GraphicsComponent, GraphicsTechnology, LibraryArtifact, PathRef, Sha256Hash, Swappability,
        Version,
    };

    use super::{
        build_swap_operation_plan, OperationPlanBlocker, OperationPlanRiskLevel,
        OperationPlanWarning,
    };

    #[test]
    fn builds_valid_swap_plan_for_swappable_component() {
        let component = sample_component(
            "component:game-a:dlss",
            "game:a",
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            "C:/Games/GameA/nvngx_dlss.dll",
            Some("3.5.0"),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        );
        let artifact = sample_artifact(
            "artifact:dlss-3.7",
            GraphicsTechnology::DlssSuperResolution,
            "D:/Library/nvngx_dlss.dll",
            Some("3.7.0"),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );

        let plan = build_swap_operation_plan(&component, &artifact).expect("plan should build");

        assert_eq!(plan.game_id(), component.game_id());
        assert_eq!(plan.operation_type().as_str(), "replace_component");
        assert_eq!(plan.target_path().as_str(), "C:/Games/GameA/nvngx_dlss.dll");
        assert_eq!(
            plan.replacement_path().as_str(),
            "D:/Library/nvngx_dlss.dll"
        );
        assert_eq!(plan.original_version().map(Version::as_str), Some("3.5.0"));
        assert_eq!(
            plan.replacement_version().map(Version::as_str),
            Some("3.7.0")
        );
        assert_eq!(
            plan.original_sha256().map(Sha256Hash::as_str),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert_eq!(
            plan.replacement_sha256().map(Sha256Hash::as_str),
            Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        );
        assert_eq!(plan.risk_level(), OperationPlanRiskLevel::Low);
        assert!(plan.requires_backup());
        assert!(!plan.requires_elevation());
        assert!(plan.blockers().is_empty());
        assert!(plan.warnings().is_empty());
    }

    #[test]
    fn blocks_technology_mismatch() {
        let component = sample_component(
            "component:game-a:dlss",
            "game:a",
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            "C:/Games/GameA/nvngx_dlss.dll",
            Some("3.5.0"),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        );
        let artifact = sample_artifact(
            "artifact:fg-3.7",
            GraphicsTechnology::DlssFrameGeneration,
            "D:/Library/nvngx_dlssg.dll",
            Some("3.7.0"),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );

        let plan = build_swap_operation_plan(&component, &artifact).expect("plan should build");

        assert_eq!(plan.risk_level(), OperationPlanRiskLevel::Blocked);
        assert!(plan
            .blockers()
            .contains(&OperationPlanBlocker::TechnologyMismatch));
    }

    #[test]
    fn blocks_invalid_artifact_with_same_hash() {
        let component = sample_component(
            "component:game-a:dlss",
            "game:a",
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            "C:/Games/GameA/nvngx_dlss.dll",
            Some("3.5.0"),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        );
        let artifact = sample_artifact(
            "artifact:same",
            GraphicsTechnology::DlssSuperResolution,
            "D:/Library/nvngx_dlss.dll",
            Some("3.5.0"),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        );

        let plan = build_swap_operation_plan(&component, &artifact).expect("plan should build");

        assert_eq!(plan.risk_level(), OperationPlanRiskLevel::Blocked);
        assert!(plan
            .blockers()
            .contains(&OperationPlanBlocker::ArtifactMatchesCurrentFile));
    }

    #[test]
    fn non_swappable_component_requires_confirmation_or_blocks() {
        let bundle_only_component = sample_component(
            "component:game-a:streamline",
            "game:a",
            GraphicsTechnology::NvidiaStreamline,
            Swappability::BundleOnly,
            "C:/Games/GameA/sl.interposer.dll",
            Some("2.4.0"),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        );
        let unsafe_component = sample_component(
            "component:game-a:unsafe",
            "game:a",
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Unsafe,
            "C:/Games/GameA/nvngx_dlss.dll",
            Some("3.5.0"),
            Some("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"),
        );
        let streamline_artifact = sample_artifact(
            "artifact:streamline-2.5",
            GraphicsTechnology::NvidiaStreamline,
            "D:/Library/sl.interposer.dll",
            Some("2.5.0"),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );
        let dlss_artifact = sample_artifact(
            "artifact:dlss-3.7",
            GraphicsTechnology::DlssSuperResolution,
            "D:/Library/nvngx_dlss.dll",
            Some("3.7.0"),
            "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        );

        let bundle_only_plan =
            build_swap_operation_plan(&bundle_only_component, &streamline_artifact)
                .expect("plan should build");
        let unsafe_plan = build_swap_operation_plan(&unsafe_component, &dlss_artifact)
            .expect("plan should build");

        assert_eq!(bundle_only_plan.risk_level(), OperationPlanRiskLevel::High);
        assert!(bundle_only_plan
            .warnings()
            .contains(&OperationPlanWarning::ConfirmationRequiredForSwappability));
        assert_eq!(unsafe_plan.risk_level(), OperationPlanRiskLevel::Blocked);
        assert!(unsafe_plan
            .blockers()
            .contains(&OperationPlanBlocker::ComponentUnsafe));
    }

    #[test]
    fn streamline_partial_swap_gets_warning() {
        let component = sample_component(
            "component:game-a:streamline",
            "game:a",
            GraphicsTechnology::NvidiaStreamline,
            Swappability::BundleOnly,
            "C:/Games/GameA/sl.interposer.dll",
            Some("2.4.0"),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        );
        let artifact = sample_artifact(
            "artifact:streamline-2.5",
            GraphicsTechnology::NvidiaStreamline,
            "D:/Library/sl.interposer.dll",
            Some("2.5.0"),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );

        let plan = build_swap_operation_plan(&component, &artifact).expect("plan should build");

        assert!(plan
            .warnings()
            .contains(&OperationPlanWarning::StreamlinePartialSwap));
    }

    #[test]
    fn missing_versions_require_manual_review_and_medium_risk() {
        let component = sample_component(
            "component:game-a:dlss",
            "game:a",
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            "C:/Games/GameA/nvngx_dlss.dll",
            None,
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        );
        let artifact = sample_artifact(
            "artifact:dlss-3.7",
            GraphicsTechnology::DlssSuperResolution,
            "D:/Library/nvngx_dlss.dll",
            Some("3.7.0"),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );

        let plan = build_swap_operation_plan(&component, &artifact).expect("plan should build");

        assert_eq!(plan.risk_level(), OperationPlanRiskLevel::Medium);
        assert!(plan
            .warnings()
            .contains(&OperationPlanWarning::ManualVersionComparisonRequired));
    }

    #[test]
    fn protected_windows_paths_require_elevation() {
        let component = sample_component(
            "component:game-a:dlss",
            "game:a",
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            "C:/Program Files/GameA/nvngx_dlss.dll",
            Some("3.5.0"),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        );
        let artifact = sample_artifact(
            "artifact:dlss-3.7",
            GraphicsTechnology::DlssSuperResolution,
            "D:/Library/nvngx_dlss.dll",
            Some("3.7.0"),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );

        let plan = build_swap_operation_plan(&component, &artifact).expect("plan should build");

        assert!(plan.requires_elevation());
        assert_eq!(plan.risk_level(), OperationPlanRiskLevel::Medium);
    }

    fn sample_component(
        component_id: &str,
        game_id: &str,
        technology: GraphicsTechnology,
        swappability: Swappability,
        path: &str,
        version: Option<&str>,
        sha256: Option<&str>,
    ) -> GraphicsComponent {
        let mut file =
            ComponentFile::new(PathRef::new(path).expect("component path should be valid"));

        if let Some(version) = version {
            file = file.with_version(Version::parse(version).expect("version should be valid"));
        }

        if let Some(sha256) = sha256 {
            file = file.with_sha256(Sha256Hash::new(sha256).expect("sha256 should be valid"));
        }

        GraphicsComponent::new(
            ComponentId::new(component_id).expect("component id should be valid"),
            GameId::new(game_id).expect("game id should be valid"),
            ComponentKind::NativeLibrary,
            technology,
            swappability,
        )
        .with_file(file)
    }

    fn sample_artifact(
        artifact_id: &str,
        technology: GraphicsTechnology,
        path: &str,
        version: Option<&str>,
        sha256: &str,
    ) -> LibraryArtifact {
        let file_name = std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .expect("artifact path should contain a file name");
        let mut file =
            ComponentFile::new(PathRef::new(path).expect("artifact path should be valid"))
                .with_sha256(Sha256Hash::new(sha256).expect("sha256 should be valid"));

        if let Some(version) = version {
            file = file.with_version(Version::parse(version).expect("version should be valid"));
        }

        LibraryArtifact::new(
            ArtifactId::new(artifact_id).expect("artifact id should be valid"),
            technology,
            file_name,
            file,
            ArtifactTrustLevel::LocalObserved,
        )
        .expect("artifact should be valid")
        .with_source("scan-folder")
        .expect("source should be valid")
    }
}
