//! Linux unnamed-file no-replace publication.

use std::{
    ffi::OsString,
    fs::File,
    io,
    os::fd::{AsFd, AsRawFd, OwnedFd},
    path::{Path, PathBuf},
};

use crate::ServiceError;

#[cfg(test)]
use super::{NoReplaceTestFault, no_replace_test_fault};
use super::{
    NoReplaceWrite, PreparedNoReplaceWrite, sync_no_replace_temp_file, write_no_replace_temp_bytes,
};

#[cfg(target_os = "linux")]
impl PreparedNoReplaceWrite {
    pub(super) fn publish_linux(&mut self) -> Result<NoReplaceWrite, ServiceError> {
        #[cfg(test)]
        no_replace_test_fault(NoReplaceTestFault::Publish)?;

        match link_linux_candidate(&self.file, &self.parent_directory, &self.destination_leaf) {
            Ok(result) => Ok(result),
            Err(publication_error) => {
                if let Err(cleanup_error) = self.discard_linux_exact() {
                    return Err(crate::failed(format!(
                        "failed to close exact unpublished Linux candidate after publication error `{publication_error}`: {cleanup_error}"
                    )));
                }
                Err(crate::failed(format!(
                    "failed to publish exact Linux candidate without replacement: {publication_error}"
                )))
            }
        }
    }

    #[cfg(test)]
    pub(super) fn discard_linux(self) -> Result<(), ServiceError> {
        self.discard_linux_exact().map_err(|error| {
            crate::failed(format!(
                "failed to close exact prepared Linux publication candidate: {error}"
            ))
        })
    }

    fn discard_linux_exact(&self) -> io::Result<()> {
        #[cfg(test)]
        no_replace_test_fault(NoReplaceTestFault::Cleanup)
            .map_err(|error| io::Error::other(error.to_string()))?;
        // O_TMPFILE has no directory entry. Closing this exact descriptor is
        // its only discard operation.
        Ok(())
    }

    pub(super) fn drop_linux(&mut self) {
        // Field drop closes the unnamed O_TMPFILE descriptor; no pathname is
        // available or consulted on Linux.
    }
}

#[cfg(target_os = "linux")]
pub(super) fn prepare_linux_no_replace(
    path: &Path,
    parent: &Path,
    bytes: &[u8],
) -> Result<PreparedNoReplaceWrite, ServiceError> {
    let destination_leaf = linux_destination_leaf(path)?;
    #[cfg(test)]
    no_replace_test_fault(NoReplaceTestFault::Create)?;
    let parent_directory = rustix::fs::open(
        parent,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .map_err(|error| {
        crate::failed(format!(
            "failed to open publication parent directory `{}`: {error}",
            parent.display()
        ))
    })?;
    let candidate_fd = rustix::fs::openat(
        &parent_directory,
        ".",
        rustix::fs::OFlags::TMPFILE | rustix::fs::OFlags::WRONLY | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .map_err(|error| {
        crate::failed(format!(
            "failed to create unnamed publication candidate in `{}`: {error}",
            parent.display()
        ))
    })?;
    let mut file = File::from(candidate_fd);
    if let Err(write_error) = write_no_replace_temp_bytes(&mut file, path, bytes) {
        let cleanup = linux_prepare_cleanup(&file);
        drop(file);
        return match cleanup {
            Ok(()) => Err(write_error),
            Err(cleanup_error) => Err(crate::failed(format!(
                "failed to close exact unnamed publication candidate after write error `{write_error}`: {cleanup_error}"
            ))),
        };
    }
    if let Err(sync_error) = sync_no_replace_temp_file(&file, path) {
        let cleanup = linux_prepare_cleanup(&file);
        drop(file);
        return match cleanup {
            Ok(()) => Err(sync_error),
            Err(cleanup_error) => Err(crate::failed(format!(
                "failed to close exact unnamed publication candidate after sync error `{sync_error}`: {cleanup_error}"
            ))),
        };
    }
    Ok(PreparedNoReplaceWrite {
        file,
        parent_directory,
        destination_leaf,
    })
}

#[cfg(target_os = "linux")]
fn linux_destination_leaf(path: &Path) -> Result<OsString, ServiceError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    let leaf = path
        .file_name()
        .filter(|leaf| *leaf != "." && *leaf != "..");
    match (parent, leaf) {
        (Some(_), Some(leaf)) if !leaf.is_empty() => Ok(leaf.to_owned()),
        _ => Err(crate::failed(format!(
            "cannot publish `{}` because its destination is not a safe parent-and-leaf path",
            path.display()
        ))),
    }
}

#[cfg(target_os = "linux")]
fn linux_prepare_cleanup(_file: &File) -> io::Result<()> {
    #[cfg(test)]
    no_replace_test_fault(NoReplaceTestFault::Cleanup)
        .map_err(|error| io::Error::other(error.to_string()))?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn link_linux_candidate(
    file: &File,
    parent_directory: &OwnedFd,
    destination_leaf: &OsString,
) -> Result<NoReplaceWrite, rustix::io::Errno> {
    match rustix::fs::linkat(
        file.as_fd(),
        "",
        parent_directory.as_fd(),
        destination_leaf,
        rustix::fs::AtFlags::EMPTY_PATH,
    ) {
        Ok(()) => Ok(NoReplaceWrite::Published),
        Err(error) if error == rustix::io::Errno::EXIST => Ok(NoReplaceWrite::Occupied),
        Err(error) if error == rustix::io::Errno::PERM || error == rustix::io::Errno::ACCESS => {
            link_linux_candidate_via_proc(file, parent_directory, destination_leaf)
        }
        Err(error) => Err(error),
    }
}

#[cfg(target_os = "linux")]
fn link_linux_candidate_via_proc(
    file: &File,
    parent_directory: &OwnedFd,
    destination_leaf: &OsString,
) -> Result<NoReplaceWrite, rustix::io::Errno> {
    let source = PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd()));
    match rustix::fs::linkat(
        rustix::fs::CWD,
        source,
        parent_directory.as_fd(),
        destination_leaf,
        rustix::fs::AtFlags::SYMLINK_FOLLOW,
    ) {
        Ok(()) => Ok(NoReplaceWrite::Published),
        Err(error) if error == rustix::io::Errno::EXIST => Ok(NoReplaceWrite::Occupied),
        Err(error) => Err(error),
    }
}
