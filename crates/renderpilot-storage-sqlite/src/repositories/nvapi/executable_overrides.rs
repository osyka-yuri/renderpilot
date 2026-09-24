use renderpilot_application::AppResult;
use rusqlite::{OptionalExtension, params};

use super::NvapiExecutableOverrideRow;
use crate::{SqliteStorage, error::storage_context};
impl SqliteStorage {
    /// Returns every executable override in one stable query.
    pub fn list_nvapi_executable_overrides(&self) -> AppResult<Vec<NvapiExecutableOverrideRow>> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare_cached(
                    "SELECT game_id, selected_path, selected_basename, updated_at
                     FROM nvapi_executable_overrides
                     ORDER BY game_id",
                )
                .map_err(|error| {
                    storage_context("could not prepare executable override list", error)
                })?;
            let rows = statement
                .query_map([], |row| {
                    Ok(NvapiExecutableOverrideRow {
                        game_id: row.get(0)?,
                        selected_path: row.get(1)?,
                        selected_basename: row.get(2)?,
                        updated_at: row.get(3)?,
                    })
                })
                .map_err(|error| storage_context("could not read executable overrides", error))?;

            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|error| storage_context("could not map executable overrides", error))
        })
    }

    /// Inserts or replaces the executable override for `game_id`.
    pub fn upsert_nvapi_executable_override(
        &self,
        game_id: &str,
        selected_path: &str,
        selected_basename: &str,
    ) -> AppResult<()> {
        self.with_connection(|connection| {
            connection
                .execute(
                    "INSERT INTO nvapi_executable_overrides
                    (game_id, selected_path, selected_basename, updated_at)
                 VALUES (?1, ?2, ?3, CAST(unixepoch('subsec') * 1000 AS INTEGER))
                 ON CONFLICT(game_id) DO UPDATE SET
                    selected_path     = excluded.selected_path,
                    selected_basename = excluded.selected_basename,
                    updated_at        = excluded.updated_at",
                    params![game_id, selected_path, selected_basename],
                )
                .map(|_| ())
                .map_err(|error| {
                    storage_context("could not upsert nvapi executable override", error)
                })
        })
    }

    /// Returns the executable override for `game_id`, if any.
    pub fn get_nvapi_executable_override(
        &self,
        game_id: &str,
    ) -> AppResult<Option<NvapiExecutableOverrideRow>> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT game_id, selected_path, selected_basename, updated_at
                 FROM nvapi_executable_overrides
                 WHERE game_id = ?1",
                    params![game_id],
                    |row| {
                        Ok(NvapiExecutableOverrideRow {
                            game_id: row.get(0)?,
                            selected_path: row.get(1)?,
                            selected_basename: row.get(2)?,
                            updated_at: row.get(3)?,
                        })
                    },
                )
                .optional()
                .map_err(|error| storage_context("could not read nvapi executable override", error))
        })
    }

    /// Deletes the executable override for `game_id`, if any.
    pub fn delete_nvapi_executable_override(&self, game_id: &str) -> AppResult<()> {
        self.with_connection(|connection| {
            connection
                .execute(
                    "DELETE FROM nvapi_executable_overrides WHERE game_id = ?1",
                    params![game_id],
                )
                .map(|_| ())
                .map_err(|error| {
                    storage_context("could not delete nvapi executable override", error)
                })
        })
    }
}
