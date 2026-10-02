//! Orchestrates the heuristic detection and classification of game executables.
//!
//! The NVIDIA Driver Settings (DRS) architecture exclusively indexes application profiles by
//! executable basename. Consequently, RenderPilot must deterministically identify a "primary"
//! executable for each game installation to guarantee accurate profile resolution and writes.
//! This module surfaces a rigorously ranked list of candidates evaluated against exclusion filters
//! and positive heuristic signals (e.g., proximity to root, stem matching, binary payload size).
//! This enables the upstream orchestration layer to either autonomously select the optimal target
//! or expose the ranked collection for manual user override.
//!
//! Detection execution is strictly bounded to a designated install directory and operates purely
//! via filesystem metadata—deliberately eschewing deep PE parsing. Version-specific PE introspection
//! lives in the global catalog (`renderpilot-detection`), not in this crate.

mod filters;
mod pe;
mod ranking;
mod scan;
mod shipping;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use renderpilot_detection::{InstallTreeCompleteness, InstallTreeWalker};

pub use filters::RejectionReason;
pub use pe::is_readable_windows_pe_executable;
pub use shipping::{is_bound_shipping_target, is_shipping_binary_name, strip_shipping_suffix};

use filters::classify;
use ranking::compute_rank_score;
use scan::collect_raw_candidates;

/// One executable discovered inside a game's install directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutableCandidate {
    /// Absolute path on disk.
    pub absolute_path: PathBuf,
    /// Path relative to the install dir root, using forward slashes
    /// regardless of the host filesystem. E.g. `"bin/Game.exe"`.
    pub relative_path: String,
    /// Just the basename (e.g. `"Game.exe"`). NVAPI is keyed by this.
    pub file_name: String,
    /// File size in bytes. Used by the ranking heuristic.
    pub size_bytes: u64,
    /// Depth relative to the install dir root (0 = directly in the root).
    pub depth: u32,
    /// Ranking score: higher = more likely to be the main game binary.
    /// Only meaningful for candidates with `rejection: None`.
    pub rank_score: i32,
    /// `None` means "looks like a game binary". `Some` means a filter
    /// rejected it; the UI can still surface it as a "show more" option
    /// in case the heuristic was wrong.
    pub rejection: Option<RejectionReason>,
}

/// Read-only executable probe for one installation tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutableDetectionReport {
    candidates: Vec<ExecutableCandidate>,
    structural_files: Vec<PathBuf>,
    completeness: InstallTreeCompleteness,
    diagnostics: Vec<String>,
    visited_entries: usize,
}

impl ExecutableDetectionReport {
    /// Ranked executable candidates.
    pub fn candidates(&self) -> &[ExecutableCandidate] {
        &self.candidates
    }

    /// Non-executable distribution files observed by the same traversal.
    ///
    /// These paths are boundary evidence only; their contents are never read
    /// or hashed by executable inspection.
    pub fn structural_files(&self) -> &[PathBuf] {
        &self.structural_files
    }

    /// Whether every reachable directory was enumerated.
    pub fn completeness(&self) -> InstallTreeCompleteness {
        self.completeness
    }

    /// Recoverable filesystem/cancellation diagnostics.
    ///
    /// An intentional advisory depth limit is reflected by [`Self::completeness`]
    /// but is not an error and therefore is not included here.
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    /// Directory entries consumed by this inspection.
    pub fn visited_entries(&self) -> usize {
        self.visited_entries
    }
}

/// Scans `install_dir` for executables and returns them ranked.
///
/// Order:
///   1. `rejection: None` (game-binary candidates) first, sorted by
///      `rank_score DESC`, then by relative path ASC for stability.
///   2. `rejection: Some(_)` last, sorted by relative path ASC.
///
/// Returns an empty vector if the directory does not exist or cannot
/// be read; never panics on filesystem errors.
pub fn detect_executable_candidates(install_dir: &Path) -> Vec<ExecutableCandidate> {
    inspect_executable_candidates(install_dir).candidates
}

/// Probes executable candidates and preserves traversal completeness evidence.
#[must_use]
pub fn inspect_executable_candidates(install_dir: &Path) -> ExecutableDetectionReport {
    inspect_executable_candidates_with_walker(install_dir, InstallTreeWalker::probe())
}

/// Performs a complete, non-hashing executable walk for an installation
/// boundary decision.
///
/// Advisory probes intentionally trade coverage for latency. A root
/// recommendation, however, must know whether another executable branch exists
/// before it can call a parent one installation. This variant traverses every
/// reachable non-reparse directory and preserves incomplete diagnostics.
#[must_use]
pub fn inspect_executable_candidates_complete(install_dir: &Path) -> ExecutableDetectionReport {
    inspect_executable_candidates_with_walker(install_dir, InstallTreeWalker::full())
}

/// Performs a strict complete walk for an installation boundary decision.
///
/// Unlike the advisory complete walk, the strict walk preserves every
/// reachable directory branch and reports reparse points or inaccessible
/// entries as incomplete evidence. It still performs no content hashing.
#[must_use]
pub fn inspect_executable_candidates_complete_strict(
    install_dir: &Path,
) -> ExecutableDetectionReport {
    inspect_executable_candidates_with_walker(install_dir, InstallTreeWalker::full_strict())
}

/// Performs a complete executable walk with an explicit entry budget and
/// cooperative cancellation.
///
/// Budget exhaustion and cancellation both produce an incomplete report;
/// callers must not use such a report as authoritative boundary evidence.
#[must_use]
pub fn inspect_executable_candidates_bounded(
    install_dir: &Path,
    max_entries: usize,
    is_cancelled: impl Fn() -> bool,
) -> ExecutableDetectionReport {
    inspect_executable_candidates_with_walker_and_cancel(
        install_dir,
        InstallTreeWalker::full().with_entry_budget(max_entries),
        is_cancelled,
    )
}

fn inspect_executable_candidates_with_walker(
    install_dir: &Path,
    walker: InstallTreeWalker,
) -> ExecutableDetectionReport {
    inspect_executable_candidates_with_walker_and_cancel(install_dir, walker, || false)
}

fn inspect_executable_candidates_with_walker_and_cancel(
    install_dir: &Path,
    walker: InstallTreeWalker,
    is_cancelled: impl Fn() -> bool,
) -> ExecutableDetectionReport {
    let install_dir_canonical = install_dir.to_path_buf();
    let install_dir_name = install_dir
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_owned)
        .unwrap_or_default();

    let (raw_candidates, structural_files, completeness, diagnostics, visited_entries) =
        collect_raw_candidates(&install_dir_canonical, walker, is_cancelled);

    let mut candidates: Vec<ExecutableCandidate> = raw_candidates
        .into_iter()
        .map(|raw| {
            let rejection = classify(&raw.relative_path, &raw.file_name_no_ext, &raw.file_name);
            let rank_score = compute_rank_score(&raw, &install_dir_name);
            ExecutableCandidate {
                absolute_path: raw.absolute_path,
                relative_path: raw.relative_path,
                file_name: raw.file_name,
                size_bytes: raw.size_bytes,
                depth: raw.depth,
                rank_score,
                rejection,
            }
        })
        .collect();

    candidates.sort_by(
        |a, b| match (a.rejection.is_some(), b.rejection.is_some()) {
            (false, true) => std::cmp::Ordering::Less,
            (true, false) => std::cmp::Ordering::Greater,
            _ => b
                .rank_score
                .cmp(&a.rank_score)
                .then_with(|| a.relative_path.cmp(&b.relative_path)),
        },
    );

    ExecutableDetectionReport {
        candidates,
        structural_files,
        completeness,
        diagnostics,
        visited_entries,
    }
}

/// Returns whether one executable directly in an installation root is an
/// accepted game candidate.
///
/// This is the non-recursive counterpart to [`inspect_executable_candidates`].
/// It deliberately reuses the same filename exclusion classifier, so callers
/// that already know the exact root entry do not need to rescan sibling trees
/// merely to reject launchers, installers, and support tools.
#[must_use]
pub fn is_accepted_root_game_executable(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(stem) = path.file_stem().and_then(|name| name.to_str()) else {
        return false;
    };
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
        && is_readable_windows_pe_executable(path)
        && classify(file_name, stem, file_name).is_none()
}
