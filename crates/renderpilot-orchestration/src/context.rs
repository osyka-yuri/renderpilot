use crate::{storage::open_catalog_storage, ServiceError};
use renderpilot_storage_sqlite::SqliteStorage;

/// Shared application context holding the catalog storage and configuration.
pub struct Context {
    storage: SqliteStorage,
}

impl Context {
    /// Opens the application context and initializes shared storage.
    pub fn open() -> Result<Self, ServiceError> {
        let storage = open_catalog_storage()?;
        Ok(Self { storage })
    }

    /// Opens the application context using a custom database path (useful for testing).
    pub fn open_at(path: impl AsRef<std::path::Path>) -> Result<Self, ServiceError> {
        let storage = SqliteStorage::open(path.as_ref())
            .map_err(|e| ServiceError::CommandFailed(e.to_string()))?;
        Ok(Self { storage })
    }

    /// Creates a Context from an existing storage connection.
    pub fn from_storage(storage: SqliteStorage) -> Self {
        Self { storage }
    }

    /// Exposes the underlying SQLite storage for orchestration internal use.
    ///
    /// Intentionally `pub(crate)`: only orchestration feature modules may drive
    /// the storage ports. Front-ends (`renderpilot-api`, `renderpilot-cli`) must
    /// go through the typed feature functions, keeping the
    /// orchestration↔presentation boundary compiler-enforced. Tests that need
    /// raw storage open their own [`SqliteStorage`] on the same database path.
    pub(crate) fn storage(&self) -> &SqliteStorage {
        &self.storage
    }
}
