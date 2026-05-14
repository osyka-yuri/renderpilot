use renderpilot_application::{AppResult, GameRepository};
use renderpilot_domain::{GameId, GameInstallation};
use rusqlite::{named_params, Connection, OptionalExtension, Statement, Transaction};

use crate::{error::storage_error, mapping, sqlite_clock};

use super::{
    catalog_select_sql::{
        FIND_GAME_SQL, LIST_DISTINCT_GAME_LAUNCHERS_SQL, LIST_DISTINCT_GAME_LIBRARIES_SQL,
        LIST_GAMES_SQL,
    },
    game_covers::{find_cover_in_connection, DeletedGameInfo},
    row_mapping::game_from_row,
    SqliteStorage,
};

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
            created_at,
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
            :created_at,
            :updated_at
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
        self.with_transaction(|transaction| upsert_game_within_transaction(transaction, game))
    }

    fn upsert_games(&self, games: &[GameInstallation]) -> AppResult<()> {
        if games.is_empty() {
            return Ok(());
        }

        self.with_transaction(|transaction| upsert_games_within_transaction(transaction, games))
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

    /// Lists distinct library values currently observed in `components`.
    pub fn list_distinct_game_libraries(&self) -> AppResult<Vec<String>> {
        self.query_list(LIST_DISTINCT_GAME_LIBRARIES_SQL, [], |row| {
            Ok(Ok(row.get::<_, String>(0)?))
        })
    }

    /// Lists distinct launcher values currently observed in `games`.
    pub fn list_distinct_game_launchers(&self) -> AppResult<Vec<String>> {
        self.query_list(LIST_DISTINCT_GAME_LAUNCHERS_SQL, [], |row| {
            Ok(Ok(row.get::<_, String>(0)?))
        })
    }

    /// Deletes a game row by id.
    ///
    /// Child rows are removed or detached according to foreign-key rules.
    /// Missing id is a no-op.
    pub fn delete_game(&self, id: &GameId) -> AppResult<DeletedGameInfo> {
        self.with_transaction(|transaction| delete_game_in_connection(transaction, id))
    }
}

fn find_game_in_connection(
    connection: &Connection,
    id: &GameId,
) -> AppResult<Option<GameInstallation>> {
    let game = connection
        .query_row(
            FIND_GAME_SQL,
            named_params! {
                ":id": id.as_str(),
            },
            game_from_row,
        )
        .optional()
        .map_err(storage_error)?;

    game.transpose()
}

/// Writes one game row within a transaction.
///
/// This function requires an active `Transaction` object.
pub(super) fn upsert_game_within_transaction(
    transaction: &Transaction<'_>,
    game: &GameInstallation,
) -> AppResult<()> {
    let params = GameSqlParams::from_game(game)?;

    execute_game_upserts_within_transaction(transaction, std::slice::from_ref(&params))
}

/// Writes game rows within a transaction.
///
/// This function requires an active `Transaction` object, ensuring that the
/// multiple upserts are atomic. If any step fails, the caller's transaction
/// will be rolled back.
///
/// All game parameters are prepared before the first database write, so
/// mapping/serialization failures cannot produce a partially written batch.
pub(super) fn upsert_games_within_transaction(
    transaction: &Transaction<'_>,
    games: &[GameInstallation],
) -> AppResult<()> {
    let params = collect_game_sql_params(games)?;

    execute_game_upserts_within_transaction(transaction, &params)
}

/// Deletes one game row using an existing connection or outer transaction.
///
/// This function intentionally does not start its own transaction.
/// The caller owns transaction boundaries.
pub(super) fn delete_game_in_connection(
    connection: &Connection,
    id: &GameId,
) -> AppResult<DeletedGameInfo> {
    let old_cover_file_name =
        find_cover_in_connection(connection, id)?.map(|record| record.file_name);

    connection
        .execute(
            DELETE_GAME_SQL,
            named_params! {
                ":id": id.as_str(),
            },
        )
        .map_err(storage_error)?;

    Ok(DeletedGameInfo {
        old_cover_file_name,
    })
}

fn collect_game_sql_params<'a>(games: &'a [GameInstallation]) -> AppResult<Vec<GameSqlParams<'a>>> {
    games.iter().map(GameSqlParams::from_game).collect()
}

fn execute_game_upserts_within_transaction(
    transaction: &Transaction<'_>,
    params: &[GameSqlParams<'_>],
) -> AppResult<()> {
    if params.is_empty() {
        return Ok(());
    }

    let timestamps = GameWriteTimestamps::now(transaction)?;
    let mut statement = transaction
        .prepare_cached(UPSERT_GAME_SQL)
        .map_err(storage_error)?;

    for game_params in params {
        execute_game_upsert(&mut statement, game_params, timestamps)?;
    }

    Ok(())
}

fn execute_game_upsert(
    statement: &mut Statement<'_>,
    params: &GameSqlParams<'_>,
    timestamps: GameWriteTimestamps,
) -> AppResult<()> {
    statement
        .execute(named_params! {
            ":id": params.id,
            ":title": params.title,
            ":launcher": params.launcher.as_str(),
            ":external_id": params.external_id,
            ":platform": params.platform.as_str(),
            ":runtime": params.runtime.as_str(),
            ":install_path": params.install_path,
            ":executable_candidates_json": params.executable_candidates_json.as_str(),
            ":created_at": timestamps.created_at_ms,
            ":updated_at": timestamps.updated_at_ms,
        })
        .map_err(storage_error)?;

    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct GameWriteTimestamps {
    created_at_ms: i64,
    updated_at_ms: i64,
}

impl GameWriteTimestamps {
    fn now(connection: &Connection) -> AppResult<Self> {
        let now_ms = sqlite_clock::now_ms(connection)?;

        Ok(Self {
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
        })
    }
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
            launcher: mapping::enum_to_text(&game.identity().launcher())?,
            external_id: game.identity().external_id(),
            platform: mapping::enum_to_text(&game.platform())?,
            runtime: mapping::enum_to_text(&game.runtime())?,
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
        let storage = in_memory_storage();

        let later = sample_game("game:zeta", "Zeta Game");
        let earlier_b = sample_game("game:beta-b", "Beta Game");
        let earlier_a = sample_game("game:beta-a", "Beta Game");

        store_game(&storage, &later);
        store_game(&storage, &earlier_b);
        store_game(&storage, &earlier_a);

        assert_stored_games(&storage, &[earlier_a, earlier_b, later]);
    }

    #[test]
    fn find_game_returns_stored_game_by_id() {
        let storage = in_memory_storage();
        let game = sample_game("game:cyberpunk", "Cyberpunk 2077");

        store_game(&storage, &game);

        let found = storage
            .find_game(game.id())
            .expect("find_game should succeed")
            .expect("game should exist");

        assert_eq!(found, game);
    }

    #[test]
    fn find_game_returns_none_for_missing_id() {
        let storage = in_memory_storage();
        let missing_id = sample_game_id("game:missing");

        assert_missing_game(&storage, &missing_id);
    }

    #[test]
    fn upsert_game_updates_existing_row() {
        let storage = in_memory_storage();

        let original = sample_game("game:shared", "Original Title");
        let updated = sample_game("game:shared", "Updated Title");

        store_game(&storage, &original);
        store_game(&storage, &updated);

        assert_stored_games(&storage, &[updated]);
    }

    #[test]
    fn upsert_games_stores_batch() {
        let storage = in_memory_storage();

        let game_a = sample_game("game:a", "Alpha");
        let game_b = sample_game("game:b", "Beta");

        storage
            .upsert_games(&[game_b.clone(), game_a.clone()])
            .expect("games should store");

        assert_stored_games(&storage, &[game_a, game_b]);
    }

    #[test]
    fn upsert_games_updates_existing_rows() {
        let storage = in_memory_storage();

        let original = sample_game("game:shared", "Original Title");
        let updated = sample_game("game:shared", "Updated Title");

        store_game(&storage, &original);

        storage
            .upsert_games(std::slice::from_ref(&updated))
            .expect("updated game should store");

        assert_stored_games(&storage, &[updated]);
    }

    #[test]
    fn upsert_games_accepts_empty_slice() {
        let storage = in_memory_storage();

        storage
            .upsert_games(&[])
            .expect("empty batch should be a no-op");

        assert_stored_games(&storage, &[]);
    }

    #[test]
    fn delete_game_removes_existing_game() {
        let storage = in_memory_storage();

        let game = sample_game("game:delete-me", "Delete Me");

        store_game(&storage, &game);

        let deleted = storage.delete_game(game.id()).expect("game should delete");

        assert_eq!(deleted.old_cover_file_name, None);
        assert_missing_game(&storage, game.id());
    }

    #[test]
    fn delete_game_is_noop_for_missing_id() {
        let storage = in_memory_storage();
        let missing_id = sample_game_id("game:missing");

        let deleted = storage
            .delete_game(&missing_id)
            .expect("delete missing game should be a no-op");

        assert_eq!(deleted.old_cover_file_name, None);
    }

    #[test]
    fn delete_game_returns_old_cover_file_name() {
        let storage = in_memory_storage();

        let game = sample_game("game:with-cover", "With Cover");

        store_game(&storage, &game);

        storage
            .upsert_game_cover(game.id(), "cover-old.webp")
            .expect("cover should store");

        let deleted = storage.delete_game(game.id()).expect("game should delete");

        assert_eq!(
            deleted.old_cover_file_name.as_deref(),
            Some("cover-old.webp")
        );
    }

    fn in_memory_storage() -> SqliteStorage {
        SqliteStorage::in_memory().expect("in-memory sqlite should open")
    }

    fn store_game(storage: &SqliteStorage, game: &GameInstallation) {
        storage.upsert_game(game).expect("game should store");
    }

    fn assert_stored_games(storage: &SqliteStorage, expected: &[GameInstallation]) {
        let games = storage.list_games().expect("games should list");

        assert_eq!(games.as_slice(), expected);
    }

    fn assert_missing_game(storage: &SqliteStorage, id: &GameId) {
        let found = storage.find_game(id).expect("find_game should succeed");

        assert_eq!(found, None);
    }

    fn sample_game(id: &str, title: &str) -> GameInstallation {
        let identity = GameIdentity::new(sample_game_id(id), title, Launcher::Manual)
            .expect("identity should be valid");

        GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            install_path_for_title(title),
        )
    }

    fn sample_game_id(id: &str) -> GameId {
        GameId::new(id).expect("game id should be valid")
    }

    fn install_path_for_title(title: &str) -> PathRef {
        PathRef::new(format!("C:/Games/{}", title.replace(' ', "_")))
            .expect("install path should be valid")
    }
}
