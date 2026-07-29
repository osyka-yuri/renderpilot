//! Installation-boundary analysis shared by add-game, recommendations, and
//! root correction.
//!
//! Filesystem and engine adapters provide facts. This module owns the policy
//! that turns those facts into one installation boundary. It deliberately does
//! not serialize its internal assessment types; transport DTOs are built at
//! the API boundary.

use std::path::{Path, PathBuf};

mod analysis;
mod classification;
mod recommendation;
#[cfg(test)]
mod tests;
mod types;

#[cfg(test)]
use recommendation::choose_best_recommendation;
pub(crate) use types::*;

/// Stateless policy object for one-install boundary decisions.
pub(crate) struct InstallBoundaryAnalyzer;

impl InstallBoundaryAnalyzer {
    /// Inspects the selected directory and its relevant ancestors. Each
    /// candidate is visited at most once, and every recommendation is backed by
    /// a complete executable walk of the recommended candidate.
    pub(crate) fn inspect(request: InstallBoundaryRequest<'_>) -> InstallBoundaryAssessment {
        analysis::inspect(&request)
    }

    /// Classifies one exact candidate without walking its ancestors.
    pub(crate) fn inspect_candidate(
        candidate: &Path,
        launcher_install_roots: &[PathBuf],
    ) -> CandidateBoundaryAssessment {
        analysis::inspect_candidate(candidate, launcher_install_roots)
    }
}
