//! Replacement-candidate matching algorithm.
//!
//! Matches each detected component bundle against same-technology artifacts,
//! applying API/version/lineage compatibility rules, then builds, sorts, and
//! deduplicates the resulting [`ReplacementCandidate`] list. The data types it
//! produces live in [`super::dto`].

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use renderpilot_domain::{
    ArtifactId, ArtifactTrustLevel, CatalogPackageAvailability, LibraryArtifact, LibraryComponent,
    LibraryTechnology, Version, fsr,
};

use super::automatic::coordinate_selection;
use super::dto::{
    ActiveCatalogPackage, CandidateComparison, CandidateSelection, CatalogCandidatePackage,
    ComponentReplacementCandidates, InstalledReleaseState, ReplacementCandidate,
};
use super::identity::{IntrinsicPackageIdentity, ResolvedTransitionIdentity};
use super::ordering::sort_and_deduplicate;
use super::xiph_matching;
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

    /// Returns catalog metadata only when its immutable receipt is canonical.
    /// A malformed catalog-backed artifact is not silently reclassified as an
    /// ordinary local artifact: doing so would let invalid provenance bypass
    /// the catalog eligibility boundary.
    fn catalog_package(
        &self,
        artifact: &LibraryArtifact,
    ) -> Result<Option<CatalogCandidatePackage>, ()> {
        if let Some(active) = self.active_catalog.get(artifact.id()) {
            return Ok(Some(CatalogCandidatePackage::from_active(
                active,
                CatalogPackageAvailability::Available,
            )));
        }
        let Some(receipt) = artifact.metadata().catalog_package_receipt() else {
            // `CatalogDownloaded` is the typed provenance boundary: every
            // artifact materialized from the validated catalog must retain its
            // immutable receipt. Never let a malformed cache entry become an
            // ordinary local candidate merely because its receipt is absent.
            if artifact.trust_level() == ArtifactTrustLevel::CatalogDownloaded {
                return Err(());
            }
            return Ok(None);
        };
        CatalogCandidatePackage::from_receipt(receipt, CatalogPackageAvailability::LocalOnly, false)
            .map(Some)
            .ok_or(())
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
    find_replacement_candidate_selection_with_lookup(components, artifacts, &lookup, context)
        .into_groups()
}

/// Finds candidates plus coordinated multi-component manual options.
#[must_use]
pub fn find_replacement_candidate_selection(
    components: &[LibraryComponent],
    artifacts: &[LibraryArtifact],
    context: &CandidateContext,
) -> CandidateSelection {
    let lookup = CandidateArtifactLookup::build(artifacts);
    find_replacement_candidate_selection_with_lookup(components, artifacts, &lookup, context)
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
    find_replacement_candidate_selection_with_lookup(
        components,
        &index.artifacts,
        &index.lookup,
        context,
    )
    .into_groups()
}

/// Matches an indexed artifact universe and preserves coordinated options.
#[must_use]
pub fn find_replacement_candidate_selection_indexed(
    components: &[LibraryComponent],
    index: &CandidateArtifactIndex,
    context: &CandidateContext,
) -> CandidateSelection {
    find_replacement_candidate_selection_with_lookup(
        components,
        &index.artifacts,
        &index.lookup,
        context,
    )
}

fn find_replacement_candidate_selection_with_lookup(
    components: &[LibraryComponent],
    artifacts: &[LibraryArtifact],
    lookup: &CandidateArtifactLookup,
    context: &CandidateContext,
) -> CandidateSelection {
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
        let has_vendor_xiph_alias = xiph_matching::component_has_vendor_alias(component);
        let mut installed_vendor_xiph_catalog_package = false;
        let mut candidates = Vec::new();
        for indexed in component_artifacts {
            let artifact = &artifacts[indexed.artifact_index];
            let CandidateEvaluation {
                candidate,
                installed_vendor_catalog_package: matches_installed_vendor_catalog_package,
            } = evaluate_replacement_candidate(
                component,
                artifact,
                indexed,
                current_version,
                has_vendor_xiph_alias,
                context,
            );
            installed_vendor_xiph_catalog_package |= matches_installed_vendor_catalog_package;
            if let Some(candidate) = candidate {
                candidates.push(candidate);
            }
        }

        if candidates.is_empty() && !installed_vendor_xiph_catalog_package {
            continue;
        }

        let candidates = sort_and_deduplicate(candidates);
        let Some(group) =
            ComponentReplacementCandidates::new(component, installed_release, candidates)
        else {
            continue;
        };
        groups.push(group);
    }

    groups.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
    let selection = coordinate_selection(components, &groups);
    for group in &mut groups {
        group.set_automatic_candidate_artifact_id(
            selection
                .automatic_by_component
                .get(group.component_id())
                .cloned(),
        );
    }
    CandidateSelection::new(groups, selection.streamline_options)
}

/// Result of evaluating one candidate artifact for a component.
#[derive(Default)]
struct CandidateEvaluation {
    candidate: Option<ReplacementCandidate>,
    installed_vendor_catalog_package: bool,
}

/// Classifies one artifact without mutating component-level candidate state.
fn evaluate_replacement_candidate(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
    indexed: &IndexedArtifact,
    current_version: Option<&Version>,
    has_vendor_xiph_alias: bool,
    context: &CandidateContext,
) -> CandidateEvaluation {
    // Ignore artifacts scanned from the exact same game. Such artifacts
    // represent mutable game paths, whose stored digest can diverge after a
    // rollback or another local change.
    if artifact.source_game_id() == Some(component.game_id()) {
        return CandidateEvaluation::default();
    }

    let compatibility = if component.technology() == LibraryTechnology::XiphVorbis {
        // Candidate cards may establish semantic compatibility, but cannot
        // manufacture the request-scoped full-root import proof required to
        // resolve a vendor alias transition.
        crate::compatibility::ensure_candidate_compatible_without_alias_proof(component, artifact)
    } else {
        ensure_replacement_compatible(component, artifact, &context.target_profile)
    };
    if !matches!(
        compatibility,
        Ok(()) | Err(SwapCompatibilityError::D3d12ExecutableRepairRequired)
    ) {
        return CandidateEvaluation::default();
    }

    let Ok(d3d12_executable_action) =
        replacement_executable_action(artifact, &context.target_profile)
    else {
        return CandidateEvaluation::default();
    };
    let resolved_identity = if has_vendor_xiph_alias {
        None
    } else {
        match ResolvedTransitionIdentity::for_replacement(component, artifact) {
            Ok(identity) => identity,
            Err(_) => return CandidateEvaluation::default(),
        }
    };
    // An unresolved identity is not safe to present for ordinary transitions.
    // Vendor Xiph aliases are the deliberate exception: their transition can
    // only be resolved later, after orchestration has supplied external-import
    // proof.
    let is_installed_transition = match resolved_identity.as_ref() {
        Some(identity) => identity.installed_projection(component).as_ref() == Some(identity),
        None => !has_vendor_xiph_alias,
    };
    if is_installed_transition {
        return CandidateEvaluation::default();
    }

    let Ok(catalog_package) = context.catalog_package(artifact) else {
        return CandidateEvaluation::default();
    };
    if has_vendor_xiph_alias
        && catalog_package.is_some()
        && xiph_matching::vendor_catalog_content_matches_for_alias(component, artifact)
    {
        return CandidateEvaluation {
            installed_vendor_catalog_package: true,
            ..CandidateEvaluation::default()
        };
    }

    let Some(comparison) = candidate_comparison(
        component,
        artifact,
        current_version,
        catalog_package.as_ref(),
    ) else {
        return CandidateEvaluation::default();
    };
    let is_downloaded = context.downloaded_ids.contains(artifact.id());
    CandidateEvaluation {
        candidate: Some(
            ReplacementCandidate::new(
                artifact,
                comparison,
                is_downloaded,
                catalog_package,
                indexed.intrinsic_identity.clone(),
                resolved_identity,
            )
            .with_d3d12_executable_action(d3d12_executable_action),
        ),
        installed_vendor_catalog_package: false,
    }
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
            let catalog_package = context.catalog_package(artifact).ok()??;
            Some((artifact, catalog_package))
        })
        .filter(|artifact| catalog_artifact_matches_component(component, artifact.0))
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

fn catalog_artifact_matches_component(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
) -> bool {
    if let Ok(Some(identity)) = ResolvedTransitionIdentity::for_replacement(component, artifact) {
        return identity.installed_projection(component).as_ref() == Some(&identity);
    }
    xiph_matching::vendor_catalog_content_matches(component, artifact)
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
    catalog_package: Option<&CatalogCandidatePackage>,
) -> Option<CandidateComparison> {
    if component.technology() == LibraryTechnology::XiphVorbis {
        return Some(xiph_matching::candidate_comparison(
            component,
            catalog_package,
        ));
    }
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
    use std::collections::BTreeMap;

    use super::super::xiph_matching::candidate_comparison as xiph_candidate_comparison;
    use renderpilot_domain::{
        ComponentFile, ComponentId, ComponentKind, GameId, PackageRelease, PackageVersion, PathRef,
        ReleaseChannel, Swappability,
    };

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

    #[test]
    fn xiph_versions_use_componentwise_partial_order_and_singleton_projection() {
        let component = |files: &[(&str, Option<&str>)]| {
            files.iter().fold(
                LibraryComponent::new(
                    ComponentId::new("component:xiph").expect("id"),
                    GameId::new("game:xiph").expect("game"),
                    ComponentKind::NativeLibrary,
                    LibraryTechnology::XiphVorbis,
                    Swappability::BundleOnly,
                ),
                |component, (name, version)| {
                    let mut file =
                        ComponentFile::new(PathRef::new(format!("C:/Game/{name}")).expect("path"));
                    if let Some(version) = version {
                        file = file.with_version(Version::parse(*version).expect("version"));
                    }
                    component.with_file(file)
                },
            )
        };
        let package = |vorbis: &str, ogg: &str| {
            CatalogCandidatePackage::new(
                "xiph:test",
                PackageRelease {
                    version: PackageVersion::parse(vorbis).expect("version"),
                    channel: ReleaseChannel::Stable,
                    label: None,
                    components: BTreeMap::from([
                        (
                            "ogg".to_owned(),
                            PackageVersion::parse(ogg).expect("Ogg version"),
                        ),
                        (
                            "vorbis".to_owned(),
                            PackageVersion::parse(vorbis).expect("Vorbis version"),
                        ),
                    ]),
                },
                CatalogPackageAvailability::Available,
                true,
            )
        };
        let pair = component(&[("vorbis.dll", Some("1.3.7")), ("ogg.dll", Some("1.3.5"))]);

        assert_eq!(
            xiph_candidate_comparison(&pair, Some(&package("1.3.7", "1.3.6"))),
            CandidateComparison::NewerVersion
        );
        assert_eq!(
            xiph_candidate_comparison(&pair, Some(&package("1.3.6", "1.3.6"))),
            CandidateComparison::MixedVersion
        );
        assert_eq!(
            xiph_candidate_comparison(&pair, Some(&package("1.3.7", "1.3.5"))),
            CandidateComparison::EqualVersion
        );

        let ogg = component(&[("ogg.dll", Some("1.3.5"))]);
        assert_eq!(
            xiph_candidate_comparison(&ogg, Some(&package("0.1.0", "1.3.6"))),
            CandidateComparison::NewerVersion,
            "Vorbis is outside the Ogg singleton projection"
        );
        let unknown = component(&[("ogg.dll", None)]);
        assert_eq!(
            xiph_candidate_comparison(&unknown, Some(&package("1.3.7", "1.3.6"))),
            CandidateComparison::UnknownVersion
        );

        let vendor = component(&[
            ("vorbis_vs2010_x64_rwdi.dll", Some("1.3.7")),
            ("ogg_vs2010_x64_rwdi.dll", Some("1.3.5")),
        ]);
        assert_eq!(
            xiph_candidate_comparison(&vendor, Some(&package("1.3.7", "1.3.6"))),
            CandidateComparison::NewerVersion,
            "runtime vendor aliases retain their semantic release axes without broadening catalog names"
        );

        let embedded = component(&[
            ("libvorbisfile.dll", Some("1.3.6")),
            ("libvorbis.dll", Some("1.3.6")),
        ]);
        assert_eq!(
            xiph_candidate_comparison(&embedded, Some(&package("1.3.7", "1.3.6"))),
            CandidateComparison::UnknownVersion,
            "an unobservable embedded Ogg version must disable unattended comparison"
        );
    }
}
