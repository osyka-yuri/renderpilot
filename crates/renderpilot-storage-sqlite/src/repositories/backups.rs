use renderpilot_application::{AppResult, BackupRecord, BackupRepository};
use renderpilot_domain::GameId;
use rusqlite::named_params;

use crate::error::storage_error;

use super::{
    catalog_select_sql::LIST_BACKUPS_FOR_GAME_SQL, row_mapping::backup_from_row, SqliteStorage,
};

const UPSERT_BACKUP_SQL: &str = r#"
    INSERT INTO backups (
        id,
        operation_id,
        game_id,
        component_id,
        original_path,
        backup_path,
        sha256,
        created_at,
        metadata_json
    )
    VALUES (
        :id,
        :operation_id,
        :game_id,
        :component_id,
        :original_path,
        :backup_path,
        :sha256,
        :created_at,
        :metadata_json
    )
    ON CONFLICT(id) DO UPDATE SET
        operation_id = excluded.operation_id,
        game_id = excluded.game_id,
        component_id = excluded.component_id,
        original_path = excluded.original_path,
        backup_path = excluded.backup_path,
        sha256 = excluded.sha256,
        created_at = excluded.created_at,
        metadata_json = excluded.metadata_json
"#;

impl BackupRepository for SqliteStorage {
    fn upsert_backup(&self, backup: &BackupRecord) -> AppResult<()> {
        let connection = self.connection()?;

        let component_id = backup.component_id.as_ref().map(|id| id.as_str());
        let sha256 = backup.sha256.as_ref().map(|sha256| sha256.as_str());
        let metadata_json = backup.metadata_json.as_deref();

        connection
            .execute(
                UPSERT_BACKUP_SQL,
                named_params! {
                    ":id": backup.id.as_str(),
                    ":operation_id": backup.operation_id.as_str(),
                    ":game_id": backup.game_id.as_str(),
                    ":component_id": component_id,
                    ":original_path": backup.original_path.as_str(),
                    ":backup_path": backup.backup_path.as_str(),
                    ":sha256": sha256,
                    ":created_at": backup.created_at.as_i64(),
                    ":metadata_json": metadata_json,
                },
            )
            .map_err(storage_error)?;

        Ok(())
    }

    fn list_backups_for_game(&self, game_id: &GameId) -> AppResult<Vec<BackupRecord>> {
        self.query_list(
            LIST_BACKUPS_FOR_GAME_SQL,
            [game_id.as_str()],
            backup_from_row,
        )
    }
}
