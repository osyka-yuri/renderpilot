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