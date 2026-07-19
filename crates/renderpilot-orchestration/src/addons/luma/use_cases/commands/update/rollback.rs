//! Rollback/undo machinery for the full set-diff update path: every disk
//! mutation [`super::apply::apply_set_diff_with_mutation`] makes is captured
//! here as it happens, so any later step's failure can undo everything applied
//! so far in one call.

use std::path::{Path, PathBuf};

use crate::ServiceError;
use crate::addons::engine;
use crate::addons::file_update::{OriginalFile, restore_originals};
use crate::addons::luma::errors;

/// Every disk mutation [`super::apply::apply_set_diff_with_mutation`] makes, in
/// the shape needed to undo each group. The three groups touch disjoint paths
/// by construction (a path is exactly one of added/changed/removed), so undoing
/// them in any order is safe — only the order *within* a group matters, and
/// each group's own undo already handles that.
#[derive(Default)]
pub(super) struct SetDiffRollback {
    /// In-place replacements: changed payload files, plus a host rewritten in
    /// its existing slot.
    pub(super) replaced: Vec<OriginalFile>,
    /// Files the engine created for additions. Reverted via `engine::uninstall`
    /// (A.2), which also restores anything an addition shadowed on disk —
    /// unlike a plain delete, which would orphan that file's `.bak`.
    pub(super) added: engine::InstallReceipt,
    /// Payload files removed by the diff.
    pub(super) removed: Vec<RemovedFileUndo>,
}

/// Update failure plus whether every mutation made after opening the sentinel
/// was restored successfully.
#[derive(Debug)]
pub(super) struct UpdateFailure {
    pub(super) error: ServiceError,
    pub(super) rollback_complete: bool,
}

impl SetDiffRollback {
    /// Same reversal, but collecting (rather than logging) each group's
    /// `Result` — for the final, post-persistence-attempt failure, where the
    /// caller needs to know whether disk state might not match what was
    /// recorded (see [`crate::addons::file_update::persistence_failure_error`]).
    pub(super) fn undo_all_collecting_errors(&self) -> Vec<Result<(), ServiceError>> {
        vec![
            undo_removed_files(&self.removed),
            engine::uninstall(&self.added.created_files, &self.added.backed_up_files),
            restore_originals(&self.replaced),
        ]
    }

    /// Rolls back every captured group and combines an incomplete rollback with
    /// the primary update error. `prior_rollback_complete` carries the outcome
    /// of a helper that already reverted its own partial writes.
    pub(super) fn fail(
        &self,
        primary: ServiceError,
        prior_rollback_complete: bool,
    ) -> UpdateFailure {
        let results = self.undo_all_collecting_errors();
        let rollback_errors: Vec<String> = results
            .iter()
            .filter_map(|result| result.as_ref().err())
            .map(ToString::to_string)
            .collect();
        let rollback_complete = prior_rollback_complete && rollback_errors.is_empty();
        let error = if rollback_complete {
            primary
        } else {
            let detail = if rollback_errors.is_empty() {
                "a nested mutation reported an incomplete rollback".to_owned()
            } else {
                rollback_errors.join("; ")
            };
            errors::failed(format!(
                "Luma update failed ({primary}), and its on-disk rollback was incomplete ({detail}); \
                 the game's files may not match its recorded state"
            ))
        };
        UpdateFailure {
            error,
            rollback_complete,
        }
    }
}

#[derive(Debug)]
pub(super) struct RemovedFileUndo {
    pub(super) path: PathBuf,
    pub(super) payload_bytes: Vec<u8>,
    pub(super) restored_original: Option<(PathBuf, Vec<u8>)>,
}

/// Removes a single recorded payload file that the fresh release no longer
/// carries. When this path shadowed a foreign pre-existing file (tracked in
/// `record.backed_up_files()`) and its `.bak` still exists, the foreign
/// original is restored instead of leaving the slot empty; otherwise the file
/// is deleted outright. Returns `None` (nothing to undo) when the path was
/// already missing.
///
/// # Contract for callers
///
/// On `Ok`, disk is consistent with the returned undo record (or unchanged for
/// `None`). On `Err`, either no mutation happened after the initial read, or
/// every mutation was fully compensated so the live path still holds the
/// pre-call payload. Callers may therefore pass `prior_rollback_complete =
/// true` into [`SetDiffRollback::fail`] for remove errors — the path is never
/// left empty without an undo entry.
pub(super) fn remove_payload_file(
    record: &renderpilot_domain::InstalledAddon,
    path: &Path,
) -> Result<Option<RemovedFileUndo>, ServiceError> {
    if !path.is_file() {
        return Ok(None);
    }
    let payload_bytes =
        std::fs::read(path).map_err(|error| errors::io("read for removal", path, &error))?;
    // same_path: Windows casing / long-path form must still recognize ownership.
    let is_backed_up = record
        .backed_up_files()
        .iter()
        .any(|tracked| crate::paths::same_path(Path::new(tracked.as_str()), path));
    let bak_path = crate::fs::backup_path(path).map_err(|error| {
        crate::addons::luma::errors::failed(format!(
            "failed to derive backup path for {}: {error}",
            path.display()
        ))
    })?;

    if is_backed_up {
        if bak_path.is_file() {
            let bak_bytes = std::fs::read(&bak_path)
                .map_err(|error| errors::io("read backup", &bak_path, &error))?;
            // Overwrite live with bak contents atomically so a mid-failure never
            // leaves the slot empty (delete-then-rename was unsafe for sentinels).
            crate::fs::write_file_atomically(path, &bak_bytes)?;
            if let Err(error) = crate::fs::remove_file_if_exists(&bak_path) {
                // Live path already holds the foreign original; bak leftover is
                // recoverable on undo (recreates bak from bytes). Prefer Ok.
                log::warn!(
                    "Luma update: restored `{}` from backup but could not remove `{}`: {error}",
                    path.display(),
                    bak_path.display()
                );
            }
            return Ok(Some(RemovedFileUndo {
                path: path.to_path_buf(),
                payload_bytes,
                restored_original: Some((bak_path, bak_bytes)),
            }));
        }
        if bak_path.exists() {
            // Present but not a regular file (e.g. directory) — refuse so we
            // never delete live after failing to restore the original.
            return Err(errors::failed(format!(
                "Luma update: backup `{}` exists but is not a regular file; \
                 refusing to remove `{}`",
                bak_path.display(),
                path.display()
            )));
        }
        log::warn!(
            "Luma update: backup `{}` is missing; removing `{}` outright",
            bak_path.display(),
            path.display()
        );
    }
    crate::fs::remove_file_if_exists(path)?;
    Ok(Some(RemovedFileUndo {
        path: path.to_path_buf(),
        payload_bytes,
        restored_original: None,
    }))
}

/// Reverts every removal in `undos`, in reverse order, and reports every path
/// that could not be restored.
fn undo_removed_files(undos: &[RemovedFileUndo]) -> Result<(), ServiceError> {
    let mut failures = Vec::new();
    for undo in undos.iter().rev() {
        if let Some((bak_path, bak_bytes)) = &undo.restored_original
            && let Err(error) = crate::fs::write_file_atomically(bak_path, bak_bytes)
        {
            log::warn!(
                "Luma update rollback: failed to restore backup `{}`: {error}",
                bak_path.display()
            );
            failures.push(format!("restore backup `{}`: {error}", bak_path.display()));
        }
        if let Err(error) = crate::fs::write_file_atomically(&undo.path, &undo.payload_bytes) {
            log::warn!(
                "Luma update rollback: failed to restore `{}`: {error}",
                undo.path.display()
            );
            failures.push(format!("restore `{}`: {error}", undo.path.display()));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(errors::failed(format!(
            "failed to restore removed payload files ({})",
            failures.join("; ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::AddonKind;
    use tempfile::tempdir;

    use super::*;
    use crate::addons::engine::{FileOp, InstallPlan};

    #[test]
    fn set_diff_rollback_default_is_a_safe_no_op() {
        // No panics, no filesystem effects, no errors collected.
        let rollback = SetDiffRollback::default();
        assert!(
            rollback
                .undo_all_collecting_errors()
                .iter()
                .all(Result::is_ok)
        );
    }

    #[test]
    fn collected_rollback_reports_a_removed_file_restore_failure() {
        let dir = tempdir().expect("tempdir");
        let directory_instead_of_file = dir.path().join("cannot-replace-directory");
        std::fs::create_dir(&directory_instead_of_file).expect("mkdir");
        let rollback = SetDiffRollback {
            removed: vec![RemovedFileUndo {
                path: directory_instead_of_file,
                payload_bytes: b"payload".to_vec(),
                restored_original: None,
            }],
            ..SetDiffRollback::default()
        };

        let results = rollback.undo_all_collecting_errors();

        assert!(results[0].is_err());
    }

    #[test]
    fn set_diff_rollback_restores_a_shadowed_addition_without_orphaning_its_backup() {
        // A.2: rolling back an addition must go through the engine's own
        // uninstall semantics (restore-from-`.bak`), not a blind delete — a
        // blind delete would leave the shadowed original's `.bak` orphaned.
        let dir = tempdir().expect("tempdir");
        let shadowed = dir.path().join("nvngx_dlss.dll");
        std::fs::write(&shadowed, b"game-own-dlss").expect("write");

        let add_plan = InstallPlan {
            kind: AddonKind::Luma,
            ops: vec![FileOp::CreateNested {
                relative_path: "nvngx_dlss.dll".to_owned(),
                bytes: b"luma-dlss".to_vec(),
            }],
        };
        let receipt = engine::install(dir.path(), &add_plan).expect("install addition");
        assert_eq!(std::fs::read(&shadowed).unwrap(), b"luma-dlss");
        assert!(dir.path().join("nvngx_dlss.dll.bak").is_file());

        let rollback = SetDiffRollback {
            added: receipt,
            ..SetDiffRollback::default()
        };
        assert!(
            rollback
                .undo_all_collecting_errors()
                .iter()
                .all(Result::is_ok)
        );

        assert_eq!(
            std::fs::read(&shadowed).unwrap(),
            b"game-own-dlss",
            "the shadowed original must be restored"
        );
        assert!(
            !dir.path().join("nvngx_dlss.dll.bak").exists(),
            "the backup must be consumed by the restore, not orphaned"
        );
    }

    fn record_with_backup(path: &Path, backed_up: &Path) -> renderpilot_domain::InstalledAddon {
        use renderpilot_domain::{AddonKind, GameId, InstalledAddon, PathRef};
        let path_ref = |p: &Path| PathRef::new(p.to_string_lossy().into_owned()).expect("path");
        InstalledAddon::from_parts(
            GameId::new("steam:403640").expect("id"),
            AddonKind::Luma,
            path_ref(path),
            None,
            vec![path_ref(path), path_ref(backed_up)],
            vec![path_ref(backed_up)],
            Vec::new(),
        )
        .expect("record")
    }

    #[test]
    fn remove_payload_file_restores_shadowed_original_via_atomic_write() {
        let dir = tempdir().expect("tempdir");
        let live = dir.path().join("nvngx_dlss.dll");
        let bak = crate::fs::backup_path(&live).expect("bak path");
        std::fs::write(&live, b"luma-payload").expect("write live");
        std::fs::write(&bak, b"game-original").expect("write bak");
        let record = record_with_backup(&dir.path().join("Luma-Game.addon"), &live);

        let undo = remove_payload_file(&record, &live)
            .expect("remove")
            .expect("undo");

        assert_eq!(std::fs::read(&live).unwrap(), b"game-original");
        assert!(!bak.exists(), "bak should be consumed after restore");
        assert_eq!(undo.payload_bytes, b"luma-payload");
        assert!(undo.restored_original.is_some());

        // Outer rollback can still reverse the removal.
        let rollback = SetDiffRollback {
            removed: vec![undo],
            ..SetDiffRollback::default()
        };
        assert!(
            rollback
                .undo_all_collecting_errors()
                .iter()
                .all(Result::is_ok)
        );
        assert_eq!(std::fs::read(&live).unwrap(), b"luma-payload");
        assert_eq!(std::fs::read(&bak).unwrap(), b"game-original");
    }

    #[test]
    fn remove_payload_file_recognizes_backed_up_paths_via_same_path() {
        // Re-joined PathBuf (same location, different construction) must still
        // count as backed-up so we restore bak instead of deleting live.
        let dir = tempdir().expect("tempdir");
        let live = dir.path().join("nvngx_dlss.dll");
        let bak = crate::fs::backup_path(&live).expect("bak path");
        std::fs::write(&live, b"luma-payload").expect("write live");
        std::fs::write(&bak, b"game-original").expect("write bak");
        // Store path via components that rebuild to the same location.
        let stored = dir.path().join(".").join("nvngx_dlss.dll");
        let record = record_with_backup(&dir.path().join("Luma-Game.addon"), &stored);

        let undo = remove_payload_file(&record, &live)
            .expect("remove")
            .expect("undo");

        assert_eq!(std::fs::read(&live).unwrap(), b"game-original");
        assert!(
            undo.restored_original.is_some(),
            "same_path ownership must restore bak, not delete"
        );
    }

    #[test]
    fn remove_payload_file_leaves_live_intact_when_backup_is_not_a_file() {
        let dir = tempdir().expect("tempdir");
        let live = dir.path().join("nvngx_dlss.dll");
        let bak = crate::fs::backup_path(&live).expect("bak path");
        std::fs::write(&live, b"luma-payload").expect("write live");
        // Bak path is a directory → not a regular file; live must remain payload.
        std::fs::create_dir(&bak).expect("mkdir as bak");
        let record = record_with_backup(&dir.path().join("Luma-Game.addon"), &live);

        let err = remove_payload_file(&record, &live).expect_err("must fail");
        assert!(
            err.to_string().contains("backup") || err.to_string().contains("regular file"),
            "error should mention backup: {err}"
        );
        assert_eq!(
            std::fs::read(&live).unwrap(),
            b"luma-payload",
            "live must not be emptied when bak cannot be restored"
        );
    }
}
