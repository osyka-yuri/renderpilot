//! Crash-safety sentinel handling for the install engine.
//!
//! A sentinel file (`renderpilot-{kind}-install.lock`) is written before any
//! mutations and removed on clean completion. Its presence after an operation
//! indicates a torn install that should be detected on next scan.

use std::path::{Path, PathBuf};

use renderpilot_domain::AddonKind;

use crate::ServiceError;

/// Ownership-aware guard for a multi-step filesystem transaction.
///
/// A marker that predated this operation is cleared only after a committed,
/// fully verified result. Rolling back the current attempt cannot prove that
/// the older torn state was repaired, so the pre-existing marker is retained.
pub(crate) struct OperationSentinel {
    path: PathBuf,
    preexisting: bool,
}

/// Filesystem install that is waiting for its durable record commit.
///
/// Dropping this guard intentionally retains the sentinel. Callers may clear it
/// only after the database record is persisted or after a complete filesystem
/// rollback restores the pre-install state.
pub(crate) struct PendingInstallCommit {
    sentinel: OperationSentinel,
}

impl std::fmt::Debug for PendingInstallCommit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingInstallCommit")
            .field("path", &self.sentinel.path())
            .finish()
    }
}

impl PendingInstallCommit {
    pub(crate) fn begin(game_dir: &Path, kind: AddonKind) -> Result<Self, ServiceError> {
        Ok(Self {
            sentinel: OperationSentinel::begin(game_dir, kind)?,
        })
    }

    pub(crate) fn finish_committed(self) {
        self.sentinel.finish_committed();
    }

    pub(crate) fn finish_rolled_back(self) {
        self.sentinel.finish_rolled_back();
    }

    #[must_use]
    pub(crate) fn path(&self) -> &Path {
        self.sentinel.path()
    }
}

impl OperationSentinel {
    /// Starts a guarded operation, creating the marker only when absent.
    pub(crate) fn begin(game_dir: &Path, kind: AddonKind) -> Result<Self, ServiceError> {
        let path = sentinel_path(game_dir, kind);
        let preexisting = path.exists();
        if !preexisting {
            write_sentinel(&path)?;
        }
        Ok(Self { path, preexisting })
    }

    /// Completes a transaction whose new state was durably persisted.
    pub(crate) fn finish_committed(self) {
        remove_sentinel(&self.path);
    }

    /// Completes an attempt that returned to its pre-attempt state.
    pub(crate) fn finish_rolled_back(self) {
        if self.preexisting {
            log::debug!(
                "rolled back current add-on operation; retaining pre-existing sentinel `{}`",
                self.path.display()
            );
        } else {
            remove_sentinel(&self.path);
        }
    }

    /// Path used for diagnostics when an incomplete rollback retains the guard.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

/// Returns the path to the sentinel for a given add-on kind.
pub(crate) fn sentinel_path(game_dir: &Path, kind: AddonKind) -> PathBuf {
    game_dir.join(format!(
        "renderpilot-{}-install.lock",
        kind.as_str().to_ascii_lowercase()
    ))
}

/// Atomically writes an empty sentinel file.
pub(crate) fn write_sentinel(path: &Path) -> Result<(), ServiceError> {
    crate::fs::write_file_atomically(path, b"")
}

/// Best-effort removal of the sentinel (logs on failure, never errors the caller).
pub(crate) fn remove_sentinel(path: &Path) {
    if let Err(error) = remove_existing(path) {
        log::warn!(
            "addon install: failed to remove sentinel `{}`: {error}",
            path.display()
        );
    }
}

/// Whether a crash-safety sentinel for `kind` is present in `game_dir`.
#[must_use]
pub fn is_install_torn(game_dir: &Path, kind: AddonKind) -> bool {
    sentinel_path(game_dir, kind).exists()
}

/// Clears the crash-safety sentinel for `kind` in `game_dir`, if present.
///
/// For a caller that has independently verified the folder is no longer torn
/// (e.g. it removed the tool-owned debris a crashed install left behind) —
/// [`is_install_torn`] then correctly reports `false` again. Best-effort: a
/// removal failure is logged, not propagated.
pub fn clear_torn_install_marker(game_dir: &Path, kind: AddonKind) {
    remove_sentinel(&sentinel_path(game_dir, kind));
}

/// Helper used by sentinel removal. Best-effort delete.
fn remove_existing(path: &Path) -> std::io::Result<()> {
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::AddonKind;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn rolled_back_attempt_keeps_a_preexisting_marker() {
        let dir = tempdir().expect("tempdir");
        write_sentinel(&sentinel_path(dir.path(), AddonKind::Luma)).expect("seed marker");

        OperationSentinel::begin(dir.path(), AddonKind::Luma)
            .expect("guard")
            .finish_rolled_back();

        assert!(is_install_torn(dir.path(), AddonKind::Luma));
    }

    #[test]
    fn committed_attempt_clears_a_preexisting_marker() {
        let dir = tempdir().expect("tempdir");
        write_sentinel(&sentinel_path(dir.path(), AddonKind::Luma)).expect("seed marker");

        OperationSentinel::begin(dir.path(), AddonKind::Luma)
            .expect("guard")
            .finish_committed();

        assert!(!is_install_torn(dir.path(), AddonKind::Luma));
    }
}
