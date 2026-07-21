//! Mutual-exclusion policy between addon tools that install into the same game
//! folder (RenoDX and Luma today — both a ReShade host plus one tool's own add-on
//! file). Exactly one of them may be installed for a given game at a time; this
//! module is the single place either tool asks "is the *other* one already here?"
//!
//! Two signals, checked in order:
//! 1. **Foreign DB record** (`records::foreign_record`) — authoritative for
//!    cross-tool ownership. Even when a primary payload was removed manually,
//!    the row may still own host/config files and SQLite intentionally refuses
//!    to overwrite it with another add-on kind.
//! 2. **On-disk unmanaged presence** (`unmanaged_files_present`) — checked only
//!    when there is no active record for *either* kind, so a hand-dropped install
//!    (or a record lost to a wiped database) still blocks the other tool rather
//!    than letting it install on top and corrupt the folder.
//!
//! Callers hold the per-game `game_mutation_lock` across the check and the
//! subsequent write, so a concurrent install of the other tool can't race between
//! the check and the write.

use std::path::Path;

use renderpilot_domain::{AddonKind, GameId};

use crate::addons::errors;
use crate::{Context, ServiceError};

use super::records;
use super::tool;

/// Why [`check_blocked`] found the other tool already present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExclusivityBlockKind {
    /// An `installed_addons` ownership record for the other kind exists.
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
    // Unmanaged peer scan only when neither kind is active (see module docs).
    // An active requesting record means this tool already owns the install;
    // leftover peer-shaped files must not contradict `state: installed`.
    if records::active_record_of_kind(context, game_id, requesting)?.is_some() {
        return Ok(None);
    }
    let Some(dirs) = scan_dirs else {
        return Ok(None);
    };
    // Use `tool::TOOLS` / exclusive_peers so adding more mutually-exclusive
    // addons only requires updating the registration table.
    for &other in tool::exclusive_peers(requesting) {
        if tool::unmanaged_files_present_in_dirs(dirs, other) {
            return Ok(Some(ExclusivityBlock {
                other,
                kind: ExclusivityBlockKind::UnmanagedFiles,
            }));
        }
    }
    Ok(None)
}

/// Shared check that refuses if blocked by another kind.
///
/// Error copy comes from the requesting tool's
/// [`tool::AddonTool::exclusive_block_message`], keyed by whether the block was
/// a managed record or unmanaged debris.
pub(crate) fn ensure_not_blocked(
    context: &Context,
    game_id: &GameId,
    requesting: AddonKind,
    scan_dirs: Option<&[&Path]>,
) -> Result<(), ServiceError> {
    let Some(block) = check_blocked(context, game_id, requesting, scan_dirs)? else {
        return Ok(());
    };
    let unmanaged = block.kind == ExclusivityBlockKind::UnmanagedFiles;
    let message = tool::require_tool(requesting).exclusive_block_message(unmanaged);
    Err(errors::invalid(message.to_owned()))
}

/// Ensures no unmanaged files for the kind are present.
pub(crate) fn ensure_not_unmanaged(
    scan_dirs: &[&Path],
    kind: AddonKind,
    message: impl Into<String>,
) -> Result<(), ServiceError> {
    if tool::unmanaged_files_present_in_dirs(scan_dirs, kind) {
        return Err(errors::invalid(message.into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use renderpilot_application::InstalledAddonRepository;
    use renderpilot_domain::{InstalledAddon, PathRef};
    use tempfile::tempdir;

    use super::*;
    use crate::Context;

    fn seed_record(context: &Context, kind: AddonKind, path: &str) {
        let record = InstalledAddon::new(
            GameId::new("steam:1").expect("game id"),
            kind,
            PathRef::new(path).expect("path"),
        );
        context
            .storage()
            .upsert_installed_addon(&record)
            .expect("seed record");
    }

    #[test]
    fn check_blocked_skips_unmanaged_peers_when_requesting_kind_has_a_record() {
        let db_dir = tempdir().expect("tempdir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:1").expect("game id");
        seed_record(&context, AddonKind::Luma, r"C:\games\x\Luma-Game.addon");

        let dir = tempdir().expect("game dir");
        std::fs::write(dir.path().join("renodx-cp2077.addon64"), b"x").expect("write peer debris");
        let dirs = [dir.path()];

        assert!(
            check_blocked(&context, &game_id, AddonKind::Luma, Some(&dirs))
                .expect("query")
                .is_none(),
            "own record must suppress unmanaged peer blocks"
        );
    }

    #[test]
    fn check_blocked_reports_unmanaged_peers_when_neither_kind_has_a_record() {
        let db_dir = tempdir().expect("tempdir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:1").expect("game id");

        let dir = tempdir().expect("game dir");
        std::fs::write(dir.path().join("renodx-cp2077.addon64"), b"x").expect("write peer debris");
        let dirs = [dir.path()];

        let block = check_blocked(&context, &game_id, AddonKind::Luma, Some(&dirs))
            .expect("query")
            .expect("must block");
        assert_eq!(block.other, AddonKind::RenoDx);
        assert_eq!(block.kind, ExclusivityBlockKind::UnmanagedFiles);
    }

    #[test]
    fn check_blocked_prefers_foreign_record_over_unmanaged_debris() {
        let db_dir = tempdir().expect("tempdir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:1").expect("game id");
        let managed_dir = tempdir().expect("managed dir");
        let managed_addon = managed_dir.path().join("renodx-game.addon64");
        std::fs::write(&managed_addon, b"managed").expect("write managed payload");
        seed_record(
            &context,
            AddonKind::RenoDx,
            managed_addon.to_string_lossy().as_ref(),
        );

        let dir = tempdir().expect("game dir");
        std::fs::write(dir.path().join("Luma-Game.addon"), b"x").expect("write luma debris");
        let dirs = [dir.path()];

        let block = check_blocked(&context, &game_id, AddonKind::Luma, Some(&dirs))
            .expect("query")
            .expect("must block");
        assert_eq!(block.other, AddonKind::RenoDx);
        assert_eq!(block.kind, ExclusivityBlockKind::Record);
    }

    #[test]
    fn stale_foreign_record_still_blocks_an_unsafe_cross_kind_overwrite() {
        let db_dir = tempdir().expect("db dir");
        let game_dir = tempdir().expect("game dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:1").expect("game id");
        let removed_addon = game_dir.path().join("renodx-game.addon64");
        seed_record(
            &context,
            AddonKind::RenoDx,
            removed_addon.to_string_lossy().as_ref(),
        );
        let dirs = [game_dir.path()];

        let block = check_blocked(&context, &game_id, AddonKind::Luma, Some(&dirs))
            .expect("query")
            .expect("stored ownership must block");
        assert_eq!(block.other, AddonKind::RenoDx);
        assert_eq!(block.kind, ExclusivityBlockKind::Record);
    }

    #[test]
    fn stale_own_record_does_not_suppress_the_unmanaged_peer_scan() {
        let db_dir = tempdir().expect("db dir");
        let game_dir = tempdir().expect("game dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:1").expect("game id");
        let removed_addon = game_dir.path().join("renodx-game.addon64");
        seed_record(
            &context,
            AddonKind::RenoDx,
            removed_addon.to_string_lossy().as_ref(),
        );
        let unmanaged_peer = game_dir.path().join("Luma-Game.addon64");
        std::fs::write(&unmanaged_peer, b"luma").expect("write unmanaged peer");
        let dirs = [game_dir.path()];

        let block = check_blocked(&context, &game_id, AddonKind::RenoDx, Some(&dirs))
            .expect("query")
            .expect("stale ownership must not hide an unmanaged peer");
        assert_eq!(block.other, AddonKind::Luma);
        assert_eq!(block.kind, ExclusivityBlockKind::UnmanagedFiles);

        std::fs::remove_file(unmanaged_peer).expect("remove unmanaged peer");
        assert!(
            check_blocked(&context, &game_id, AddonKind::RenoDx, Some(&dirs))
                .expect("query")
                .is_none(),
            "without peer files, a stale own record must allow reinstall"
        );
    }

    #[test]
    fn unmanaged_renodx_detects_addon_file_case_insensitively() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(dir.path().join("RenoDX-cp2077.Addon64"), b"x").expect("write");
        assert!(tool::unmanaged_files_present(dir.path(), AddonKind::RenoDx));
    }

    #[test]
    fn unmanaged_renodx_ignores_unrelated_files() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(dir.path().join("game.exe"), b"x").expect("write");
        std::fs::write(dir.path().join("dxgi.dll"), b"x").expect("write");
        assert!(!tool::unmanaged_files_present(
            dir.path(),
            AddonKind::RenoDx
        ));
    }

    #[test]
    fn unmanaged_luma_detects_addon_file() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(dir.path().join("Luma-Dishonored_2.addon"), b"x").expect("write");
        assert!(tool::unmanaged_files_present(dir.path(), AddonKind::Luma));
    }

    #[test]
    fn unmanaged_luma_detects_a_framework_shaped_luma_directory_without_an_addon_file() {
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join("Luma")).expect("mkdir");
        std::fs::write(dir.path().join("Luma").join("Global.hlsl"), b"x").expect("write");
        assert!(tool::unmanaged_files_present(dir.path(), AddonKind::Luma));
    }

    #[test]
    fn unmanaged_luma_ignores_an_empty_luma_directory() {
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join("Luma")).expect("mkdir");
        assert!(!tool::unmanaged_files_present(dir.path(), AddonKind::Luma));
    }

    #[test]
    fn unmanaged_luma_ignores_a_luma_directory_with_only_unrelated_junk() {
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join("Luma")).expect("mkdir");
        std::fs::write(dir.path().join("Luma").join("readme.txt"), b"notes").expect("write");
        assert!(!tool::unmanaged_files_present(dir.path(), AddonKind::Luma));
    }

    #[test]
    fn unmanaged_luma_ignores_addon_bak_siblings() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(dir.path().join("Luma-Game.addon.bak"), b"x").expect("write");
        assert!(!tool::unmanaged_files_present(dir.path(), AddonKind::Luma));
    }

    #[test]
    fn unmanaged_luma_ignores_renodx_files_and_vice_versa() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(dir.path().join("renodx-cp2077.addon64"), b"x").expect("write");
        assert!(!tool::unmanaged_files_present(dir.path(), AddonKind::Luma));

        let dir2 = tempdir().expect("tempdir");
        std::fs::write(dir2.path().join("Luma-Game.addon"), b"x").expect("write");
        assert!(!tool::unmanaged_files_present(
            dir2.path(),
            AddonKind::RenoDx
        ));
    }

    #[test]
    fn exclusive_peers_are_the_pairwise_opposite() {
        assert_eq!(tool::exclusive_peers(AddonKind::RenoDx), &[AddonKind::Luma]);
        assert_eq!(tool::exclusive_peers(AddonKind::Luma), &[AddonKind::RenoDx]);
    }
}
