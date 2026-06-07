use std::collections::{HashMap, HashSet};

use renderpilot_domain::{
    ArtifactId, ComponentFile, ComponentId, GameId, GraphicsComponent, GraphicsTechnology,
    LibraryArtifact, PathRef, Version,
};

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

                let is_downloaded = context.downloaded_ids.contains(artifact.id());
                let entry_id = context.manifest_entry_ids.get(artifact.id()).cloned();
                let is_debug = entry_id
                    .as_ref()
                    .is_some_and(|id| context.is_debug_entry(id));
                ReplacementCandidate::for_component(
                    component,
                    artifact,
                    is_downloaded,
                    entry_id,
                    is_debug,
                )
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

/// Replacement candidates applicable to one detected component (bundle).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentReplacementCandidates {
    component_id: ComponentId,
    game_id: GameId,
    technology: GraphicsTechnology,
    file_path: PathRef,
    current_version: Option<Version>,
    candidates: Vec<ReplacementCandidate>,
}

impl ComponentReplacementCandidates {
    /// Creates a per-component candidate group.
    ///
    /// `current_version` describes the component's version representative, while
    /// `file_path` is the user-facing display path. For cohesive FSR these may be
    /// different files: the upscaler carries the FSR 4.x version, but the dx12
    /// entry point is still the path the user expects to see.
    pub fn new(component: &GraphicsComponent, candidates: Vec<ReplacementCandidate>) -> Self {
        let representative = component
            .files()
            .first()
            .expect("component candidate group requires at least one file");
        let display = crate::fsr::display_component_file(component.files())
            .expect("component candidate group requires at least one display file");

        Self {
            component_id: component.id().clone(),
            game_id: component.game_id().clone(),
            technology: component.technology(),
            file_path: display.path().clone(),
            current_version: representative.version().cloned(),
            candidates,
        }
    }

    fn sort_key(&self) -> (&'static str, &str, &str) {
        (
            self.technology.as_slug(),
            self.game_id.as_str(),
            self.file_path.as_str(),
        )
    }

    /// Returns the detected component identifier.
    pub fn component_id(&self) -> &ComponentId {
        &self.component_id
    }

    /// Returns the game that owns the component file.
    pub fn game_id(&self) -> &GameId {
        &self.game_id
    }

    /// Returns the graphics technology of the component.
    pub fn technology(&self) -> GraphicsTechnology {
        self.technology
    }

    /// Returns the detected file path of the component.
    pub fn file_path(&self) -> &PathRef {
        &self.file_path
    }

    /// Returns the currently detected version, when available.
    pub fn current_version(&self) -> Option<&Version> {
        self.current_version.as_ref()
    }

    /// Returns replacement candidates sorted by best automatic match first.
    pub fn candidates(&self) -> &[ReplacementCandidate] {
        &self.candidates
    }
}

/// One replacement artifact that can be applied to a component file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacementCandidate {
    artifact_id: ArtifactId,
    file_name: String,
    file_path: Option<PathRef>,
    version: Option<Version>,
    sha256: String,
    source_game_id: Option<GameId>,
    comparison: CandidateComparison,
    manifest_entry_id: Option<String>,
    is_downloaded: bool,
    is_debug: bool,
}

impl ReplacementCandidate {
    fn for_component(
        component: &GraphicsComponent,
        artifact: &LibraryArtifact,
        is_downloaded: bool,
        manifest_entry_id: Option<String>,
        is_debug: bool,
    ) -> Option<Self> {
        let comparison = candidate_comparison(component, artifact)?;

        Some(Self {
            artifact_id: artifact.id().clone(),
            file_name: artifact.file_name().to_owned(),
            file_path: if is_downloaded {
                Some(artifact.path().clone())
            } else {
                None
            },
            version: artifact.version().cloned(),
            sha256: artifact.sha256().as_str().to_owned(),
            source_game_id: artifact.source_game_id().cloned(),
            comparison,
            manifest_entry_id,
            is_downloaded,
            is_debug,
        })
    }

    fn ordering_key(
        &self,
    ) -> (
        u8,
        std::cmp::Reverse<Option<Version>>,
        std::cmp::Reverse<bool>,
        &str,
        &str,
    ) {
        (
            self.comparison.priority(),
            std::cmp::Reverse(self.version.clone()),
            std::cmp::Reverse(self.is_downloaded),
            self.file_name.as_str(),
            self.file_path.as_ref().map(|p| p.as_str()).unwrap_or(""),
        )
    }

    /// Returns the candidate artifact identifier.
    pub fn artifact_id(&self) -> &ArtifactId {
        &self.artifact_id
    }

    /// Returns the candidate file name.
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// Returns the candidate file path when the artifact is materialized locally.
    pub fn file_path(&self) -> Option<&PathRef> {
        self.file_path.as_ref()
    }

    /// Returns true if this artifact was downloaded.
    pub fn is_downloaded(&self) -> bool {
        self.is_downloaded
    }

    /// Returns true if this artifact is known to be a debug build.
    pub fn is_debug(&self) -> bool {
        self.is_debug
    }

    /// Returns the SHA256 hash of the artifact.
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// Returns the candidate version, when known.
    pub fn version(&self) -> Option<&Version> {
        self.version.as_ref()
    }

    /// Returns the source game where the candidate was observed, when known.
    pub fn source_game_id(&self) -> Option<&GameId> {
        self.source_game_id.as_ref()
    }

    /// Returns how confidently the candidate can be compared to the current component.
    pub fn comparison(&self) -> CandidateComparison {
        self.comparison
    }

    /// Returns the manifest entry id if this candidate is from a manifest entry.
    pub fn manifest_entry_id(&self) -> Option<&str> {
        self.manifest_entry_id.as_deref()
    }
}

/// Result of comparing a candidate artifact to the currently installed component file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CandidateComparison {
    /// Both versions were known and the candidate is newer than the current file.
    NewerVersion,
    /// At least one side has no version, so the candidate can only be reviewed manually.
    UnknownVersion,
    /// Both versions were known and the candidate is older than the current file.
    OlderVersion,
}

impl CandidateComparison {
    /// Returns the stable CLI text for this comparison result.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NewerVersion => "newer_version",
            Self::UnknownVersion => "unknown_version",
            Self::OlderVersion => "older_version",
        }
    }

    const fn priority(self) -> u8 {
        match self {
            Self::NewerVersion => 0,
            Self::UnknownVersion => 1,
            Self::OlderVersion => 2,
        }
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
        if !seen_ids.insert(candidate.artifact_id.clone()) {
            continue;
        }
        let version_is_new = match &candidate.version {
            Some(version) => seen_version_keys.insert((
                candidate.file_name.clone(),
                version.clone(),
                candidate.is_debug,
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
) -> Option<CandidateComparison> {
    require_not_split_downgrade(component, artifact)?;
    require_compatible_graphics_api(component, artifact)?;
    require_version_compatible(
        component.technology(),
        primary_component_version(component),
        artifact.version(),
    )?;

    compare_versions(primary_component_version(component), artifact.version())
}

/// Prevents cross-API FSR replacements (e.g., offering a DX12 artifact to a Vulkan game).
fn require_compatible_graphics_api(
    component: &GraphicsComponent,
    artifact: &LibraryArtifact,
) -> Option<()> {
    if component.technology().family() != GraphicsTechnology::AmdFsr {
        return Some(());
    }

    let Some(artifact_api) = crate::fsr::fsr_graphics_api(artifact.file_name()) else {
        return Some(()); // API-neutral artifact cannot produce a mismatch.
    };

    let component_has_conflicting_api = component
        .files()
        .iter()
        .filter_map(|f| f.path().file_name())
        .filter_map(crate::fsr::fsr_graphics_api)
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
    let artifact_is_unified = !crate::fsr::is_split_marker(artifact.file_name());
    if crate::fsr::is_split_set(component.files())
        && !crate::fsr::has_entry_point(component.files())
        && artifact_is_unified
    {
        return None;
    }
    Some(())
}

fn primary_component_version(component: &GraphicsComponent) -> Option<&Version> {
    component.files().first().and_then(ComponentFile::version)
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
    use renderpilot_domain::{
        ArtifactId, ArtifactTrustLevel, ComponentFile, ComponentId, ComponentKind, GameId,
        GraphicsComponent, GraphicsTechnology, LibraryArtifact, PathRef, Sha256Hash, Swappability,
        Version,
    };

    use super::{
        find_replacement_candidates, require_version_compatible, CandidateComparison,
        CandidateContext, CompatibilityPolicy, ComponentReplacementCandidates,
    };

    #[test]
    fn selects_only_same_technology_candidates() {
        let component = sample_component(
            "component:game-a:dlss",
            "game:a",
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            Some("3.5.0"),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "C:/Games/GameA/nvngx_dlss.dll",
        );

        let sr_candidate = sample_artifact(
            "artifact:sr-3.7",
            GraphicsTechnology::DlssSuperResolution,
            Some("3.7.0"),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "C:/Games/GameB/nvngx_dlss.dll",
            Some("game:b"),
        );
        let fg_candidate = sample_artifact(
            "artifact:fg-3.7",
            GraphicsTechnology::DlssFrameGeneration,
            Some("3.7.0"),
            "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            "C:/Games/GameB/nvngx_dlssg.dll",
            Some("game:b"),
        );
        let rr_candidate = sample_artifact(
            "artifact:rr-3.7",
            GraphicsTechnology::DlssRayReconstruction,
            Some("3.7.0"),
            "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
            "C:/Games/GameB/nvngx_dlssd.dll",
            Some("game:b"),
        );

        let groups = find_test_candidates(
            &[component],
            &[
                sr_candidate.clone(),
                fg_candidate.clone(),
                rr_candidate.clone(),
            ],
        );

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].candidates().len(), 1);
        assert_eq!(groups[0].candidates()[0].artifact_id(), sr_candidate.id());
        assert_eq!(
            groups[0].candidates()[0].comparison(),
            CandidateComparison::NewerVersion
        );
    }

    #[test]
    fn dlss_v1_is_incompatible_with_v2_and_ignored() {
        let component = sample_component(
            "component:game-a:dlss",
            "game:a",
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            Some("1.0.0"),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "C:/Games/GameA/nvngx_dlss.dll",
        );
        let v2_artifact = sample_artifact(
            "artifact:dlss-v2",
            GraphicsTechnology::DlssSuperResolution,
            Some("2.0.0"),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "C:/Games/GameB/nvngx_dlss.dll",
            Some("game:b"),
        );
        let v1_artifact = sample_artifact(
            "artifact:dlss-v1-other",
            GraphicsTechnology::DlssSuperResolution,
            Some("1.5.0"),
            "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            "C:/Games/GameC/nvngx_dlss.dll",
            Some("game:c"),
        );

        let groups = find_test_candidates(&[component], &[v2_artifact, v1_artifact]);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].candidates().len(), 1);
        assert_eq!(
            groups[0].candidates()[0].artifact_id().as_str(),
            "artifact:dlss-v1-other"
        );
    }

    #[test]
    fn dlss_v2_is_compatible_with_v3_only() {
        let component = sample_component(
            "component:game-a:dlss",
            "game:a",
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            Some("2.0.0"),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "C:/Games/GameA/nvngx_dlss.dll",
        );
        let v3_artifact = sample_artifact(
            "artifact:dlss-v3",
            GraphicsTechnology::DlssSuperResolution,
            Some("3.0.0"),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "C:/Games/GameB/nvngx_dlss.dll",
            Some("game:b"),
        );
        let v1_artifact = sample_artifact(
            "artifact:dlss-v1",
            GraphicsTechnology::DlssSuperResolution,
            Some("1.0.0"),
            "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            "C:/Games/GameC/nvngx_dlss.dll",
            Some("game:c"),
        );

        let groups = find_test_candidates(&[component], &[v3_artifact, v1_artifact]);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].candidates().len(), 1);
    }

    #[test]
    fn includes_all_known_versions_for_replacement() {
        let component = sample_component(
            "component:game-a:dlss",
            "game:a",
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            Some("3.7.0"),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "C:/Games/GameA/nvngx_dlss.dll",
        );
        let older = sample_artifact(
            "artifact:older",
            GraphicsTechnology::DlssSuperResolution,
            Some("3.5.0"),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "C:/Games/GameB/nvngx_dlss.dll",
            Some("game:b"),
        );
        let same = sample_artifact(
            "artifact:same",
            GraphicsTechnology::DlssSuperResolution,
            Some("3.7.0"),
            "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            "C:/Games/GameC/nvngx_dlss.dll",
            Some("game:c"),
        );
        let newer = sample_artifact(
            "artifact:newer",
            GraphicsTechnology::DlssSuperResolution,
            Some("3.8.0"),
            "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
            "C:/Games/GameD/nvngx_dlss.dll",
            Some("game:d"),
        );

        let groups = find_test_candidates(&[component], &[older, same, newer]);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].candidates().len(), 3);
        assert_eq!(
            groups[0].candidates()[0].comparison(),
            CandidateComparison::NewerVersion
        );
        assert_eq!(
            groups[0].candidates()[1].comparison(),
            CandidateComparison::UnknownVersion
        );
        assert_eq!(
            groups[0].candidates()[2].comparison(),
            CandidateComparison::OlderVersion
        );
    }

    #[test]
    fn unknown_versions_are_manual_candidates() {
        let component = sample_component(
            "component:game-a:dlss",
            "game:a",
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            None,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "C:/Games/GameA/nvngx_dlss.dll",
        );
        let candidate = sample_artifact(
            "artifact:unknown",
            GraphicsTechnology::DlssSuperResolution,
            Some("3.7.0"),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "C:/Games/GameB/nvngx_dlss.dll",
            Some("game:b"),
        );

        let groups = find_test_candidates(&[component], &[candidate]);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].candidates().len(), 1);
        assert_eq!(
            groups[0].candidates()[0].comparison(),
            CandidateComparison::UnknownVersion
        );
    }

    #[test]
    fn matches_same_family_artifacts_across_file_names() {
        // A Streamline component and a Streamline artifact with different file
        // names are a valid bundle swap now (the engine swaps the whole bundle),
        // so the candidate is offered instead of skipped on a name mismatch.
        let component = sample_component(
            "component:game-a:streamline",
            "game:a",
            GraphicsTechnology::NvidiaStreamline,
            Swappability::BundleOnly,
            Some("2.4.0"),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "C:/Games/GameA/sl.common.dll",
        );
        let artifact = sample_artifact(
            "artifact:streamline-interposer",
            GraphicsTechnology::NvidiaStreamline,
            Some("2.5.0"),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "C:/Games/GameB/sl.interposer.dll",
            Some("game:b"),
        );

        let groups = find_test_candidates(&[component], &[artifact]);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].candidates().len(), 1);
    }

    #[test]
    fn deduplicates_identical_candidates_observed_in_multiple_games() {
        let component = sample_component(
            "component:game-a:dlss",
            "game:a",
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            Some("3.5.0"),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "C:/Games/GameA/nvngx_dlss.dll",
        );
        // Identical content has the same bundle id (`ArtifactId::for_bundle`), so
        // the same DLL observed in two different games is one artifact id.
        let duplicate_a = sample_artifact(
            "artifact:dlss-3.7",
            GraphicsTechnology::DlssSuperResolution,
            Some("3.7.0"),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "C:/Games/GameB/nvngx_dlss.dll",
            Some("game:b"),
        );
        let duplicate_b = sample_artifact(
            "artifact:dlss-3.7",
            GraphicsTechnology::DlssSuperResolution,
            Some("3.7.0"),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "D:/Games/GameC/nvngx_dlss.dll",
            Some("game:c"),
        );

        let groups = find_test_candidates(&[component], &[duplicate_a.clone(), duplicate_b]);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].candidates().len(), 1);
        assert_eq!(groups[0].candidates()[0].artifact_id(), duplicate_a.id());
    }

    #[test]
    fn identical_sha256_is_returned_as_candidate() {
        let component = sample_component(
            "component:game-a:dlss",
            "game:a",
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            None,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "C:/Games/GameA/nvngx_dlss.dll",
        );
        let artifact = sample_artifact(
            "artifact:same-sha",
            GraphicsTechnology::DlssSuperResolution,
            None,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "C:/Games/GameB/nvngx_dlss.dll",
            Some("game:b"),
        );

        let groups = find_test_candidates(&[component], &[artifact]);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].candidates().len(), 1);
    }

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

    fn find_test_candidates(
        components: &[GraphicsComponent],
        artifacts: &[LibraryArtifact],
    ) -> Vec<ComponentReplacementCandidates> {
        find_replacement_candidates(components, artifacts, &CandidateContext::empty())
    }

    fn sample_component(
        component_id: &str,
        game_id: &str,
        technology: GraphicsTechnology,
        swappability: Swappability,
        version: Option<&str>,
        sha256: &str,
        path: &str,
    ) -> GraphicsComponent {
        let mut file =
            ComponentFile::new(PathRef::new(path).expect("component path should be valid"))
                .with_sha256(Sha256Hash::new(sha256).expect("sha256 should be valid"));

        if let Some(version) = version {
            file = file.with_version(Version::parse(version).expect("version should be valid"));
        }

        GraphicsComponent::new(
            ComponentId::new(component_id).expect("component id should be valid"),
            GameId::new(game_id).expect("game id should be valid"),
            ComponentKind::NativeLibrary,
            technology,
            swappability,
        )
        .with_file(file)
    }

    fn sample_artifact(
        artifact_id: &str,
        technology: GraphicsTechnology,
        version: Option<&str>,
        sha256: &str,
        path: &str,
        source_game_id: Option<&str>,
    ) -> LibraryArtifact {
        let file_name = std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .expect("artifact path should contain a file name");
        let mut file =
            ComponentFile::new(PathRef::new(path).expect("artifact path should be valid"))
                .with_sha256(Sha256Hash::new(sha256).expect("sha256 should be valid"));

        if let Some(version) = version {
            file = file.with_version(Version::parse(version).expect("version should be valid"));
        }

        let artifact = LibraryArtifact::new(
            ArtifactId::new(artifact_id).expect("artifact id should be valid"),
            technology,
            file_name,
            vec![file],
            ArtifactTrustLevel::LocalObserved,
        )
        .expect("artifact should be valid")
        .with_source("scan-folder")
        .expect("source should be valid");

        match source_game_id {
            Some(source_game_id) => artifact.with_source_game_id(
                GameId::new(source_game_id).expect("source game id should be valid"),
            ),
            None => artifact,
        }
    }

    /// Builds a multi-file (split) FSR package artifact with virtual `manifest://`
    /// member paths — like a composed FSR 4 release: upscaler (primary) + loader.
    fn split_package_artifact(artifact_id: &str, version: &str) -> LibraryArtifact {
        let upscaler = ComponentFile::new(PathRef::new("manifest://upscaler").unwrap())
            .with_sha256(Sha256Hash::new("a".repeat(64)).unwrap())
            .with_version(Version::parse(version).unwrap());
        let loader = ComponentFile::new(PathRef::new("manifest://loader").unwrap())
            .with_sha256(Sha256Hash::new("b".repeat(64)).unwrap())
            .with_version(Version::parse("2.1.0").unwrap());

        LibraryArtifact::new(
            ArtifactId::new(artifact_id).expect("artifact id should be valid"),
            GraphicsTechnology::AmdFsr,
            "amd_fidelityfx_upscaler_dx12.dll",
            vec![upscaler, loader],
            ArtifactTrustLevel::ManifestDownloaded,
        )
        .expect("split package artifact should be valid")
    }

    #[test]
    fn split_fsr_component_is_not_offered_a_unified_single_file_downgrade() {
        let component = sample_component(
            "component:game-a:fsr",
            "game:a",
            GraphicsTechnology::AmdFsr,
            Swappability::BundleOnly,
            Some("4.0.3"),
            &"f".repeat(64),
            "C:/Game/amd_fidelityfx_upscaler_dx12.dll",
        );
        // The unified FSR 3.x backend is a single `amd_fidelityfx_dx12.dll`.
        let unified = sample_artifact(
            "artifact:fsr-3.1",
            GraphicsTechnology::AmdFsr,
            Some("3.1.0"),
            &"e".repeat(64),
            "C:/Lib/amd_fidelityfx_dx12.dll",
            None,
        );

        let groups = find_test_candidates(&[component], &[unified]);
        assert!(
            groups.is_empty(),
            "a split FSR set must not be offered a unified single-file downgrade"
        );
    }

    #[test]
    fn split_fsr_component_accepts_another_split_package() {
        let component = sample_component(
            "component:game-a:fsr",
            "game:a",
            GraphicsTechnology::AmdFsr,
            Swappability::BundleOnly,
            Some("4.0.3"),
            &"f".repeat(64),
            "C:/Game/amd_fidelityfx_upscaler_dx12.dll",
        );
        let newer = split_package_artifact("artifact:fsr-4.1", "4.1.0");

        let groups = find_test_candidates(&[component], &[newer]);
        assert_eq!(groups.len(), 1, "a newer split package is a valid update");
        assert_eq!(groups[0].candidates().len(), 1);
    }

    #[test]
    fn unified_fsr_component_accepts_both_unified_and_split() {
        let component = sample_component(
            "component:game-a:fsr",
            "game:a",
            GraphicsTechnology::AmdFsr,
            Swappability::Swappable,
            Some("3.1.0"),
            &"f".repeat(64),
            "C:/Game/amd_fidelityfx_dx12.dll",
        );
        let unified = sample_artifact(
            "artifact:fsr-3.1.1",
            GraphicsTechnology::AmdFsr,
            Some("3.1.1"),
            &"e".repeat(64),
            "C:/Lib/amd_fidelityfx_dx12.dll",
            None,
        );
        let split = split_package_artifact("artifact:fsr-4.0", "4.0.3");

        let groups = find_test_candidates(&[component], &[unified, split]);
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0].candidates().len(),
            2,
            "a unified FSR 3.x set accepts both a unified swap and a split upgrade"
        );
    }

    #[test]
    fn cohesive_fsr_candidate_group_uses_entry_point_as_display_path() {
        let component = fsr_component(&[
            "amd_fidelityfx_upscaler_dx12.dll",
            "amd_fidelityfx_dx12.dll",
            "amd_fidelityfx_framegeneration_dx12.dll",
        ]);
        let split = split_package_artifact("artifact:fsr-4.0", "4.0.3");

        let groups = find_test_candidates(&[component], &[split]);

        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0].file_path().as_str(),
            "C:/Game/amd_fidelityfx_dx12.dll"
        );
        assert_eq!(
            groups[0].current_version().map(|version| version.as_str()),
            Some("4.0.3")
        );
    }

    #[test]
    fn native_fsr_upscaler_component_only_matches_upscaler_singles() {
        let component = sample_component(
            "component:game-a:fsr-upscaler",
            "game:a",
            GraphicsTechnology::AmdFsrUpscaler,
            Swappability::Swappable,
            Some("4.0.3"),
            &"f".repeat(64),
            "C:/Game/amd_fidelityfx_upscaler_dx12.dll",
        );
        let upscaler = sample_artifact(
            "artifact:fsr-upscaler-4.1",
            GraphicsTechnology::AmdFsrUpscaler,
            Some("4.1.0"),
            &"e".repeat(64),
            "C:/Lib/amd_fidelityfx_upscaler_dx12.dll",
            None,
        );
        let framegen = sample_artifact(
            "artifact:fsr-framegen-4.1",
            GraphicsTechnology::AmdFsrFrameGeneration,
            Some("4.1.0"),
            &"d".repeat(64),
            "C:/Lib/amd_fidelityfx_framegeneration_dx12.dll",
            None,
        );
        let package = split_package_artifact("artifact:fsr-4.1", "4.1.0");

        let groups = find_test_candidates(&[component], &[upscaler.clone(), framegen, package]);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].candidates().len(), 1);
        assert_eq!(groups[0].candidates()[0].artifact_id(), upscaler.id());
    }

    /// Builds a multi-file FSR component with the given file basenames (FSR family,
    /// the first file is the primary). Used to model dx12-lineage vs native FSR 4.
    fn fsr_component(file_names: &[&str]) -> GraphicsComponent {
        let mut component = GraphicsComponent::new(
            ComponentId::new("component:game-a:fsr").expect("component id"),
            GameId::new("game:a").expect("game id"),
            ComponentKind::NativeLibrary,
            GraphicsTechnology::AmdFsr,
            Swappability::BundleOnly,
        );
        for (index, name) in file_names.iter().enumerate() {
            let sha = char::from(b'a' + index as u8).to_string().repeat(64);
            component = component.with_file(
                ComponentFile::new(PathRef::new(format!("C:/Game/{name}")).expect("path"))
                    .with_sha256(Sha256Hash::new(sha).expect("sha"))
                    .with_version(Version::parse("4.0.3").expect("version")),
            );
        }
        component
    }

    #[test]
    fn dx12_lineage_fsr4_is_offered_a_unified_fsr3_downgrade() {
        // A game we upgraded to FSR 4 still loads `amd_fidelityfx_dx12.dll` (the loader
        // is installed under that name), so it can return to FSR 3.1.
        let upgraded = fsr_component(&[
            "amd_fidelityfx_upscaler_dx12.dll",
            "amd_fidelityfx_dx12.dll",
            "amd_fidelityfx_framegeneration_dx12.dll",
        ]);
        let unified = sample_artifact(
            "artifact:fsr-3.1.4",
            GraphicsTechnology::AmdFsr,
            Some("3.1.4"),
            &"e".repeat(64),
            "C:/Lib/amd_fidelityfx_dx12.dll",
            None,
        );

        let groups = find_test_candidates(&[upgraded], &[unified]);
        assert_eq!(
            groups.len(),
            1,
            "a dx12-lineage FSR 4 set can pick a unified FSR 3.1 again"
        );
        assert_eq!(groups[0].candidates().len(), 1);
    }

    #[test]
    fn native_fsr4_is_not_offered_a_unified_fsr3_downgrade() {
        // A native FSR 4 game loads its own loader and has no dx12 entry point — there
        // is no FSR 3 to return to.
        let native = fsr_component(&[
            "amd_fidelityfx_upscaler_dx12.dll",
            "amd_fidelityfx_loader_dx12.dll",
            "amd_fidelityfx_framegeneration_dx12.dll",
        ]);
        let unified = sample_artifact(
            "artifact:fsr-3.1.4",
            GraphicsTechnology::AmdFsr,
            Some("3.1.4"),
            &"e".repeat(64),
            "C:/Lib/amd_fidelityfx_dx12.dll",
            None,
        );

        let groups = find_test_candidates(&[native], &[unified]);
        assert!(
            groups.is_empty(),
            "a native FSR 4 set must not be offered a unified FSR 3 downgrade"
        );
    }
}
