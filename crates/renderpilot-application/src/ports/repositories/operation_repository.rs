use renderpilot_domain::{GameId, OperationId};

use crate::{AppResult, OperationItemRecord, OperationRecord};

/// Repository port for operation journal records and their item rows.
pub trait OperationRepository: Send + Sync {
    /// Inserts or updates one operation journal record.
    fn upsert_operation(&self, operation: &OperationRecord) -> AppResult<()>;

    /// Finds one operation journal record by identifier.
    fn find_operation(&self, operation_id: &OperationId) -> AppResult<Option<OperationRecord>>;

    /// Lists operation journal records for a game.
    fn list_operations_for_game(&self, game_id: &GameId) -> AppResult<Vec<OperationRecord>>;

    /// Replaces all operation items for one operation.
    fn replace_operation_items(
        &self,
        operation_id: &OperationId,
        items: &[OperationItemRecord],
    ) -> AppResult<()>;

    /// Lists operation items for one operation.
    fn list_operation_items(
        &self,
        operation_id: &OperationId,
    ) -> AppResult<Vec<OperationItemRecord>>;
}
