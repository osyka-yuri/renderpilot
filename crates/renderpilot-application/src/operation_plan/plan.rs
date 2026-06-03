use renderpilot_domain::{
    ArtifactId, ComponentFile, GameId, GraphicsComponent, LibraryArtifact, OperationId, PathRef,
    Sha256Hash, Version,
};

use crate::OperationKind;

use super::{
    OperationPlanAssessment, OperationPlanBlocker, OperationPlanIdentity, OperationPlanRiskLevel,
    OperationPlanWarning,
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
    requires_elevation: bool,
    artifact_id: ArtifactId,
    blockers: Vec<OperationPlanBlocker>,
    warnings: Vec<OperationPlanWarning>,
    files: Vec<OperationPlanFile>,
}

impl OperationPlan {
    pub(crate) fn new(
        component: &GraphicsComponent,
        artifact: &LibraryArtifact,
        target_file: &ComponentFile,
        files: Vec<OperationPlanFile>,
        assessment: OperationPlanAssessment,
        identity: OperationPlanIdentity,
    ) -> Self {
        let OperationPlanAssessment {
            blockers,
            warnings,
            risk_level,
            requires_elevation,
        } = assessment;
        let OperationPlanIdentity {
            operation_id,
            confirmation_token,
        } = identity;

        Self {
            operation_id,
            confirmation_token,
            game_id: component.game_id().clone(),
            operation_type: OperationKind::ReplaceComponent,
            target_path: target_file.path().clone(),
            replacement_path: artifact.path().clone(),
            original_version: target_file.version().cloned(),
            replacement_version: artifact.version().cloned(),
            original_sha256: target_file.sha256().cloned(),
            replacement_sha256: Some(artifact.sha256().clone()),
            risk_level,
            requires_elevation,
            artifact_id: artifact.id().clone(),
            blockers,
            warnings,
            files,
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

    /// Returns every file the swap would write, add, or remove.
    ///
    /// For a single-file swap this contains one [`OperationPlanFile`]; for a
    /// bundle it enumerates the whole set so the UI can show, e.g., "1 replaced,
    /// 2 added".
    pub fn files(&self) -> &[OperationPlanFile] {
        &self.files
    }
}

/// What a swap will do to one file in the package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationPlanFileAction {
    /// An existing file at the install target is backed up and replaced.
    Replace,
    /// A new file is added at an install target the component did not have.
    Add,
}

impl OperationPlanFileAction {
    /// Returns the stable text form used by CLI output.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Replace => "replace",
            Self::Add => "add",
        }
    }
}

/// One file affected by a planned bundle swap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationPlanFile {
    action: OperationPlanFileAction,
    target_path: PathRef,
    replacement_path: Option<PathRef>,
    original_version: Option<Version>,
    replacement_version: Option<Version>,
    original_sha256: Option<Sha256Hash>,
    replacement_sha256: Option<Sha256Hash>,
}

impl OperationPlanFile {
    /// An artifact file replaces an existing component file of the same name.
    pub(crate) fn replace(current: &ComponentFile, artifact_file: &ComponentFile) -> Self {
        Self {
            action: OperationPlanFileAction::Replace,
            target_path: current.path().clone(),
            replacement_path: Some(artifact_file.path().clone()),
            original_version: current.version().cloned(),
            replacement_version: artifact_file.version().cloned(),
            original_sha256: current.sha256().cloned(),
            replacement_sha256: artifact_file.sha256().cloned(),
        }
    }

    /// A new artifact file is added at `target_path` (no prior component file).
    pub(crate) fn add(target_path: PathRef, artifact_file: &ComponentFile) -> Self {
        Self {
            action: OperationPlanFileAction::Add,
            target_path,
            replacement_path: Some(artifact_file.path().clone()),
            original_version: None,
            replacement_version: artifact_file.version().cloned(),
            original_sha256: None,
            replacement_sha256: artifact_file.sha256().cloned(),
        }
    }

    /// Returns what the swap does to this file.
    pub fn action(&self) -> OperationPlanFileAction {
        self.action
    }

    /// Returns the on-disk path that will be written, added, or removed.
    pub fn target_path(&self) -> &PathRef {
        &self.target_path
    }

    /// Returns the source artifact path copied into place, when applicable.
    pub fn replacement_path(&self) -> Option<&PathRef> {
        self.replacement_path.as_ref()
    }

    /// Returns the currently installed version of this file, when known.
    pub fn original_version(&self) -> Option<&Version> {
        self.original_version.as_ref()
    }

    /// Returns the replacement version of this file, when known.
    pub fn replacement_version(&self) -> Option<&Version> {
        self.replacement_version.as_ref()
    }

    /// Returns the currently installed hash of this file, when known.
    pub fn original_sha256(&self) -> Option<&Sha256Hash> {
        self.original_sha256.as_ref()
    }

    /// Returns the replacement hash of this file, when known.
    pub fn replacement_sha256(&self) -> Option<&Sha256Hash> {
        self.replacement_sha256.as_ref()
    }
}
