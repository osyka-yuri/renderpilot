use renderpilot_application::{AppResult, GameRepository};
use renderpilot_domain::{GameId, GameInstallation};
use rusqlite::{named_params, CachedStatement, Connection, OptionalExtension};

use crate::{error::storage_error, mapping};

use super::{row_mapping::game_from_row, SqliteStorage};

const FIND_GAME_SQL: &str = "
    SELECT
        id,
        title,
        launcher,
        external_id,
        platform,
        runtime,
        install_path,
        executable_candidates_json
    FROM games
    WHERE id = :id
";

const LIST_GAMES_SQL: &str = "
    SELECT
        id,
        title,
        launcher,
        external_id,
        platform,
        runtime,
        install_path,
        executable_candidates_json
    FROM games
    ORDER BY title, id
";

const UPSERT_GAME_SQL: &str = "
    INSERT INTO games
        (
            id,
            title,
            launcher,
            external_id,
            platform,
            runtime,
            install_path,
            executable_candidates_json,
            updated_at
        )
    VALUES
        (
            :id,
            :title,
            :launcher,
            :external_id,
            :platform,
            :runtime,
            :install_path,
            :executable_candidates_json,
            CAST(unixepoch('subsec') * 1000 AS INTEGER)
        )
    ON CONFLICT(id) DO UPDATE SET
        title                      = excluded.title,
        launcher                   = excluded.launcher,
        external_id                = excluded.external_id,
        platform                   = excluded.platform,
        runtime                    = excluded.runtime,
        install_path               = excluded.install_path,
        executable_candidates_json = excluded.executable_candidates_json,
        updated_at                 = excluded.updated_at
";

const DELETE_GAME_SQL: &str = "
    DELETE FROM games
    WHERE id = :id
";

impl GameRepository for SqliteStorage {
    fn upsert_game(&self, game: &GameInstallation) -> AppResult<()> {
        let connection = self.connection()?;

        upsert_game_in_connection(&connection, game)
    }

    fn upsert_games(&self, games: &[GameInstallation]) -> AppResult<()> {
        if games.is_empty() {
            return Ok(());
        }

        self.with_transaction(|transaction| upsert_games_in_connection(transaction, games))
    }

    fn find_game(&self, id: &GameId) -> AppResult<Option<GameInstallation>> {
        let connection = self.connection()?;

        find_game_in_connection(&connection, id)
    }
}

impl SqliteStorage {
    /// Lists all stored game installations ordered by title and identifier.
    pub fn list_games(&self) -> AppResult<Vec<GameInstallation>> {
        self.query_list(LIST_GAMES_SQL, [], game_from_row)
    }

    /// Deletes a game row by id.
    ///
    /// Child rows are removed or detached according to foreign-key rules.
    /// Missing id is a no-op.
    pub fn delete_game(&self, id: &GameId) -> AppResult<()> {
        let connection = self.connection()?;

        delete_game_in_connection(&connection, id)
    }
}

fn find_game_in_connection(
    connection: &Connection,
    id: &GameId,
) -> AppResult<Option<GameInstallation>> {
    connection
        .query_row(
            FIND_GAME_SQL,
            named_params! {
                ":id": id.as_str(),
            },
            game_from_row,
        )
        .optional()
        .map_err(storage_error)?
        .transpose()
}

/// Writes one game row using an existing connection or outer transaction.
///
/// This function intentionally does not start its own transaction.
/// The caller owns transaction boundaries.
pub(super) fn upsert_game_in_connection(
    connection: &Connection,
    game: &GameInstallation,
) -> AppResult<()> {
    let mut statement = connection
        .prepare_cached(UPSERT_GAME_SQL)
        .map_err(storage_error)?;

    execute_game_upsert(&mut statement, game)
}

/// Writes game rows using an existing connection or outer transaction.
///
/// This function intentionally does not start its own transaction.
/// The caller owns transaction boundaries.
pub(super) fn upsert_games_in_connection(
    connection: &Connection,
    games: &[GameInstallation],
) -> AppResult<()> {
    if games.is_empty() {
        return Ok(());
    }

    let mut statement = connection
        .prepare_cached(UPSERT_GAME_SQL)
        .map_err(storage_error)?;

    for game in games {
        execute_game_upsert(&mut statement, game)?;
    }

    Ok(())
}

/// Deletes one game row using an existing connection or outer transaction.
///
/// This function intentionally does not start its own transaction.
/// The caller owns transaction boundaries.
pub(super) fn delete_game_in_connection(connection: &Connection, id: &GameId) -> AppResult<()> {
    connection
        .execute(
            DELETE_GAME_SQL,
            named_params! {
                ":id": id.as_str(),
            },
        )
        .map_err(storage_error)?;

    Ok(())
}

fn execute_game_upsert(
    statement: &mut CachedStatement<'_>,
    game: &GameInstallation,
) -> AppResult<()> {
    let params = GameSqlParams::from_game(game)?;

    statement
        .execute(named_params! {
            ":id": params.id,
            ":title": params.title,
            ":launcher": params.launcher,
            ":external_id": params.external_id,
            ":platform": params.platform,
            ":runtime": params.runtime,
            ":install_path": params.install_path,
            ":executable_candidates_json": params.executable_candidates_json,
        })
        .map_err(storage_error)?;

    Ok(())
}

#[derive(Debug)]
struct GameSqlParams<'a> {
    id: &'a str,
    title: &'a str,
    launcher: String,
    external_id: Option<&'a str>,
    platform: String,
    runtime: String,
    install_path: &'a str,
    executable_candidates_json: String,
}

impl<'a> GameSqlParams<'a> {
    fn from_game(game: &'a GameInstallation) -> AppResult<Self> {
        Ok(Self {
            id: game.id().as_str(),
            title: game.identity().title(),
            launcher: mapping::enum_to_text(game.identity().launcher())?,
            external_id: game.identity().external_id(),
            platform: mapping::enum_to_text(game.platform())?,
            runtime: mapping::enum_to_text(game.runtime())?,
            install_path: game.install_path().as_str(),
            executable_candidates_json: mapping::serialize_json(game.executable_candidates())?,
        })
    }
}

#[cfg(test)]
mod tests {
    use renderpilot_application::GameRepository;
    use renderpilot_domain::{
        GameId, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
    };

    use crate::SqliteStorage;

    #[test]
    fn list_games_returns_games_sorted_by_title_then_id() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");

        let later = sample_game("game:zeta", "Zeta Game");
        let earlier_b = sample_game("game:beta-b", "Beta Game");
        let earlier_a = sample_game("game:beta-a", "Beta Game");

        storage
            .upsert_game(&later)
            .expect("later game should store");
        storage
            .upsert_game(&earlier_b)
            .expect("beta-b should store");
        storage
            .upsert_game(&earlier_a)
            .expect("beta-a should store");

        let games = storage.list_games().expect("games should list");

        assert_eq!(games, vec![earlier_a, earlier_b, later]);
    }

    #[test]
    fn find_game_returns_stored_game_by_id() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");
        let game = sample_game("game:cyberpunk", "Cyberpunk 2077");

        storage.upsert_game(&game).expect("game should store");

        let found = storage
            .find_game(game.id())
            .expect("find_game should succeed")
            .expect("game should exist");

        assert_eq!(found, game);
    }

    #[test]
    fn find_game_returns_none_for_missing_id() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");
        let missing_id = GameId::new("game:missing").expect("game id should be valid");

        let found = storage
            .find_game(&missing_id)
            .expect("find_game should succeed");

        assert_eq!(found, None);
    }

    #[test]
    fn upsert_game_updates_existing_row() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");

        let original = sample_game("game:shared", "Original Title");
        let updated = sample_game("game:shared", "Updated Title");

        storage
            .upsert_game(&original)
            .expect("original game should store");
        storage
            .upsert_game(&updated)
            .expect("updated game should store");

        let games = storage.list_games().expect("games should list");

        assert_eq!(games, vec![updated]);
    }

    #[test]
    fn upsert_games_stores_batch() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");

        let game_a = sample_game("game:a", "Alpha");
        let game_b = sample_game("game:b", "Beta");

        storage
            .upsert_games(&[game_b.clone(), game_a.clone()])
            .expect("games should store");

        let games = storage.list_games().expect("games should list");

        assert_eq!(games, vec![game_a, game_b]);
    }

    #[test]
    fn upsert_games_accepts_empty_slice() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");

        storage
            .upsert_games(&[])
            .expect("empty batch should be a no-op");

        let games = storage.list_games().expect("games should list");

        assert!(games.is_empty());
    }

    #[test]
    fn delete_game_removes_existing_game() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");

        let game = sample_game("game:delete-me", "Delete Me");

        storage.upsert_game(&game).expect("game should store");
        storage.delete_game(game.id()).expect("game should delete");

        let found = storage
            .find_game(game.id())
            .expect("find_game should succeed");

        assert_eq!(found, None);
    }

    #[test]
    fn delete_game_is_noop_for_missing_id() {
        let storage = SqliteStorage::in_memory().expect("in-memory sqlite should open");
        let missing_id = GameId::new("game:missing").expect("game id should be valid");

        storage
            .delete_game(&missing_id)
            .expect("delete missing game should be a no-op");
    }

    fn sample_game(id: &str, title: &str) -> GameInstallation {
        let identity = GameIdentity::new(
            GameId::new(id).expect("game id should be valid"),
            title,
            Launcher::Manual,
        )
        .expect("identity should be valid");

        GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            install_path_for_title(title),
        )
    }

    fn install_path_for_title(title: &str) -> PathRef {
        PathRef::new(format!("C:/Games/{}", title.replace(' ', "_")))
            .expect("install path should be valid")
    }
}
