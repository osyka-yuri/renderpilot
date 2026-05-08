use renderpilot_domain::{
    ArtifactId, ComponentFile, GameId, GraphicsComponent, LibraryArtifact, OperationId, PathRef,
    Sha256Hash, Version,
};

use crate::OperationKind;

use super::builder::{generate_confirmation_token, generate_operation_id, REQUIRES_BACKUP};
use super::{
    OperationPlanAssessment, OperationPlanBlocker, OperationPlanRiskLevel, OperationPlanWarning,
};

/// Read-only operation plan for a DLL replacement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationPlan {
    operation_id: OperationId,
    confirmation_token: String,
    game_id: GameId,
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
    pub(crate) fn new(
        component: &GraphicsComponent,
        artifact: &LibraryArtifact,
        target_file: &ComponentFile,
        assessment: OperationPlanAssessment,
    ) -> Self {
        let OperationPlanAssessment {
            blockers,
            warnings,
            risk_level,
            requires_elevation,
        } = assessment;

        Self {
            operation_id: generate_operation_id(component, artifact),
            confirmation_token: generate_confirmation_token(),
            game_id: component.game_id().clone(),
            operation_type: OperationKind::ReplaceComponent,
            target_path: target_file.path().clone(),
            replacement_path: artifact.path().clone(),
            original_version: target_file.version().cloned(),
            replacement_version: artifact.version().cloned(),
            original_sha256: target_file.sha256().cloned(),
            replacement_sha256: Some(artifact.sha256().clone()),
            risk_level,
            requires_backup: REQUIRES_BACKUP,
            requires_elevation,
            artifact_id: artifact.id().clone(),
            blockers,
            warnings,
        }
    }

    /// Returns the generated operation identifier.
    pub fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }

    /// Returns the one-time confirmation token.
    pub fn confirmation_token(&self) -> &str {
        &self.confirmation_token
    }

    /// Returns the affected game identifier.
    pub fn game_id(&self) -> &GameId {
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

    /// Returns the selected artifact hash.
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
