//! Filename predicates used by Luma install recovery before tool registration.

/// Matches `Luma-<Game>.addon`/`.addon64`/`.addon32` (case-insensitive input).
#[must_use]
pub(crate) fn is_luma_addon_file_name(lower: &str) -> bool {
    lower.starts_with("luma-")
        && (lower.ends_with(".addon") || lower.ends_with(".addon64") || lower.ends_with(".addon32"))
}

/// Matches a `.bak` sibling left by a torn install or engine backup.
#[must_use]
pub(crate) fn is_luma_addon_backup_file_name(lower: &str) -> bool {
    lower.ends_with(".bak") && is_luma_addon_file_name(lower.strip_suffix(".bak").unwrap_or(lower))
}