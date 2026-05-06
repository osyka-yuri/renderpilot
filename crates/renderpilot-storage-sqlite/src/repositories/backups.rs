use renderpilot_application::{AppResult, BackupRecord, BackupRepository};
use renderpilot_domain::GameId;
use rusqlite::params;

use crate::error::storage_error;

use super::{row_mapping::backup_from_row, SqliteStorage};

impl BackupRepository for SqliteStorage {
    fn upsert_backup(&self, backup: &BackupRecord) -> AppResult<()> {
        let connection = self.connection()?;

        connection
            .execute(
                "INSERT INTO backups
                    (id, operation_id, game_id, component_id, original_path, backup_path, sha256,
                     created_at, metadata_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(id) DO UPDATE SET
                    operation_id = excluded.operation_id,
                    game_id = excluded.game_id,
                    component_id = excluded.component_id,
                    original_path = excluded.original_path,
                    backup_path = excluded.backup_path,
                    sha256 = excluded.sha256,
                    created_at = excluded.created_at,
                    metadata_json = excluded.metadata_json",
                params![
                    backup.id.as_str(),
                    backup.operation_id.as_str(),
                    backup.game_id.as_str(),
                    backup.component_id.as_ref().map(|id| id.as_str()),
                    backup.original_path.as_str(),
                    backup.backup_path.as_str(),
                    backup.sha256.as_ref().map(|sha256| sha256.as_str()),
                    backup.created_at.as_i64(),
                    backup.metadata_json.as_deref()
                ],
            )
            .map_err(storage_error)?;

        Ok(())
    }

    fn list_backups_for_game(&self, game_id: &GameId) -> AppResult<Vec<BackupRecord>> {
        self.query_list(
            "SELECT id, operation_id, game_id, component_id, original_path, backup_path,
                    sha256, created_at, metadata_json
             FROM backups
             WHERE game_id = ?1
             ORDER BY created_at, id",
            [game_id.as_str()],
            backup_from_row,
        )
    }
}
