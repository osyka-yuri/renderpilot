//! Replacement-candidate lookup: which library artifacts can replace a game's
//! detected components.
//!
//! * [`matcher`] — the matching algorithm and its compatibility rules.
//! * [`dto`] — the data types the algorithm produces for presentation layers.

mod dto;
mod identity;
mod matcher;

#[cfg(test)]
mod tests;

pub use dto::{
    ActiveCatalogPackage, CandidateComparison, ComponentReplacementCandidates,
    InstalledReleaseState, ReplacementCandidate, is_automatic_catalog_candidate,
};
pub use matcher::{CandidateContext, find_replacement_candidates};
