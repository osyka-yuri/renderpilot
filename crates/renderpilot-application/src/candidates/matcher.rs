//! Replacement-candidate matching algorithm.
//!
//! Matches each detected component bundle against same-technology artifacts,
//! applying API/version/lineage compatibility rules, then builds, sorts, and
//! deduplicates the resulting [`ReplacementCandidate`] list. The data types it
//! produces live in [`super::dto`].

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use renderpilot_domain::{
    ArtifactId, CatalogPackageAvailability, LibraryArtifact, LibraryComponent, LibraryTechnology,
    PackageVersion, Version, fsr,
};

use super::dto::{
    ActiveCatalogPackage, CandidateComparison, CatalogCandidatePackage,
    ComponentReplacementCandidates, InstalledReleaseState, ReplacementCandidate,
};
use super::identity::{IntrinsicPackageIdentity, ResolvedTransitionIdentity};
use crate::{
    SwapCompatibilityError, SwapTargetProfile, ensure_replacement_compatible,
    replacement_executable_action,
};

/// Context for candidate lookup that carries source metadata for artifacts.
#[derive(Debug, Clone)]
pub struct CandidateContext {
    downloaded_ids: Arc<HashSet<ArtifactId>>,
    active_catalog: Arc<HashMap<ArtifactId, ActiveCatalogPackage>>,
    target_profile: SwapTargetProfile,
}

impl CandidateContext {
    /// Creates a new candidate context from the given lookup tables.
    pub fn new(
        downloaded_ids: HashSet<ArtifactId>,
        active_catalog: HashMap<ArtifactId, ActiveCatalogPackage>,
    ) -> Self {
        Self {
            downloaded_ids: Arc::new(downloaded_ids),
            active_catalog: Arc::new(active_catalog),
            target_profile: SwapTargetProfile::default(),
        }
    }

    /// Returns an empty context with no source metadata.
    pub fn empty() -> Self {
        Self {
            downloaded_ids: Arc::new(HashSet::new()),
            active_catalog: Arc::new(HashMap::new()),
            target_profile: SwapTargetProfile::default(),
        }
    }

    /// Attaches fresh executable facts used by Microsoft runtime policies.
    #[must_use]
    pub fn with_target_profile(&self, profile: SwapTargetProfile) -> Self {
        Self {
            downloaded_ids: Arc::clone(&self.downloaded_ids),
            active_catalog: Arc::clone(&self.active_catalog),
            target_profile: profile,
        }
    }

    fn catalog_package(&self, artifact: &LibraryArtifact) -> Option<CatalogCandidatePackage> {
        if let Some(active) = self.active_catalog.get(artifact.id()) {
            return Some(CatalogCandidatePackage::new(
                active.package_id(),
                active.release().clone(),
                CatalogPackageAvailability::Available,
                active.automatic_selection_allowed(),
            ));
        }
        let receipt = artifact.metadata().catalog_package_receipt()?;
        Some(CatalogCandidatePackage::new(
            receipt.package_id.clone(),
            receipt.release.clone(),
            CatalogPackageAvailability::LocalOnly,
            false,
        ))
    }
}

/// Finds replacement candidates for the detected components of one game.
///
/// Matching is per *component bundle*, not per file: a component is matched
/// against artifacts of the same exact technology whose bundle content differs
/// from what is currently installed. A cohesive FSR component still uses
/// [`LibraryTechnology::AmdFsr`], so an FSR 3 (single-file) component can still
/// be replaced by an FSR 4 (three-file) artifact.
#[must_use]
pub fn find_replacement_candidates(
    components: &[LibraryComponent],
    artifacts: &[LibraryArtifact],
    context: &CandidateContext,
) -> Vec<ComponentReplacementCandidates> {
    let lookup = CandidateArtifactLookup::build(artifacts);
    find_replacement_candidates_with_lookup(components, artifacts, &lookup, context)
}

/// Immutable artifact universe indexed once for repeated per-game matching.
#[derive(Debug)]
pub struct CandidateArtifactIndex {
    artifacts: Vec<LibraryArtifact>,
    lookup: CandidateArtifactLookup,
}

impl CandidateArtifactIndex {
    /// Builds one reusable technology and intrinsic-identity index.
    #[must_use]
    pub fn new(artifacts: Vec<LibraryArtifact>) -> Self {
        let lookup = CandidateArtifactLookup::build(&artifacts);
        Self { artifacts, lookup }
    }

    /// Returns the authoritative artifacts in stable universe order.
    #[must_use]
    pub fn artifacts(&self) -> &[LibraryArtifact] {
        &self.artifacts
    }
}

/// Matches components against an already-indexed artifact universe.
#[must_use]
pub fn find_replacement_candidates_indexed(
    components: &[LibraryComponent],
    index: &CandidateArtifactIndex,
    context: &CandidateContext,
) -> Vec<ComponentReplacementCandidates> {
    find_replacement_candidates_with_lookup(components, &index.artifacts, &index.lookup, context)
}

fn find_replacement_candidates_with_lookup(
    components: &[LibraryComponent],
    artifacts: &[LibraryArtifact],
    lookup: &CandidateArtifactLookup,
    context: &CandidateContext,
) -> Vec<ComponentReplacementCandidates> {
    let mut groups = Vec::new();

    for component in components {
        if component.files().is_empty() {
            continue;
        }

        let Some(component_artifacts) = lookup.by_technology.get(&component.technology()) else {
            continue;
        };

        let installed_release =
            installed_release_state(component, artifacts, component_artifacts, context);
        let current_version = installed_release.known_version();
        let candidates = component_artifacts
            .iter()
            .filter_map(|indexed| {
                let artifact = &artifacts[indexed.artifact_index];
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
                Some(
                    ReplacementCandidate::new(
                        artifact,
                        comparison,
                        is_downloaded,
                        context.catalog_package(artifact),
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
        let Some(group) =
            ComponentReplacementCandidates::new(component, installed_release, candidates)
        else {
            continue;
        };
        groups.push(group);
    }

    groups.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
    groups
}

fn installed_release_state(
    component: &LibraryComponent,
    universe: &[LibraryArtifact],
    artifacts: &[IndexedArtifact],
    context: &CandidateContext,
) -> InstalledReleaseState {
    if let Some(release) = installed_catalog_release(component, universe, artifacts, context) {
        return release;
    }
    if component.technology() == LibraryTechnology::OpenVr {
        return InstalledReleaseState::Unknown;
    }
    InstalledReleaseState::from_version_report(super::dto::component_version_state(component))
}

fn installed_catalog_release(
    component: &LibraryComponent,
    universe: &[LibraryArtifact],
    artifacts: &[IndexedArtifact],
    context: &CandidateContext,
) -> Option<InstalledReleaseState> {
    let matched = artifacts
        .iter()
        .map(|indexed| &universe[indexed.artifact_index])
        .filter_map(|artifact| {
            let catalog_package = context.catalog_package(artifact)?;
            Some((artifact, catalog_package))
        })
        .filter(|artifact| {
            let Ok(Some(identity)) =
                ResolvedTransitionIdentity::for_replacement(component, artifact.0)
            else {
                return false;
            };
            identity.installed_projection(component).as_ref() == Some(&identity)
        })
        .map(|(artifact, catalog_package)| {
            (
                catalog_package.release().version.clone(),
                artifact.version().cloned(),
                catalog_package.package_id().to_owned(),
                catalog_package.release().clone(),
            )
        })
        .max_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
                .then_with(|| right.2.cmp(&left.2))
        });
    matched.map(|(_, version, _, release)| InstalledReleaseState::known_catalog(version, release))
}

#[derive(Debug, Clone)]
struct IndexedArtifact {
    artifact_index: usize,
    intrinsic_identity: Option<IntrinsicPackageIdentity>,
}

#[derive(Debug)]
struct CandidateArtifactLookup {
    by_technology: HashMap<LibraryTechnology, Vec<IndexedArtifact>>,
}

impl CandidateArtifactLookup {
    fn build(artifacts: &[LibraryArtifact]) -> Self {
        let mut by_technology = HashMap::<LibraryTechnology, Vec<IndexedArtifact>>::new();

        for (artifact_index, artifact) in artifacts.iter().enumerate() {
            by_technology
                .entry(artifact.technology())
                .or_default()
                .push(IndexedArtifact {
                    artifact_index,
                    intrinsic_identity: IntrinsicPackageIdentity::for_artifact(artifact),
                });
        }

        Self { by_technology }
    }
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
    candidates.sort_by(|left, right| candidate_sort_key(left).cmp(&candidate_sort_key(right)));

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

impl From<LibraryTechnology> for CompatibilityPolicy {
    fn from(technology: LibraryTechnology) -> Self {
        match technology {
            LibraryTechnology::DlssSuperResolution => Self::DlssSuperResolution,
            _ => Self::AlwaysCompatible,
        }
    }
}

fn candidate_comparison(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
    current_version: Option<&Version>,
) -> Option<CandidateComparison> {
    require_not_split_downgrade(component, artifact)?;
    require_compatible_graphics_api(component, artifact)?;
    require_version_compatible(component.technology(), current_version, artifact.version())?;

    compare_versions(current_version, artifact.version())
}

/// Prevents cross-API FSR replacements (e.g., offering a DX12 artifact to a Vulkan game).
fn require_compatible_graphics_api(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
) -> Option<()> {
    if component.technology().family() != LibraryTechnology::AmdFsr {
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
    component: &LibraryComponent,
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
    technology: LibraryTechnology,
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
                LibraryTechnology::DlssSuperResolution,
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
                LibraryTechnology::DlssSuperResolution,
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
                LibraryTechnology::DlssFrameGeneration,
                Some(&v1),
                Some(&v2),
            )
            .is_some()
        );
    }
}
