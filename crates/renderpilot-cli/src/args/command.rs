use std::path::PathBuf;

use renderpilot_orchestration::domain::{ArtifactId, ComponentId, GameId, LibraryTechnology};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AddGameRootChoiceArg {
    Auto,
    Selected,
    Recommended,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Command {
    Summary,
    Help,
    Version,
    AddGame {
        path: PathBuf,
        executable: Option<PathBuf>,
        root_choice: AddGameRootChoiceArg,
        allow_root_correction: bool,
    },
    ListArtifacts {
        technology: Option<LibraryTechnology>,
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
        confirmation_token: Option<String>,
        safety_context_token: Option<String>,
    },
    PlanRollback {
        game_id: GameId,
        component_id: ComponentId,
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
    RenodxCheckUpdate {
        game_id: GameId,
    },
    RenodxCheckUpdates,
    LumaStatus {
        game_id: GameId,
    },
    LumaUninstall {
        game_id: GameId,
    },
    LumaCheckUpdate {
        game_id: GameId,
        deep: bool,
    },
    LumaCheckUpdates,
}
