//! Content-durable write and copy (temp file + sync + atomic rename).

#[cfg(test)]
use std::cell::Cell;
#[cfg(any(windows, target_os = "linux"))]
use std::ffi::OsString;
#[cfg(not(windows))]
use std::fs;
use std::fs::File;
use std::io::Write;
#[cfg(target_os = "linux")]
use std::os::fd::OwnedFd;
use std::path::Path;
#[cfg(any(
    test,
    windows,
    all(
        not(any(windows, target_os = "linux")),
        feature = "development-host-fallback"
    )
))]
use std::path::PathBuf;

use crate::ServiceError;

#[cfg(all(
    not(any(windows, target_os = "linux")),
    feature = "development-host-fallback"
))]
mod development;
#[cfg(target_os = "linux")]
mod linux;
mod replace;
#[cfg(windows)]
mod windows;

#[cfg(all(
    not(any(windows, target_os = "linux")),
    feature = "development-host-fallback"
))]
use development::prepare_development_no_replace;
#[cfg(target_os = "linux")]
use linux::prepare_linux_no_replace;
pub(crate) use replace::{
    copy_file_atomically, move_file_no_replace, publish_staged_replace, write_file_atomically,
};
#[cfg(windows)]
use windows::prepare_windows_no_replace;

/// Result of attempting one durable publication without replacing an existing
/// destination entry.
pub(super) enum NoReplaceWrite {
    Published,
    Occupied,
}

/// A fully written, flushed candidate that this process alone owns until it is
/// published or discarded. Supported targets keep the candidate's native
/// object open, so neither action reselects a mutable temporary pathname.
pub(super) struct PreparedNoReplaceWrite {
    #[cfg(windows)]
    file: Option<File>,
    #[cfg(windows)]
    parent_directory: Option<File>,
    #[cfg(windows)]
    destination_leaf: OsString,
    #[cfg(windows)]
    destination_leaf_wide: Vec<u16>,
    #[cfg(windows)]
    temp_path: Option<PathBuf>,
    #[cfg(windows)]
    destination: PathBuf,
    #[cfg(target_os = "linux")]
    file: File,
    #[cfg(target_os = "linux")]
    parent_directory: OwnedFd,
    #[cfg(target_os = "linux")]
    destination_leaf: OsString,
    #[cfg(all(
        not(any(windows, target_os = "linux")),
        feature = "development-host-fallback"
    ))]
    temp_path: Option<PathBuf>,
    #[cfg(all(
        not(any(windows, target_os = "linux")),
        feature = "development-host-fallback"
    ))]
    destination: PathBuf,
}

impl PreparedNoReplaceWrite {
    /// Atomically publishes the owned temporary file only if the destination
    /// remains unoccupied. A publication race never replaces its winner.
    pub(super) fn publish(mut self) -> Result<NoReplaceWrite, ServiceError> {
        #[cfg(windows)]
        return self.publish_windows();

        #[cfg(target_os = "linux")]
        return self.publish_linux();

        #[cfg(all(
            not(any(windows, target_os = "linux")),
            feature = "development-host-fallback"
        ))]
        return self.publish_development_fallback();
    }

    /// Discards exactly the prepared candidate. A failure stays diagnostic so
    /// callers never silently leave an owned candidate behind.
    #[cfg(any(windows, test))]
    pub(super) fn discard(self) -> Result<(), ServiceError> {
        #[cfg(windows)]
        return self.discard_windows();

        #[cfg(target_os = "linux")]
        return self.discard_linux();

        #[cfg(all(
            not(any(windows, target_os = "linux")),
            feature = "development-host-fallback"
        ))]
        return self.discard_development_fallback();
    }
}

impl Drop for PreparedNoReplaceWrite {
    fn drop(&mut self) {
        #[cfg(windows)]
        self.drop_windows();

        #[cfg(target_os = "linux")]
        self.drop_linux();

        #[cfg(all(
            not(any(windows, target_os = "linux")),
            feature = "development-host-fallback"
        ))]
        self.drop_development_fallback();
    }
}

/// Writes and flushes an exact no-replace candidate without publishing it.
pub(super) fn prepare_file_atomically_no_replace(
    path: &Path,
    bytes: &[u8],
) -> Result<PreparedNoReplaceWrite, ServiceError> {
    #[cfg(windows)]
    {
        prepare_windows_no_replace(path, bytes)
    }

    #[cfg(not(windows))]
    {
        let parent = path.parent().ok_or_else(|| {
            crate::failed(format!(
                "cannot write file `{}` because it has no parent directory",
                path.display()
            ))
        })?;
        fs::create_dir_all(parent).map_err(|error| {
            crate::failed(format!(
                "failed to create directory `{}`: {error}",
                parent.display()
            ))
        })?;

        #[cfg(target_os = "linux")]
        return prepare_linux_no_replace(path, parent, bytes);

        #[cfg(all(
            not(any(windows, target_os = "linux")),
            feature = "development-host-fallback"
        ))]
        return prepare_development_no_replace(path, bytes);
    }
}

/// Writes and flushes a same-directory temporary file, then atomically
/// publishes it only if `path` is still unoccupied. The temporary file is
/// always owned by this call and is the only pathname this primitive removes.
pub(super) fn write_file_atomically_no_replace(
    path: &Path,
    bytes: &[u8],
) -> Result<NoReplaceWrite, ServiceError> {
    prepare_file_atomically_no_replace(path, bytes)?.publish()
}

fn write_no_replace_temp_bytes(
    temp_file: &mut File,
    temp_path: &Path,
    bytes: &[u8],
) -> Result<(), ServiceError> {
    #[cfg(test)]
    no_replace_test_fault(NoReplaceTestFault::Write)?;
    temp_file.write_all(bytes).map_err(|error| {
        crate::failed(format!(
            "failed to write publication temporary file `{}`: {error}",
            temp_path.display()
        ))
    })
}

fn sync_no_replace_temp_file(temp_file: &File, temp_path: &Path) -> Result<(), ServiceError> {
    #[cfg(test)]
    no_replace_test_fault(NoReplaceTestFault::Sync)?;
    temp_file.sync_all().map_err(|error| {
        crate::failed(format!(
            "failed to flush publication temporary file `{}`: {error}",
            temp_path.display()
        ))
    })
}

/// Test-only faults exercise every owned diagnostic-publication operation
/// without depending on filesystem permissions or timing races.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NoReplaceTestFault {
    Create,
    Write,
    Sync,
    Publish,
    #[cfg(all(
        not(any(windows, target_os = "linux")),
        feature = "development-host-fallback"
    ))]
    Inspect,
    Cleanup,
}

#[cfg(test)]
thread_local! {
    static NO_REPLACE_TEST_FAULT: Cell<Option<NoReplaceTestFault>> = const { Cell::new(None) };
}

#[cfg(test)]
pub(super) struct NoReplaceTestFaultGuard(Option<NoReplaceTestFault>);

#[cfg(test)]
pub(super) fn inject_no_replace_test_fault(fault: NoReplaceTestFault) -> NoReplaceTestFaultGuard {
    let previous = NO_REPLACE_TEST_FAULT.with(|current| {
        let previous = current.get();
        current.set(Some(fault));
        previous
    });
    NoReplaceTestFaultGuard(previous)
}

#[cfg(test)]
impl Drop for NoReplaceTestFaultGuard {
    fn drop(&mut self) {
        NO_REPLACE_TEST_FAULT.with(|current| current.set(self.0));
    }
}

#[cfg(test)]
fn no_replace_test_fault(fault: NoReplaceTestFault) -> Result<(), ServiceError> {
    if NO_REPLACE_TEST_FAULT.with(|current| current.get()) == Some(fault) {
        Err(crate::failed(format!(
            "injected no-replace publication fault at {fault:?}"
        )))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
