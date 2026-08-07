//! Cooperative no-replace fallback for unsupported development hosts.

use std::{
    fs::{self, OpenOptions},
    io,
    path::Path,
};

use crate::ServiceError;

use super::replace::temporary_file_path;
#[cfg(test)]
use super::{NoReplaceTestFault, no_replace_test_fault};
use super::{
    NoReplaceWrite, PreparedNoReplaceWrite, sync_no_replace_temp_file, write_no_replace_temp_bytes,
};

#[cfg(all(
    not(any(windows, target_os = "linux")),
    feature = "development-host-fallback"
))]
impl PreparedNoReplaceWrite {
    pub(super) fn publish_development_fallback(&mut self) -> Result<NoReplaceWrite, ServiceError> {
        let temp_path = self
            .temp_path
            .take()
            .expect("development fallback retains its temporary path");
        match move_file_no_replace(&temp_path, &self.destination) {
            Ok(()) => Ok(NoReplaceWrite::Published),
            Err(publication_error) => {
                let inspection =
                    inspect_no_replace_destination(&self.destination, &publication_error);
                let cleanup = remove_owned_no_replace_temp(&temp_path);
                match (inspection, cleanup) {
                    (_, Err(cleanup_error)) => Err(crate::failed(format!(
                        "failed to remove development publication temporary file `{}` after publication error `{publication_error}`: {cleanup_error}",
                        temp_path.display()
                    ))),
                    (Err(inspection_error), Ok(())) => Err(inspection_error),
                    (Ok(true), Ok(())) => Ok(NoReplaceWrite::Occupied),
                    (Ok(false), Ok(())) => Err(crate::failed(format!(
                        "failed to publish development temporary file `{}` to `{}` without replacement: {publication_error}",
                        temp_path.display(),
                        self.destination.display()
                    ))),
                }
            }
        }
    }

    #[cfg(test)]
    pub(super) fn discard_development_fallback(mut self) -> Result<(), ServiceError> {
        let Some(temp_path) = self.temp_path.take() else {
            return Ok(());
        };
        remove_owned_no_replace_temp(&temp_path).map_err(|error| {
            crate::failed(format!(
                "failed to remove development publication temporary file `{}`: {error}",
                temp_path.display()
            ))
        })
    }

    pub(super) fn drop_development_fallback(&mut self) {
        let Some(temp_path) = self.temp_path.take() else {
            return;
        };
        if let Err(error) = remove_owned_no_replace_temp(&temp_path) {
            log::error!(
                "failed to remove abandoned development publication temporary file `{}`: {error}",
                temp_path.display()
            );
        }
    }
}

#[cfg(all(
    not(any(windows, target_os = "linux")),
    feature = "development-host-fallback"
))]
pub(super) fn prepare_development_no_replace(
    path: &Path,
    bytes: &[u8],
) -> Result<PreparedNoReplaceWrite, ServiceError> {
    let temp_path = temporary_file_path(path, "publish");
    write_no_replace_temp_file(&temp_path, bytes)?;
    Ok(PreparedNoReplaceWrite {
        temp_path: Some(temp_path),
        destination: path.to_owned(),
    })
}

#[cfg(all(
    not(any(windows, target_os = "linux")),
    feature = "development-host-fallback"
))]
fn inspect_no_replace_destination(
    path: &Path,
    publication_error: &io::Error,
) -> Result<bool, ServiceError> {
    #[cfg(test)]
    no_replace_test_fault(NoReplaceTestFault::Inspect)?;
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(inspection_error) if inspection_error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(inspection_error) => Err(crate::failed(format!(
            "failed to inspect development destination `{}` after publication error `{publication_error}`: {inspection_error}",
            path.display()
        ))),
    }
}

#[cfg(all(
    not(any(windows, target_os = "linux")),
    feature = "development-host-fallback"
))]
fn move_file_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(test)]
    no_replace_test_fault(NoReplaceTestFault::Publish)
        .map_err(|error| io::Error::other(error.to_string()))?;
    // This cooperative development fallback has no hostile-namespace claim.
    fs::hard_link(source, destination)?;
    fs::remove_file(source)
}

#[cfg(all(
    not(any(windows, target_os = "linux")),
    feature = "development-host-fallback"
))]
fn write_no_replace_temp_file(temp_path: &Path, bytes: &[u8]) -> Result<(), ServiceError> {
    #[cfg(test)]
    no_replace_test_fault(NoReplaceTestFault::Create)?;
    let mut temp_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp_path)
        .map_err(|error| {
            crate::failed(format!(
                "failed to create development publication temporary file `{}`: {error}",
                temp_path.display()
            ))
        })?;
    let write = write_no_replace_temp_bytes(&mut temp_file, temp_path, bytes);
    if let Err(write_error) = write {
        drop(temp_file);
        return match remove_owned_no_replace_temp(temp_path) {
            Err(cleanup_error) => Err(crate::failed(format!(
                "failed to remove development publication temporary file `{}` after write error `{write_error}`: {cleanup_error}",
                temp_path.display()
            ))),
            Ok(()) => Err(write_error),
        };
    }
    let sync = sync_no_replace_temp_file(&temp_file, temp_path);
    if let Err(sync_error) = sync {
        drop(temp_file);
        return match remove_owned_no_replace_temp(temp_path) {
            Err(cleanup_error) => Err(crate::failed(format!(
                "failed to remove development publication temporary file `{}` after sync error `{sync_error}`: {cleanup_error}",
                temp_path.display()
            ))),
            Ok(()) => Err(sync_error),
        };
    }
    Ok(())
}

#[cfg(all(
    not(any(windows, target_os = "linux")),
    feature = "development-host-fallback"
))]
fn remove_owned_no_replace_temp(temp_path: &Path) -> io::Result<()> {
    #[cfg(test)]
    no_replace_test_fault(NoReplaceTestFault::Cleanup)
        .map_err(|error| io::Error::other(error.to_string()))?;
    fs::remove_file(temp_path)
}
