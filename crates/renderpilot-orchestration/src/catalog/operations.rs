use renderpilot_application::{GameRepository, OperationRepository};
use renderpilot_domain::GameId;

use crate::ServiceError;

use super::{OperationListCatalogEntry, OperationListCatalogResult};

/// Returns the operation history list for a game using a caller-provided storage connection.
pub fn list_operations(
    context: &crate::Context,
    game_id: &GameId,
) -> Result<super::OperationListCatalogResult, ServiceError> {
    let storage = context.storage();

    storage.require_game(game_id)?;
    let entries = storage
        .list_operation_entries_for_game(game_id)?
        .into_iter()
        .map(|entry| {
            let (operation, items) = entry.into_parts();
            OperationListCatalogEntry {
                item_count: super::execute::component_file_item_count(&items),
                component_ids: items
                    .iter()
                    .filter(|item| super::execute::journal_item_is_component_file(item))
                    .map(|item| item.component_id.as_str().to_owned())
                    .collect(),
                operation,
            }
        })
        .collect();

    Ok(OperationListCatalogResult {
        game_id: game_id.clone(),
        operations: entries,
    })
}
