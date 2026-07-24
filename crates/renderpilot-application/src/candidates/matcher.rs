//! Replacement-candidate matching algorithm.
//!
//! Matches each detected component bundle against same-technology artifacts,
//! applying API/version/lineage compatibility rules, then builds, sorts, and
//! deduplicates the resulting [`ReplacementCandidate`] list. The data types it
//! produces live in [`super::dto`].

use std::collections::{HashMap, HashSet};

use renderpilot_domain::{
    ArtifactId, GraphicsComponent, GraphicsTechnology, LibraryArtifact, Version, fsr,
};

use super::dto::{
    CandidateComparison, ComponentReplacementCandidates, InstalledReleaseState,
    ReplacementCandidate,
};
use super::identity::{IntrinsicPackageIdentity, ResolvedTransitionIdentity};
use crate::{
    SwapCompatibilityError, SwapTargetProfile, ensure_replacement_compatible,
    replacement_executable_action,
};

/// Context for candidate lookup that carries source metadata for artifacts.
#[derive(Debug, Clone)]
pub struct CandidateContext {
    downloaded_ids: HashSet<ArtifactId>,
    catalog_package_ids: HashMap<ArtifactId, String>,
    debug_package_ids: HashSet<String>,
    target_profile: SwapTargetProfile,
}

impl CandidateContext {
    /// Creates a new candidate context from the given lookup tables.
    pub fn new(
        downloaded_ids: HashSet<ArtifactId>,
        catalog_package_ids: HashMap<ArtifactId, String>,
        debug_package_ids: HashSet<String>,
    ) -> Self {
        Self {
            downloaded_ids,
            catalog_package_ids,
            debug_package_ids,
            target_profile: SwapTargetProfile::default(),
        }
    }

    /// Returns an empty context with no source metadata.
    pub fn empty() -> Self {
        Self {
            downloaded_ids: HashSet::new(),
            catalog_package_ids: HashMap::new(),
            debug_package_ids: HashSet::new(),
            target_profile: SwapTargetProfile::default(),
        }
    }

    /// Attaches fresh executable facts used by Microsoft runtime policies.
    #[must_use]
    pub fn with_target_profile(mut self, profile: SwapTargetProfile) -> Self {
        self.target_profile = profile;
        self
    }

    /// Returns true if the given catalog package belongs to a debug build.
    pub fn is_debug_package(&self, package_id: &str) -> bool {
        self.debug_package_ids.contains(package_id)
    }
}

/// Finds replacement candidates for the detected components of one game.
///
/// Matching is per *component bundle*, not per file: a component is matched
/// against artifacts of the same exact technology whose bundle content differs
/// from what is currently installed. A cohesive FSR component still uses
/// [`GraphicsTechnology::AmdFsr`], so an FSR 3 (single-file) component can still
/// be replaced by an FSR 4 (three-file) artifact.
#[must_use]
pub fn find_replacement_candidates(
    components: &[GraphicsComponent],
    artifacts: &[LibraryArtifact],
    context: &CandidateContext,
) -> Vec<ComponentReplacementCandidates> {
    let artifacts_by_technology = group_artifacts_by_technology(artifacts);
    let mut groups = Vec::new();

    for component in components {
        if component.files().is_empty() {
            continue;
        }

        let Some(component_artifacts) = artifacts_by_technology.get(&component.technology()) else {
            continue;
        };

        let installed_release = installed_release_state(component, component_artifacts, context);
        let current_version = installed_release.known_version();
        let candidates = component_artifacts
            .iter()
            .filter_map(|indexed| {
                let artifact = indexed.artifact;
                // Ignore artifacts scanned from the exact same game.
                // Such artifacts represent the game's own mutable file paths.
                // If the game was modified (e.g. rolled back), the artifact's
                // stored SHA-256 no longer matches its path, leading to swap errors.
                if artifact.source_game_id() == Some(component.game_id()) {
                    return None;
                }

                match ensure_replacement_compatible(component, artifact, &context.target_profile) {
                    Ok(()) | Err(SwapCompatibilityError::D3d12ExecutableRepairRequired) => {}
                    Err(_) => return None,
                }
                let d3d12_executable_action =
                    replacement_executable_action(artifact, &context.target_profile).ok()?;
                let resolved_identity =
                    ResolvedTransitionIdentity::for_replacement(component, artifact).ok()?;
                if resolved_identity
                    .as_ref()
                    .and_then(|identity| identity.installed_projection(component))
                    .as_ref()
                    == resolved_identity.as_ref()
                {
                    return None;
                }
                let comparison = candidate_comparison(component, artifact, current_version)?;
                let is_downloaded = context.downloaded_ids.contains(artifact.id());
                let package_id = context.catalog_package_ids.get(artifact.id()).cloned();
                let is_debug = package_id
                    .as_ref()
                    .is_some_and(|id| context.is_debug_package(id));
                Some(
                    ReplacementCandidate::new(
                        artifact,
                        comparison,
                        is_downloaded,
                        package_id,
                        is_debug,
                        indexed.intrinsic_identity.clone(),
                        resolved_identity,
                    )
                    .with_d3d12_executable_action(d3d12_executable_action),
                )
            })
            .collect::<Vec<_>>();

        if candidates.is_empty() {
            continue;
        }

        let candidates = sort_and_deduplicate_candidates(candidates);
        groups.push(ComponentReplacementCandidates::new(
            component,
            installed_release,
            candidates,
        ));
    }

    groups.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
    groups
}

fn installed_release_state(
    component: &GraphicsComponent,
    artifacts: &[IndexedArtifact<'_>],
    context: &CandidateContext,
) -> InstalledReleaseState {
    if let Some(release) = installed_catalog_release(component, artifacts, context) {
        return release;
    }
    if component.technology() == GraphicsTechnology::OpenVr {
        return InstalledReleaseState::Unknown;
    }
    InstalledReleaseState::from_version_report(super::dto::component_version_state(component))
}

fn installed_catalog_release(
    component: &GraphicsComponent,
    artifacts: &[IndexedArtifact<'_>],
    context: &CandidateContext,
) -> Option<InstalledReleaseState> {
    let matched = artifacts
        .iter()
        .map(|indexed| indexed.artifact)
        .filter(|artifact| context.catalog_package_ids.contains_key(artifact.id()))
        .filter(|artifact| {
            let Ok(Some(identity)) =
                ResolvedTransitionIdentity::for_replacement(component, artifact)
            else {
                return false;
            };
            identity.installed_projection(component).as_ref() == Some(&identity)
        })
        .filter_map(|artifact| {
            Some((
                artifact.release_version()?.clone(),
                context.catalog_package_ids.get(artifact.id())?,
                artifact.metadata().release_label().map(str::to_owned),
            ))
        })
        .max_by(|left, right| left.0.cmp(&right.0).then_with(|| right.1.cmp(left.1)));
    matched.map(|(version, _, label)| InstalledReleaseState::known(version, label))
}

#[derive(Debug, Clone)]
struct IndexedArtifact<'a> {
    artifact: &'a LibraryArtifact,
    intrinsic_identity: Option<IntrinsicPackageIdentity>,
}

/// Groups artifacts by their exact technology.
fn group_artifacts_by_technology(
    artifacts: &[LibraryArtifact],
) -> HashMap<GraphicsTechnology, Vec<IndexedArtifact<'_>>> {
    let mut grouped = HashMap::<GraphicsTechnology, Vec<IndexedArtifact<'_>>>::new();

    for artifact in artifacts {
        grouped
            .entry(artifact.technology())
            .or_default()
            .push(IndexedArtifact {
                artifact,
                intrinsic_identity: IntrinsicPackageIdentity::for_artifact(artifact),
            });
    }

    grouped
}

fn compare_versions(
    current: Option<&Version>,
    candidate: Option<&Version>,
) -> Option<CandidateComparison> {
    match (current, candidate) {
        (Some(current), Some(candidate)) => match current.cmp(candidate) {
            std::cmp::Ordering::Less => Some(CandidateComparison::NewerVersion),
            std::cmp::Ordering::Equal => Some(CandidateComparison::UnknownVersion),
            std::cmp::Ordering::Greater => Some(CandidateComparison::OlderVersion),
        },
        _ => Some(CandidateComparison::UnknownVersion),
    }
}

/// Sorts candidates into their stable presentation order (the
/// `ordering_key`), then collapses duplicates — first occurrence wins.
///
/// Two candidates are duplicates only when either their artifact id or their
/// complete install-target + member-digest identity is equal. Releases that
/// merely share a version remain independently selectable.
fn sort_and_deduplicate_candidates(
    mut candidates: Vec<ReplacementCandidate>,
) -> Vec<ReplacementCandidate> {
    candidates.sort_by(|left, right| left.ordering_key().cmp(&right.ordering_key()));

    let mut seen_ids = HashSet::<ArtifactId>::new();
    let mut seen_intrinsic = HashSet::<IntrinsicPackageIdentity>::new();
    let mut seen_resolved = HashSet::<ResolvedTransitionIdentity>::new();
    let mut deduplicated = Vec::with_capacity(candidates.len());

    for candidate in candidates {
        let duplicate = seen_ids.contains(candidate.artifact_id())
            || candidate
                .intrinsic_identity()
                .is_some_and(|identity| seen_intrinsic.contains(identity))
            || candidate
                .resolved_identity()
                .is_some_and(|identity| seen_resolved.contains(identity));
        if duplicate {
            continue;
        }
        seen_ids.insert(candidate.artifact_id().clone());
        if let Some(identity) = candidate.intrinsic_identity() {
            seen_intrinsic.insert(identity.clone());
        }
        if let Some(identity) = candidate.resolved_identity() {
            seen_resolved.insert(identity.clone());
        }
        deduplicated.push(candidate);
    }

    deduplicated
}

/// Policy that determines whether a candidate artifact can replace a component file
/// based on their version compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompatibilityPolicy {
    /// Any version transition is allowed.
    AlwaysCompatible,
    /// DLSS Super Resolution specific rules: v1 cannot be replaced by v2+,
    /// but all other transitions are allowed.
    DlssSuperResolution,
}

impl CompatibilityPolicy {
    /// Returns true if the candidate version can replace the current version.
    fn is_compatible(self, current: Option<&Version>, candidate: Option<&Version>) -> bool {
        match self {
            Self::AlwaysCompatible => true,
            Self::DlssSuperResolution => match (current, candidate) {
                (Some(current), Some(candidate)) => {
                    renderpilot_domain::dlss::versions_are_compatible(current, candidate)
                }
                _ => true,
            },
        }
    }
}

impl From<GraphicsTechnology> for CompatibilityPolicy {
    fn from(technology: GraphicsTechnology) -> Self {
        match technology {
            GraphicsTechnology::DlssSuperResolution => Self::DlssSuperResolution,
            _ => Self::AlwaysCompatible,
        }
    }
}

fn candidate_comparison(
    component: &GraphicsComponent,
    artifact: &LibraryArtifact,
    current_version: Option<&Version>,
) -> Option<CandidateComparison> {
    require_not_split_downgrade(component, artifact)?;
    require_compatible_graphics_api(component, artifact)?;
    require_version_compatible(
        component.technology(),
        current_version,
        artifact.release_version(),
    )?;

    compare_versions(current_version, artifact.release_version())
}

/// Prevents cross-API FSR replacements (e.g., offering a DX12 artifact to a Vulkan game).
fn require_compatible_graphics_api(
    component: &GraphicsComponent,
    artifact: &LibraryArtifact,
) -> Option<()> {
    if component.technology().family() != GraphicsTechnology::AmdFsr {
        return Some(());
    }

    let Some(artifact_api) = fsr::fsr_graphics_api(artifact.file_name()) else {
        return Some(()); // API-neutral artifact cannot produce a mismatch.
    };

    let component_has_conflicting_api = component
        .files()
        .iter()
        .filter_map(|f| f.path().file_name())
        .filter_map(fsr::fsr_graphics_api)
        .any(|api| api != artifact_api);

    if component_has_conflicting_api {
        None
    } else {
        Some(())
    }
}

/// Whether a unified single-file FSR 3.x backend may replace this component.
///
/// The deciding factor is the entry-point file. A component that still loads an FSR 3.1
/// entry point (`amd_fidelityfx_dx12.dll` or `amd_fidelityfx_vk.dll`) is **FSR 3.1
/// lineage** (pure FSR 3.1, or one we upgraded — the FSR 4 loader sits under that name):
/// it can always return to FSR 3.1, and the swap engine cleans up the FSR 4 members, so
/// a unified candidate is offered. A split set with **no** entry point is native FSR 4
/// (loads its own loader): there is no FSR 3 to return to, so a unified backend is
/// blocked — it would only strand the split members. Split → split (upgrades and FSR 4
/// updates) is always allowed.
fn require_not_split_downgrade(
    component: &GraphicsComponent,
    artifact: &LibraryArtifact,
) -> Option<()> {
    // A composed FSR package's primary file name is the upscaler (the split marker);
    // the unified FSR 3.x backend's is an entry point (`amd_fidelityfx_dx12.dll` or
    // `amd_fidelityfx_vk.dll`) — so the artifact side is exact even though a package's
    // member paths are virtual.
    let artifact_is_unified = !fsr::is_split_marker(artifact.file_name());
    if fsr::is_split_set(component.files())
        && !fsr::has_entry_point(component.files())
        && artifact_is_unified
    {
        return None;
    }
    Some(())
}

fn require_version_compatible(
    technology: GraphicsTechnology,
    current: Option<&Version>,
    candidate: Option<&Version>,
) -> Option<()> {
    let policy = CompatibilityPolicy::from(technology);
    if policy.is_compatible(current, candidate) {
        Some(())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_policy_always_compatible_allows_any_transition() {
        let policy = CompatibilityPolicy::AlwaysCompatible;
        let v1 = Version::parse("1.0.0").unwrap();
        let v2 = Version::parse("2.0.0").unwrap();

        assert!(policy.is_compatible(Some(&v1), Some(&v2)));
        assert!(policy.is_compatible(Some(&v2), Some(&v1)));
        assert!(policy.is_compatible(None, Some(&v1)));
        assert!(policy.is_compatible(Some(&v1), None));
        assert!(policy.is_compatible(None, None));
    }

    #[test]
    fn compatibility_policy_dlss_blocks_v1_to_v2_plus() {
        let policy = CompatibilityPolicy::DlssSuperResolution;
        let v1 = Version::parse("1.0.0").unwrap();
        let v2 = Version::parse("2.0.0").unwrap();
        let v3 = Version::parse("3.7.0").unwrap();

        assert!(!policy.is_compatible(Some(&v1), Some(&v2)));
        assert!(!policy.is_compatible(Some(&v1), Some(&v3)));
    }

    #[test]
    fn compatibility_policy_dlss_allows_v2_to_v3_and_back() {
        let policy = CompatibilityPolicy::DlssSuperResolution;
        let v2 = Version::parse("2.0.0").unwrap();
        let v3 = Version::parse("3.7.0").unwrap();

        assert!(policy.is_compatible(Some(&v2), Some(&v3)));
        assert!(policy.is_compatible(Some(&v3), Some(&v2)));
    }

    #[test]
    fn compatibility_policy_dlss_blocks_v2_to_v1() {
        let policy = CompatibilityPolicy::DlssSuperResolution;
        let v1 = Version::parse("1.0.0").unwrap();
        let v2 = Version::parse("2.0.0").unwrap();

        assert!(!policy.is_compatible(Some(&v2), Some(&v1)));
    }

    #[test]
    fn compatibility_policy_dlss_allows_unknown_versions() {
        let policy = CompatibilityPolicy::DlssSuperResolution;
        let v1 = Version::parse("1.0.0").unwrap();

        assert!(policy.is_compatible(None, Some(&v1)));
        assert!(policy.is_compatible(Some(&v1), None));
        assert!(policy.is_compatible(None, None));
    }

    #[test]
    fn require_version_compatible_blocks_v1_to_v2_for_dlss() {
        let v1 = Version::parse("1.0.0").unwrap();
        let v2 = Version::parse("2.0.0").unwrap();

        assert!(
            require_version_compatible(
                GraphicsTechnology::DlssSuperResolution,
                Some(&v1),
                Some(&v2),
            )
            .is_none()
        );
    }

    #[test]
    fn require_version_compatible_allows_v2_to_v3_for_dlss() {
        let v2 = Version::parse("2.0.0").unwrap();
        let v3 = Version::parse("3.7.0").unwrap();

        assert!(
            require_version_compatible(
                GraphicsTechnology::DlssSuperResolution,
                Some(&v2),
                Some(&v3),
            )
            .is_some()
        );
    }

    #[test]
    fn require_version_compatible_allows_any_transition_for_non_dlss() {
        let v1 = Version::parse("1.0.0").unwrap();
        let v2 = Version::parse("2.0.0").unwrap();

        assert!(
            require_version_compatible(
                GraphicsTechnology::DlssFrameGeneration,
                Some(&v1),
                Some(&v2),
            )
            .is_some()
        );
    }
}
