//! Per-game catalog UI state.
//!
//! Stores lightweight, user-managed catalog flags such as favorite and hidden.
//! The row is optional: absence means the default state (`false` / `false`).

use renderpilot_application::AppResult;
use rusqlite::{params, OptionalExtension};

use crate::{error::storage_context, SqliteStorage};

/// One row from `game_ui_state`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameUiStateRow {
    /// Game this UI state applies to.
    pub game_id: String,
    /// Whether the game is pinned as a favorite.
    pub is_favorite: bool,
    /// Whether the game is hidden from the default catalog.
    pub is_hidden: bool,
    /// Unix milliseconds of the last write.
    pub updated_at: i64,
}

impl SqliteStorage {
    /// Persists one game's UI state.
    ///
    /// The default state (`false` / `false`) removes the row entirely because
    /// absence already represents that state.
    pub fn save_game_ui_state(
        &self,
        game_id: &str,
        is_favorite: bool,
        is_hidden: bool,
    ) -> AppResult<()> {
        if !is_favorite && !is_hidden {
            return self.delete_game_ui_state(game_id);
        }

        let connection = self.connection()?;
        connection
            .execute(
                "INSERT INTO game_ui_state
                    (game_id, is_favorite, is_hidden, updated_at)
                 VALUES (?1, ?2, ?3, CAST(unixepoch('subsec') * 1000 AS INTEGER))
                 ON CONFLICT(game_id) DO UPDATE SET
                    is_favorite = excluded.is_favorite,
                    is_hidden   = excluded.is_hidden,
                    updated_at  = excluded.updated_at",
                params![game_id, i32::from(is_favorite), i32::from(is_hidden)],
            )
            .map(|_| ())
            .map_err(|error| storage_context("could not save game ui state", error))
    }

    /// Reads the UI state for `game_id`, if any.
    pub fn get_game_ui_state(&self, game_id: &str) -> AppResult<Option<GameUiStateRow>> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT game_id, is_favorite, is_hidden, updated_at
                 FROM game_ui_state
                 WHERE game_id = ?1",
                params![game_id],
                |row| {
                    let is_favorite: i32 = row.get(1)?;
                    let is_hidden: i32 = row.get(2)?;

                    Ok(GameUiStateRow {
                        game_id: row.get(0)?,
                        is_favorite: is_favorite != 0,
                        is_hidden: is_hidden != 0,
                        updated_at: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(|error| storage_context("could not read game ui state", error))
    }

    /// Lists all stored UI-state rows in stable game-id order.
    pub fn list_all_game_ui_state(&self) -> AppResult<Vec<GameUiStateRow>> {
        self.query_list(
            "SELECT game_id, is_favorite, is_hidden, updated_at
             FROM game_ui_state
             ORDER BY game_id",
            [],
            |row| {
                let is_favorite: i32 = row.get(1)?;
                let is_hidden: i32 = row.get(2)?;

                Ok(Ok(GameUiStateRow {
                    game_id: row.get(0)?,
                    is_favorite: is_favorite != 0,
                    is_hidden: is_hidden != 0,
                    updated_at: row.get(3)?,
                }))
            },
        )
    }

    /// Deletes the UI-state row for `game_id`, if any.
    pub fn delete_game_ui_state(&self, game_id: &str) -> AppResult<()> {
        let connection = self.connection()?;
        connection
            .execute(
                "DELETE FROM game_ui_state WHERE game_id = ?1",
                params![game_id],
            )
            .map(|_| ())
            .map_err(|error| storage_context("could not delete game ui state", error))
    }
}

#[cfg(test)]
mod tests {
    use renderpilot_application::GameRepository;
    use renderpilot_domain::{
        GameId, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
    };

    use crate::SqliteStorage;

    fn seeded_storage() -> (SqliteStorage, GameId) {
        let storage = SqliteStorage::in_memory().expect("open in-memory sqlite");
        let game_id = GameId::new("manual:test-game").expect("game id");
        let identity =
            GameIdentity::new(game_id.clone(), "Test Game", Launcher::Manual).expect("identity");
        let installation = GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new("C:/Games/Test").expect("install path"),
        );

        storage.upsert_game(&installation).expect("seed game");

        (storage, game_id)
    }

    #[test]
    fn game_ui_state_roundtrips_and_default_state_clears_the_row() {
        let (storage, game_id) = seeded_storage();

        assert_eq!(
            storage
                .get_game_ui_state(game_id.as_str())
                .expect("ui state query should succeed"),
            None
        );

        storage
            .save_game_ui_state(game_id.as_str(), true, false)
            .expect("favorite state should save");

        let row = storage
            .get_game_ui_state(game_id.as_str())
            .expect("saved ui state should load")
            .expect("row should exist");
        assert_eq!(row.game_id, game_id.as_str());
        assert!(row.is_favorite);
        assert!(!row.is_hidden);

        let all = storage
            .list_all_game_ui_state()
            .expect("ui state rows should list");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0], row);

        storage
            .save_game_ui_state(game_id.as_str(), false, false)
            .expect("default state should clear the row");

        assert_eq!(
            storage
                .get_game_ui_state(game_id.as_str())
                .expect("ui state query should succeed after clear"),
            None
        );
        assert!(storage
            .list_all_game_ui_state()
            .expect("ui state rows should list after clear")
            .is_empty());
    }

    #[test]
    fn deleting_a_game_cascades_its_ui_state() {
        let (storage, game_id) = seeded_storage();

        storage
            .save_game_ui_state(game_id.as_str(), true, true)
            .expect("ui state should save");

        storage
            .delete_game(&game_id)
            .expect("deleting the game should succeed");

        assert_eq!(
            storage
                .get_game_ui_state(game_id.as_str())
                .expect("ui state query should succeed after game delete"),
            None
        );
        assert!(storage
            .list_all_game_ui_state()
            .expect("ui state rows should list after game delete")
            .is_empty());
    }
}
