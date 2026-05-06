use renderpilot_application::{AppResult, GameRepository};
use renderpilot_domain::{GameId, GameInstallation};
use rusqlite::{params, Connection, OptionalExtension};

use crate::{error::storage_error, mapping};

use super::{row_mapping::game_from_row, SqliteStorage};

impl GameRepository for SqliteStorage {
    fn upsert_game(&self, game: &GameInstallation) -> AppResult<()> {
        let connection = self.connection()?;

        upsert_game(&connection, game)
    }

    fn upsert_games(&self, games: &[GameInstallation]) -> AppResult<()> {
        self.with_transaction(|transaction| {
            for game in games {
                upsert_game(transaction, game)?;
            }

            Ok(())
        })
    }

    fn find_game(&self, id: &GameId) -> AppResult<Option<GameInstallation>> {
        let connection = self.connection()?;

        connection
            .query_row(
                "SELECT id, title, launcher, external_id, platform, runtime, install_path,
                        executable_candidates_json
                 FROM games
                 WHERE id = ?1",
                [id.as_str()],
                game_from_row,
            )
            .optional()
            .map_err(storage_error)?
            .transpose()
    }
}

impl SqliteStorage {
    /// Lists all stored game installations ordered by title and identifier.
    pub fn list_games(&self) -> AppResult<Vec<GameInstallation>> {
        self.query_list(
            "SELECT id, title, launcher, external_id, platform, runtime, install_path,
                    executable_candidates_json
             FROM games
             ORDER BY title, id",
            [],
            game_from_row,
        )
    }
}

fn upsert_game(connection: &Connection, game: &GameInstallation) -> AppResult<()> {
    let launcher = mapping::enum_to_text(game.identity().launcher())?;
    let platform = mapping::enum_to_text(game.platform())?;
    let runtime = mapping::enum_to_text(game.runtime())?;
    let executable_candidates_json = mapping::serialize_json(game.executable_candidates())?;

    connection
        .execute(
            "INSERT INTO games
                (id, title, launcher, external_id, platform, runtime, install_path,
                 executable_candidates_json, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, unixepoch('subsec') * 1000)
             ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                launcher = excluded.launcher,
                external_id = excluded.external_id,
                platform = excluded.platform,
                runtime = excluded.runtime,
                install_path = excluded.install_path,
                executable_candidates_json = excluded.executable_candidates_json,
                updated_at = excluded.updated_at",
            params![
                game.id().as_str(),
                game.identity().title(),
                launcher,
                game.identity().external_id(),
                platform,
                runtime,
                game.install_path().as_str(),
                executable_candidates_json
            ],
        )
        .map_err(storage_error)?;

    Ok(())
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
            PathRef::new(format!("C:/Games/{}", title.replace(' ', "_")))
                .expect("install path should be valid"),
        )
    }
}
