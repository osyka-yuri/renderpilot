//! Tool-agnostic install engine.
//!
//! Applies a serializable [`InstallPlan`] of file operations into a game folder,
//! running ops in list order and rolling back in strict reverse order if any step
//! fails, and reverses an install from the file lists it recorded. Each [`FileOp`]
//! declares its own backup policy: [`FileOp::Create`]/[`FileOp::BackupAndReplace`]/
//! [`FileOp::MergeText`] move a pre-existing file aside to `.bak` first (for an
//! artifact worth manually recovering); [`FileOp::Replace`]/[`FileOp::UpdateText`]
//! never do (for an artifact whose identity is never ambiguous — see their own
//! docs). A crash-safety **sentinel** is written before the first mutation and
//! removed once the folder is in a consistent state (a clean install or a fully
//! reverted rollback); a rollback that cannot complete leaves the sentinel behind
//! so a torn install is detectable on the next scan instead of silently
//! half-applied.
//!
//! The engine is pure over the filesystem (tempdir-testable) and knows nothing
//! tool-specific: a tool layer (RenoDX today, OptiScaler tomorrow) builds the plan —
//! which files to place, which config keys to merge — and maps the returned
//! [`InstallReceipt`] into its own persisted install record.

use std::path::Path;

use super::canonicalize_best_effort;
use super::errors;
use crate::ServiceError;

mod apply;
mod changes;
mod helpers;
mod rollback;
mod sentinel;
mod types;

pub(crate) use changes::{Action, InstallChanges};
pub use rollback::{cleanup_empty_dirs_best_effort, uninstall, uninstall_tree};
pub use sentinel::{clear_torn_install_marker, is_install_torn};
pub use types::{
    FileOp, IniSection, IniSectionRemoval, InstallOptions, InstallPlan, InstallReceipt,
    MergeStrategy,
};

pub(crate) use sentinel::{remove_sentinel, sentinel_path, write_sentinel};

/// Installs `plan` into `game_dir`, returning the receipt needed to reverse it.
///
/// Ops run in list order; any failure rolls every applied op back in reverse order.
/// A crash-safety sentinel guards the window: it is removed on success or after a
/// clean rollback, and left behind if a rollback step itself fails.
pub fn install(game_dir: &Path, plan: &InstallPlan) -> Result<InstallReceipt, ServiceError> {
    install_with_options(game_dir, plan, InstallOptions::default())
}

/// Like [`install`], but allows an outer orchestrator to own the crash-safety sentinel.
pub fn install_with_options(
    game_dir: &Path,
    plan: &InstallPlan,
    options: InstallOptions,
) -> Result<InstallReceipt, ServiceError> {
    let sentinel = options
        .manage_sentinel
        .then(|| sentinel::sentinel_path(game_dir, plan.kind));
    if let Some(ref path) = sentinel {
        sentinel::write_sentinel(path)?;
    }

    let mut changes = InstallChanges::default();
    match apply::apply_ops(game_dir, &plan.ops, &mut changes) {
        Ok(()) => {
            changes.sync_touched_dirs();
            changes.cleanup_remove_backups();
            if let Some(ref path) = sentinel {
                sentinel::remove_sentinel(path);
            }
            Ok(changes.into_receipt())
        }
        Err(error) => {
            let rollback_complete = changes.undo().is_complete();
            if let Some(ref path) = sentinel {
                if rollback_complete {
                    sentinel::remove_sentinel(path);
                } else {
                    log::warn!(
                        "addon install rollback was incomplete; leaving sentinel `{}` to flag a torn install",
                        path.display()
                    );
                }
            }
            Err(error)
        }
    }
}

/// Atomically replaces an installed file in place with new bytes (for an update),
/// fsyncing its directory. Every other tracked file is left untouched.
pub fn replace_file(path: &Path, bytes: &[u8]) -> Result<(), ServiceError> {
    crate::fs::write_file_atomically(path, bytes)?;
    if let Some(parent) = path.parent() {
        crate::fs::sync_directory_best_effort(parent);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
