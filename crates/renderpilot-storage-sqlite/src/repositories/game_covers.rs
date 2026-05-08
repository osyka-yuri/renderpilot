//! Persisted cover image metadata (`game_covers` table).
//!
//! Image bytes live on disk beside the catalog database; this crate stores only
//! validated basenames. Stored names must not contain path separators.

use std::{collections::HashMap, error::Error, fmt};

use renderpilot_application::AppResult;
use renderpilot_domain::GameId;
use rusqlite::{named_params, Connection, OptionalExtension, Row, Transaction};

use crate::{error::invalid_row, error::storage_error, sqlite_clock};

use super::SqliteStorage;

/// Row from `game_covers` for one game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameCoverRecord {
    /// Cover image basename inside the catalog `covers/` directory.
    ///
    /// This must be a basename only, with no `/` or `\` separators.
    pub file_name: String,

    /// Last update time for this cover row in Unix milliseconds.
    pub updated_at_ms: i64,
}

/// Information returned after deleting a game row.
///
/// Covers may need filesystem cleanup after the database row has been deleted.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeletedGameInfo {
    /// Basename of the cover file that was associated with the game, if any.
    pub old_cover_file_name: Option<String>,
}

const FIND_COVER_SQL: &str = "
    SELECT file_name, updated_at
    FROM game_covers
    WHERE game_id = :game_id
";

const UPSERT_COVER_SQL: &str = "
    INSERT INTO game_covers (game_id, file_name, updated_at)
    VALUES (:game_id, :file_name, :updated_at)
    ON CONFLICT(game_id) DO UPDATE SET
        file_name = excluded.file_name,
        updated_at = excluded.updated_at
";

const DELETE_COVER_SQL: &str = "
    DELETE FROM game_covers
    WHERE game_id = :game_id
";

const LIST_COVER_FILE_NAMES_SQL: &str = "
    SELECT file_name
    FROM game_covers
    ORDER BY file_name
";

const LIST_ALL_COVERS_SQL: &str = "
    SELECT game_id, file_name, updated_at
    FROM game_covers
";

impl SqliteStorage {
    /// Loads persisted cover metadata for a game, if present.
    pub fn find_game_cover(&self, game_id: &GameId) -> AppResult<Option<GameCoverRecord>> {
        self.with_connection(|connection| find_cover_in_connection(connection, game_id))
    }

    /// Inserts or replaces the cover basename for a game.
    pub fn upsert_game_cover(&self, game_id: &GameId, file_name: &str) -> AppResult<()> {
        self.with_transaction(|transaction| {
            upsert_cover_in_transaction(transaction, game_id, file_name)
        })
    }

    /// Deletes the cover row for a game without touching on-disk bytes.
    pub fn clear_game_cover_row(&self, game_id: &GameId) -> AppResult<()> {
        self.with_transaction(|transaction| clear_cover_in_connection(transaction, game_id))
    }

    /// Basenames of all cover files referenced by the catalog.
    ///
    /// Used by orphan garbage collection.
    pub fn list_game_cover_file_names(&self) -> AppResult<Vec<String>> {
        self.query_list(LIST_COVER_FILE_NAMES_SQL, [], |row| {
            let file_name: String = row.get(0)?;

            Ok(validate_cover_file_name(&file_name).map(|()| file_name))
        })
    }

    /// Loads all persisted cover rows in one query.
    ///
    /// Used for batch UI hydration.
    pub fn list_all_game_covers(&self) -> AppResult<HashMap<GameId, GameCoverRecord>> {
        let rows: Vec<(String, GameCoverRecord)> =
            self.query_list(LIST_ALL_COVERS_SQL, [], |row| {
                let game_id: String = row.get(0)?;
                let record = cover_record_from_row(row, 1, 2)?;

                Ok(validate_cover_file_name(&record.file_name).map(|()| (game_id, record)))
            })?;

        let mut covers = HashMap::with_capacity(rows.len());

        for (game_id, record) in rows {
            let game_id = GameId::new(game_id).map_err(invalid_row)?;
            covers.insert(game_id, record);
        }

        Ok(covers)
    }
}

pub(super) fn find_cover_in_connection(
    connection: &Connection,
    game_id: &GameId,
) -> AppResult<Option<GameCoverRecord>> {
    let record = connection
        .query_row(
            FIND_COVER_SQL,
            named_params! {
                ":game_id": game_id.as_str(),
            },
            |row| cover_record_from_row(row, 0, 1),
        )
        .optional()
        .map_err(storage_error)?;

    if let Some(record) = &record {
        validate_cover_file_name(&record.file_name)?;
    }

    Ok(record)
}

pub(super) fn upsert_cover_in_transaction(
    transaction: &Transaction<'_>,
    game_id: &GameId,
    file_name: &str,
) -> AppResult<()> {
    validate_cover_file_name(file_name)?;

    let updated_at_ms = sqlite_clock::now_ms(transaction)?;

    transaction
        .execute(
            UPSERT_COVER_SQL,
            named_params! {
                ":game_id": game_id.as_str(),
                ":file_name": file_name,
                ":updated_at": updated_at_ms,
            },
        )
        .map(|_| ())
        .map_err(storage_error)
}

pub(super) fn clear_cover_in_connection(
    connection: &Connection,
    game_id: &GameId,
) -> AppResult<()> {
    connection
        .execute(
            DELETE_COVER_SQL,
            named_params! {
                ":game_id": game_id.as_str(),
            },
        )
        .map(|_| ())
        .map_err(storage_error)
}

fn cover_record_from_row(
    row: &Row<'_>,
    file_name_index: usize,
    updated_at_index: usize,
) -> rusqlite::Result<GameCoverRecord> {
    Ok(GameCoverRecord {
        file_name: row.get(file_name_index)?,
        updated_at_ms: row.get(updated_at_index)?,
    })
}

fn validate_cover_file_name(file_name: &str) -> AppResult<()> {
    let is_invalid = file_name.is_empty()
        || file_name == "."
        || file_name == ".."
        || file_name.contains('/')
        || file_name.contains('\\');

    if is_invalid {
        return Err(storage_error(rusqlite::Error::ToSqlConversionFailure(
            Box::new(InvalidCoverFileName(file_name.to_owned())),
        )));
    }

    Ok(())
}

#[derive(Debug)]
struct InvalidCoverFileName(String);

impl fmt::Display for InvalidCoverFileName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid cover file name {:?}: expected a basename without path separators",
            self.0
        )
    }
}

impl Error for InvalidCoverFileName {}

#[cfg(test)]
mod tests {
    use renderpilot_application::GameRepository;
    use renderpilot_domain::{
        GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
    };

    use crate::SqliteStorage;

    #[test]
    fn upsert_find_clear_cover_roundtrip() {
        let storage = SqliteStorage::in_memory().expect("sqlite should open");
        let game = sample_game("game:covered", "Covered Game");

        storage.upsert_game(&game).expect("game should store");

        storage
            .upsert_game_cover(game.id(), "cover-game-covered-ulid123.webp")
            .expect("cover should upsert");

        let record = storage
            .find_game_cover(game.id())
            .expect("find should succeed")
            .expect("cover should exist");

        assert_eq!(record.file_name, "cover-game-covered-ulid123.webp");
        assert!(record.updated_at_ms >= 0);

        storage
            .clear_game_cover_row(game.id())
            .expect("clear should succeed");

        assert!(storage
            .find_game_cover(game.id())
            .expect("find should succeed")
            .is_none());
    }

    #[test]
    fn upsert_replaces_existing_cover() {
        let storage = SqliteStorage::in_memory().expect("sqlite should open");
        let game = sample_game("game:replace-cover", "Replace Cover");

        storage.upsert_game(&game).expect("game should store");

        storage
            .upsert_game_cover(game.id(), "cover-old.webp")
            .expect("first cover should upsert");

        storage
            .upsert_game_cover(game.id(), "cover-new.webp")
            .expect("second cover should replace first");

        let record = storage
            .find_game_cover(game.id())
            .expect("find should succeed")
            .expect("cover should exist");

        assert_eq!(record.file_name, "cover-new.webp");
        assert!(record.updated_at_ms >= 0);
    }

    #[test]
    fn upsert_rejects_non_basename_cover_file_names() {
        let storage = SqliteStorage::in_memory().expect("sqlite should open");
        let game = sample_game("game:invalid-cover", "Invalid Cover");

        storage.upsert_game(&game).expect("game should store");

        for invalid_file_name in [
            "",
            ".",
            "..",
            "../cover.webp",
            "nested/cover.webp",
            r"nested\cover.webp",
        ] {
            assert!(
                storage
                    .upsert_game_cover(game.id(), invalid_file_name)
                    .is_err(),
                "expected {invalid_file_name:?} to be rejected"
            );
        }

        assert!(storage
            .find_game_cover(game.id())
            .expect("find should succeed")
            .is_none());
    }

    #[test]
    fn list_game_cover_file_names_is_sorted() {
        let storage = SqliteStorage::in_memory().expect("sqlite should open");
        let g1 = sample_game("game:a", "A");
        let g2 = sample_game("game:b", "B");

        storage
            .upsert_games(&[g1.clone(), g2.clone()])
            .expect("games should store");

        storage
            .upsert_game_cover(g1.id(), "cover-z.webp")
            .expect("cover g1 should store");

        storage
            .upsert_game_cover(g2.id(), "cover-a.webp")
            .expect("cover g2 should store");

        let names = storage
            .list_game_cover_file_names()
            .expect("list should succeed");

        assert_eq!(
            names,
            vec!["cover-a.webp".to_owned(), "cover-z.webp".to_owned()]
        );
    }

    #[test]
    fn list_all_game_covers_builds_map() {
        let storage = SqliteStorage::in_memory().expect("sqlite should open");
        let g1 = sample_game("game:a", "A");
        let g2 = sample_game("game:b", "B");

        storage
            .upsert_games(&[g1.clone(), g2.clone()])
            .expect("games should store");

        storage
            .upsert_game_cover(g1.id(), "cover-a.webp")
            .expect("cover g1 should store");

        let mut covers = storage
            .list_all_game_covers()
            .expect("list all should succeed");

        assert_eq!(covers.len(), 1);

        let record = covers.remove(g1.id()).expect("g1 cover should exist");

        assert_eq!(record.file_name, "cover-a.webp");
        assert!(record.updated_at_ms >= 0);
        assert!(!covers.contains_key(g2.id()));
    }

    fn sample_game(id: &str, title: &str) -> GameInstallation {
        let identity = GameIdentity::new(
            renderpilot_domain::GameId::new(id).expect("id should be valid"),
            title,
            Launcher::Manual,
        )
        .expect("identity should be valid");

        GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(format!("C:/Games/{title}")).expect("path should be valid"),
        )
    }
}
