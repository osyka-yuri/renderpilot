//! SQLite storage adapter for RenderPilot.
//!
//! This crate owns SQLite schema management, connection pragmas, and repository
//! implementations. Domain types remain SQLite-agnostic.

mod error;
mod mapping;
mod repositories;
mod schema;

use std::{
    path::Path,
    sync::{Mutex, MutexGuard},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use renderpilot_application::{AppError, AppResult};
use rusqlite::{Connection, OptionalExtension, Transaction};

use crate::error::{storage_context, storage_error};

pub use repositories::file_hash_cache::FileHashCacheRow;
pub use repositories::SqliteStorage;

const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy)]
enum JournalModePreference {
    /// Prefer WAL for persistent file-backed databases.
    Wal,

    /// Keep SQLite's default journal mode.
    ///
    /// This is used for in-memory databases because WAL is not useful there and
    /// SQLite may ignore it or report `memory` as the active journal mode.
    Default,
}

impl SqliteStorage {
    /// Opens a SQLite database file and applies required pragmas and migrations.
    pub fn open(path: impl AsRef<Path>) -> AppResult<Self> {
        let connection = Connection::open(path)
            .map_err(|error| storage_context("failed to open sqlite database", error))?;

        Self::from_connection(connection, JournalModePreference::Wal)
    }

    /// Opens an in-memory SQLite database for tests and temporary use.
    pub fn in_memory() -> AppResult<Self> {
        let connection = Connection::open_in_memory()
            .map_err(|error| storage_context("failed to open in-memory sqlite database", error))?;

        Self::from_connection(connection, JournalModePreference::Default)
    }

    fn from_connection(
        connection: Connection,
        journal_mode: JournalModePreference,
    ) -> AppResult<Self> {
        configure_connection(&connection, journal_mode)?;
        schema::apply(&connection)?;

        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn connection(&self) -> AppResult<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| storage_error("sqlite connection lock is poisoned"))
    }

    fn with_connection<T>(
        &self,
        operation: impl FnOnce(&Connection) -> AppResult<T>,
    ) -> AppResult<T> {
        let connection = self.connection()?;
        operation(&connection)
    }

    fn with_transaction<T>(
        &self,
        operation: impl FnOnce(&Transaction<'_>) -> AppResult<T>,
    ) -> AppResult<T> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction()
            .map_err(|error| storage_context("failed to open sqlite transaction", error))?;
        let value = operation(&transaction)?;

        transaction
            .commit()
            .map_err(|error| storage_context("failed to commit sqlite transaction", error))?;

        Ok(value)
    }

    /// Returns the active SQLite journal mode.
    pub fn journal_mode(&self) -> AppResult<String> {
        self.with_connection(|connection| {
            connection
                .pragma_query_value(None, "journal_mode", |row| row.get(0))
                .map_err(|error| storage_context("failed to read sqlite journal mode", error))
        })
    }

    /// Sets a string setting value.
    pub fn set_setting(&self, key: &str, value: &str) -> AppResult<()> {
        let updated_at = unix_time_millis()?;

        self.with_connection(|connection| {
            connection
                .execute(
                    "INSERT INTO settings (key, value, updated_at)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(key) DO UPDATE SET
                        value = excluded.value,
                        updated_at = excluded.updated_at",
                    (key, value, updated_at),
                )
                .map_err(|error| storage_context("failed to save setting", error))?;

            Ok(())
        })
    }

    /// Reads a string setting value.
    pub fn get_setting(&self, key: &str) -> AppResult<Option<String>> {
        self.with_connection(|connection| {
            connection
                .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                    row.get(0)
                })
                .optional()
                .map_err(|error| storage_context("failed to read setting", error))
        })
    }
}

fn configure_connection(
    connection: &Connection,
    journal_mode: JournalModePreference,
) -> AppResult<()> {
    connection
        .busy_timeout(SQLITE_BUSY_TIMEOUT)
        .map_err(|error| storage_context("failed to set sqlite busy timeout", error))?;

    connection
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(|error| storage_context("failed to enable sqlite foreign keys", error))?;

    if matches!(journal_mode, JournalModePreference::Wal) {
        enable_wal_journal_mode(connection)?;
    }

    connection
        .pragma_update(None, "synchronous", "NORMAL")
        .map_err(|error| storage_context("failed to set sqlite synchronous mode", error))?;

    Ok(())
}

fn enable_wal_journal_mode(connection: &Connection) -> AppResult<()> {
    let active_mode: String = connection
        .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
        .map_err(|error| storage_context("failed to enable sqlite WAL journal mode", error))?;

    if active_mode.eq_ignore_ascii_case("wal") {
        return Ok(());
    }

    Err(AppError::storage_failed(format!(
        "failed to enable sqlite WAL journal mode: active mode is {active_mode:?}"
    )))
}

fn unix_time_millis() -> AppResult<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| storage_context("failed to create unix timestamp", error))?;

    i64::try_from(duration.as_millis())
        .map_err(|_| storage_error("unix timestamp in milliseconds does not fit into i64"))
}
