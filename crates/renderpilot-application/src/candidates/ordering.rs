//! Stable presentation order and operationally canonical candidate identity.

use std::collections::HashSet;

use renderpilot_domain::{ArtifactId, PackageVersion, Version};

use super::dto::ReplacementCandidate;
use super::identity::{IntrinsicPackageIdentity, ResolvedTransitionIdentity};

pub(super) fn sort_and_deduplicate(
    mut candidates: Vec<ReplacementCandidate>,
) -> Vec<ReplacementCandidate> {
    // Resolve equivalent install outcomes before applying presentation order.
    // A candidate's row order must never accidentally decide which artifact is
    // selected for an otherwise identical transition.
    candidates
        .sort_by(|left, right| canonical_candidate_key(left).cmp(&canonical_candidate_key(right)));

    let mut seen_ids = HashSet::<ArtifactId>::new();
    let mut seen_resolved = HashSet::<ResolvedTransitionIdentity>::new();
    let mut deduplicated = Vec::with_capacity(candidates.len());

    for candidate in candidates {
        let duplicate = seen_ids.contains(candidate.artifact_id())
            || candidate
                .resolved_identity()
                .is_some_and(|identity| seen_resolved.contains(identity));
        if duplicate {
            continue;
        }
        seen_ids.insert(candidate.artifact_id().clone());
        if let Some(identity) = candidate.resolved_identity() {
            seen_resolved.insert(identity.clone());
        }
        deduplicated.push(candidate);
    }

    deduplicated.sort_by(|left, right| candidate_sort_key(left).cmp(&candidate_sort_key(right)));
    deduplicated
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CanonicalCandidateKey<'a> {
    source_rank: u8,
    downloaded: std::cmp::Reverse<bool>,
    trust_rank: u8,
    artifact_id: &'a str,
}

fn canonical_candidate_key(candidate: &ReplacementCandidate) -> CanonicalCandidateKey<'_> {
    CanonicalCandidateKey {
        source_rank: candidate.canonical_source_rank(),
        downloaded: std::cmp::Reverse(candidate.is_downloaded()),
        trust_rank: candidate.trust_level().candidate_preference_rank(),
        artifact_id: candidate.artifact_id().as_str(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CandidateSortKey<'a> {
    presentation_version: std::cmp::Reverse<Option<CandidatePresentationVersion<'a>>>,
    technical_version: std::cmp::Reverse<Option<&'a Version>>,
    file_count: std::cmp::Reverse<usize>,
    file_name: &'a str,
    is_debug: bool,
    trust_rank: u8,
    downloaded: std::cmp::Reverse<bool>,
    intrinsic_identity: Option<&'a IntrinsicPackageIdentity>,
    resolved_identity: Option<&'a ResolvedTransitionIdentity>,
    artifact_id: &'a str,
}

#[derive(Debug, Clone, Copy)]
enum CandidatePresentationVersion<'a> {
    Package(&'a PackageVersion),
    Technical(&'a Version),
}

impl PartialEq for CandidatePresentationVersion<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}

impl Eq for CandidatePresentationVersion<'_> {}

impl Ord for CandidatePresentationVersion<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let numeric = self.numeric_core().cmp(other.numeric_core());
        if numeric != std::cmp::Ordering::Equal {
            return numeric;
        }
        match (self, other) {
            (Self::Package(left), Self::Package(right)) => left.cmp(right),
            (Self::Technical(left), Self::Technical(right)) => left.cmp(right),
            (Self::Package(package), Self::Technical(_)) => {
                if package.is_prerelease() {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            }
            (Self::Technical(_), Self::Package(package)) => {
                if package.is_prerelease() {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            }
        }
    }
}

impl PartialOrd for CandidatePresentationVersion<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> CandidatePresentationVersion<'a> {
    fn numeric_core(self) -> &'a Version {
        match self {
            Self::Package(version) => version.numeric_core(),
            Self::Technical(version) => version,
        }
    }
}

fn candidate_sort_key(candidate: &ReplacementCandidate) -> CandidateSortKey<'_> {
    let presentation_version = candidate
        .catalog_package()
        .map(|package| CandidatePresentationVersion::Package(&package.release().version))
        .or_else(|| {
            candidate
                .technical_version()
                .map(CandidatePresentationVersion::Technical)
        });
    CandidateSortKey {
        presentation_version: std::cmp::Reverse(presentation_version),
        technical_version: std::cmp::Reverse(candidate.technical_version()),
        file_count: std::cmp::Reverse(candidate.file_count()),
        file_name: candidate.file_name(),
        is_debug: candidate.is_debug(),
        trust_rank: candidate.trust_level().candidate_preference_rank(),
        downloaded: std::cmp::Reverse(candidate.is_downloaded()),
        intrinsic_identity: candidate.intrinsic_identity(),
        resolved_identity: candidate.resolved_identity(),
        artifact_id: candidate.artifact_id().as_str(),
    }
}
