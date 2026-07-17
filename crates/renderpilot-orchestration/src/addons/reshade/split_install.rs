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

use super::scan;
use crate::paths::same_path;

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
#[derive(Debug, Clone, Copy)]
pub enum PayloadRollback {
    /// Flat file-list reversal (RenoDX: single add-on file).
    Flat,
    /// Tree reversal bounded to the add-on directory (Luma: nested payload).
    Tree,
}

/// Successful filesystem install that still holds the crash-safety sentinel.
///
/// The sentinel is cleared only after the caller durably persists the install
/// record ([`engine::PendingInstallCommit::finish_committed`]), or after a complete
/// filesystem revert ([`engine::PendingInstallCommit::finish_rolled_back`]). Dropping the
/// commit without finishing leaves the marker on disk (torn) — intentional so a
/// crash between FS apply and DB upsert remains recoverable.
pub struct SplitInstallSuccess {
    pub receipt: InstallReceipt,
    pub commit: engine::PendingInstallCommit,
}

/// Runs a unified or split install with one sentinel anchored at [`InstallRoots::sentinel_dir`].
///
/// On success the sentinel stays open until the caller finishes
/// [`SplitInstallSuccess::commit`] after durable record persistence.
pub fn run_split_install(
    roots: &InstallRoots,
    kind: AddonKind,
    unified_ops: Vec<engine::FileOp>,
    payload_ops: Vec<engine::FileOp>,
    game_ops: Vec<engine::FileOp>,
    payload_rollback: PayloadRollback,
) -> Result<SplitInstallSuccess, ServiceError> {
    let commit = engine::PendingInstallCommit::begin(roots.sentinel_dir(), kind)?;
    let no_sentinel = InstallOptions {
        manage_sentinel: false,
    };

    let result = if roots.is_unified {
        let plan = InstallPlan {
            kind,
            ops: unified_ops,
        };
        engine::install_with_options_outcome(&roots.game_dir, &plan, no_sentinel)
            .map_err(SplitInstallFailure::from)
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

    match result {
        Ok(receipt) => Ok(SplitInstallSuccess { receipt, commit }),
        Err(failure) => {
            if failure.rollback_complete {
                commit.finish_rolled_back();
            } else {
                log::warn!(
                    "split install rollback was incomplete; leaving sentinel `{}`",
                    commit.path().display()
                );
            }
            Err(failure.error)
        }
    }
}

struct SplitInstallFailure {
    error: ServiceError,
    rollback_complete: bool,
}

impl From<engine::InstallFailure> for SplitInstallFailure {
    fn from(value: engine::InstallFailure) -> Self {
        Self {
            error: value.error,
            rollback_complete: value.rollback_complete,
        }
    }
}

fn run_split_phases(
    roots: &InstallRoots,
    kind: AddonKind,
    payload_ops: Vec<engine::FileOp>,
    game_ops: Vec<engine::FileOp>,
    _payload_rollback: PayloadRollback,
    options: InstallOptions,
) -> Result<InstallReceipt, SplitInstallFailure> {
    // Bind with non-_ name so the value is "used" (moved) and clippy pass-by-value
    // lint is satisfied; the _ prefix on param silences "unused param".
    let payload_rollback = _payload_rollback;

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
        engine::install_with_options_outcome(&roots.addon_dir, &plan, options)
            .map_err(SplitInstallFailure::from)?
    };

    if game_ops.is_empty() {
        return Ok(payload_receipt);
    }

    let host_plan = InstallPlan {
        kind,
        ops: game_ops,
    };
    match engine::install_with_options_outcome(&roots.game_dir, &host_plan, options) {
        Ok(host_receipt) => Ok(merge_receipts(payload_receipt, host_receipt)),
        Err(failure) => {
            let payload_rollback_complete =
                rollback_payload(&payload_receipt, &roots.addon_dir, payload_rollback).is_ok();
            Err(SplitInstallFailure {
                error: failure.error,
                rollback_complete: failure.rollback_complete && payload_rollback_complete,
            })
        }
    }
}

fn rollback_payload(
    receipt: &InstallReceipt,
    addon_dir: &Path,
    _mode: PayloadRollback,
) -> Result<(), ServiceError> {
    let mode = _mode;
    let rollback_result = match mode {
        PayloadRollback::Flat => {
            engine::uninstall(&receipt.created_files, &receipt.backed_up_files)
        }
        PayloadRollback::Tree => {
            engine::uninstall_tree(&receipt.created_files, &receipt.backed_up_files, addon_dir)
        }
    };
    if let Err(revert_error) = &rollback_result {
        log::warn!(
            "split install: host-phase failed and payload rollback also failed: {revert_error}"
        );
    }
    rollback_result
}

fn merge_receipts(mut left: InstallReceipt, right: InstallReceipt) -> InstallReceipt {
    left.created_files.extend(right.created_files);
    left.backed_up_files.extend(right.backed_up_files);
    left
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::addons::engine::FileOp;

    fn split_roots() -> (tempfile::TempDir, tempfile::TempDir, InstallRoots) {
        let game = tempdir().expect("game dir");
        let addon = tempdir().expect("addon dir");
        let roots = InstallRoots {
            game_dir: game.path().to_path_buf(),
            addon_dir: addon.path().to_path_buf(),
            is_unified: false,
        };
        (game, addon, roots)
    }

    fn failing_split(roots: &InstallRoots) -> Result<SplitInstallSuccess, ServiceError> {
        run_split_install(
            roots,
            AddonKind::Luma,
            Vec::new(),
            vec![FileOp::Create {
                name: "Luma-Test.addon".to_owned(),
                bytes: b"payload".to_vec(),
            }],
            vec![FileOp::Create {
                name: "../escape.dll".to_owned(),
                bytes: b"invalid".to_vec(),
            }],
            PayloadRollback::Flat,
        )
    }

    #[test]
    fn clean_split_rollback_clears_a_new_marker() {
        let (_game, _addon, roots) = split_roots();

        assert!(failing_split(&roots).is_err());

        assert!(!engine::is_install_torn(
            roots.sentinel_dir(),
            AddonKind::Luma
        ));
        assert!(!roots.addon_dir.join("Luma-Test.addon").exists());
    }

    #[test]
    fn clean_split_rollback_keeps_a_preexisting_marker() {
        let (_game, _addon, roots) = split_roots();
        engine::write_sentinel(&engine::sentinel_path(
            roots.sentinel_dir(),
            AddonKind::Luma,
        ))
        .expect("seed marker");

        assert!(failing_split(&roots).is_err());

        assert!(engine::is_install_torn(
            roots.sentinel_dir(),
            AddonKind::Luma
        ));
    }

    #[test]
    fn successful_fs_apply_leaves_marker_until_commit() {
        let (game, addon, roots) = split_roots();

        let success = run_split_install(
            &roots,
            AddonKind::Luma,
            Vec::new(),
            vec![FileOp::Create {
                name: "Luma-Test.addon".to_owned(),
                bytes: b"payload".to_vec(),
            }],
            vec![FileOp::Create {
                name: "dxgi.dll".to_owned(),
                bytes: b"host".to_vec(),
            }],
            PayloadRollback::Flat,
        )
        .expect("fs apply");

        assert!(
            engine::is_install_torn(roots.sentinel_dir(), AddonKind::Luma),
            "sentinel must remain until durable DB commit"
        );
        assert!(addon.path().join("Luma-Test.addon").is_file());
        assert!(game.path().join("dxgi.dll").is_file());

        success.commit.finish_committed();
        assert!(!engine::is_install_torn(
            roots.sentinel_dir(),
            AddonKind::Luma
        ));
    }
}
