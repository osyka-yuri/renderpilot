//! Backend-owned automatic selection and coordinated Streamline options.

use std::collections::BTreeMap;

use renderpilot_domain::{
    ArtifactId, ComponentId, LibraryComponent, LibraryTechnology, PackageRelease, PackageVersion,
    ReleaseChannel,
    xiph::{XiphReleaseAxes, XiphReleaseVersions},
};
use sha2::{Digest, Sha256};

use super::dto::{
    CandidateComparison, CatalogCandidatePackage, ComponentReplacementCandidates,
    CoordinatedCandidateItem, CoordinatedCandidateOption, ReplacementCandidate,
};
use crate::D3d12ExecutableActionKind;

const COORDINATED_OPTION_SCHEMA_VERSION: u32 = 1;

/// An atomically calculated replacement selection for one game.
#[derive(Debug, Default)]
pub(super) struct SelectionOutcome {
    pub(super) automatic_by_component: BTreeMap<ComponentId, ArtifactId>,
    pub(super) streamline_options: Vec<CoordinatedCandidateOption>,
}

/// Calculates every automatic selection before any presentation DTO is mutated.
#[must_use]
pub(super) fn coordinate_selection(
    components: &[LibraryComponent],
    groups: &[ComponentReplacementCandidates],
) -> SelectionOutcome {
    let mut outcome = SelectionOutcome::default();
    let mut components = components
        .iter()
        .filter(|component| !component.files().is_empty())
        .collect::<Vec<_>>();
    components.sort_by(|left, right| left.id().as_str().cmp(right.id().as_str()));

    for component in &components {
        if component.technology() == LibraryTechnology::NvidiaStreamline {
            continue;
        }
        let Some(group) = group_for(groups, component.id()) else {
            continue;
        };
        if let Some(artifact_id) = select_component_automatic(component, group.candidates()) {
            outcome
                .automatic_by_component
                .insert(component.id().clone(), artifact_id);
        }
    }

    let streamline = components
        .iter()
        .copied()
        .filter(|component| component.technology() == LibraryTechnology::NvidiaStreamline)
        .collect::<Vec<_>>();
    if streamline.is_empty() {
        return outcome;
    }

    outcome.streamline_options = coordinated_streamline_options(&streamline, groups);
    if streamline.len() == 1 {
        let component = streamline[0];
        if let Some(group) = group_for(groups, component.id())
            && let Some(artifact_id) = select_component_automatic(component, group.candidates())
        {
            outcome
                .automatic_by_component
                .insert(component.id().clone(), artifact_id);
        }
        return outcome;
    }

    if let Some(selections) = coordinated_streamline_automatic(&streamline, groups) {
        outcome.automatic_by_component.extend(selections);
    }
    outcome
}

/// Returns whether a candidate's catalog package permits unattended use.
#[must_use]
pub(super) fn is_automatic_catalog_candidate(candidate: &ReplacementCandidate) -> bool {
    candidate
        .catalog_package()
        .is_some_and(CatalogCandidatePackage::automatic_selection_allowed)
}

fn select_component_automatic(
    component: &LibraryComponent,
    candidates: &[ReplacementCandidate],
) -> Option<ArtifactId> {
    let axes = required_xiph_axes(component)?;
    let eligible = candidates
        .iter()
        .filter(|candidate| candidate_is_eligible(candidate, axes.as_ref()))
        .collect::<Vec<_>>();
    let maxima = eligible
        .iter()
        .copied()
        .filter(|candidate| {
            !eligible.iter().any(|other| {
                other.artifact_id() != candidate.artifact_id()
                    && candidate_dominates(other, candidate, axes.as_ref())
            })
        })
        .collect::<Vec<_>>();
    let [maximum] = maxima.as_slice() else {
        return None;
    };
    Some(maximum.artifact_id().clone())
}

fn coordinated_streamline_automatic(
    components: &[&LibraryComponent],
    groups: &[ComponentReplacementCandidates],
) -> Option<BTreeMap<ComponentId, ArtifactId>> {
    let eligible_by_release = candidate_release_index(components, groups, |candidate| {
        candidate_is_eligible(candidate, None)
    });
    let complete = complete_cohorts(components, groups, &eligible_by_release);
    let newest_version = complete
        .keys()
        .map(|identity| &identity.version)
        .max()
        .cloned()?;
    let newest = complete
        .into_iter()
        .filter(|(identity, _)| identity.version == newest_version)
        .collect::<Vec<_>>();
    let [(_, items)] = newest.as_slice() else {
        return None;
    };
    Some(items.clone())
}

fn coordinated_streamline_options(
    components: &[&LibraryComponent],
    groups: &[ComponentReplacementCandidates],
) -> Vec<CoordinatedCandidateOption> {
    let candidates_by_release = candidate_release_index(components, groups, |_| true);
    let complete = complete_cohorts(components, groups, &candidates_by_release);

    let mut options = complete
        .into_iter()
        .filter_map(|(identity, items)| {
            let release = canonical_release_for_identity(&identity, groups)?;
            let items = items
                .into_iter()
                .map(|(component_id, artifact_id)| {
                    CoordinatedCandidateItem::new(component_id, artifact_id)
                })
                .collect::<Vec<_>>();
            Some(CoordinatedCandidateOption::new(
                coordinated_option_id(&identity, &items),
                release,
                items,
            ))
        })
        .collect::<Vec<_>>();
    options.sort_by(|left, right| {
        right
            .release()
            .version
            .cmp(&left.release().version)
            .then_with(|| left.option_id().cmp(right.option_id()))
    });
    options
}

/// Returns complete and unambiguous component→artifact cohorts. Every
/// installed component has exactly one canonical replacement candidate in each
/// returned cohort; partial swaps are not a coordinated option.
fn complete_cohorts(
    components: &[&LibraryComponent],
    groups: &[ComponentReplacementCandidates],
    candidates_by_release: &BTreeMap<
        SemanticReleaseIdentity,
        BTreeMap<ComponentId, Vec<ArtifactId>>,
    >,
) -> BTreeMap<SemanticReleaseIdentity, BTreeMap<ComponentId, ArtifactId>> {
    let mut complete = BTreeMap::new();
    for (identity, by_component) in candidates_by_release {
        let mut items = BTreeMap::new();
        let mut valid = true;
        for component in components {
            if group_for(groups, component.id()).is_none() {
                valid = false;
                break;
            }
            let Some(matches) = by_component.get(component.id()) else {
                valid = false;
                break;
            };
            let [artifact_id] = matches.as_slice() else {
                valid = false;
                break;
            };
            items.insert(component.id().clone(), artifact_id.clone());
        }
        if valid {
            complete.insert(identity.clone(), items);
        }
    }
    complete
}

fn candidate_release_index(
    components: &[&LibraryComponent],
    groups: &[ComponentReplacementCandidates],
    include: impl Fn(&ReplacementCandidate) -> bool,
) -> BTreeMap<SemanticReleaseIdentity, BTreeMap<ComponentId, Vec<ArtifactId>>> {
    let mut by_release = BTreeMap::new();
    for component in components {
        let Some(group) = group_for(groups, component.id()) else {
            continue;
        };
        for candidate in group
            .candidates()
            .iter()
            .filter(|candidate| include(candidate))
        {
            let Some(package) = candidate.catalog_package() else {
                continue;
            };
            by_release
                .entry(SemanticReleaseIdentity::from_release(package.release()))
                .or_insert_with(BTreeMap::new)
                .entry(component.id().clone())
                .or_insert_with(Vec::new)
                .push(candidate.artifact_id().clone());
        }
    }
    for by_component in by_release.values_mut() {
        for artifacts in by_component.values_mut() {
            artifacts.sort_by(|left, right| left.as_str().cmp(right.as_str()));
            artifacts.dedup();
        }
    }
    by_release
}

fn canonical_release_for_identity(
    identity: &SemanticReleaseIdentity,
    groups: &[ComponentReplacementCandidates],
) -> Option<PackageRelease> {
    // The identity deliberately excludes `label`. It is presentation-only;
    // choose the stable lexical minimum if catalog sources disagree on it.
    let label = groups
        .iter()
        .flat_map(ComponentReplacementCandidates::candidates)
        .filter_map(|candidate| {
            candidate
                .catalog_package()
                .map(CatalogCandidatePackage::release)
        })
        .filter(|release| SemanticReleaseIdentity::from_release(release) == *identity)
        .map(|release| release.label.clone())
        .min()?;
    Some(PackageRelease {
        version: identity.version.clone(),
        channel: identity.channel,
        label,
        components: identity.components.clone(),
    })
}

fn group_for<'a>(
    groups: &'a [ComponentReplacementCandidates],
    component_id: &ComponentId,
) -> Option<&'a ComponentReplacementCandidates> {
    groups
        .iter()
        .find(|group| group.component_id() == component_id)
}

fn required_xiph_axes(component: &LibraryComponent) -> Option<Option<XiphReleaseAxes>> {
    if component.technology() != LibraryTechnology::XiphVorbis {
        return Some(None);
    }
    XiphReleaseAxes::from_component_files(component.files()).map(Some)
}

fn candidate_is_eligible(
    candidate: &ReplacementCandidate,
    xiph_axes: Option<&XiphReleaseAxes>,
) -> bool {
    candidate.comparison() == CandidateComparison::NewerVersion
        && is_automatic_catalog_candidate(candidate)
        && candidate
            .d3d12_executable_action()
            .is_none_or(|action| action.kind() != D3d12ExecutableActionKind::RepairRequired)
        && candidate.catalog_package().is_some_and(|package| {
            xiph_axes.is_none_or(|axes| {
                XiphReleaseVersions::from_catalog_components(axes, &package.release().components)
                    .is_some()
            })
        })
}

fn candidate_dominates(
    left: &ReplacementCandidate,
    right: &ReplacementCandidate,
    xiph_axes: Option<&XiphReleaseAxes>,
) -> bool {
    let (Some(left), Some(right)) = (left.catalog_package(), right.catalog_package()) else {
        return false;
    };
    release_dominates(left.release(), right.release(), xiph_axes)
}

/// Strict partial order over the physical deployment's required axes.
fn release_dominates(
    left: &PackageRelease,
    right: &PackageRelease,
    xiph_axes: Option<&XiphReleaseAxes>,
) -> bool {
    let Some(axes) = xiph_axes else {
        return left.version > right.version;
    };
    let (Some(left), Some(right)) = (
        XiphReleaseVersions::from_catalog_components(axes, &left.components),
        XiphReleaseVersions::from_catalog_components(axes, &right.components),
    ) else {
        return false;
    };

    let mut greater = false;
    for axis in axes.iter() {
        match left
            .get(axis)
            .expect("required axis")
            .cmp(right.get(axis).expect("required axis"))
        {
            std::cmp::Ordering::Less => return false,
            std::cmp::Ordering::Equal => {}
            std::cmp::Ordering::Greater => greater = true,
        }
    }
    greater
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SemanticReleaseIdentity {
    version: PackageVersion,
    channel: ReleaseChannel,
    components: BTreeMap<String, PackageVersion>,
}

impl Ord for SemanticReleaseIdentity {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.version
            .cmp(&other.version)
            .then_with(|| {
                release_channel_slug(self.channel).cmp(release_channel_slug(other.channel))
            })
            .then_with(|| self.components.cmp(&other.components))
    }
}

impl PartialOrd for SemanticReleaseIdentity {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl SemanticReleaseIdentity {
    fn from_release(release: &PackageRelease) -> Self {
        Self {
            version: release.version.clone(),
            channel: release.channel,
            components: release.components.clone(),
        }
    }
}

fn coordinated_option_id(
    identity: &SemanticReleaseIdentity,
    items: &[CoordinatedCandidateItem],
) -> String {
    let mut hasher = Sha256::new();
    hash_u32(&mut hasher, COORDINATED_OPTION_SCHEMA_VERSION);
    hash_text(&mut hasher, "nvidia_streamline");
    hash_text(&mut hasher, identity.version.as_str());
    hash_text(&mut hasher, release_channel_slug(identity.channel));
    hash_u32(
        &mut hasher,
        u32::try_from(identity.components.len()).expect("component count"),
    );
    for (axis, version) in &identity.components {
        hash_text(&mut hasher, axis);
        hash_text(&mut hasher, version.as_str());
    }
    hash_u32(&mut hasher, u32::try_from(items.len()).expect("item count"));
    for item in items {
        hash_text(&mut hasher, item.component_id().as_str());
        hash_text(&mut hasher, item.artifact_id().as_str());
    }
    hex::encode(hasher.finalize())
}

fn release_channel_slug(channel: ReleaseChannel) -> &'static str {
    match channel {
        ReleaseChannel::Stable => "stable",
        ReleaseChannel::Beta => "beta",
        ReleaseChannel::Preview => "preview",
        ReleaseChannel::Debug => "debug",
    }
}

fn hash_u32(hasher: &mut Sha256, value: u32) {
    hasher.update(value.to_be_bytes());
}

fn hash_text(hasher: &mut Sha256, value: &str) {
    hash_u32(hasher, u32::try_from(value.len()).expect("text length"));
    hasher.update(value.as_bytes());
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use renderpilot_domain::xiph::{XiphMember, XiphReleaseAxis};
    use renderpilot_domain::{ComponentFile, ComponentKind, GameId, PathRef, Swappability};

    use super::*;

    #[test]
    fn componentwise_dominance_is_partial_and_fail_closed() {
        let release = |vorbis: &str, ogg: Option<&str>| PackageRelease {
            version: PackageVersion::parse(vorbis).expect("version"),
            channel: ReleaseChannel::Stable,
            label: None,
            components: [
                Some((
                    "vorbis".to_owned(),
                    PackageVersion::parse(vorbis).expect("Vorbis version"),
                )),
                ogg.map(|version| {
                    (
                        "ogg".to_owned(),
                        PackageVersion::parse(version).expect("Ogg version"),
                    )
                }),
            ]
            .into_iter()
            .flatten()
            .collect::<BTreeMap<_, _>>(),
        };
        let axes = XiphReleaseAxes::from_members([XiphMember::Vorbis]);
        let baseline = release("1.3.7", Some("1.3.5"));
        let dominates = release("1.3.8", Some("1.3.6"));
        let incomparable = release("1.3.8", Some("1.3.4"));
        let incomplete = release("1.3.9", None);

        assert!(release_dominates(&dominates, &baseline, Some(&axes)));
        assert!(!release_dominates(&incomparable, &baseline, Some(&axes)));
        assert!(!release_dominates(&incomplete, &baseline, Some(&axes)));
    }

    #[test]
    fn xiph_axes_cover_every_reviewed_alias_and_embedded_ogg() {
        let component = [
            "libvorbisfile-3.dll",
            "libvorbisenc-2.dll",
            "libvorbis-0.dll",
            "libogg-0.dll",
        ]
        .iter()
        .fold(
            LibraryComponent::new(
                ComponentId::new("component:test:xiph-abi").expect("component id"),
                GameId::new("game:test").expect("game id"),
                ComponentKind::NativeLibrary,
                LibraryTechnology::XiphVorbis,
                Swappability::BundleOnly,
            ),
            |component, name| {
                component.with_file(ComponentFile::new(
                    PathRef::new(format!("C:/Game/{name}")).expect("path"),
                ))
            },
        );
        let axes = required_xiph_axes(&component)
            .expect("axes")
            .expect("Xiph axes");
        assert_eq!(
            axes.iter().collect::<Vec<_>>(),
            vec![XiphReleaseAxis::Ogg, XiphReleaseAxis::Vorbis]
        );
    }
}
