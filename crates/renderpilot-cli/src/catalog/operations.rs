use renderpilot_application::{AppError, AppResult, GameRepository, OperationRepository};
use renderpilot_domain::GameId;

use crate::error::CliError;

use super::{OperationListCatalogEntry, OperationListCatalogResult};

pub(super) fn list_operations_with_storage<S>(
    storage: &S,
    game_id: &GameId,
) -> Result<OperationListCatalogResult, CliError>
where
    S: GameRepository + OperationRepository,
{
    ensure_game_exists(storage, game_id)?;
    let operations = storage.list_operation_headers_for_game(game_id)?;

    let mut entries = Vec::with_capacity(operations.len());

    for operation in operations {
        let item_count = storage.count_operation_items(&operation.id)?;

        let component_ids = storage
            .find_operation_entry(&operation.id)?
            .map(|entry| {
                entry
                    .items()
                    .iter()
                    .map(|item| item.component_id.as_str().to_owned())
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();

        entries.push(OperationListCatalogEntry {
            operation,
            item_count,
            component_ids,
        });
    }

    Ok(OperationListCatalogResult {
        game_id: game_id.clone(),
        operations: entries,
    })
}

fn ensure_game_exists<S: GameRepository>(storage: &S, game_id: &GameId) -> AppResult<()> {
    storage
        .find_game(game_id)?
        .ok_or_else(|| AppError::invalid_input(format!("game not found: {}", game_id.as_str())))?;

    Ok(())
}
