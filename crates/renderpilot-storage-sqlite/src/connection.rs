//! SQLite connection open, pragmas, and storage handle helpers.

use std::{
    path::Path,
    sync::{Mutex, MutexGuard},
    time::Duration,
};

use renderpilot_application::{AppError, AppResult};
use rusqlite::{Connection, Transaction};

use crate::error::{storage_context, storage_error};
use crate::repositories::SqliteStorage;
use crate::schema;

const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum JournalModePreference {
    /// Prefer WAL for persistent file-backed databases.
    Wal,

    /// Keep SQLite's default journal mode.
    ///
    /// Used for in-memory databases because WAL is not useful there and
    /// SQLite may ignore it or report `memory` as the active journal mode.
    Default,
}

impl JournalModePreference {
    fn apply(self, connection: &Connection) -> AppResult<()> {
        match self {
            Self::Wal => enable_wal_journal_mode(connection),
            Self::Default => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct ConnectionOptions {
    journal_mode: JournalModePreference,
    busy_timeout: Duration,
}

impl ConnectionOptions {
    const fn persistent_database() -> Self {
        Self {
            journal_mode: JournalModePreference::Wal,
            busy_timeout: SQLITE_BUSY_TIMEOUT,
        }
    }

    const fn transient_database() -> Self {
        Self {
            journal_mode: JournalModePreference::Default,
            busy_timeout: SQLITE_BUSY_TIMEOUT,
        }
    }
}

impl SqliteStorage {
    /// Opens a SQLite database file and applies required pragmas and migrations.
    pub fn open(path: impl AsRef<Path>) -> AppResult<Self> {
        let connection = Connection::open(path)
            .map_err(|error| storage_context("failed to open sqlite database", error))?;

        Self::from_connection(connection, ConnectionOptions::persistent_database())
    }

    /// Opens an in-memory SQLite database for tests and temporary use.
    pub fn in_memory() -> AppResult<Self> {
        let connection = Connection::open_in_memory()
            .map_err(|error| storage_context("failed to open in-memory sqlite database", error))?;

        Self::from_connection(connection, ConnectionOptions::transient_database())
    }

    fn from_connection(mut connection: Connection, options: ConnectionOptions) -> AppResult<Self> {
        configure_connection(&connection, options)?;
        schema::apply(&mut connection)?;

        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub(crate) fn connection(&self) -> AppResult<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| storage_error("sqlite connection lock is poisoned"))
    }

    pub(crate) fn with_connection<T>(
        &self,
        operation: impl FnOnce(&Connection) -> AppResult<T>,
    ) -> AppResult<T> {
        let connection = self.connection()?;
        operation(&connection)
    }

    pub(crate) fn with_connection_mut<T>(
        &self,
        operation: impl FnOnce(&mut Connection) -> AppResult<T>,
    ) -> AppResult<T> {
        let mut connection = self.connection()?;
        operation(&mut connection)
    }

    pub(crate) fn with_transaction<T>(
        &self,
        operation: impl FnOnce(&Transaction<'_>) -> AppResult<T>,
    ) -> AppResult<T> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|error| storage_context("failed to open sqlite transaction", error))?;

        let value = match operation(&transaction) {
            Ok(value) => value,
            Err(error) => {
                // Dropping a rusqlite transaction rolls it back. Returning the
                // operation error preserves the original failure cause.
                drop(transaction);
                return Err(error);
            }
        };

        transaction
            .commit()
            .map_err(|error| storage_context("failed to commit sqlite transaction", error))?;

        Ok(value)
    }

    /// Returns the active SQLite journal mode.
    pub fn journal_mode(&self) -> AppResult<String> {
        self.with_connection(read_journal_mode)
    }
}

fn configure_connection(connection: &Connection, options: ConnectionOptions) -> AppResult<()> {
    set_busy_timeout(connection, options.busy_timeout)?;
    enable_foreign_keys(connection)?;
    options.journal_mode.apply(connection)?;
    set_synchronous_normal(connection)?;

    Ok(())
}

fn set_busy_timeout(connection: &Connection, timeout: Duration) -> AppResult<()> {
    connection
        .busy_timeout(timeout)
        .map_err(|error| storage_context("failed to set sqlite busy timeout", error))
}

fn enable_foreign_keys(connection: &Connection) -> AppResult<()> {
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(|error| storage_context("failed to enable sqlite foreign keys", error))
}

fn set_synchronous_normal(connection: &Connection) -> AppResult<()> {
    connection
        .pragma_update(None, "synchronous", "NORMAL")
        .map_err(|error| storage_context("failed to set sqlite synchronous mode", error))
}

fn enable_wal_journal_mode(connection: &Connection) -> AppResult<()> {
    let active_mode = set_journal_mode_wal(connection)?;

    if active_mode.eq_ignore_ascii_case("wal") {
        return Ok(());
    }

    Err(AppError::storage_failed(format!(
        "failed to enable sqlite WAL journal mode: active mode is {active_mode:?}"
    )))
}

fn set_journal_mode_wal(connection: &Connection) -> AppResult<String> {
    connection
        .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
        .map_err(|error| storage_context("failed to enable sqlite WAL journal mode", error))
}

fn read_journal_mode(connection: &Connection) -> AppResult<String> {
    connection
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .map_err(|error| storage_context("failed to read sqlite journal mode", error))
}
