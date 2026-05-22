use std::collections::{HashMap, HashSet};

use renderpilot_domain::{
    ArtifactId, ComponentId, GameId, GraphicsComponent, GraphicsTechnology, LibraryArtifact,
    PathRef, Version,
};

/// Context for candidate lookup that carries source metadata for artifacts.
#[derive(Debug, Clone)]
pub struct CandidateContext {
    downloaded_ids: HashSet<ArtifactId>,
    manifest_entry_ids: HashMap<ArtifactId, String>,
}

impl CandidateContext {
    /// Creates a new candidate context from the given lookup tables.
    pub fn new(
        downloaded_ids: HashSet<ArtifactId>,
        manifest_entry_ids: HashMap<ArtifactId, String>,
    ) -> Self {
        Self {
            downloaded_ids,
            manifest_entry_ids,
        }
    }

    /// Returns an empty context with no source metadata.
    pub fn empty() -> Self {
        Self {
            downloaded_ids: HashSet::new(),
            manifest_entry_ids: HashMap::new(),
        }
    }
}

/// Finds replacement candidates for the detected component files of one game.
#[must_use]
pub fn find_replacement_candidates(
    components: &[GraphicsComponent],
    artifacts: &[LibraryArtifact],
    context: &CandidateContext,
) -> Vec<ComponentFileReplacementCandidates> {
    let artifacts_by_technology = group_artifacts_by_technology(artifacts);
    let mut groups = Vec::new();

    for component in components {
        let Some(component_artifacts) = artifacts_by_technology.get(&component.technology()) else {
            continue;
        };

        for file in component.files() {
            let mut candidates = component_artifacts
                .iter()
                .filter_map(|artifact| {
                    let is_downloaded = context.downloaded_ids.contains(artifact.id());
                    let entry_id = context.manifest_entry_ids.get(artifact.id()).cloned();
                    ReplacementCandidate::for_component_file(
                        component,
                        file,
                        artifact,
                        is_downloaded,
                        entry_id,
                    )
                })
                .collect::<Vec<_>>();

            if candidates.is_empty() {
                continue;
            }

            candidates.sort_by(|left, right| left.ordering_key().cmp(&right.ordering_key()));
            let candidates = deduplicate_candidates(candidates);
            groups.push(ComponentFileReplacementCandidates::new(
                component, file, candidates,
            ));
        }
    }

    groups.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
    groups
}

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

/// Replacement candidates applicable to one detected component file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentFileReplacementCandidates {
    component_id: ComponentId,
    game_id: GameId,
    technology: GraphicsTechnology,
    file_path: PathRef,
    current_version: Option<Version>,
    candidates: Vec<ReplacementCandidate>,
}

impl ComponentFileReplacementCandidates {
    /// Creates a new candidate group for a component file.
    pub fn new(
        component: &GraphicsComponent,
        file: &renderpilot_domain::ComponentFile,
        candidates: Vec<ReplacementCandidate>,
    ) -> Self {
        Self {
            component_id: component.id().clone(),
            game_id: component.game_id().clone(),
            technology: component.technology(),
            file_path: file.path().clone(),
            current_version: file.version().cloned(),
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
    warning: Option<CandidateWarning>,
    manifest_entry_id: Option<String>,
    is_downloaded: bool,
}

impl ReplacementCandidate {
    fn for_component_file(
        component: &GraphicsComponent,
        file: &renderpilot_domain::ComponentFile,
        artifact: &LibraryArtifact,
        is_downloaded: bool,
        manifest_entry_id: Option<String>,
    ) -> Option<Self> {
        let comparison = candidate_comparison(component, file, artifact)?;

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
            warning: candidate_warning(component),
            manifest_entry_id,
            is_downloaded,
        })
    }

    fn ordering_key(&self) -> (u8, std::cmp::Reverse<Option<Version>>, &str, &str) {
        (
            self.comparison.priority(),
            std::cmp::Reverse(self.version.clone()),
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

    /// Returns whether this candidate is already downloaded locally.
    pub fn is_downloaded(&self) -> bool {
        self.is_downloaded
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

    /// Returns a warning that must be surfaced with a replacement candidate.
    pub fn warning(&self) -> Option<CandidateWarning> {
        self.warning
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

/// Warning that must be surfaced with a replacement candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CandidateWarning {
    /// The candidate targets one Streamline file only and should not be treated as a safe bundle swap.
    StreamlineSingleFileSwap,
}

impl CandidateWarning {
    /// Returns the stable CLI text for this warning.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StreamlineSingleFileSwap => "streamline_single_file_swap_requires_warning",
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

fn candidate_warning(component: &GraphicsComponent) -> Option<CandidateWarning> {
    match component.technology() {
        GraphicsTechnology::NvidiaStreamline => Some(CandidateWarning::StreamlineSingleFileSwap),
        _ => None,
    }
}

fn deduplicate_candidates(candidates: Vec<ReplacementCandidate>) -> Vec<ReplacementCandidate> {
    let mut seen = HashSet::<CandidateDedupeKey>::new();
    let mut deduplicated = Vec::with_capacity(candidates.len());

    for candidate in candidates {
        if seen.insert(CandidateDedupeKey::from(&candidate)) {
            deduplicated.push(candidate);
        }
    }

    deduplicated
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CandidateDedupeKey {
    file_name: String,
    comparison: CandidateComparison,
    version: Option<Version>,
    sha256: String,
}

impl From<&ReplacementCandidate> for CandidateDedupeKey {
    fn from(candidate: &ReplacementCandidate) -> Self {
        Self {
            file_name: candidate.file_name.clone(),
            comparison: candidate.comparison,
            version: candidate.version.clone(),
            sha256: candidate.sha256.clone(),
        }
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
                    !(current.major() == 1 && candidate.major() > 1)
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
    file: &renderpilot_domain::ComponentFile,
    artifact: &LibraryArtifact,
) -> Option<CandidateComparison> {
    require_matching_file_name(file, artifact)?;
    require_different_origin(component, file, artifact)?;
    require_different_content(file, artifact)?;
    require_version_compatible(component.technology(), file.version(), artifact.version())?;

    compare_versions(file.version(), artifact.version())
}

fn require_matching_file_name(
    file: &renderpilot_domain::ComponentFile,
    artifact: &LibraryArtifact,
) -> Option<()> {
    let file_name = std::path::Path::new(file.path().as_str())
        .file_name()
        .and_then(|name| name.to_str())?;

    if artifact.file_name() == file_name {
        Some(())
    } else {
        None
    }
}

fn require_different_origin(
    component: &GraphicsComponent,
    file: &renderpilot_domain::ComponentFile,
    artifact: &LibraryArtifact,
) -> Option<()> {
    if artifact.source_game_id() == Some(component.game_id()) && artifact.path() == file.path() {
        None
    } else {
        Some(())
    }
}

fn require_different_content(
    file: &renderpilot_domain::ComponentFile,
    artifact: &LibraryArtifact,
) -> Option<()> {
    if file.sha256() == Some(artifact.sha256()) {
        None
    } else {
        Some(())
    }
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
        find_replacement_candidates, require_different_content, require_different_origin,
        require_matching_file_name, require_version_compatible, CandidateComparison,
        CandidateContext, CandidateWarning, CompatibilityPolicy,
        ComponentFileReplacementCandidates,
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
    fn dlss_v2_is_compatible_with_v3_and_v1() {
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
        assert_eq!(groups[0].candidates().len(), 2);
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
    fn streamline_candidates_require_warning() {
        let component = sample_component(
            "component:game-a:streamline",
            "game:a",
            GraphicsTechnology::NvidiaStreamline,
            Swappability::BundleOnly,
            Some("2.4.0"),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "C:/Games/GameA/sl.interposer.dll",
        );
        let candidate = sample_artifact(
            "artifact:streamline",
            GraphicsTechnology::NvidiaStreamline,
            Some("2.5.0"),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "C:/Games/GameB/sl.interposer.dll",
            Some("game:b"),
        );

        let groups = find_test_candidates(&[component], &[candidate]);

        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0].candidates()[0].warning(),
            Some(CandidateWarning::StreamlineSingleFileSwap)
        );
    }

    #[test]
    fn skips_same_technology_artifacts_with_different_file_names() {
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

        assert!(groups.is_empty());
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
        let duplicate_a = sample_artifact(
            "artifact:dlss-3.7-a",
            GraphicsTechnology::DlssSuperResolution,
            Some("3.7.0"),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "C:/Games/GameB/nvngx_dlss.dll",
            Some("game:b"),
        );
        let duplicate_b = sample_artifact(
            "artifact:dlss-3.7-b",
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
    fn identical_sha256_is_not_recommended_back_to_the_same_file() {
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

        assert!(groups.is_empty());
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
    fn compatibility_policy_dlss_allows_v2_to_v1() {
        let policy = CompatibilityPolicy::DlssSuperResolution;
        let v1 = Version::parse("1.0.0").unwrap();
        let v2 = Version::parse("2.0.0").unwrap();

        assert!(policy.is_compatible(Some(&v2), Some(&v1)));
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
    fn require_matching_file_name_accepts_same_name() {
        let component = sample_component(
            "component:game-a:dlss",
            "game:a",
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            Some("3.5.0"),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "C:/Games/GameA/nvngx_dlss.dll",
        );
        let artifact = sample_artifact(
            "artifact:dlss-3.7",
            GraphicsTechnology::DlssSuperResolution,
            Some("3.7.0"),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "C:/Games/GameB/nvngx_dlss.dll",
            Some("game:b"),
        );

        assert!(require_matching_file_name(&component.files()[0], &artifact).is_some());
    }

    #[test]
    fn require_matching_file_name_rejects_different_name() {
        let component = sample_component(
            "component:game-a:dlss",
            "game:a",
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            Some("3.5.0"),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "C:/Games/GameA/nvngx_dlss.dll",
        );
        let artifact = sample_artifact(
            "artifact:dlssg-3.7",
            GraphicsTechnology::DlssFrameGeneration,
            Some("3.7.0"),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "C:/Games/GameB/nvngx_dlssg.dll",
            Some("game:b"),
        );

        assert!(require_matching_file_name(&component.files()[0], &artifact).is_none());
    }

    #[test]
    fn require_different_origin_rejects_same_game_and_path() {
        let component = sample_component(
            "component:game-a:dlss",
            "game:a",
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            Some("3.5.0"),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "C:/Games/GameA/nvngx_dlss.dll",
        );
        let artifact = sample_artifact(
            "artifact:same",
            GraphicsTechnology::DlssSuperResolution,
            Some("3.7.0"),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "C:/Games/GameA/nvngx_dlss.dll",
            Some("game:a"),
        );

        assert!(require_different_origin(&component, &component.files()[0], &artifact).is_none());
    }

    #[test]
    fn require_different_origin_accepts_different_game() {
        let component = sample_component(
            "component:game-a:dlss",
            "game:a",
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            Some("3.5.0"),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "C:/Games/GameA/nvngx_dlss.dll",
        );
        let artifact = sample_artifact(
            "artifact:other",
            GraphicsTechnology::DlssSuperResolution,
            Some("3.7.0"),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "C:/Games/GameB/nvngx_dlss.dll",
            Some("game:b"),
        );

        assert!(require_different_origin(&component, &component.files()[0], &artifact).is_some());
    }

    #[test]
    fn require_different_content_rejects_same_sha256() {
        let component = sample_component(
            "component:game-a:dlss",
            "game:a",
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            Some("3.5.0"),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "C:/Games/GameA/nvngx_dlss.dll",
        );
        let artifact = sample_artifact(
            "artifact:same-sha",
            GraphicsTechnology::DlssSuperResolution,
            Some("3.7.0"),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "C:/Games/GameB/nvngx_dlss.dll",
            Some("game:b"),
        );

        assert!(require_different_content(&component.files()[0], &artifact).is_none());
    }

    #[test]
    fn require_different_content_accepts_different_sha256() {
        let component = sample_component(
            "component:game-a:dlss",
            "game:a",
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            Some("3.5.0"),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "C:/Games/GameA/nvngx_dlss.dll",
        );
        let artifact = sample_artifact(
            "artifact:different-sha",
            GraphicsTechnology::DlssSuperResolution,
            Some("3.7.0"),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "C:/Games/GameB/nvngx_dlss.dll",
            Some("game:b"),
        );

        assert!(require_different_content(&component.files()[0], &artifact).is_some());
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
    ) -> Vec<ComponentFileReplacementCandidates> {
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
            file,
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
}
