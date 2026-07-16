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
//! tool-specific: tool layers (RenoDX, Luma, …) build the plan — which files to
//! place, which config keys to merge — and map the returned [`InstallReceipt`]
//! into their own persisted install record.

use std::path::Path;

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

pub(crate) use sentinel::{
    OperationSentinel, PendingInstallCommit, remove_sentinel, sentinel_path, write_sentinel,
};

/// Engine failure plus whether its in-call rollback restored every mutation.
pub(crate) struct InstallFailure {
    pub(crate) error: ServiceError,
    pub(crate) rollback_complete: bool,
}

/// Successful filesystem apply whose sentinel remains open until the caller
/// durably persists the install record.
pub(crate) struct PendingInstall {
    pub(crate) receipt: InstallReceipt,
    pub(crate) commit: PendingInstallCommit,
}

/// Installs `plan` into `game_dir`, returning the receipt needed to reverse it.
///
/// Ops run in list order; any failure rolls every applied op back in reverse order.
/// A crash-safety sentinel guards the window: it is removed on success or after a
/// clean rollback, and left behind if a rollback step itself fails.
pub fn install(game_dir: &Path, plan: &InstallPlan) -> Result<InstallReceipt, ServiceError> {
    install_with_options(game_dir, plan, InstallOptions::default())
}

/// Applies a plan while retaining its sentinel across the following database
/// commit. This is the transaction shape used by every managed add-on install.
pub(crate) fn install_pending(
    game_dir: &Path,
    plan: &InstallPlan,
) -> Result<PendingInstall, ServiceError> {
    let commit = PendingInstallCommit::begin(game_dir, plan.kind)?;
    match install_with_options_outcome(
        game_dir,
        plan,
        InstallOptions {
            manage_sentinel: false,
        },
    ) {
        Ok(receipt) => Ok(PendingInstall { receipt, commit }),
        Err(failure) => {
            if failure.rollback_complete {
                commit.finish_rolled_back();
            } else {
                log::warn!(
                    "addon install rollback was incomplete; leaving sentinel `{}` to flag a torn install",
                    commit.path().display()
                );
            }
            Err(failure.error)
        }
    }
}

/// Like [`install`], but allows an outer orchestrator to own the crash-safety sentinel.
pub fn install_with_options(
    game_dir: &Path,
    plan: &InstallPlan,
    options: InstallOptions,
) -> Result<InstallReceipt, ServiceError> {
    install_with_options_outcome(game_dir, plan, options).map_err(|failure| failure.error)
}

/// Internal outcome-preserving variant used by an outer transaction that owns
/// the sentinel and must distinguish a clean rollback from a torn one.
pub(crate) fn install_with_options_outcome(
    game_dir: &Path,
    plan: &InstallPlan,
    options: InstallOptions,
) -> Result<InstallReceipt, InstallFailure> {
    let sentinel = if options.manage_sentinel {
        Some(
            sentinel::OperationSentinel::begin(game_dir, plan.kind).map_err(|error| {
                InstallFailure {
                    error,
                    rollback_complete: true,
                }
            })?,
        )
    } else {
        None
    };

    let mut changes = InstallChanges::default();
    match apply::apply_ops(game_dir, &plan.ops, &mut changes) {
        Ok(()) => {
            changes.sync_touched_dirs();
            changes.cleanup_remove_backups();
            if let Some(sentinel) = sentinel {
                sentinel.finish_committed();
            }
            Ok(changes.into_receipt())
        }
        Err(error) => {
            let rollback_complete = changes.undo().is_complete();
            if let Some(sentinel) = sentinel {
                if rollback_complete {
                    sentinel.finish_rolled_back();
                } else {
                    log::warn!(
                        "addon install rollback was incomplete; leaving sentinel `{}` to flag a torn install",
                        sentinel.path().display()
                    );
                }
            }
            Err(InstallFailure {
                error,
                rollback_complete,
            })
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
