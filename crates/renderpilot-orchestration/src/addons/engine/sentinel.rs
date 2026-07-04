//! Crash-safety sentinel handling for the install engine.
//!
//! A sentinel file (`renderpilot-{kind}-install.lock`) is written before any
//! mutations and removed on clean completion. Its presence after an operation
//! indicates a torn install that should be detected on next scan.

use std::path::{Path, PathBuf};

use renderpilot_domain::AddonKind;

use crate::ServiceError;

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
