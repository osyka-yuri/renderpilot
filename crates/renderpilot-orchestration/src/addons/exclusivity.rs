//! Mutual-exclusion policy between addon tools that install into the same game
//! folder (RenoDX and Luma today — both a ReShade host plus one tool's own add-on
//! file). Exactly one of them may be installed for a given game at a time; this
//! module is the single place either tool asks "is the *other* one already here?"
//!
//! Two signals, checked in order:
//! 1. **DB record** ([`records::foreign_record`]) — authoritative. A record for
//!    the other kind means that tool is genuinely managing this game.
//! 2. **On-disk unmanaged presence** ([`unmanaged_files_present`]) — checked only
//!    when there is no record for *either* kind, so a hand-dropped install (or a
//!    record lost to a wiped database) still blocks the other tool rather than
//!    letting it install on top and corrupt the folder.
//!
//! Callers hold the per-game [`super::operation_lock`] across the check and the
//! subsequent write, so a concurrent install of the other tool can't race between
//! the check and the write.

use std::path::Path;

use renderpilot_domain::{AddonKind, GameId};

use crate::addons::errors;
use crate::{Context, ServiceError};

use super::records;
use super::registry;

/// Why [`check_blocked`] found the other tool already present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExclusivityBlockKind {
    /// An `installed_addons` record for the other kind exists.
    Record,
    /// No record for either kind, but the other tool's files are on disk.
    UnmanagedFiles,
}

/// The other addon tool is already present for this game.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ExclusivityBlock {
    pub(crate) other: AddonKind,
    pub(crate) kind: ExclusivityBlockKind,
}

/// Checks whether `requesting` is blocked from installing/acting for `game_id` by
/// the other addon tool. `game_dir`, when known, backstops the DB check with an
/// on-disk scan; pass `None` when the install target isn't resolved yet (the
/// authoritative DB check still runs on its own).
pub(crate) fn check_blocked(
    context: &Context,
    game_id: &GameId,
    requesting: AddonKind,
    scan_dirs: Option<&[&Path]>,
) -> Result<Option<ExclusivityBlock>, ServiceError> {
    if let Some(foreign) = records::foreign_record(context, game_id, requesting)? {
        return Ok(Some(ExclusivityBlock {
            other: foreign.kind(),
            kind: ExclusivityBlockKind::Record,
        }));
    }
    let Some(dirs) = scan_dirs else {
        return Ok(None);
    };
    // Use the central registry so adding more mutually-exclusive addons only
    // requires updating one place.
    for &other in registry::exclusive_peers(requesting) {
        if registry::unmanaged_files_present_in_dirs(dirs, other) {
            return Ok(Some(ExclusivityBlock {
                other,
                kind: ExclusivityBlockKind::UnmanagedFiles,
            }));
        }
    }
    Ok(None)
}

/// Shared check that refuses if blocked by another kind. Caller provides the error message.
pub(crate) fn ensure_not_blocked(
    context: &Context,
    game_id: &GameId,
    requesting: AddonKind,
    scan_dirs: Option<&[&Path]>,
    message: impl Into<String>,
) -> Result<(), ServiceError> {
    if check_blocked(context, game_id, requesting, scan_dirs)?.is_some() {
        return Err(errors::invalid(message.into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn unmanaged_renodx_detects_addon_file_case_insensitively() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(dir.path().join("RenoDX-cp2077.Addon64"), b"x").expect("write");
        assert!(registry::unmanaged_files_present(
            dir.path(),
            AddonKind::RenoDx
        ));
    }

    #[test]
    fn unmanaged_renodx_ignores_unrelated_files() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(dir.path().join("game.exe"), b"x").expect("write");
        std::fs::write(dir.path().join("dxgi.dll"), b"x").expect("write");
        assert!(!registry::unmanaged_files_present(
            dir.path(),
            AddonKind::RenoDx
        ));
    }

}
