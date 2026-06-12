//! Representative file selection for FSR components.
//!
//! Provides the user-facing display file (the entry point when present), the
//! version-representative file (the upscaler when the set is cohesive, the
//! entry point otherwise), and the canonical representative-first ordering of
//! a component's stored files. Selection and ordering share one sort key, so
//! they cannot disagree.

use crate::ComponentFile;

use super::{lineage, naming};

/// Returns the file that should represent an FSR component to users.
///
/// For cohesive FSR, the entry point is the user-facing path even
/// though another member (usually the upscaler) may be the version
/// representative. Native FSR 4 has no entry point, so it falls back to the
/// first file.
#[must_use]
pub fn display_component_file(files: &[ComponentFile]) -> Option<&ComponentFile> {
    files
        .iter()
        .find(|file| file.path().file_name().is_some_and(naming::is_entry_point))
        .or_else(|| files.first())
}

/// Rank of one FSR file for representative-first ordering — lower is more
/// representative; pair with the file name for a total order.
///
/// The upscaler and the entry point swap places depending on who represents
/// the set (see [`lineage::upscaler_represents_set`]); every other member ranks last.
#[must_use]
pub fn primary_rank(file_name: &str, upscaler_represents: bool) -> u8 {
    let (upscaler_rank, entry_point_rank) = if upscaler_represents { (0, 1) } else { (1, 0) };

    if naming::is_split_marker(file_name) {
        upscaler_rank
    } else if naming::is_entry_point(file_name) {
        entry_point_rank
    } else {
        2
    }
}

/// Returns the file whose version represents the component.
///
/// For FSR sets this is the entry point or the upscaler per
/// [`lineage::upscaler_represents_set`]; sets without any FSR name (other technologies)
/// fall through to the first file.
#[must_use]
pub fn version_representative(files: &[ComponentFile]) -> Option<&ComponentFile> {
    if !is_fsr_named_set(files) {
        return files.first();
    }

    let upscaler_represents = upscaler_represents(files);
    files
        .iter()
        .min_by_key(|file| representative_key(file, upscaler_represents))
}

/// Sorts an FSR component's files representative-first, ties broken by
/// case-insensitive file name for determinism. Sets without any FSR name
/// (other technologies) keep their given order, mirroring
/// [`version_representative`]'s fallback to the first file.
///
/// Detection orders scanned groups this way; callers that rebuild a stored
/// component's file list (e.g. the swap executor) apply the same ordering so
/// the stored `files()[0]` stays the right version source until the next rescan.
pub fn sort_representative_first(files: &mut [ComponentFile]) {
    if !is_fsr_named_set(files) {
        return;
    }

    let upscaler_represents = upscaler_represents(files);
    files.sort_by_cached_key(|file| representative_key(file, upscaler_represents));
}

/// Whether any file carries an FSR name — the gate for FSR-specific
/// representative selection and ordering.
fn is_fsr_named_set(files: &[ComponentFile]) -> bool {
    files.iter().any(|file| {
        file.path()
            .file_name()
            .is_some_and(|name| naming::is_entry_point(name) || naming::is_split_member(name))
    })
}

/// Whether the upscaler represents this file set — the slice-based form of
/// [`lineage::upscaler_represents_set`].
fn upscaler_represents(files: &[ComponentFile]) -> bool {
    lineage::upscaler_represents_set(
        files
            .iter()
            .filter_map(|file| file.path().file_name().map(|name| (name, file.version()))),
    )
}

/// The shared sort key behind [`version_representative`] (min) and
/// [`sort_representative_first`] (ascending sort): [`primary_rank`], then the
/// case-insensitive file name.
fn representative_key(file: &ComponentFile, upscaler_represents: bool) -> (u8, String) {
    let name = file.path().file_name().unwrap_or("");
    (
        primary_rank(name, upscaler_represents),
        name.to_ascii_lowercase(),
    )
}
