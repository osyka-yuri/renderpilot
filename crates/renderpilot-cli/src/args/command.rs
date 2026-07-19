use std::path::PathBuf;

use renderpilot_orchestration::domain::{ArtifactId, ComponentId, GameId, GraphicsTechnology};
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Command {
    Summary,
    Help,
    Version,
    ScanFolder {
        path: PathBuf,
    },
    ListArtifacts {
        technology: Option<GraphicsTechnology>,
    },
    ListOperations {
        game_id: GameId,
    },
    Candidates {
        game_id: GameId,
    },
    PlanSwap {
        game_id: GameId,
        component_id: ComponentId,
        artifact_id: ArtifactId,
    },
    ApplyOperation {
        game_id: GameId,
        component_id: ComponentId,
        artifact_id: ArtifactId,
    },
    RollbackOperation {
        game_id: GameId,
        component_id: ComponentId,
    },
    RenodxStatus {
        game_id: GameId,
    },
    RenodxUninstall {
        game_id: GameId,
    },
}
