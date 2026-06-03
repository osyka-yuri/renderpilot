//! Persistence for component swap baselines (the pre-swap original file set).
//!
//! The `.bak` sidecars on disk hold the original *bytes*; this table holds their
//! *identity* so an N-to-1 rollback can restore exactly the right files even when
//! the active bundle was renamed (1→N upgrades) or re-swapped (A→B→C).

use std::collections::HashSet;

use renderpilot_application::AppResult;
use renderpilot_domain::{ComponentFile, ComponentId, GameId, GraphicsComponent};
use rusqlite::{named_params, OptionalExtension, Transaction};

use crate::{error::storage_error, mapping, sqlite_clock};

use super::{components, SqliteStorage};

const SELECT_BACKUP_SQL: &str = "
    SELECT files_json
    FROM component_backups
    WHERE component_id = :component_id
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
    /// `Some` means the component currently has an applied swap whose original
    /// files can be restored; `None` means there is nothing to roll back.
    pub fn get_component_backup(
        &self,
        component_id: &ComponentId,
    ) -> AppResult<Option<Vec<ComponentFile>>> {
        let connection = self.connection()?;
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
            Some(files_json) => Ok(Some(mapping::component_files(files_json)?)),
            None => Ok(None),
        }
    }

    /// Returns the ids of components in a game that currently have a swap baseline
    /// (i.e. can be rolled back). One query per game.
    pub fn component_backup_ids_for_game(&self, game_id: &GameId) -> AppResult<HashSet<String>> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare_cached("SELECT component_id FROM component_backups WHERE game_id = :game_id")
            .map_err(storage_error)?;

        let rows = statement
            .query_map(named_params! { ":game_id": game_id.as_str() }, |row| {
                row.get::<_, String>(0)
            })
            .map_err(storage_error)?;

        let mut ids = HashSet::new();
        for row in rows {
            ids.insert(row.map_err(storage_error)?);
        }
        Ok(ids)
    }

    /// Persists the post-apply component set and records the swap baseline in one
    /// transaction. `backup` is `Some` only on the first swap of a component, so
    /// the original baseline is preserved across re-swaps (A→B→C).
    pub fn commit_bundle_apply(
        &self,
        game_id: &GameId,
        components: &[GraphicsComponent],
        backup: Option<(&ComponentId, &[ComponentFile])>,
    ) -> AppResult<()> {
        self.with_transaction(|transaction| {
            components::replace_components_for_game_within_transaction(
                transaction,
                game_id,
                components,
            )?;
            if let Some((component_id, files)) = backup {
                set_component_backup_within_transaction(transaction, game_id, component_id, files)?;
            }
            Ok(())
        })
    }

    /// Restores the post-rollback component set and clears the swap baseline in
    /// one transaction.
    pub fn commit_bundle_rollback(
        &self,
        game_id: &GameId,
        components: &[GraphicsComponent],
        component_id: &ComponentId,
    ) -> AppResult<()> {
        self.with_transaction(|transaction| {
            components::replace_components_for_game_within_transaction(
                transaction,
                game_id,
                components,
            )?;
            delete_component_backup_within_transaction(transaction, component_id)?;
            Ok(())
        })
    }
}

fn set_component_backup_within_transaction(
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

fn delete_component_backup_within_transaction(
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
