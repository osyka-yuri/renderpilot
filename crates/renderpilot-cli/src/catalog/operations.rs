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
    let entries = storage
        .list_operation_entries_for_game(game_id)?
        .into_iter()
        .map(|entry| OperationListCatalogEntry {
            item_count: entry.len(),
            component_ids: entry
                .items()
                .iter()
                .map(|item| item.component_id.as_str().to_owned())
                .collect(),
            operation: entry.operation().clone(),
        })
        .collect();

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
