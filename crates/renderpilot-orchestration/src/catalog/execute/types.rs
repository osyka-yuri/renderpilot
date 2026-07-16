//! Data types shared across the swap-execution submodules.

use std::path::PathBuf;

use renderpilot_domain::{ComponentFile, ComponentId, GameId, GraphicsComponent, LibraryArtifact};
use serde::Serialize;

/// Metadata recorded alongside a swap or rollback operation in the journal.
#[derive(Debug, Serialize)]
pub struct OperationMetadata {
    /// Human-readable game name at the time of the operation.
    pub game_name: String,
    /// Technology slug (e.g. `dlss`, `fsr`).
    pub library: String,
    /// Version string of the component before the operation, if known.
    pub from_version: Option<String>,
    /// Version string the component was swapped to.
    pub to_version: String,
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
}

/// Component, artifact and baseline loaded before an apply is planned.
pub(super) struct LoadedApplySwap {
    pub(super) component: GraphicsComponent,
    pub(super) artifact: LibraryArtifact,
    pub(super) baseline: Vec<ComponentFile>,
    pub(super) first_swap: bool,
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
    pub(super) planned: Vec<PlannedFile>,
    /// FSR split members the (unified) target abandons and must delete — see
    /// [`super::planning::fsr_members_to_remove`]. Empty for every non-downgrade swap.
    pub(super) removed: Vec<ComponentFile>,
    pub(super) first_swap: bool,
}

impl PreparedApplySwap {
    pub(super) fn applied_path(&self) -> String {
        self.artifact
            .files()
            .iter()
            .zip(&self.planned)
            .find_map(|(artifact_file, plan)| {
                artifact_file
                    .install_as()
                    .map(|_| plan.file.path().as_str().to_owned())
            })
            .or_else(|| {
                self.planned
                    .first()
                    .map(|plan| plan.file.path().as_str().to_owned())
            })
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
