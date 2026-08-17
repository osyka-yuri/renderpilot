use renderpilot_application::{AppError, AppResult};
use renderpilot_domain::GameId;
use rusqlite::{OptionalExtension, Transaction};

use crate::error::storage_error;

use super::super::observations;
use super::model::CatalogBinding;

pub(super) fn classify_catalog_binding_within_transaction(
    transaction: &Transaction<'_>,
    game_id: &GameId,
) -> AppResult<CatalogBinding> {
    let game_exists = transaction
        .query_row(
            "SELECT 1 FROM games WHERE id = ?1 LIMIT 1",
            [game_id.as_str()],
            |_| Ok(()),
        )
        .optional()
        .map_err(storage_error)?
        .is_some();
    let authority_exists = transaction
        .query_row(
            "SELECT 1 FROM catalog_scan_authority WHERE game_id = ?1 LIMIT 1",
            [game_id.as_str()],
            |_| Ok(()),
        )
        .optional()
        .map_err(storage_error)?
        .is_some();
    match (game_exists, authority_exists) {
        (false, false) => Ok(CatalogBinding::CatalogAbsent),
        (true, true) => Ok(CatalogBinding::CatalogPresent(
            observations::readiness_within_transaction(transaction, game_id)?,
        )),
        (true, false) => Err(AppError::storage_failed(format!(
            "catalog game {} is missing scan authority",
            game_id.as_str()
        ))),
        (false, true) => Err(AppError::storage_failed(format!(
            "scan authority exists without catalog game {}",
            game_id.as_str()
        ))),
    }
}
