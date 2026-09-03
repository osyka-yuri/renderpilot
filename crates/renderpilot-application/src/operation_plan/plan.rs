use renderpilot_domain::{
    ArtifactId, ComponentFile, ComponentId, GameId, LibraryArtifact, LibraryComponent, OperationId,
    PathRef, Sha256Hash, Version,
};

use crate::{D3d12ExecutableAction, D3d12ExecutableActionKind, OperationKind};

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
    component_id: ComponentId,
    operation_type: OperationKind,
    target_path: PathRef,
    replacement_path: PathRef,
    original_version: Option<Version>,
    replacement_version: Option<Version>,
    original_sha256: Option<Sha256Hash>,
    replacement_sha256: Option<Sha256Hash>,
    risk_level: OperationPlanRiskLevel,
    artifact_id: ArtifactId,
    blockers: Vec<OperationPlanBlocker>,
    warnings: Vec<OperationPlanWarning>,
    files: Vec<OperationPlanFile>,
    d3d12_executable_action: Option<D3d12ExecutableAction>,
}

impl OperationPlan {
    pub(crate) fn new(
        component: &LibraryComponent,
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
        } = assessment;
        let OperationPlanIdentity {
            operation_id,
            confirmation_token,
        } = identity;

        Self {
            operation_id,
            confirmation_token,
            game_id: component.game_id().clone(),
            component_id: component.id().clone(),
            operation_type: OperationKind::ReplaceComponent,
            target_path: target_file.path().clone(),
            replacement_path: artifact.path().clone(),
            original_version: target_file.version().cloned(),
            replacement_version: artifact.version().cloned(),
            original_sha256: target_file.sha256().cloned(),
            replacement_sha256: Some(artifact.sha256().clone()),
            risk_level,
            artifact_id: artifact.id().clone(),
            blockers,
            warnings,
            files,
            d3d12_executable_action: None,
        }
    }

    /// Attaches the fresh D3D12 executable assessment and canonical fingerprint.
    #[must_use]
    pub fn with_d3d12_executable_action(
        mut self,
        action: D3d12ExecutableAction,
        confirmation_token: String,
        current_sha256: Sha256Hash,
        target_sha256: Option<Sha256Hash>,
    ) -> Self {
        self.confirmation_token = confirmation_token;
        match action.kind() {
            D3d12ExecutableActionKind::Patch => {
                self.files.push(OperationPlanFile::executable(
                    OperationPlanFileAction::PatchExecutable,
                    &action,
                    current_sha256,
                    target_sha256,
                ));
                if action.current_sdk_version() == action.original_sdk_version() {
                    self.warnings
                        .push(OperationPlanWarning::ExecutableSignatureMayBecomeInvalid);
                }
            }
            D3d12ExecutableActionKind::Restore => {
                self.files.push(OperationPlanFile::executable(
                    OperationPlanFileAction::RestoreExecutable,
                    &action,
                    current_sha256,
                    target_sha256,
                ));
            }
            D3d12ExecutableActionKind::RepairRequired => {
                let _ = self.insert_blocker(OperationPlanBlocker::D3d12ExecutableRepairRequired);
            }
            D3d12ExecutableActionKind::None => {}
        }
        self.recalculate_risk();
        self.d3d12_executable_action = Some(action);
        self
    }

    /// Adds a prerequisite blocker discovered after the pure plan was built.
    ///
    /// Platform and environment checks intentionally happen at orchestration
    /// boundaries. Keeping their enrichment here preserves blocker
    /// de-duplication and risk recalculation as `OperationPlan` invariants.
    #[must_use]
    pub fn with_prerequisite_blocker(mut self, blocker: OperationPlanBlocker) -> Self {
        if self.insert_blocker(blocker) {
            self.recalculate_risk();
        }
        self
    }

    fn insert_blocker(&mut self, blocker: OperationPlanBlocker) -> bool {
        if !self.blockers.contains(&blocker) {
            self.blockers.push(blocker);
            return true;
        }
        false
    }

    fn recalculate_risk(&mut self) {
        self.risk_level = OperationPlanRiskLevel::from_findings(&self.blockers, &self.warnings);
    }

    /// Returns the generated operation identifier.
    pub fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }

    /// Returns the fresh confirmation token bound to the planned state.
    pub fn confirmation_token(&self) -> &str {
        &self.confirmation_token
    }

    /// Returns the affected game identifier.
    pub fn game_id(&self) -> &GameId {
        &self.game_id
    }

    /// Returns the affected component identifier.
    pub fn component_id(&self) -> &ComponentId {
        &self.component_id
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

    /// Returns the D3D12 executable action included in this operation.
    pub const fn d3d12_executable_action(&self) -> Option<&D3d12ExecutableAction> {
        self.d3d12_executable_action.as_ref()
    }
}

/// What a swap will do to one file in the package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationPlanFileAction {
    /// An existing file at the install target is backed up and replaced.
    Replace,
    /// A new file is added at an install target the component did not have.
    Add,
    /// Preserve an immutable original in its sidecar, then remove the live
    /// target from the active component.
    ArchiveAndRemove,
    /// Remove a current unowned addition without creating a baseline sidecar.
    Remove,
    /// Patch the inline `D3D12SDKVersion` value in the main EXE.
    PatchExecutable,
    /// Restore the original main EXE from its immutable backup.
    RestoreExecutable,
}

impl OperationPlanFileAction {
    /// Returns the stable text form used by CLI output.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Replace => "replace",
            Self::Add => "add",
            Self::ArchiveAndRemove => "archive_and_remove",
            Self::Remove => "remove",
            Self::PatchExecutable => "patch_executable",
            Self::RestoreExecutable => "restore_executable",
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

    /// An immutable original is sidecar-preserved and removed from the active
    /// set. There is intentionally no replacement source.
    pub(crate) fn archive_and_remove(target_path: PathRef, original: &ComponentFile) -> Self {
        Self {
            action: OperationPlanFileAction::ArchiveAndRemove,
            target_path,
            replacement_path: None,
            original_version: original.version().cloned(),
            replacement_version: None,
            original_sha256: original.sha256().cloned(),
            replacement_sha256: None,
        }
    }

    /// A current addition with no immutable original is removed. There is
    /// intentionally no replacement source.
    pub(crate) fn remove(target_path: PathRef, current: &ComponentFile) -> Self {
        Self {
            action: OperationPlanFileAction::Remove,
            target_path,
            replacement_path: None,
            original_version: current.version().cloned(),
            replacement_version: None,
            original_sha256: current.sha256().cloned(),
            replacement_sha256: None,
        }
    }

    fn executable(
        file_action: OperationPlanFileAction,
        action: &D3d12ExecutableAction,
        current_sha256: Sha256Hash,
        target_sha256: Option<Sha256Hash>,
    ) -> Self {
        Self {
            action: file_action,
            target_path: action.executable_path().clone(),
            replacement_path: Some(action.backup_path().clone()),
            original_version: None,
            replacement_version: None,
            original_sha256: Some(current_sha256),
            replacement_sha256: target_sha256,
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
