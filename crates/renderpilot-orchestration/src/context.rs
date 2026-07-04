use crate::{ServiceError, storage::open_catalog_storage};
use renderpilot_storage_sqlite::SqliteStorage;
use std::sync::{PoisonError, RwLock};

use crate::addons::capabilities::ProfileCapabilitySnapshot;

/// Shared application context holding the catalog storage and configuration.
pub struct Context {
    storage: SqliteStorage,
    profile_capabilities: RwLock<ProfileCapabilitySnapshot>,
}

impl Context {
    /// Opens the application context and initializes shared storage.
    pub fn open() -> Result<Self, ServiceError> {
        let storage = open_catalog_storage()?;
        Ok(Self::from_storage(storage))
    }

    /// Opens the application context using a custom database path (useful for testing).
    pub fn open_at(path: impl AsRef<std::path::Path>) -> Result<Self, ServiceError> {
        let storage = SqliteStorage::open(path.as_ref())
            .map_err(|e| ServiceError::CommandFailed(e.to_string()))?;
        Ok(Self::from_storage(storage))
    }

    /// Creates a Context from an existing storage connection.
    pub fn from_storage(storage: SqliteStorage) -> Self {
        Self {
            storage,
            profile_capabilities: RwLock::new(ProfileCapabilitySnapshot::default()),
        }
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

    pub(crate) fn profile_capability_snapshot(&self) -> ProfileCapabilitySnapshot {
        self.profile_capabilities
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn replace_profile_capability_snapshot(&self, snapshot: ProfileCapabilitySnapshot) {
        *self
            .profile_capabilities
            .write()
            .unwrap_or_else(PoisonError::into_inner) = snapshot;
    }
}
