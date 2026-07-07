//! Forward apply logic for install plans (apply_ops, place_file, etc.).

use std::fs;
use std::path::Path;

use super::errors::{self, invalid};
use super::helpers;
use super::{Action, InstallChanges};
use crate::ServiceError;

pub(crate) fn apply_ops(
    game_dir: &Path,
    ops: &[super::FileOp],
    changes: &mut InstallChanges,
) -> Result<(), ServiceError> {
    for op in ops {
        apply_op(game_dir, op, changes)?;
    }
    Ok(())
}

fn apply_op(
    game_dir: &Path,
    op: &super::FileOp,
    changes: &mut InstallChanges,
) -> Result<(), ServiceError> {
    match op {
        super::FileOp::Create { name, bytes } | super::FileOp::BackupAndReplace { name, bytes } => {
            let path = helpers::safe_join(game_dir, "file name", name)?;
            place_file(&path, bytes, changes)
        }
        super::FileOp::Replace { name, bytes } => {
            let path = helpers::safe_join(game_dir, "file name", name)?;
            replace_file_tracked(&path, bytes, changes)
        }
        super::FileOp::MergeText {
            name,
            default,
            strategy,
        } => {
            helpers::ensure_bare_file_name("merge file name", name)?;
            let path = helpers::existing_case_insensitive(game_dir, name)
                .unwrap_or_else(|| game_dir.join(name));
            let owned = fs::read_to_string(&path).ok();
            let base = owned.as_deref().unwrap_or(default.as_str());
            let merged = strategy.apply(base);
            place_file(&path, merged.as_bytes(), changes)
        }
        super::FileOp::UpdateText {
            name,
            default,
            strategy,
        } => {
            helpers::ensure_bare_file_name("update file name", name)?;
            let path = helpers::existing_case_insensitive(game_dir, name)
                .unwrap_or_else(|| game_dir.join(name));

            let (original_bytes, owned_text) = if path.exists() {
                let bytes = fs::read(&path)
                    .map_err(|error| errors::io("read for update", &path, &error))?;
                let text = String::from_utf8_lossy(&bytes).into_owned();
                (Some(bytes), Some(text))
            } else {
                (None, None)
            };
            let current = owned_text.as_deref().unwrap_or(default.as_str());

            let merged = strategy.apply(current);
            crate::fs::write_file_atomically(&path, merged.as_bytes())?;
            changes.actions.push(Action::Updated {
                path,
                original_bytes,
                whole_file_owned: false,
            });
            Ok(())
        }
        super::FileOp::Remove { name } => {
            let path = helpers::safe_join(game_dir, "file name", name)?;
            remove_file_with_backup(&path, changes)
        }
        super::FileOp::CreateNested {
            relative_path,
            bytes,
        } => {
            let relative = helpers::ensure_safe_relative_path("relative path", relative_path)?;
            let path = game_dir.join(&relative);
            ensure_parent_dirs(game_dir, &path, changes)?;
            place_file(&path, bytes, changes)
        }
    }
}

/// Writes `bytes` to `path`, first moving any pre-existing regular file aside to
/// `.bak`, recording the action for rollback.
pub(crate) fn place_file(
    path: &Path,
    bytes: &[u8],
    changes: &mut InstallChanges,
) -> Result<(), ServiceError> {
    if path.exists() {
        if !path.is_file() {
            return Err(invalid(format!(
                "cannot back up `{}`: not a regular file",
                path.display()
            )));
        }
        let bak = helpers::bak_path(path)?;
        if bak.exists() {
            fs::remove_file(&bak)
                .map_err(|error| errors::io("clear stale backup", &bak, &error))?;
        }
        fs::rename(path, &bak).map_err(|error| errors::io("back up", path, &error))?;
        crate::fs::write_file_atomically(path, bytes)?;
        changes.actions.push(Action::Replaced {
            path: path.to_path_buf(),
            bak,
        });
    } else {
        crate::fs::write_file_atomically(path, bytes)?;
        changes.actions.push(Action::Created(path.to_path_buf()));
    }
    Ok(())
}

/// Writes `bytes` to `path` with no on-disk backup, capturing any pre-write bytes
/// in memory so a same-call rollback ([`InstallChanges::undo`]) can restore them.
/// Unlike [`place_file`], a pre-existing file is never moved aside to `.bak` — for
/// [`FileOp::Replace`], the caller has already decided the artifact's identity is
/// unambiguous enough that nothing here is worth preserving for manual recovery.
pub(crate) fn replace_file_tracked(
    path: &Path,
    bytes: &[u8],
    changes: &mut InstallChanges,
) -> Result<(), ServiceError> {
    let original_bytes = if path.exists() {
        if !path.is_file() {
            return Err(invalid(format!(
                "cannot replace `{}`: not a regular file",
                path.display()
            )));
        }
        Some(fs::read(path).map_err(|error| errors::io("read before replace", path, &error))?)
    } else {
        None
    };
    crate::fs::write_file_atomically(path, bytes)?;
    changes.actions.push(Action::Updated {
        path: path.to_path_buf(),
        original_bytes,
        whole_file_owned: true,
    });
    Ok(())
}

/// Moves `path` to a `.bak` and then deletes the original, recording the action
/// for rollback. A missing file is a no-op. The `.bak` is cleaned up on success by
/// [`InstallChanges::cleanup_remove_backups`]; it exists only so a failed plan can
/// restore the removed file.
pub(crate) fn remove_file_with_backup(
    path: &Path,
    changes: &mut InstallChanges,
) -> Result<(), ServiceError> {
    if !path.exists() {
        return Ok(());
    }
    if !path.is_file() {
        return Err(invalid(format!(
            "cannot remove `{}`: not a regular file",
            path.display()
        )));
    }
    let bak = helpers::bak_path(path)?;
    if bak.exists() {
        fs::remove_file(&bak).map_err(|error| errors::io("clear stale backup", &bak, &error))?;
    }
    fs::rename(path, &bak).map_err(|error| errors::io("back up before remove", path, &error))?;
    changes.actions.push(Action::Removed {
        path: path.to_path_buf(),
        bak,
    });
    Ok(())
}

/// Creates every missing ancestor directory of `target` below (and excluding)
/// `game_dir`, one level at a time, shallowest first, recording each one actually
/// created as an [`Action::CreatedDir`]. A directory that already exists (whether
/// pre-existing or created by an earlier op in the same plan) is left as-is and
/// never recorded — so rollback only ever considers directories this same
/// `install` call actually created.
pub(crate) fn ensure_parent_dirs(
    game_dir: &Path,
    target: &Path,
    changes: &mut InstallChanges,
) -> Result<(), ServiceError> {
    let Some(parent) = target.parent() else {
        return Ok(());
    };
    if parent == game_dir {
        return Ok(());
    }

    let mut missing = Vec::new();
    let mut current = parent;
    while current != game_dir && !current.exists() {
        missing.push(current.to_path_buf());
        let Some(next) = current.parent() else {
            break;
        };
        current = next;
    }

    for dir in missing.into_iter().rev() {
        fs::create_dir(&dir).map_err(|error| errors::io("create directory", &dir, &error))?;
        changes.actions.push(Action::CreatedDir(dir));
    }
    Ok(())
}
