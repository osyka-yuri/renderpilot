use renderpilot_application::{AppResult, ComponentRepository};
use renderpilot_domain::{GameId, GraphicsComponent};
use rusqlite::{params, Connection};

use crate::{error::storage_error, mapping};

use super::{row_mapping::component_from_row, SqliteStorage};

impl ComponentRepository for SqliteStorage {
    fn replace_components_for_game(
        &self,
        game_id: &GameId,
        components: &[GraphicsComponent],
    ) -> AppResult<()> {
        self.with_transaction(|transaction| {
            transaction
                .execute(
                    "DELETE FROM components WHERE game_id = ?1",
                    [game_id.as_str()],
                )
                .map_err(storage_error)?;

            for component in components {
                insert_component(transaction, component)?;
            }

            Ok(())
        })
    }

    fn list_components_for_game(&self, game_id: &GameId) -> AppResult<Vec<GraphicsComponent>> {
        self.query_list(
            "SELECT id, game_id, kind, technology, swappability, files_json
             FROM components
             WHERE game_id = ?1
             ORDER BY id",
            [game_id.as_str()],
            component_from_row,
        )
    }
}

fn insert_component(connection: &Connection, component: &GraphicsComponent) -> AppResult<()> {
    let kind = mapping::enum_to_text(component.kind())?;
    let technology = mapping::enum_to_text(component.technology())?;
    let swappability = mapping::enum_to_text(component.swappability())?;
    let files_json = mapping::serialize_json(component.files())?;

    connection
        .execute(
            "INSERT INTO components
                (id, game_id, kind, technology, swappability, files_json, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, unixepoch('subsec') * 1000)
             ON CONFLICT(id) DO UPDATE SET
                game_id = excluded.game_id,
                kind = excluded.kind,
                technology = excluded.technology,
                swappability = excluded.swappability,
                files_json = excluded.files_json,
                updated_at = excluded.updated_at",
            params![
                component.id().as_str(),
                component.game_id().as_str(),
                kind,
                technology,
                swappability,
                files_json
            ],
        )
        .map_err(storage_error)?;

    Ok(())
}