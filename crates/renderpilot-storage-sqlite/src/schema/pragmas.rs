//! Captures and restores the connection's `foreign_keys` pragma across a
//! schema migration (a rebuild must drop tables with FKs disabled).

use renderpilot_application::AppResult;
use rusqlite::Connection;

use crate::error::storage_context;

#[derive(Debug)]
pub(super) struct ForeignKeysState {
    was_enabled: bool,
}

impl ForeignKeysState {
    pub(super) fn capture_and_disable(connection: &Connection) -> AppResult<Self> {
        let was_enabled = foreign_keys_enabled(connection)?;

        set_foreign_keys(
            connection,
            false,
            "could not disable sqlite foreign_keys before schema migration",
        )?;

        Ok(Self { was_enabled })
    }

    pub(super) fn restore(self, connection: &Connection, result: AppResult<()>) -> AppResult<()> {
        let restore_result = set_foreign_keys(
            connection,
            self.was_enabled,
            "could not restore sqlite foreign_keys after schema migration",
        );

        match (result, restore_result) {
            (Err(error), _) => Err(error),
            (Ok(()), Err(error)) => Err(error),
            (Ok(()), Ok(())) => Ok(()),
        }
    }
}

fn foreign_keys_enabled(connection: &Connection) -> AppResult<bool> {
    let enabled: i64 = connection
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .map_err(|error| storage_context("could not read sqlite foreign_keys pragma", error))?;

    Ok(enabled != 0)
}

fn set_foreign_keys(connection: &Connection, enabled: bool, context: &str) -> AppResult<()> {
    let sql = if enabled {
        "PRAGMA foreign_keys = ON"
    } else {
        "PRAGMA foreign_keys = OFF"
    };

    connection
        .execute_batch(sql)
        .map_err(|error| storage_context(context, error))
}
