//! Change tracking for install plans (actions taken so rollback can undo them).
//! Separated for clarity and to keep the main engine module smaller.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use super::helpers;

/// Tracks the filesystem actions an install takes, as one ordered log so rollback
/// is the log replayed in reverse.
#[derive(Default)]
pub(crate) struct InstallChanges {
    pub(crate) actions: Vec<Action>,
}

/// One reversible filesystem action.
pub(crate) enum Action {
    /// A file written where none existed (removed on rollback).
    Created(PathBuf),
    /// A pre-existing file moved to `bak`, then overwritten at `path`.
    Replaced { path: PathBuf, bak: PathBuf },
    /// A file moved to `bak`, then the original deleted.
    Removed { path: PathBuf, bak: PathBuf },
    /// A file updated in-place with no on-disk `.bak`.
    Updated {
        path: PathBuf,
        original_bytes: Option<Vec<u8>>,
        whole_file_owned: bool,
    },
    /// A directory created for a nested file.
    CreatedDir(PathBuf),
}

/// The outcome of a rollback.
pub(crate) struct UndoOutcome {
    pub(crate) failures: usize,
}

impl UndoOutcome {
    pub(crate) fn is_complete(&self) -> bool {
        self.failures == 0
    }
}

impl InstallChanges {
    pub(crate) fn undo(&self) -> UndoOutcome {
        let mut failures = 0;
        for action in self.actions.iter().rev() {
            match action {
                Action::Created(path) => {
                    if let Err(e) = fs::remove_file(path)
                        && e.kind() != std::io::ErrorKind::NotFound
                    {
                        log::warn!("rollback created {}: {}", path.display(), e);
                        failures += 1;
                    }
                }
                Action::Replaced { path, bak } => {
                    let _ = fs::remove_file(path);
                    if let Err(e) = fs::rename(bak, path) {
                        log::warn!("rollback restore {}: {}", path.display(), e);
                        failures += 1;
                    }
                }
                Action::Removed { path, bak } => {
                    if let Err(e) = fs::rename(bak, path) {
                        log::warn!("rollback restore removed {}: {}", path.display(), e);
                        failures += 1;
                    }
                }
                Action::Updated {
                    path,
                    original_bytes,
                    ..
                } => {
                    if let Some(bytes) = original_bytes {
                        if let Err(e) = crate::fs::write_file_atomically(path, bytes) {
                            log::warn!("rollback update {}: {}", path.display(), e);
                            failures += 1;
                        }
                    } else {
                        let _ = fs::remove_file(path);
                    }
                }
                Action::CreatedDir(dir) => {
                    let _ = helpers::remove_dir_if_empty(dir);
                }
            }
            // failures counted above
        }
        UndoOutcome { failures }
    }

    pub(crate) fn sync_touched_dirs(&self) {
        let mut touched: HashSet<PathBuf> = HashSet::new();
        for action in &self.actions {
            match action {
                Action::Created(p)
                | Action::Replaced { path: p, .. }
                | Action::Removed { path: p, .. }
                | Action::Updated { path: p, .. } => {
                    if let Some(parent) = p.parent() {
                        touched.insert(parent.to_path_buf());
                    }
                }
                Action::CreatedDir(d) => {
                    touched.insert(d.clone());
                }
            }
        }
        for dir in touched {
            crate::fs::sync_directory_best_effort(&dir);
        }
    }

    pub(crate) fn cleanup_remove_backups(&self) {
        for action in &self.actions {
            if let Action::Removed { bak, .. } = action {
                let _ = fs::remove_file(bak);
            }
        }
    }

    pub(crate) fn into_receipt(self) -> super::InstallReceipt {
        let mut created = Vec::new();
        let mut backed = Vec::new();
        for action in self.actions {
            match action {
                Action::Created(path) => created.push(path),
                Action::Replaced { path, .. } => {
                    // path is needed in both lists — single intentional clone
                    backed.push(path.clone());
                    created.push(path);
                }
                Action::Updated {
                    path,
                    whole_file_owned,
                    original_bytes,
                } if whole_file_owned || original_bytes.is_none() => {
                    created.push(path);
                }
                // Removed files don't appear in the receipt; the caller knows
                // which file it asked to remove and updates its record directly.
                Action::Removed { .. } | Action::Updated { .. } | Action::CreatedDir(_) => {}
            }
        }
        super::InstallReceipt {
            created_files: created,
            backed_up_files: backed,
        }
    }
}
