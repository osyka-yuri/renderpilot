//! Persistence for component swap baselines (the pre-swap original file set).
//!
//! The `.bak` sidecars on disk hold the original *bytes*; this table holds their
//! *identity* so an N-to-1 rollback can restore exactly the right files even when
//! the active bundle was renamed (1→N upgrades) or re-swapped (A→B→C).
//!
//! Follows the `method()` / `method_within_transaction()` pattern: the public
//! method wraps in a fresh transaction; the `pub(super)` variant accepts an
//! existing one for composition in `game_mutations.rs`.

use std::collections::HashMap;

use renderpilot_application::AppResult;
use renderpilot_domain::{ComponentFile, ComponentId, GameId};
use rusqlite::{OptionalExtension, Transaction, named_params};

use crate::{error::storage_error, mapping, sqlite_clock};

use super::SqliteStorage;

const SELECT_BACKUP_SQL: &str = "
    SELECT files_json
    FROM component_backups
    WHERE component_id = :component_id
";

const SELECT_BACKUPS_FOR_GAME_SQL: &str = "
    SELECT component_id, files_json
    FROM component_backups
    WHERE game_id = :game_id
";

const UPSERT_BACKUP_SQL: &str = "
    INSERT INTO component_backups
        (component_id, game_id, files_json, created_at, updated_at)
    VALUES
        (:component_id, :game_id, :files_json, :now_ms, :now_ms)
    ON CONFLICT(component_id) DO UPDATE SET
        game_id    = excluded.game_id,
        files_json = excluded.files_json,
        updated_at = excluded.updated_at
";

const DELETE_BACKUP_SQL: &str = "
    DELETE FROM component_backups
    WHERE component_id = :component_id
";

impl SqliteStorage {
    /// Returns the recorded pre-swap baseline files for a component, if any.
    ///
    /// `Some` only means the identity row exists. Orchestration must still
    /// confirm that the corresponding sidecars or unchanged live bytes exist.
    pub fn get_component_backup(
        &self,
        component_id: &ComponentId,
    ) -> AppResult<Option<Vec<ComponentFile>>> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare_cached(SELECT_BACKUP_SQL)
                .map_err(storage_error)?;

            let files_json: Option<String> = statement
                .query_row(
                    named_params! { ":component_id": component_id.as_str() },
                    |row| row.get(0),
                )
                .optional()
                .map_err(storage_error)?;

            match files_json {
                Some(files_json) => Ok(Some(mapping::component_files(&files_json)?)),
                None => Ok(None),
            }
        })
    }

    /// Returns every persisted component baseline for a game in one query.
    pub fn component_backups_for_game(
        &self,
        game_id: &GameId,
    ) -> AppResult<HashMap<ComponentId, Vec<ComponentFile>>> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare_cached(SELECT_BACKUPS_FOR_GAME_SQL)
                .map_err(storage_error)?;

            let rows = statement
                .query_map(named_params! { ":game_id": game_id.as_str() }, |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(storage_error)?;

            let mut backups = HashMap::new();
            for row in rows {
                let (component_id, files_json) = row.map_err(storage_error)?;
                backups.insert(
                    mapping::component_id(component_id)?,
                    mapping::component_files(&files_json)?,
                );
            }
            Ok(backups)
        })
    }

    /// Explicitly sets the backup baseline for a component outside of a normal swap.
    /// Used by the scan process to automatically recover orphaned `.bak` files.
    pub fn recover_component_backup(
        &self,
        game_id: &GameId,
        component_id: &ComponentId,
        files: &[ComponentFile],
    ) -> AppResult<()> {
        self.with_transaction(|transaction| {
            set_component_backup_within_transaction(transaction, game_id, component_id, files)
        })
    }
}

pub(super) fn set_component_backup_within_transaction(
    transaction: &Transaction<'_>,
    game_id: &GameId,
    component_id: &ComponentId,
    files: &[ComponentFile],
) -> AppResult<()> {
    let now_ms = sqlite_clock::now_ms(transaction)?;
    let files_json = mapping::serialize_json(files)?;

    transaction
        .prepare_cached(UPSERT_BACKUP_SQL)
        .map_err(storage_error)?
        .execute(named_params! {
            ":component_id": component_id.as_str(),
            ":game_id": game_id.as_str(),
            ":files_json": files_json,
            ":now_ms": now_ms,
        })
        .map_err(storage_error)?;

    Ok(())
}

pub(super) fn delete_component_backup_within_transaction(
    transaction: &Transaction<'_>,
    component_id: &ComponentId,
) -> AppResult<()> {
    transaction
        .prepare_cached(DELETE_BACKUP_SQL)
        .map_err(storage_error)?
        .execute(named_params! { ":component_id": component_id.as_str() })
        .map_err(storage_error)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use renderpilot_application::GameRepository;
    use renderpilot_domain::{
        GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
    };

    use super::*;

    #[test]
    fn component_backups_for_game_returns_complete_rows_for_only_that_game() {
        let storage = SqliteStorage::in_memory().expect("storage");
        let game_a = game("steam:1");
        let game_b = game("steam:2");
        storage.upsert_game(&game_a).expect("game a");
        storage.upsert_game(&game_b).expect("game b");

        let component_a = ComponentId::new("component:a").expect("component a");
        let component_b = ComponentId::new("component:b").expect("component b");
        let other_component = ComponentId::new("component:other").expect("other component");
        let baseline_a = baseline("C:/Games/Test/a.dll");
        let baseline_b = baseline("C:/Games/Test/b.dll");
        storage
            .recover_component_backup(game_a.id(), &component_a, &baseline_a)
            .expect("baseline a");
        storage
            .recover_component_backup(game_a.id(), &component_b, &baseline_b)
            .expect("baseline b");
        storage
            .recover_component_backup(game_b.id(), &other_component, &baseline("D:/other.dll"))
            .expect("other baseline");

        let backups = storage
            .component_backups_for_game(game_a.id())
            .expect("game backups");
        assert_eq!(backups.len(), 2);
        assert_eq!(backups.get(&component_a), Some(&baseline_a));
        assert_eq!(backups.get(&component_b), Some(&baseline_b));
        assert!(!backups.contains_key(&other_component));
    }

    fn game(id: &str) -> GameInstallation {
        GameInstallation::new(
            GameIdentity::new(
                GameId::new(id).expect("game id"),
                format!("Game {id}"),
                Launcher::Manual,
            )
            .expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(format!("C:/Games/{id}")).expect("install path"),
        )
    }

    fn baseline(path: &str) -> Vec<ComponentFile> {
        vec![ComponentFile::new(
            PathRef::new(path).expect("component path"),
        )]
    }
}
