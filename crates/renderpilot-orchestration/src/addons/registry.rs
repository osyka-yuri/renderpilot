//! Thin facade over the central [`super::tool`] registration table.
//!
//! Cross-kind facts (exclusive peers, unmanaged signatures) live on each
//! tool's [`super::tool::AddonTool`] impl. This module keeps the historical
//! function names so exclusivity call sites stay stable. Filename predicates
//! live on each tool's `tool` module; catalog capability policy lives on
//! [`super::capabilities`].

use std::path::Path;

use renderpilot_domain::AddonKind;

use super::tool::tool;

/// Returns the other add-on kind(s) that are mutually exclusive with `kind`.
#[must_use]
pub(crate) fn exclusive_peers(kind: AddonKind) -> &'static [AddonKind] {
    tool(kind).map(|t| t.exclusive_peers()).unwrap_or(&[])
}

/// Returns true if the on-disk signature of `kind` is present in `game_dir`
/// (used as a backstop when there is no DB record).
pub(crate) fn unmanaged_files_present(game_dir: &Path, kind: AddonKind) -> bool {
    tool(kind).is_some_and(|t| t.unmanaged_present(game_dir))
}

/// Like [`unmanaged_files_present`], but scans every directory in `dirs`.
pub(crate) fn unmanaged_files_present_in_dirs(dirs: &[&Path], kind: AddonKind) -> bool {
    dirs.iter().any(|dir| unmanaged_files_present(dir, kind))
}
