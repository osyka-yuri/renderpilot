use renderpilot_domain::{GameId, LibraryComponent};

use crate::AppResult;

/// Repository port for storing detected graphics components.
pub trait ComponentRepository: Send + Sync {
    /// Replaces all detected components for a game with the latest scan result.
    fn replace_components_for_game(
        &self,
        game_id: &GameId,
        components: &[LibraryComponent],
    ) -> AppResult<()>;

    /// Lists detected components currently stored for a game.
    fn list_components_for_game(&self, game_id: &GameId) -> AppResult<Vec<LibraryComponent>>;
}
