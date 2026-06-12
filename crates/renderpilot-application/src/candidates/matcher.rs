//! Replacement-candidate matching algorithm.
//!
//! Matches each detected component bundle against same-technology artifacts,
//! applying API/version/lineage compatibility rules, then builds, sorts, and
//! deduplicates the resulting [`ReplacementCandidate`] list. The data types it
//! produces live in [`super::dto`].

use std::collections::{HashMap, HashSet};

use renderpilot_domain::{
    fsr, ArtifactId, ComponentFile, GraphicsComponent, GraphicsTechnology, LibraryArtifact, Version,
};

use super::dto::{CandidateComparison, ComponentReplacementCandidates, ReplacementCandidate};

/// Context for candidate lookup that carries source metadata for artifacts.
#[derive(Debug, Clone)]
pub struct CandidateContext {
    downloaded_ids: HashSet<ArtifactId>,
    manifest_entry_ids: HashMap<ArtifactId, String>,
    debug_entry_ids: HashSet<String>,
}

impl CandidateContext {
    /// Creates a new candidate context from the given lookup tables.
    pub fn new(
        downloaded_ids: HashSet<ArtifactId>,
        manifest_entry_ids: HashMap<ArtifactId, String>,
        debug_entry_ids: HashSet<String>,
    ) -> Self {
        Self {
            downloaded_ids,
            manifest_entry_ids,
            debug_entry_ids,
        }
    }

    /// Returns an empty context with no source metadata.
    pub fn empty() -> Self {
        Self {
            downloaded_ids: HashSet::new(),
            manifest_entry_ids: HashMap::new(),
            debug_entry_ids: HashSet::new(),
        }
    }

    /// Returns true if the given manifest entry id belongs to a debug build.
    pub fn is_debug_entry(&self, entry_id: &str) -> bool {
        self.debug_entry_ids.contains(entry_id)
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

        let current_version = primary_component_version(component);
        let mut candidates = component_artifacts
            .iter()
            .filter_map(|artifact| {
                // Ignore artifacts scanned from the exact same game.
                // Such artifacts represent the game's own mutable file paths.
                // If the game was modified (e.g. rolled back), the artifact's
                // stored SHA-256 no longer matches its path, leading to swap errors.
                if artifact.source_game_id() == Some(component.game_id()) {
                    return None;
                }

                let comparison = candidate_comparison(component, artifact, current_version)?;
                let is_downloaded = context.downloaded_ids.contains(artifact.id());
                let entry_id = context.manifest_entry_ids.get(artifact.id()).cloned();
                let is_debug = entry_id
                    .as_ref()
                    .is_some_and(|id| context.is_debug_entry(id));
                Some(ReplacementCandidate::new(
                    artifact,
                    comparison,
                    is_downloaded,
                    entry_id,
                    is_debug,
                ))
            })
            .collect::<Vec<_>>();

        if candidates.is_empty() {
            continue;
        }

        candidates.sort_by(|left, right| left.ordering_key().cmp(&right.ordering_key()));
        let candidates = deduplicate_candidates(candidates);
        groups.push(ComponentReplacementCandidates::new(component, candidates));
    }

    groups.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
    groups
}

/// Groups artifacts by their exact technology.
fn group_artifacts_by_technology(
    artifacts: &[LibraryArtifact],
) -> HashMap<GraphicsTechnology, Vec<&LibraryArtifact>> {
    let mut grouped = HashMap::<GraphicsTechnology, Vec<&LibraryArtifact>>::new();

    for artifact in artifacts {
        grouped
            .entry(artifact.technology())
            .or_default()
            .push(artifact);
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

/// Collapses candidates that are the same artifact. The artifact id is the
/// bundle's content identity (`ArtifactId::for_bundle`), so distinct bundles that
/// merely share a primary DLL keep their separate entries, while the same content
/// observed twice (e.g. two manifest entries) collapses to one.
fn deduplicate_candidates(candidates: Vec<ReplacementCandidate>) -> Vec<ReplacementCandidate> {
    let mut seen_ids = HashSet::<ArtifactId>::new();
    // Secondary dedup key: collapses manifest entries whose DLL is byte-identical
    // to a scanned artifact (same file name + version + build type) so the user
    // does not see the same logical version twice in the list.
    let mut seen_version_keys = HashSet::<(String, Version, bool)>::new();
    let mut deduplicated = Vec::with_capacity(candidates.len());

    for candidate in candidates {
        if !seen_ids.insert(candidate.artifact_id().clone()) {
            continue;
        }
        let version_is_new = match candidate.version() {
            Some(version) => seen_version_keys.insert((
                candidate.file_name().to_owned(),
                version.clone(),
                candidate.is_debug(),
            )),
            None => true,
        };
        if version_is_new {
            deduplicated.push(candidate);
        }
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
                    (current.major() == 1) == (candidate.major() == 1)
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
    require_version_compatible(component.technology(), current_version, artifact.version())?;

    compare_versions(current_version, artifact.version())
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

/// The version the component is compared by: the FSR-aware representative
/// (entry point vs upscaler per release cohesion), not blindly `files()[0]` —
/// components persisted before the representative-ranking fix may still carry
/// an arbitrary stored order.
fn primary_component_version(component: &GraphicsComponent) -> Option<&Version> {
    fsr::version_representative(component.files()).and_then(ComponentFile::version)
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

        assert!(require_version_compatible(
            GraphicsTechnology::DlssSuperResolution,
            Some(&v1),
            Some(&v2),
        )
        .is_none());
    }

    #[test]
    fn require_version_compatible_allows_v2_to_v3_for_dlss() {
        let v2 = Version::parse("2.0.0").unwrap();
        let v3 = Version::parse("3.7.0").unwrap();

        assert!(require_version_compatible(
            GraphicsTechnology::DlssSuperResolution,
            Some(&v2),
            Some(&v3),
        )
        .is_some());
    }

    #[test]
    fn require_version_compatible_allows_any_transition_for_non_dlss() {
        let v1 = Version::parse("1.0.0").unwrap();
        let v2 = Version::parse("2.0.0").unwrap();

        assert!(require_version_compatible(
            GraphicsTechnology::DlssFrameGeneration,
            Some(&v1),
            Some(&v2),
        )
        .is_some());
    }
}
