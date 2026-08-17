mod classification;
mod grouping;
mod paths;
mod scan;
mod xiph_grouping;

#[cfg(test)]
mod tests;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use renderpilot_application::{AppResult, ComponentDetector};
use renderpilot_domain::{
    ComponentFile, ComponentKind, GameInstallation, LibraryComponent, LibraryTechnology, PathRef,
    PeCompatibilityProfile, RuntimeCompatibility, RuntimeTarget, Sha256Hash, Swappability, Version,
};
use serde::Serialize;

use crate::{
    FileIdentityProbeResult, FileObservation, FileObservationSource, LibraryPatternError,
    LibraryPatternSet, PatternPlatform, StrongFileCacheKey, VersionDetectionStatus,
    file_metadata::{DetectedFileMetadata, try_read_detected_file_metadata},
};

use self::classification::LibraryFileClassification;
use self::paths::{
    file_name_for_matching, install_root_path, path_ref_from_path, sort_detected_library_files,
    sorted_unique_paths,
};
use self::scan::{WalkCompleteness, collect_files_filtered};

pub use self::grouping::{group_into_artifacts, group_into_components};
pub use self::scan::{
    InstallTreeReport, InstallTreeWalker, InstallWalkMode,
    WalkCompleteness as InstallTreeCompleteness, WalkDiagnostic, WalkDiagnosticKind,
};

const DETECTOR_NAME: &str = "library-pattern-detector";

/// One native library file detected inside a game folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DetectedLibraryFile {
    file_name: String,
    file_path: PathRef,
    technology: LibraryTechnology,
    kind: ComponentKind,
    detection_confidence: DetectionConfidence,
    swappability: Swappability,
    version: Option<Version>,
    status: VersionDetectionStatus,
    sha256: Sha256Hash,
    observation: Option<FileObservation>,
    #[serde(skip)]
    runtime_target: Option<RuntimeTarget>,
    #[serde(skip)]
    pe_compatibility: Option<PeCompatibilityProfile>,
}

/// Complete persisted facts eligible for a zero-content-read reuse after a
/// fresh strong identity lease proves the exact object is unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReusableFileMetadata {
    /// Stable observation recorded for the exact normalized path.
    pub observation: FileObservation,
    /// Observed file version; `None` is an observed lack of version metadata.
    pub version: Option<Version>,
    /// Runtime fact extracted from the original stable object, when applicable.
    pub runtime_target: Option<RuntimeTarget>,
    /// Strict PE compatibility fact from the original stable object, when applicable.
    pub pe_compatibility: Option<PeCompatibilityProfile>,
}

impl ReusableFileMetadata {
    fn matches(&self, path: &PathRef, identity: &StrongFileCacheKey) -> bool {
        self.observation.path == *path
            && self.observation.identity_kind == identity.kind
            && self.observation.object_identity == identity.object_identity
            && self.observation.change_token == identity.change_token
            && self.observation.size == identity.size
    }
}

impl DetectedLibraryFile {
    fn from_parts(
        file_name: String,
        file_path: PathRef,
        classification: LibraryFileClassification,
        mut metadata: DetectedFileMetadata,
        runtime_target: Option<RuntimeTarget>,
        pe_compatibility: Option<PeCompatibilityProfile>,
    ) -> Self {
        let observation = FileObservation::from_metadata(file_path.clone(), &mut metadata);
        Self {
            file_name,
            file_path,
            technology: classification.technology,
            kind: classification.kind,
            detection_confidence: classification.confidence,
            swappability: classification.swappability,
            version: metadata.pe.version,
            status: metadata.status,
            sha256: metadata.sha256,
            observation,
            runtime_target,
            pe_compatibility,
        }
    }

    fn from_reusable(
        file_name: String,
        file_path: PathRef,
        classification: LibraryFileClassification,
        reusable: &ReusableFileMetadata,
    ) -> Self {
        Self {
            file_name,
            file_path,
            technology: classification.technology,
            kind: classification.kind,
            detection_confidence: classification.confidence,
            swappability: classification.swappability,
            version: reusable.version.clone(),
            status: match reusable.version {
                Some(_) => VersionDetectionStatus::KnownVersion,
                None => VersionDetectionStatus::UnknownVersion,
            },
            sha256: reusable.observation.sha256.clone(),
            observation: Some(reusable.observation.clone()),
            runtime_target: reusable.runtime_target.clone(),
            pe_compatibility: reusable.pe_compatibility.clone(),
        }
    }

    /// Returns the file name that matched a known library pattern.
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// Returns the full normalized file path.
    pub fn file_path(&self) -> &PathRef {
        &self.file_path
    }

    /// Returns the detected library technology.
    pub fn technology(&self) -> LibraryTechnology {
        self.technology
    }

    /// Returns the detected component kind.
    pub fn kind(&self) -> ComponentKind {
        self.kind
    }

    /// Returns confidence derived from the matched pattern type.
    pub fn detection_confidence(&self) -> DetectionConfidence {
        self.detection_confidence
    }

    /// Returns the replacement policy inferred by detection.
    pub fn swappability(&self) -> Swappability {
        self.swappability
    }

    /// Returns the parsed Windows FileVersion or ProductVersion when available.
    pub fn version(&self) -> Option<&Version> {
        self.version.as_ref()
    }

    /// Returns the status of file-version metadata extraction.
    pub fn status(&self) -> VersionDetectionStatus {
        self.status
    }

    /// Returns the SHA-256 hash of the detected file.
    pub fn sha256(&self) -> &Sha256Hash {
        &self.sha256
    }

    /// Returns the strong observation that produced this detection.
    pub fn observation(&self) -> Option<&FileObservation> {
        self.observation.as_ref()
    }

    /// Returns runtime facts extracted while the file was detected.
    pub fn runtime_target(&self) -> Option<&RuntimeTarget> {
        self.runtime_target.as_ref()
    }

    /// Returns strict PE compatibility facts extracted during detection.
    pub fn pe_compatibility(&self) -> Option<&PeCompatibilityProfile> {
        self.pe_compatibility.as_ref()
    }

    /// Converts this detection into its domain file representation.
    pub(crate) fn component_file(&self) -> ComponentFile {
        let mut file = ComponentFile::new(self.file_path.clone()).with_sha256(self.sha256.clone());
        if let Some(version) = &self.version {
            file = file.with_version(version.clone());
        }
        if let Some(profile) = &self.pe_compatibility {
            file = file.with_pe_compatibility(profile.clone());
        }
        file
    }
}

/// Confidence assigned by the data-driven detector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum DetectionConfidence {
    /// Exact filename match from the pattern set.
    High,
    /// Glob filename match for a known family.
    Medium,
    /// Glob filename match where the concrete technology is intentionally unknown.
    Low,
}

/// Component detector that classifies files by data-driven library patterns.
#[derive(Clone)]
pub struct LibraryPatternComponentDetector {
    patterns: LibraryPatternSet,
    platform: PatternPlatform,
    max_depth: Option<usize>,
    observation_source: Arc<dyn FileObservationSource>,
}

impl LibraryPatternComponentDetector {
    /// Creates a detector with an explicit pattern set and platform filter.
    pub fn new(patterns: LibraryPatternSet, platform: PatternPlatform) -> Self {
        Self {
            patterns,
            platform,
            max_depth: None,
            observation_source: Arc::new(crate::file_observation::SystemFileObservationSource),
        }
    }

    /// Creates a Windows detector from the bundled RenderPilot pattern catalog.
    pub fn windows_default() -> Result<Self, LibraryPatternError> {
        let patterns = LibraryPatternSet::bundled_defaults()?;
        Ok(Self::new(patterns, PatternPlatform::Windows))
    }

    /// Sets the maximum recursion depth used when scanning a game folder.
    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = Some(max_depth);
        self
    }

    /// Uses a Send+Sync observation source for deterministic test scenarios.
    #[cfg(any(test, feature = "test-instrumentation"))]
    pub fn with_file_observation_source(
        mut self,
        observation_source: Arc<dyn FileObservationSource>,
    ) -> Self {
        self.observation_source = observation_source;
        self
    }

    /// Returns the pattern set used by this detector.
    pub fn patterns(&self) -> &LibraryPatternSet {
        &self.patterns
    }

    /// Returns the platform filter used by this detector.
    pub fn platform(&self) -> PatternPlatform {
        self.platform
    }

    /// Returns the maximum recursion depth used when scanning a game folder.
    pub fn max_depth(&self) -> Option<usize> {
        self.max_depth
    }

    /// Detects native library files and returns file-level detection records.
    pub fn detect_library_files(
        &self,
        game: &GameInstallation,
    ) -> AppResult<Vec<DetectedLibraryFile>> {
        let files = self.collect_candidate_library_paths(game)?;
        self.detect_library_files_from_paths(files, None)
    }

    /// Detects native libraries using only same-game persisted facts whose
    /// strong identity is reproved through an identity-only lease. A mismatch
    /// falls back to one full stable-object read; an unstable probe fails the
    /// scan closed rather than publishing a partial result.
    pub fn detect_library_files_with_reuse(
        &self,
        game: &GameInstallation,
        reusable: &HashMap<String, ReusableFileMetadata>,
    ) -> AppResult<Vec<DetectedLibraryFile>> {
        let files = self.collect_candidate_library_paths(game)?;
        self.detect_library_files_from_paths(files, Some(reusable))
    }

    fn collect_candidate_library_paths(&self, game: &GameInstallation) -> AppResult<Vec<PathBuf>> {
        let root = install_root_path(game);
        let candidate_extensions = self.patterns.candidate_file_extensions(self.platform);
        let report = collect_files_filtered(&root, self.max_depth, |file_name: &str| {
            candidate_extensions.allows_file_name(file_name)
        })?;
        if report.completeness() == WalkCompleteness::Incomplete {
            let paths = report
                .diagnostics()
                .iter()
                .take(3)
                .map(|diagnostic| diagnostic.path().display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(renderpilot_application::AppError::detection_failed(
                format!("installation scan was incomplete; catalog state was preserved ({paths})"),
            ));
        }

        Ok(sorted_unique_paths(report.into_files()))
    }

    fn detect_library_files_from_paths(
        &self,
        files: Vec<PathBuf>,
        reusable: Option<&HashMap<String, ReusableFileMetadata>>,
    ) -> AppResult<Vec<DetectedLibraryFile>> {
        let mut detected = Vec::new();

        for file in files {
            let Some(library) = self.detect_library_file(&file, reusable)? else {
                continue;
            };

            detected.push(library);
        }

        sort_detected_library_files(&mut detected);

        Ok(detected)
    }

    fn detect_library_file(
        &self,
        file: &Path,
        reusable: Option<&HashMap<String, ReusableFileMetadata>>,
    ) -> AppResult<Option<DetectedLibraryFile>> {
        let Some(file_name) = file_name_for_matching(file) else {
            return Ok(None);
        };

        let Some(classification) = self.classify_file_name(file_name) else {
            return Ok(None);
        };

        let file_path = path_ref_from_path(file)?;
        if let Some(reusable) = reusable.and_then(|facts| facts.get(file_path.as_str())) {
            match self.observation_source.probe_identity(file)? {
                FileIdentityProbeResult::Available(identity)
                    if reusable.matches(&file_path, &identity) =>
                {
                    return Ok(Some(DetectedLibraryFile::from_reusable(
                        file_name.to_owned(),
                        file_path,
                        classification,
                        reusable,
                    )));
                }
                FileIdentityProbeResult::Available(_) => {}
                FileIdentityProbeResult::Missing => return Ok(None),
                FileIdentityProbeResult::Uncacheable => {}
                FileIdentityProbeResult::Unavailable => {
                    return Err(renderpilot_application::AppError::detection_failed(
                        format!(
                            "file identity lease was unavailable or unstable for {}",
                            file.display()
                        ),
                    ));
                }
            }
        }
        let Some(metadata) =
            try_read_detected_file_metadata(file, self.observation_source.as_ref())?
        else {
            return Ok(None);
        };
        let inspection = matches!(
            classification.technology,
            LibraryTechnology::MicrosoftDxc
                | LibraryTechnology::D3D12Agility
                | LibraryTechnology::OpenVr
                | LibraryTechnology::XiphVorbis
        )
        .then_some(&metadata.pe);

        let runtime_target = runtime_target_from_inspection(classification.technology, inspection);
        let pe_compatibility = matches!(
            classification.technology,
            LibraryTechnology::OpenVr | LibraryTechnology::XiphVorbis
        )
        .then(|| inspection?.compatibility_profile())
        .flatten();

        Ok(Some(DetectedLibraryFile::from_parts(
            file_name.to_owned(),
            file_path,
            classification,
            metadata,
            runtime_target,
            pe_compatibility,
        )))
    }

    fn classify_file_name(&self, file_name: &str) -> Option<LibraryFileClassification> {
        let matched = self
            .patterns
            .find_match_on_platform(file_name, self.platform)?;

        Some(LibraryFileClassification::new(
            matched.technology(),
            matched.kind(),
            file_name,
        ))
    }
}

fn runtime_target_from_inspection(
    technology: LibraryTechnology,
    inspection: Option<&crate::PeInspection>,
) -> Option<RuntimeTarget> {
    if !matches!(
        technology,
        LibraryTechnology::MicrosoftDxc | LibraryTechnology::D3D12Agility
    ) {
        return None;
    }

    let inspection = inspection?;
    let architecture = inspection.architecture?;
    let mut target = RuntimeTarget::new(architecture);
    if technology == LibraryTechnology::D3D12Agility {
        let sdk_version = inspection
            .version
            .as_ref()
            .and_then(|version| version.segments().get(1))
            .and_then(|segment| u32::try_from(*segment).ok())?;
        target = target.with_compatibility(RuntimeCompatibility::D3d12Sdk {
            version: sdk_version,
        });
    }
    Some(target)
}

impl ComponentDetector for LibraryPatternComponentDetector {
    fn name(&self) -> &str {
        DETECTOR_NAME
    }

    fn detect_components(&self, game: &GameInstallation) -> AppResult<Vec<LibraryComponent>> {
        let libraries = self.detect_library_files(game)?;
        group_into_components(game, &libraries)
    }
}
