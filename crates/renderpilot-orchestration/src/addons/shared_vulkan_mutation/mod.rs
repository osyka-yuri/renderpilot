//! SVAM-v1 coordinator for the shared ReShade Vulkan resource.
//!
//! The module deliberately separates the persisted manifest, deterministic
//! planning, filesystem/registry I/O, transaction sequencing, and recovery.
//! [`transaction::execute`] owns the singleton reservation lifecycle.

mod capability;
mod composer;
mod io;
mod manifest;
mod plan;
mod recovery;
mod transaction;

#[cfg(test)]
mod tests;

use std::path::{Component, Path, PathBuf};

pub(crate) use capability::{CapabilityPath, TrustedRoots};
pub(crate) use composer::compose;
pub(crate) use manifest::{Manifest, RegistryValue, Scope};
pub(crate) use plan::{FileIntent, MutationPlan, RegistryIntent};
pub(crate) use recovery::recover_pending;
pub(crate) use transaction::{
    CatalogProjection, MutationIdentity, PhysicalParticipants, Request, ScopeSpec, execute,
};

#[derive(Debug)]
pub(crate) enum MutationError {
    Io(std::io::Error),
    Manifest(manifest::ManifestError),
    Service(crate::ServiceError),
    Conflict(String),
}

impl MutationError {
    pub(crate) fn io(error: std::io::Error) -> Self {
        Self::Io(error)
    }

    pub(crate) fn manifest(error: manifest::ManifestError) -> Self {
        Self::Manifest(error)
    }

    pub(crate) fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }
}

impl std::fmt::Display for MutationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "shared Vulkan filesystem error: {error}"),
            Self::Manifest(error) => write!(formatter, "invalid shared Vulkan manifest: {error}"),
            Self::Service(error) => error.fmt(formatter),
            Self::Conflict(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for MutationError {}

impl From<MutationError> for crate::ServiceError {
    fn from(error: MutationError) -> Self {
        match error {
            MutationError::Service(error) => error,
            MutationError::Io(error) => crate::ServiceError::command_failed(error.to_string()),
            MutationError::Manifest(error) => crate::ServiceError::invalid_input(error.to_string()),
            MutationError::Conflict(error) => crate::ServiceError::invalid_input(error),
        }
    }
}

/// Resolves a persisted transaction directory without allowing an id to
/// escape the mutation root. Reservation ids are ULIDs in production, but
/// recovery must treat the durable value as untrusted input.
pub(crate) fn transaction_root(root: &Path, id: &str) -> Result<PathBuf, MutationError> {
    let path = Path::new(id);
    let mut components = path.components();
    if path.is_absolute()
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(MutationError::conflict(
            "shared Vulkan transaction id escapes the mutation root",
        ));
    }
    Ok(transaction_namespace(root).join(path))
}

/// SVAM owns a sibling namespace rather than a child of the legacy file-
/// mutation root. The legacy orphan sweep is intentionally closed over its
/// own pending table and must never see or delete an active SVAM directory.
fn transaction_namespace(file_mutation_root: &Path) -> PathBuf {
    let mut name = file_mutation_root
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("file-transactions"))
        .to_os_string();
    name.push(".shared-vulkan-v1");
    file_mutation_root.with_file_name(name)
}

fn ensure_transaction_namespace(file_mutation_root: &Path) -> Result<PathBuf, MutationError> {
    let namespace = transaction_namespace(file_mutation_root);
    let parent = namespace
        .parent()
        .ok_or_else(|| MutationError::conflict("SVAM transaction namespace has no parent"))?;
    std::fs::create_dir_all(parent).map_err(MutationError::io)?;
    match std::fs::symlink_metadata(&namespace) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(MutationError::conflict(format!(
                "SVAM transaction namespace is not a regular directory: {}",
                namespace.display()
            )));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(&namespace).map_err(MutationError::io)?;
            crate::fs::sync_directory_best_effort(parent);
        }
        Err(error) => return Err(MutationError::io(error)),
    }
    Ok(namespace)
}
