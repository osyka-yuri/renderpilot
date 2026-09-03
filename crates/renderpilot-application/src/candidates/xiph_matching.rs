//! Xiph-specific candidate recognition and release comparison.

use std::collections::{BTreeMap, BTreeSet};

use renderpilot_domain::{LibraryArtifact, LibraryComponent, LibraryTechnology, Sha256Hash, xiph};

use super::dto::{CandidateComparison, CatalogCandidatePackage};

/// Whether a component uses a vendor-suffixed Xiph runtime layout.
pub(super) fn component_has_vendor_alias(component: &LibraryComponent) -> bool {
    component.technology() == LibraryTechnology::XiphVorbis
        && component.files().iter().any(|file| {
            file.path()
                .file_name()
                .and_then(|name| xiph::parse_runtime_file_name(name).ok().flatten())
                .is_some_and(|runtime| runtime.is_vendor())
        })
}

/// Recognizes an already-applied vendor-alias Xiph package by its installed
/// member content. This is recognition-only: it never infers an external
/// import proof and cannot authorize a future transition.
pub(super) fn vendor_catalog_content_matches(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
) -> bool {
    component_has_vendor_alias(component)
        && vendor_catalog_content_matches_for_alias(component, artifact)
}

pub(super) fn vendor_catalog_content_matches_for_alias(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
) -> bool {
    if crate::compatibility::ensure_candidate_compatible_without_alias_proof(component, artifact)
        .is_err()
    {
        return false;
    }

    let Some(installed_members) = component_member_hashes(component) else {
        return false;
    };
    let Some(artifact_members) = artifact_member_hashes(artifact) else {
        return false;
    };

    // A game component is a projection of a catalog package: every member it
    // owns must match, while optional package members such as `vorbisenc.dll`
    // need not be installed into the game.
    !installed_members.is_empty()
        && installed_members
            .iter()
            .all(|(member, hash)| artifact_members.get(member) == Some(hash))
}

/// Maps a detected game component to the Xiph members it actually owns.
/// Duplicate or unclassifiable members are ambiguous and fail closed.
fn component_member_hashes(
    component: &LibraryComponent,
) -> Option<BTreeMap<xiph::XiphMember, Option<&Sha256Hash>>> {
    component
        .files()
        .iter()
        .try_fold(BTreeMap::new(), |mut members, file| {
            let runtime = file
                .path()
                .file_name()
                .and_then(|name| xiph::parse_runtime_file_name(name).ok().flatten())?;
            members
                .insert(runtime.member(), file.sha256())
                .is_none()
                .then_some(members)
        })
}

/// Maps every canonical member of a catalog artifact, rejecting ambiguity.
fn artifact_member_hashes(
    artifact: &LibraryArtifact,
) -> Option<BTreeMap<xiph::XiphMember, Option<&Sha256Hash>>> {
    artifact
        .files()
        .iter()
        .try_fold(BTreeMap::new(), |mut members, file| {
            let file_name = file.install_as().or_else(|| file.path().file_name())?;
            let (member, _) = xiph::classify_canonical_file_name(file_name)?;
            members
                .insert(member, file.sha256())
                .is_none()
                .then_some(members)
        })
}

/// Compares Xiph packages componentwise, retaining mixed releases explicitly.
pub(super) fn candidate_comparison(
    component: &LibraryComponent,
    catalog_package: Option<&CatalogCandidatePackage>,
) -> CandidateComparison {
    let Some(release) = catalog_package.map(CatalogCandidatePackage::release) else {
        return CandidateComparison::UnknownVersion;
    };
    let Some(required_axes) = xiph::XiphReleaseAxes::from_component_files(component.files()) else {
        return CandidateComparison::UnknownVersion;
    };
    let Some(candidate_versions) =
        xiph::XiphReleaseVersions::from_catalog_components(&required_axes, &release.components)
    else {
        return CandidateComparison::UnknownVersion;
    };
    let mut has_newer = false;
    let mut has_older = false;
    let mut observed_axes = BTreeSet::new();
    for file in component.files() {
        let component_name = match file
            .path()
            .file_name()
            .and_then(|name| xiph::parse_runtime_file_name(name).ok().flatten())
        {
            Some(runtime) => xiph::XiphReleaseAxis::for_member(runtime.member()),
            None => return CandidateComparison::UnknownVersion,
        };
        observed_axes.insert(component_name);
        let (Some(current), Some(candidate)) =
            (file.version(), candidate_versions.get(component_name))
        else {
            return CandidateComparison::UnknownVersion;
        };
        match current.cmp(candidate.numeric_core()) {
            std::cmp::Ordering::Less => has_newer = true,
            std::cmp::Ordering::Equal => {}
            std::cmp::Ordering::Greater => has_older = true,
        }
    }
    if observed_axes.into_iter().collect::<Vec<_>>() != required_axes.iter().collect::<Vec<_>>() {
        // Embedded Ogg has no physical FileVersion to compare. Treat it as an
        // unknown mandatory dimension instead of inferring it from Vorbis.
        return CandidateComparison::UnknownVersion;
    }
    match (has_newer, has_older) {
        (true, true) => CandidateComparison::MixedVersion,
        (true, false) => CandidateComparison::NewerVersion,
        (false, true) => CandidateComparison::OlderVersion,
        (false, false) => CandidateComparison::EqualVersion,
    }
}
