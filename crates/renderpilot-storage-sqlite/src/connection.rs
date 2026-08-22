//! SQLite connection open, pragmas, and storage handle helpers.

use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use renderpilot_application::{AppError, AppResult};
use rusqlite::{Connection, OpenFlags, Transaction, TransactionBehavior};

use crate::error::{storage_context, storage_error};
use crate::repositories::SqliteStorage;
use crate::schema;

const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const CATALOG_PROJECTION_TABLES: &[&str] = &[
    "games",
    "components",
    "game_ui_state",
    "component_backups",
    "nvapi_executable_overrides",
    "operations",
    "operation_items",
    "installed_addons",
    "profile_addon_capabilities",
    "library_artifacts",
    "catalog_scan_authority",
];

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

    /// Opens one freshly created portable catalog after the supervisor issued
    /// its durable CommitPermit. This is intentionally not the general schema
    /// opener: it accepts only an empty SQLite catalog and initializes it once
    /// through the portable catalog boundary on this exact connection.
    pub fn open_fresh_portable(path: impl AsRef<Path>) -> AppResult<Self> {
        let path = path.as_ref();
        let mut connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )
        .map_err(|error| storage_context("failed to open fresh portable sqlite catalog", error))?;
        configure_portable_pre_transaction(&connection)?;
        schema::portable_catalog::initialize_fresh_portable_catalog(&mut connection).map_err(
            |error| storage_context("failed to initialize fresh portable sqlite catalog", error),
        )?;
        configure_portable_post_commit(&connection)?;
        Self::finalize_connection(connection)
    }

    /// Opens one already-current portable catalog after the supervisor issued
    /// its durable CommitPermit. It never creates, migrates, repairs, backs up,
    /// or reopens the catalog.
    pub fn open_current_portable(path: impl AsRef<Path>) -> AppResult<Self> {
        let path = path.as_ref();
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(|error| {
                storage_context("failed to open current portable sqlite catalog", error)
            })?;
        configure_portable_pre_transaction(&connection)?;
        schema::portable_catalog::validate_current_portable_catalog(&connection).map_err(
            |error| storage_context("failed to validate current portable sqlite catalog", error),
        )?;
        configure_portable_post_commit(&connection)?;
        Self::finalize_connection(connection)
    }

    fn from_connection(mut connection: Connection, options: ConnectionOptions) -> AppResult<Self> {
        configure_connection(&connection, options)?;
        schema::apply(&mut connection)?;

        Self::finalize_connection(connection)
    }

    fn finalize_connection(connection: Connection) -> AppResult<Self> {
        let catalog_generation = Arc::new(AtomicU64::new(0));
        let hook_generation = Arc::clone(&catalog_generation);
        connection
            .update_hook(Some(move |_, database: &str, table: &str, _| {
                if database == "main" && CATALOG_PROJECTION_TABLES.contains(&table) {
                    hook_generation.fetch_add(1, Ordering::AcqRel);
                }
            }))
            .map_err(|error| storage_context("failed to install catalog update hook", error))?;

        Ok(Self {
            connection: Mutex::new(connection),
            catalog_generation,
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

    /// Runs one catalog operation behind SQLite's write reservation.
    ///
    /// Durable mutation reservation uses this boundary so the singleton
    /// resource decision and its insert cannot race another writer.
    pub(crate) fn with_immediate_transaction<T>(
        &self,
        operation: impl FnOnce(&Transaction<'_>) -> AppResult<T>,
    ) -> AppResult<T> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| {
                storage_context("failed to open immediate sqlite transaction", error)
            })?;

        let value = match operation(&transaction) {
            Ok(value) => value,
            Err(error) => {
                drop(transaction);
                return Err(error);
            }
        };

        transaction.commit().map_err(|error| {
            storage_context("failed to commit immediate sqlite transaction", error)
        })?;
        Ok(value)
    }

    /// Returns the active SQLite journal mode.
    pub fn journal_mode(&self) -> AppResult<String> {
        self.with_connection(read_journal_mode)
    }

    /// Runs a diagnostic operation and reports how many top-level SQLite
    /// `SELECT` statements were prepared. This is primarily used by regression
    /// tests that prove a batch read path does not grow with catalog size.
    #[cfg(feature = "test-instrumentation")]
    #[doc(hidden)]
    pub fn with_select_statement_count<T, E>(
        &self,
        operation: impl FnOnce(&Self) -> Result<T, E>,
    ) -> Result<(T, u64), E>
    where
        E: From<renderpilot_application::AppError>,
    {
        use rusqlite::hooks::{AuthAction, AuthContext, Authorization};

        let count = Arc::new(AtomicU64::new(0));
        let hook_count = Arc::clone(&count);
        {
            let connection = self.connection().map_err(E::from)?;
            connection
                .authorizer(Some(move |context: AuthContext<'_>| {
                    if matches!(context.action, AuthAction::Select) {
                        hook_count.fetch_add(1, Ordering::Relaxed);
                    }
                    Authorization::Allow
                }))
                .map_err(|error| {
                    E::from(storage_context(
                        "failed to install SQLite SELECT counter",
                        error,
                    ))
                })?;
        }

        let result = operation(self);
        let cleanup = self.connection().and_then(|connection| {
            connection
                .authorizer(None::<fn(rusqlite::hooks::AuthContext<'_>) -> Authorization>)
                .map_err(|error| storage_context("failed to remove SQLite SELECT counter", error))
        });
        if let Err(error) = cleanup {
            return Err(E::from(error));
        }

        result.map(|value| (value, count.load(Ordering::Relaxed)))
    }
}

fn configure_connection(connection: &Connection, options: ConnectionOptions) -> AppResult<()> {
    set_busy_timeout(connection, options.busy_timeout)?;
    enable_foreign_keys(connection)?;
    options.journal_mode.apply(connection)?;
    set_synchronous_normal(connection)?;

    Ok(())
}

fn configure_portable_pre_transaction(connection: &Connection) -> AppResult<()> {
    set_busy_timeout(connection, SQLITE_BUSY_TIMEOUT)?;
    enable_foreign_keys(connection)
}

fn configure_portable_post_commit(connection: &Connection) -> AppResult<()> {
    enable_wal_journal_mode(connection)?;
    set_synchronous_normal(connection)
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
