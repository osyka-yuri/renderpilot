use renderpilot_domain::{GameId, GameInstallation};

use crate::AppResult;

/// Repository port for storing and loading game installations.
pub trait GameRepository: Send + Sync {
    /// Inserts or updates one game installation.
    fn upsert_game(&self, game: &GameInstallation) -> AppResult<()>;

    /// Inserts or updates several game installations.
    ///
    /// Implementations may override this method to provide transactional batching.
    fn upsert_games(&self, games: &[GameInstallation]) -> AppResult<()> {
        for game in games {
            self.upsert_game(game)?;
        }

        Ok(())
    }

    /// Loads a game installation by its stable ID.
    fn find_game(&self, id: &GameId) -> AppResult<Option<GameInstallation>>;
}
