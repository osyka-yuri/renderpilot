use std::collections::HashMap;

use renderpilot_domain::{
    ArtifactId, ComponentId, GameId, GraphicsComponent, GraphicsTechnology, LibraryArtifact,
    PathRef, Version,
};

/// Finds replacement candidates for the detected component files of one game.
#[must_use]
pub fn find_replacement_candidates(
    components: &[GraphicsComponent],
    artifacts: &[LibraryArtifact],
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
                .filter_map(|artifact| ReplacementCandidate::for_component_file(component, file, artifact))
                .collect::<Vec<_>>();

            if candidates.is_empty() {
                continue;
            }

            candidates.sort_by(|left, right| left.ordering_key().cmp(&right.ordering_key()));
            groups.push(ComponentFileReplacementCandidates::new(component, file, candidates));
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
        grouped.entry(artifact.technology()).or_default().push(artifact);
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
    fn new(
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
    file_path: PathRef,
    version: Option<Version>,
    source_game_id: Option<GameId>,
    comparison: CandidateComparison,
    warning: Option<CandidateWarning>,
}

impl ReplacementCandidate {
    fn for_component_file(
        component: &GraphicsComponent,
        file: &renderpilot_domain::ComponentFile,
        artifact: &LibraryArtifact,
    ) -> Option<Self> {
        let comparison = candidate_comparison(component, file, artifact)?;

        Some(Self {
            artifact_id: artifact.id().clone(),
            file_name: artifact.file_name().to_owned(),
            file_path: artifact.path().clone(),
            version: artifact.version().cloned(),
            source_game_id: artifact.source_game_id().cloned(),
            comparison,
            warning: candidate_warning(component),
        })
    }

    fn ordering_key(&self) -> (u8, std::cmp::Reverse<Option<Version>>, &str, &str) {
        (
            self.comparison.priority(),
            std::cmp::Reverse(self.version.clone()),
            self.file_name.as_str(),
            self.file_path.as_str(),
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

    /// Returns the candidate file path.
    pub fn file_path(&self) -> &PathRef {
        &self.file_path
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

    /// Returns a warning that must be shown before offering this candidate, when required.
    pub fn warning(&self) -> Option<CandidateWarning> {
        self.warning
    }
}

/// Result of comparing a candidate artifact to the currently installed component file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CandidateComparison {
    /// Both versions were known and the candidate is newer than the current file.
    NewerVersion,
    /// At least one side has no version, so the candidate can only be reviewed manually.
    UnknownVersion,
}

impl CandidateComparison {
    /// Returns the stable CLI text for this comparison result.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NewerVersion => "newer_version",
            Self::UnknownVersion => "unknown_version",
        }
    }

    const fn priority(self) -> u8 {
        match self {
            Self::NewerVersion => 0,
            Self::UnknownVersion => 1,
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
        (Some(current), Some(candidate)) if candidate > current => Some(CandidateComparison::NewerVersion),
        (Some(current), Some(candidate)) if candidate <= current => None,
        _ => Some(CandidateComparison::UnknownVersion),
    }
}

fn candidate_warning(component: &GraphicsComponent) -> Option<CandidateWarning> {
    match component.technology() {
        GraphicsTechnology::NvidiaStreamline => Some(CandidateWarning::StreamlineSingleFileSwap),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{
        ArtifactId, ArtifactTrustLevel, ComponentFile, ComponentId, ComponentKind, GameId,
        GraphicsComponent, GraphicsTechnology, LibraryArtifact, PathRef, Sha256Hash,
        Swappability, Version,
    };

    use super::{find_replacement_candidates, CandidateComparison, CandidateWarning};

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

        let groups = find_replacement_candidates(&[component], &[sr_candidate.clone(), fg_candidate, rr_candidate]);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].technology(), GraphicsTechnology::DlssSuperResolution);
        assert_eq!(groups[0].candidates().len(), 1);
        assert_eq!(groups[0].candidates()[0].artifact_id(), sr_candidate.id());
        assert_eq!(groups[0].candidates()[0].comparison(), CandidateComparison::NewerVersion);
    }

    #[test]
    fn skips_same_or_older_known_versions() {
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

        let groups = find_replacement_candidates(&[component], &[older, same]);

        assert!(groups.is_empty());
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

        let groups = find_replacement_candidates(&[component], &[candidate]);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].candidates().len(), 1);
        assert_eq!(groups[0].candidates()[0].comparison(), CandidateComparison::UnknownVersion);
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

        let groups = find_replacement_candidates(&[component], &[candidate]);

        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0].candidates()[0].warning(),
            Some(CandidateWarning::StreamlineSingleFileSwap)
        );
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

        let groups = find_replacement_candidates(&[component], &[artifact]);

        assert!(groups.is_empty());
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
        let mut file = ComponentFile::new(PathRef::new(path).expect("component path should be valid"))
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
        let mut file = ComponentFile::new(PathRef::new(path).expect("artifact path should be valid"))
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

fn candidate_comparison(
    component: &GraphicsComponent,
    file: &renderpilot_domain::ComponentFile,
    artifact: &LibraryArtifact,
) -> Option<CandidateComparison> {
    if artifact.source_game_id() == Some(component.game_id()) && artifact.path() == file.path() {
        return None;
    }

    if file.sha256() == Some(artifact.sha256()) {
        return None;
    }

    compare_versions(file.version(), artifact.version())
}