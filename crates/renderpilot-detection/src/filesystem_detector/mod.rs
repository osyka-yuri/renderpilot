mod scan;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use renderpilot_application::{AppResult, ComponentDetector};
use renderpilot_domain::{
    ComponentFile, ComponentId, ComponentKind, GameInstallation, GraphicsComponent,
    GraphicsTechnology, PathRef, Sha256Hash, Swappability, Version,
};
use serde::Serialize;

use crate::{
    error::detection_error,
    file_metadata::read_detected_file_metadata,
    FileCacheKey,
    LibraryPatternError,
    LibraryPatternSet,
    PatternKind,
    PatternPlatform,
    VersionDetectionStatus,
};

use self::scan::collect_files;

const DEFAULT_LIBRARY_PATTERNS_JSON: &str = include_str!("../../../../data/library_patterns.json");
const DEFAULT_MAX_RECURSION_DEPTH: usize = 8;

/// One graphics library file detected inside a game folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DetectedLibraryFile {
    file_name: String,
    file_path: PathRef,
    technology: GraphicsTechnology,
    kind: ComponentKind,
    detection_confidence: DetectionConfidence,
    swappability: Swappability,
    version: Option<Version>,
    status: VersionDetectionStatus,
    sha256: Sha256Hash,
    cache_key: FileCacheKey,
}

impl DetectedLibraryFile {
    /// Returns the file name that matched a known library pattern.
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// Returns the full normalized file path.
    pub fn file_path(&self) -> &PathRef {
        &self.file_path
    }

    /// Returns the detected graphics technology.
    pub fn technology(&self) -> GraphicsTechnology {
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

    /// Returns the cache key derived from path, size, modification time, and hash.
    pub fn cache_key(&self) -> &FileCacheKey {
        &self.cache_key
    }

    /// Converts the detected file into a component record for the given game.
    pub fn into_component(self, game: &GameInstallation) -> AppResult<GraphicsComponent> {
        let component_id = ComponentId::new(format!("component:{}:{}", game.id(), self.file_path))
            .map_err(detection_error)?;
        let mut file = ComponentFile::new(self.file_path).with_sha256(self.sha256);

        if let Some(version) = self.version {
            file = file.with_version(version);
        }

        Ok(GraphicsComponent::new(
            component_id,
            game.id().clone(),
            self.kind,
            self.technology,
            self.swappability,
        )
        .with_file(file))
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryPatternComponentDetector {
    patterns: LibraryPatternSet,
    platform: PatternPlatform,
    max_depth: usize,
}

impl LibraryPatternComponentDetector {
    /// Creates a detector with an explicit pattern set and platform filter.
    pub fn new(patterns: LibraryPatternSet, platform: PatternPlatform) -> Self {
        Self {
            patterns,
            platform,
            max_depth: DEFAULT_MAX_RECURSION_DEPTH,
        }
    }

    /// Creates a Windows detector from the workspace `data/library_patterns.json`.
    pub fn windows_default() -> Result<Self, LibraryPatternError> {
        Ok(Self::new(
            LibraryPatternSet::from_json_str(DEFAULT_LIBRARY_PATTERNS_JSON)?,
            PatternPlatform::Windows,
        ))
    }

    /// Sets the maximum recursion depth used when scanning a game folder.
    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Returns the pattern set used by this detector.
    pub fn patterns(&self) -> &LibraryPatternSet {
        &self.patterns
    }

    /// Returns the maximum recursion depth used when scanning a game folder.
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }

    /// Detects graphics library files and returns file-level detection records.
    pub fn detect_library_files(
        &self,
        game: &GameInstallation,
    ) -> AppResult<Vec<DetectedLibraryFile>> {
        let root = PathBuf::from(game.install_path().as_str());
        let mut detected = Vec::new();

        for file in collect_files(&root, self.max_depth)? {
            if let Some(library) = self.detect_library_file(&file)? {
                detected.push(library);
            }
        }

        Ok(detected)
    }

    fn detect_library_file(&self, file: &Path) -> AppResult<Option<DetectedLibraryFile>> {
        let Some(file_name) = file.file_name().and_then(|name| name.to_str()) else {
            return Ok(None);
        };

        let Some(matched) = self
            .patterns
            .find_match_on_platform(file_name, self.platform)
        else {
            return Ok(None);
        };

        let file_path = path_ref_from_path(file)?;
        let file_metadata = read_detected_file_metadata(file, file_path.clone())?;

        Ok(Some(DetectedLibraryFile {
            file_name: file_name.to_owned(),
            file_path,
            technology: matched.technology(),
            kind: component_kind_for(matched.technology()),
            detection_confidence: confidence_for(matched.kind(), matched.technology()),
            swappability: swappability_for(matched.technology()),
            version: file_metadata.version,
            status: file_metadata.status,
            sha256: file_metadata.sha256,
            cache_key: file_metadata.cache_key,
        }))
    }
}

impl ComponentDetector for LibraryPatternComponentDetector {
    fn name(&self) -> &str {
        "library-pattern-detector"
    }

    fn detect_components(&self, game: &GameInstallation) -> AppResult<Vec<GraphicsComponent>> {
        self.detect_library_files(game)?
            .into_iter()
            .map(|library| library.into_component(game))
            .collect()
    }
}

fn path_ref_from_path(path: &Path) -> AppResult<PathRef> {
    PathRef::new(path.to_string_lossy().as_ref()).map_err(detection_error)
}

fn component_kind_for(technology: GraphicsTechnology) -> ComponentKind {
    match technology {
        GraphicsTechnology::NvidiaStreamline => ComponentKind::StreamlineComponent,
        _ => ComponentKind::NativeLibrary,
    }
}

fn confidence_for(kind: PatternKind, technology: GraphicsTechnology) -> DetectionConfidence {
    match (kind, technology) {
        (_, GraphicsTechnology::Unknown) => DetectionConfidence::Low,
        (PatternKind::Exact, _) => DetectionConfidence::High,
        (PatternKind::Glob, _) => DetectionConfidence::Medium,
    }
}

fn swappability_for(technology: GraphicsTechnology) -> Swappability {
    match technology {
        GraphicsTechnology::NvidiaStreamline => Swappability::BundleOnly,
        GraphicsTechnology::Unknown => Swappability::Unknown,
        _ => Swappability::Swappable,
    }
}