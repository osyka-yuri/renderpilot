//! Shared split-directory install orchestration for proxy-host add-on tools.
//!
//! When `ReShade.ini`'s `[ADDON] AddonPath` points away from the game folder,
//! payload files land in `addon_dir` while the proxy host lands beside the
//! executable in `game_dir`. A single crash-safety sentinel is always anchored
//! at `game_dir` so torn-install detection stays consistent.

use std::path::{Path, PathBuf};

use renderpilot_domain::AddonKind;

use crate::ServiceError;
use crate::addons::engine::{self, InstallOptions, InstallPlan, InstallReceipt};

use super::scan::{self, same_path};

/// Resolved install roots for a proxy-host add-on install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallRoots {
    /// Directory beside the game's executable (install target).
    pub game_dir: PathBuf,
    /// ReShade's effective add-on search path from `ReShade.ini`.
    pub addon_dir: PathBuf,
    /// Whether payload and host ops share one directory.
    pub is_unified: bool,
}

impl InstallRoots {
    /// Resolves roots from `ReShade.ini` alone (before a host assessment exists).
    #[must_use]
    pub fn resolve_from_ini(game_dir: &Path) -> Self {
        let paths = scan::resolve_paths(game_dir, None);
        let addon_dir = paths.effective_addon_path;
        let is_unified = same_path(game_dir, &addon_dir);
        Self {
            game_dir: game_dir.to_path_buf(),
            addon_dir,
            is_unified,
        }
    }

    /// Resolves roots from the game folder and the assessed ReShade host path.
    #[must_use]
    pub fn resolve(game_dir: &Path, host_path: &Path) -> Self {
        let paths = scan::resolve_paths(game_dir, Some(host_path));
        let addon_dir = paths.effective_addon_path;
        let is_unified = same_path(game_dir, &addon_dir);
        Self {
            game_dir: game_dir.to_path_buf(),
            addon_dir,
            is_unified,
        }
    }

    /// Directory where the crash-safety sentinel is always written and checked.
    #[must_use]
    pub fn sentinel_dir(&self) -> &Path {
        &self.game_dir
    }

    /// Directories to scan for unmanaged files and torn-install debris.
    pub fn scan_dir_paths(&self) -> Vec<&Path> {
        if self.is_unified {
            vec![self.game_dir.as_path()]
        } else {
            vec![self.game_dir.as_path(), self.addon_dir.as_path()]
        }
    }
}

/// How to roll back a payload-phase receipt when the host-phase install fails.
///
/// Nested multi-file payloads (tree rollback) land with the second tool; RenoDX
/// only needs flat reversal of a single add-on file.
#[derive(Debug, Clone, Copy)]
pub enum PayloadRollback {
    /// Flat file-list reversal (single add-on file tools).
    Flat,
}

/// Runs a unified or split install with one sentinel anchored at [`InstallRoots::sentinel_dir`].
pub fn run_split_install(
    roots: &InstallRoots,
    kind: AddonKind,
    unified_ops: Vec<engine::FileOp>,
    payload_ops: Vec<engine::FileOp>,
    game_ops: Vec<engine::FileOp>,
    payload_rollback: PayloadRollback,
) -> Result<InstallReceipt, ServiceError> {
    let sentinel = engine::sentinel_path(roots.sentinel_dir(), kind);
    engine::write_sentinel(&sentinel)?;
    let no_sentinel = InstallOptions {
        manage_sentinel: false,
    };

    let result = if roots.is_unified {
        let plan = InstallPlan {
            kind,
            ops: unified_ops,
        };
        engine::install_with_options(&roots.game_dir, &plan, no_sentinel)
    } else {
        run_split_phases(
            roots,
            kind,
            payload_ops,
            game_ops,
            payload_rollback,
            no_sentinel,
        )
    };

    match &result {
        Ok(_) => engine::remove_sentinel(&sentinel),
        Err(_) => {
            // A partial rollback leaves the sentinel for torn-install detection.
            if sentinel.exists() {
                log::debug!(
                    "split install failed; leaving sentinel `{}` for torn-install detection",
                    sentinel.display()
                );
            }
        }
    }

    result
}

fn run_split_phases(
    roots: &InstallRoots,
    kind: AddonKind,
    payload_ops: Vec<engine::FileOp>,
    game_ops: Vec<engine::FileOp>,
    payload_rollback: PayloadRollback,
    options: InstallOptions,
) -> Result<InstallReceipt, ServiceError> {
    if payload_ops.is_empty() && game_ops.is_empty() {
        return Ok(InstallReceipt::default());
    }

    let payload_receipt = if payload_ops.is_empty() {
        InstallReceipt::default()
    } else {
        let plan = InstallPlan {
            kind,
            ops: payload_ops,
        };
        engine::install_with_options(&roots.addon_dir, &plan, options)?
    };

    if game_ops.is_empty() {
        return Ok(payload_receipt);
    }

    let host_plan = InstallPlan {
        kind,
        ops: game_ops,
    };
    match engine::install_with_options(&roots.game_dir, &host_plan, options) {
        Ok(host_receipt) => Ok(merge_receipts(payload_receipt, host_receipt)),
        Err(error) => {
            rollback_payload(&payload_receipt, &roots.addon_dir, payload_rollback);
            Err(error)
        }
    }
}

fn rollback_payload(receipt: &InstallReceipt, _addon_dir: &Path, mode: PayloadRollback) {
    let rollback_result = match mode {
        PayloadRollback::Flat => {
            engine::uninstall(&receipt.created_files, &receipt.backed_up_files)
        }
    };
    if let Err(revert_error) = rollback_result {
        log::warn!(
            "split install: host-phase failed and payload rollback also failed: {revert_error}"
        );
    }
}

fn merge_receipts(mut left: InstallReceipt, right: InstallReceipt) -> InstallReceipt {
    left.created_files.extend(right.created_files);
    left.backed_up_files.extend(right.backed_up_files);
    left
}
