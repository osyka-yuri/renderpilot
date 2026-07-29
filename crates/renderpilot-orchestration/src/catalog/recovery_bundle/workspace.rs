//! Lifecycle of one atomically published recovery-bundle staging directory.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use renderpilot_storage_sqlite::SqliteStorage;

use crate::ServiceError;

#[derive(Clone, Copy)]
pub(super) enum RecoveryBundleKind {
    Consolidation,
    RootCorrection,
    ManagedCleanup,
}

impl RecoveryBundleKind {
    const fn slug(self) -> &'static str {
        match self {
            Self::Consolidation => "consolidation",
            Self::RootCorrection => "root-correction",
            Self::ManagedCleanup => "managed-cleanup",
        }
    }

    const fn in_memory_error(self) -> &'static str {
        match self {
            Self::Consolidation => "cannot create a recovery bundle for in-memory catalog storage",
            Self::RootCorrection => {
                "cannot create a root-correction recovery bundle for in-memory catalog storage"
            }
            Self::ManagedCleanup => {
                "cannot create a managed-cleanup recovery bundle for in-memory catalog storage"
            }
        }
    }
}

pub(super) struct BundleWorkspace {
    catalog_path: PathBuf,
    temporary: PathBuf,
    published: PathBuf,
    timestamp: u128,
}

impl BundleWorkspace {
    pub(super) fn create(
        storage: &SqliteStorage,
        kind: RecoveryBundleKind,
    ) -> Result<Self, ServiceError> {
        let catalog_path = storage
            .catalog_file_path()?
            .ok_or_else(|| ServiceError::command_failed(kind.in_memory_error()))?;
        let catalog_dir = catalog_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let recovery_root = catalog_dir.join("recovery");
        fs::create_dir_all(&recovery_root).map_err(|error| {
            ServiceError::command_failed(format!(
                "could not create recovery directory {}: {error}",
                recovery_root.display()
            ))
        })?;

        let id = ulid::Ulid::generate().to_string();
        let timestamp = unix_millis();
        let slug = kind.slug();
        let temporary = recovery_root.join(format!(".tmp-{slug}-{id}"));
        let published = recovery_root.join(format!("{slug}-{timestamp}-{id}.bundle"));
        fs::create_dir(&temporary).map_err(|error| {
            ServiceError::command_failed(format!(
                "could not create {slug} recovery staging directory {}: {error}",
                temporary.display()
            ))
        })?;

        Ok(Self {
            catalog_path,
            temporary,
            published,
            timestamp,
        })
    }

    pub(super) fn catalog_path(&self) -> &Path {
        &self.catalog_path
    }

    pub(super) fn temporary(&self) -> &Path {
        &self.temporary
    }

    pub(super) fn published(&self) -> &Path {
        &self.published
    }

    pub(super) const fn timestamp(&self) -> u128 {
        self.timestamp
    }

    pub(super) fn build(
        self,
        builder: impl FnOnce(&Self) -> Result<PathBuf, ServiceError>,
    ) -> Result<PathBuf, ServiceError> {
        let result = builder(&self);
        if result.is_err() {
            let _ = fs::remove_dir_all(&self.temporary);
        }
        result
    }
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}
