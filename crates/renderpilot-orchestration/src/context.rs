use crate::{ServiceError, storage::open_catalog_storage};
use renderpilot_storage_sqlite::SqliteStorage;
use std::path::{Path, PathBuf};
use std::sync::{PoisonError, RwLock};

use crate::addons::capabilities::ProfileCapabilitySnapshot;

/// Shared application context holding the catalog storage and configuration.
pub struct Context {
    storage: SqliteStorage,
    profile_capabilities: RwLock<ProfileCapabilitySnapshot>,
    file_mutation_root: PathBuf,
}

impl Context {
    /// Opens the application context and initializes shared storage.
    pub fn open() -> Result<Self, ServiceError> {
        let storage = open_catalog_storage()?;
        let root = crate::app_dir::app_dir()?.join("file-transactions");
        Ok(Self::from_storage_with_mutation_root(storage, root))
    }

    /// Opens the application context using a custom database path (useful for testing).
    pub fn open_at(path: impl AsRef<std::path::Path>) -> Result<Self, ServiceError> {
        let path = path.as_ref();
        let storage =
            SqliteStorage::open(path).map_err(|e| ServiceError::command_failed(e.to_string()))?;
        let root = mutation_root_for_catalog(path);
        Ok(Self::from_storage_with_mutation_root(storage, root))
    }

    /// Creates a [`Context`] from an existing storage connection, for tests.
    ///
    /// The file-mutation root is a fresh, nondeterministic temp directory
    /// (`<temp>/renderpilot-file-transactions/<pid>/<ulid>`), so each call is
    /// isolated. Only available under `#[cfg(test)]`; production code must use
    /// [`Context::open`] / [`Context::open_at`], which derive a stable
    /// mutation root from the catalog path.
    #[cfg(test)]
    pub fn from_storage(storage: SqliteStorage) -> Self {
        let root = std::env::temp_dir()
            .join("renderpilot-file-transactions")
            .join(std::process::id().to_string())
            .join(ulid::Ulid::generate().to_string());
        Self::from_storage_with_mutation_root(storage, root)
    }

    fn from_storage_with_mutation_root(
        storage: SqliteStorage,
        file_mutation_root: PathBuf,
    ) -> Self {
        Self {
            storage,
            profile_capabilities: RwLock::new(ProfileCapabilitySnapshot::default()),
            file_mutation_root,
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

    pub(crate) fn file_mutation_root(&self) -> &Path {
        &self.file_mutation_root
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

fn mutation_root_for_catalog(path: &Path) -> PathBuf {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let catalog_name = path
        .file_name()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| std::ffi::OsStr::new("catalog.sqlite"));

    parent.join("file-transactions").join(catalog_name)
}

#[cfg(test)]
mod tests {
    use super::mutation_root_for_catalog;
    use std::path::Path;

    #[test]
    fn custom_catalogs_get_stable_isolated_transaction_namespaces() {
        let first = mutation_root_for_catalog(Path::new("C:/temp/first.sqlite"));
        let first_again = mutation_root_for_catalog(Path::new("C:/temp/first.sqlite"));
        let second = mutation_root_for_catalog(Path::new("C:/temp/second.sqlite"));

        assert_eq!(first, first_again);
        assert_ne!(first, second);
        assert_eq!(first, Path::new("C:/temp/file-transactions/first.sqlite"));
    }
}
