use renderpilot_application::{AppError, AppResult, BackupRepository, GameRepository, OperationRepository};
use renderpilot_domain::GameId;
use renderpilot_storage_sqlite::SqliteStorage;

use crate::error::CliError;

use super::{storage::open_catalog_storage, OperationListCatalogEntry, OperationListCatalogResult};

pub(super) fn list_operations_impl(game_id: GameId) -> Result<OperationListCatalogResult, CliError> {
    let storage = open_catalog_storage()?;
    ensure_game_exists(&storage, &game_id)?;
    let operations = storage.list_operations_for_game(&game_id)?;
    let backups = storage.list_backups_for_game(&game_id)?;

    let mut entries = Vec::with_capacity(operations.len());

    for operation in operations {
        let item_count = storage.list_operation_items(&operation.id)?.len();
        let backup_count = backups
            .iter()
            .filter(|backup| backup.operation_id == operation.id)
            .count();

        entries.push(OperationListCatalogEntry {
            operation,
            item_count,
            backup_count,
        });
    }

    Ok(OperationListCatalogResult {
        game_id,
        operations: entries,
    })
}

fn ensure_game_exists(storage: &SqliteStorage, game_id: &GameId) -> AppResult<()> {
    storage
        .find_game(game_id)?
        .ok_or_else(|| AppError::invalid_input(format!("game not found: {}", game_id.as_str())))?;

    Ok(())
}