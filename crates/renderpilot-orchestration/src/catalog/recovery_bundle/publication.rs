//! Durable file writes and atomic bundle publication.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use crate::ServiceError;

pub(super) fn write_and_sync(path: &Path, bytes: &[u8]) -> Result<(), ServiceError> {
    let mut file = fs::File::create(path).map_err(|error| {
        ServiceError::command_failed(format!(
            "could not create recovery file {}: {error}",
            path.display()
        ))
    })?;
    file.write_all(bytes).map_err(|error| {
        ServiceError::command_failed(format!(
            "could not write recovery file {}: {error}",
            path.display()
        ))
    })?;
    file.sync_all().map_err(|error| {
        ServiceError::command_failed(format!(
            "could not flush recovery file {}: {error}",
            path.display()
        ))
    })
}

pub(super) fn sync_file(path: &Path) -> Result<(), ServiceError> {
    // Windows requires a write-capable handle for FlushFileBuffers. Reopen
    // copied files without truncation so sync_all has the same durability
    // semantics as files created directly by write_and_sync.
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| {
            ServiceError::command_failed(format!(
                "could not reopen recovery file {}: {error}",
                path.display()
            ))
        })?;
    file.sync_all().map_err(|error| {
        ServiceError::command_failed(format!(
            "could not flush recovery file {}: {error}",
            path.display()
        ))
    })
}

pub(super) fn publish_directory(
    temporary: &Path,
    published: &Path,
    description: &str,
) -> Result<PathBuf, ServiceError> {
    sync_directory_tree(temporary).map_err(|error| {
        ServiceError::command_failed(format!(
            "could not flush staged {description} {}: {error}",
            temporary.display()
        ))
    })?;
    fs::rename(temporary, published).map_err(|error| {
        ServiceError::command_failed(format!(
            "could not atomically publish {description} {}: {error}",
            published.display()
        ))
    })?;
    if let Some(parent) = published.parent() {
        crate::fs::sync_directory(parent).map_err(|error| {
            ServiceError::command_failed(format!(
                "could not flush published {description} directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    Ok(published.to_path_buf())
}

fn sync_directory_tree(directory: &Path) -> std::io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() && !file_type.is_symlink() {
            sync_directory_tree(&entry.path())?;
        }
    }
    crate::fs::sync_directory(directory)
}
