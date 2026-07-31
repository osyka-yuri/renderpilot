//! Replacement-candidate lookup: which library artifacts can replace a game's
//! detected components.
//!
//! * [`matcher`] — compatibility matching and group construction.
//! * [`automatic`] — unattended-selection policy.
//! * [`ordering`] — stable presentation order and deduplication.
//! * [`dto`] — the data types produced for presentation layers.

mod automatic;
mod dto;
mod identity;
mod matcher;
mod ordering;

#[cfg(test)]
mod tests;

pub use dto::{
    ActiveCatalogPackage, CandidateComparison, CandidateSelection, ComponentReplacementCandidates,
    CoordinatedCandidateItem, CoordinatedCandidateOption, InstalledReleaseState,
    ReplacementCandidate,
};
pub use matcher::{
    CandidateArtifactIndex, CandidateContext, find_replacement_candidate_selection,
    find_replacement_candidate_selection_indexed, find_replacement_candidates,
    find_replacement_candidates_indexed,
};
