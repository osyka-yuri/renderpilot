//! Representative file selection for graphics components.
//!
//! Provides the user-facing display file (the entry point when present), the
//! version-representative file (the upscaler when the set is cohesive, the
//! entry point otherwise), and the canonical representative-first ordering of
//! a component's stored files. Selection and ordering share one sort key, so
//! they cannot disagree.
//!
//! FSR-named sets use a cohesion-aware key; every other technology (e.g. an
//! NVIDIA Streamline bundle of `sl.*.dll`) is a single-technology set in
//! practice, so it falls back to file-name order — exactly detection's non-FSR
//! tiebreak (`order_with_primary_first`). Mirroring detection keeps a swapped
//! component's stored `files()[0]` identical to what the next rescan produces.

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
/// [`lineage::upscaler_represents_set`]; every other technology (e.g. a
/// Streamline bundle) selects the file-name-minimum member, mirroring detection.
/// The choice is order-independent, so `current_version` is correct regardless
/// of how a caller happened to store the file list.
///
/// For user-facing Streamline state, prefer [`crate::component_version_report`]
/// — a mixed `sl.*` folder must not look fully updated just because
/// `sl.common` matches.
#[must_use]
pub fn version_representative(files: &[ComponentFile]) -> Option<&ComponentFile> {
    if !is_fsr_named_set(files) {
        return files
            .iter()
            .min_by(|left, right| file_name(left).cmp(file_name(right)));
    }

    let upscaler_represents = upscaler_represents(files);
    files
        .iter()
        .min_by_key(|file| representative_key(file, upscaler_represents))
}

/// Sorts a component's files representative-first so the stored `files()[0]`
/// matches what detection's `order_with_primary_first` would produce.
///
/// FSR-named sets use the cohesion-aware key (entry point vs upscaler per
/// [`lineage::upscaler_represents_set`]), ties broken by case-insensitive file
/// name. Every other technology falls back to file-name order — detection's
/// non-FSR tiebreak.
///
/// Callers that rebuild a stored component's file list (the swap executor) apply
/// this so `files()[0]` stays the right version source until the next rescan: a
/// swap's `additive_active_files` appends kept baseline files first, which would
/// otherwise leave a stale file (an FSR denoiser, or a Streamline plugin) in
/// front of the freshly installed representative.
pub fn sort_representative_first(files: &mut [ComponentFile]) {
    if !is_fsr_named_set(files) {
        files.sort_by(|left, right| file_name(left).cmp(file_name(right)));
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
/// [`sort_representative_first`] (ascending sort) for FSR sets: [`primary_rank`],
/// then the case-insensitive file name.
fn representative_key(file: &ComponentFile, upscaler_represents: bool) -> (u8, String) {
    let name = file_name(file);
    (
        primary_rank(name, upscaler_represents),
        name.to_ascii_lowercase(),
    )
}

/// A file's base name, or `""` when the path has none — the shared key for
/// non-FSR selection and ordering.
fn file_name(file: &ComponentFile) -> &str {
    file.path().file_name().unwrap_or("")
}
