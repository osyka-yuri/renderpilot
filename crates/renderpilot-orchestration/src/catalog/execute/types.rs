//! Data types shared across the swap-execution submodules.

use std::path::{Path, PathBuf};

use renderpilot_application::{D3d12ExecutableAction, D3d12ExecutableActionKind};
use renderpilot_domain::{
    ComponentFile, ComponentId, ComponentRollbackBaseline, GameId, GraphicsComponent,
    LibraryArtifact, PathRef,
};
use serde::{Deserialize, Serialize};

/// Metadata recorded alongside a swap or rollback operation in the journal.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperationMetadata {
    /// Human-readable game name at the time of the operation.
    pub game_name: String,
    /// Technology slug (e.g. `dlss`, `fsr`).
    pub library: String,
    /// Version string of the component before the operation, if known.
    pub from_version: Option<String>,
    /// Version string the component was swapped to.
    pub to_version: String,
    /// EXE action recorded separately so legacy DLL item counts stay stable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub d3d12_executable_action: Option<D3d12ExecutableActionResult>,
}

/// Result of a successfully applied swap.
#[derive(Debug, Serialize)]
pub struct SwapResult {
    /// String form of the game id.
    pub game_id: String,
    /// String form of the component id.
    pub component_id: String,
    /// Install path of the primary applied file.
    pub applied_path: String,
    /// Source path of the artifact package that was installed.
    pub replacement_path: String,
    /// Number of live component files updated by the operation.
    pub updated_file_count: usize,
    /// Executable action actually completed by this swap.
    pub d3d12_executable_action: Option<D3d12ExecutableActionResult>,
}

/// Result of a successfully applied rollback.
#[derive(Debug, Serialize)]
pub struct RollbackResult {
    /// String form of the game id.
    pub game_id: String,
    /// String form of the component id.
    pub component_id: String,
    /// Path of the first restored baseline file.
    pub restored_path: String,
    /// Number of baseline files restored by the operation.
    pub restored_file_count: usize,
    /// Executable action actually completed by this rollback.
    pub d3d12_executable_action: Option<D3d12ExecutableActionResult>,
}

/// Read-only preflight for a component rollback.
#[derive(Debug, Clone)]
pub struct RollbackPlan {
    pub(crate) game_id: GameId,
    pub(crate) component_id: ComponentId,
    pub(crate) affected_files: Vec<PathRef>,
    pub(crate) d3d12_executable_action: Option<D3d12ExecutableAction>,
}

impl RollbackPlan {
    /// Returns the game receiving the rollback.
    pub const fn game_id(&self) -> &GameId {
        &self.game_id
    }

    /// Returns the component receiving the rollback.
    pub const fn component_id(&self) -> &ComponentId {
        &self.component_id
    }

    /// Returns all DLL/EXE paths touched by the rollback.
    pub fn affected_files(&self) -> &[PathRef] {
        &self.affected_files
    }

    /// Returns the executable action, when this aggregate tracks an EXE.
    pub const fn d3d12_executable_action(&self) -> Option<&D3d12ExecutableAction> {
        self.d3d12_executable_action.as_ref()
    }
}

/// Stable serialized record of an EXE action completed by apply/rollback.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct D3d12ExecutableActionResult {
    /// `patch` or `restore`.
    pub kind: D3d12ExecutableActionResultKind,
    /// Active executable path.
    pub executable_path: String,
    /// SDK line before the operation.
    pub from_sdk_version: u32,
    /// SDK line after the operation.
    pub to_sdk_version: u32,
    /// SDK line exported by the immutable original executable.
    pub original_sdk_version: u32,
}

/// Executable mutations that can appear in a completed operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum D3d12ExecutableActionResultKind {
    /// The active executable was patched to another supported SDK line.
    Patch,
    /// The immutable original executable was restored.
    Restore,
}

impl D3d12ExecutableActionResult {
    pub(super) fn from_action(action: &D3d12ExecutableAction) -> Option<Self> {
        let kind = match action.kind() {
            D3d12ExecutableActionKind::Patch => D3d12ExecutableActionResultKind::Patch,
            D3d12ExecutableActionKind::Restore => D3d12ExecutableActionResultKind::Restore,
            D3d12ExecutableActionKind::None | D3d12ExecutableActionKind::RepairRequired => {
                return None;
            }
        };
        Some(Self {
            kind,
            executable_path: action.executable_path().as_str().to_owned(),
            from_sdk_version: action.current_sdk_version(),
            to_sdk_version: action.target_sdk_version(),
            original_sdk_version: action.original_sdk_version(),
        })
    }
}

/// Fully prepared apply state, ready for the filesystem and storage steps.
///
/// Component rows written to the catalog are rebuilt after the FS overlay so
/// they can rebind PE version / hash from the installed files.
pub(super) struct PreparedApplySwap {
    pub(super) game_id: GameId,
    pub(super) component_id: ComponentId,
    pub(super) component: GraphicsComponent,
    pub(super) artifact: LibraryArtifact,
    pub(super) baseline: Vec<ComponentFile>,
    pub(super) rollback_baseline: Option<ComponentRollbackBaseline>,
    pub(super) planned: Vec<PlannedFile>,
    /// FSR split members the (unified) target abandons and must delete — see
    /// [`super::planning::fsr_members_to_remove`]. Empty for every non-downgrade swap.
    pub(super) removed: Vec<ComponentFile>,
    pub(super) first_swap: bool,
    /// Complete mutation-grade D3D12 context, when the swap manages an EXE.
    pub(super) d3d12: Option<PreparedD3d12Execution>,
}

/// State-bound D3D12 inputs prepared once before filesystem execution.
pub(super) struct PreparedD3d12Execution {
    pub(super) state: crate::catalog::runtime_compatibility::D3d12ExecutableState,
    pub(super) action: D3d12ExecutableAction,
    /// Fresh plan token; the action deliberately carries no confirmation state.
    pub(super) confirmation_token: String,
}

impl PreparedApplySwap {
    pub(super) fn applied_path(&self) -> String {
        for artifact_file in self
            .artifact
            .files()
            .iter()
            .filter(|file| file.install_as().is_some())
        {
            let source = Path::new(artifact_file.path().as_str());
            if let Some(plan) = self.planned.iter().find(|plan| plan.source == source) {
                return plan.file.path().as_str().to_owned();
            }
        }

        self.planned
            .first()
            .map(|plan| plan.file.path().as_str().to_owned())
            .unwrap_or_default()
    }

    pub(super) fn replacement_path(&self) -> String {
        self.artifact.path().as_str().to_owned()
    }
}

/// One artifact file resolved to where it will be installed.
#[derive(Debug)]
pub(super) struct PlannedFile {
    /// Source artifact file on disk to copy from.
    pub(super) source: PathBuf,
    /// The component file the install target becomes (its path is the install
    /// target; `install_as` is cleared because it is now in place).
    pub(super) file: ComponentFile,
}

impl PlannedFile {
    pub(super) fn target(&self) -> PathBuf {
        PathBuf::from(self.file.path().as_str())
    }
}

/// Records the filesystem paths touched by an overlay so directory fsync can
/// flush them after apply. The durable receipt is the [`DurableFileTransaction`]
/// manifest — this struct only tracks sidecars/copies for best-effort fsync.
///
/// Pre-mutation validation of the path set is performed once by
/// `DurableFileTransaction::prepare` (→ `build_manifest`) in the production apply
/// path. Tests that call `perform_apply_fs` directly rely on the
/// `DurableFileTransaction` they prepare (or on their controlled fixtures) for
/// validation, so no separate capture pass is needed here.
#[derive(Default)]
pub(super) struct AppliedFsLog {
    /// Classic sidecars created by this apply (target, sidecar).
    pub(super) created_sidecars: Vec<(PathBuf, PathBuf)>,
    /// Files copied into place.
    pub(super) copied: Vec<PathBuf>,
}

#[cfg(test)]
mod tests {
    use renderpilot_application::D3d12ExecutableProfile;

    use super::*;

    #[test]
    fn managed_rollback_restore_is_reported_even_without_confirmation() {
        let context = D3d12ExecutableProfile::new(
            PathRef::new("C:/Game/game.exe").expect("EXE path"),
            PathRef::new("C:/Game/game.exe.bak").expect("backup path"),
            606,
            619,
            true,
            false,
        );
        let action =
            D3d12ExecutableAction::for_managed_rollback(&context).expect("restore assessment");

        assert!(!action.requires_confirmation());
        let result =
            D3d12ExecutableActionResult::from_action(&action).expect("executed restore result");
        assert_eq!(result.kind, D3d12ExecutableActionResultKind::Restore);
        assert_eq!(result.from_sdk_version, 619);
        assert_eq!(result.to_sdk_version, 606);
    }
}
