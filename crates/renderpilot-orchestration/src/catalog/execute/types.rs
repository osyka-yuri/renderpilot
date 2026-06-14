//! Data types shared across the swap-execution submodules.

use std::fs;
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
    pub(super) next_components: Vec<GraphicsComponent>,
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

    pub(super) fn target_bak(&self) -> PathBuf {
        PathBuf::from(format!("{}.bak", self.file.path().as_str()))
    }
}

/// Records the filesystem mutations of an overlay so they can be undone on failure.
#[derive(Default)]
pub(super) struct AppliedFsChanges {
    /// Files renamed to `.bak` (target, bak) before being overwritten.
    pub(super) renamed_to_bak: Vec<(PathBuf, PathBuf)>,
    /// Files copied into place.
    pub(super) copied: Vec<PathBuf>,
}

impl AppliedFsChanges {
    /// Best-effort reversal of the overlay: remove copies, restore backups.
    ///
    /// A re-swap's revert-to-baseline (run before the overlay) is intentionally
    /// not tracked here — the recorded baseline `.bak` files remain intact, so a
    /// later `rollback` always recovers the original.
    ///
    /// Reversal is best-effort by necessity (the disk may be full, a file may be
    /// locked by anti-virus), but every failure is logged at error level: a swap
    /// whose rollback could not complete leaves the game folder in a mixed state,
    /// and that must be diagnosable rather than silently swallowed.
    pub(super) fn undo(&self) {
        for copied in &self.copied {
            if let Err(error) = fs::remove_file(copied) {
                log::error!(
                    "swap rollback: failed to remove copied file {}: {error}",
                    copied.display()
                );
            }
        }
        for (target, bak) in &self.renamed_to_bak {
            if let Err(error) = fs::rename(bak, target) {
                log::error!(
                    "swap rollback: failed to restore backup {} to {}: {error}",
                    bak.display(),
                    target.display()
                );
            }
        }
    }
}
