use renderpilot_domain::GameId;

use crate::{AppResult, BackupRecord};

/// Repository port for backup records created before file operations.
pub trait BackupRepository: Send + Sync {
    /// Inserts or updates one backup record.
    fn upsert_backup(&self, backup: &BackupRecord) -> AppResult<()>;

    /// Lists backup records for a game.
    fn list_backups_for_game(&self, game_id: &GameId) -> AppResult<Vec<BackupRecord>>;
}
