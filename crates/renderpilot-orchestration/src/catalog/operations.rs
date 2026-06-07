use renderpilot_application::{AppError, AppResult, GameRepository, OperationRepository};
use renderpilot_domain::GameId;

use crate::ServiceError;

use super::{OperationListCatalogEntry, OperationListCatalogResult};

/// Returns the operation history list for a game using a caller-provided storage connection.
pub fn list_operations(
    context: &crate::Context,
    game_id: &GameId,
) -> Result<super::OperationListCatalogResult, ServiceError> {
    let storage = context.storage();

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
