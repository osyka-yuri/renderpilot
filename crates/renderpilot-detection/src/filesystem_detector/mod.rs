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
    file_metadata::{read_detected_file_metadata, DetectedFileMetadata, FileHashCache},
    FileCacheKey, LibraryPatternError, LibraryPatternSet, PatternKind, PatternPlatform,
    VersionDetectionStatus,
};

use self::scan::collect_files;

const DEFAULT_LIBRARY_PATTERNS_JSON: &str = include_str!("../../../../data/library_patterns.json");
const DEFAULT_MAX_RECURSION_DEPTH: usize = 8;
const DETECTOR_NAME: &str = "library-pattern-detector";

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
    fn from_parts(
        file_name: String,
        file_path: PathRef,
        classification: LibraryFileClassification,
        metadata: DetectedFileMetadata,
    ) -> Self {
        Self {
            file_name,
            file_path,
            technology: classification.technology,
            kind: classification.kind,
            detection_confidence: classification.confidence,
            swappability: classification.swappability,
            version: metadata.version,
            status: metadata.status,
            sha256: metadata.sha256,
            cache_key: metadata.cache_key,
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
        let Self {
            file_path,
            technology,
            kind,
            swappability,
            version,
            sha256,
            ..
        } = self;

        let component_id = component_id_for(game, &file_path)?;
        let file = component_file_from_detection(file_path, sha256, version);

        Ok(GraphicsComponent::new(
            component_id,
            game.id().clone(),
            kind,
            technology,
            swappability,
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
        let patterns = LibraryPatternSet::from_json_str(DEFAULT_LIBRARY_PATTERNS_JSON)?;
        Ok(Self::new(patterns, PatternPlatform::Windows))
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

    /// Returns the platform filter used by this detector.
    pub fn platform(&self) -> PatternPlatform {
        self.platform
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
        self.detect_library_files_with_optional_cache(game, None)
    }

    /// Like [`Self::detect_library_files`], but skips hashing when size/mtime match `cache`.
    pub fn detect_library_files_with_cache(
        &self,
        game: &GameInstallation,
        cache: &FileHashCache,
    ) -> AppResult<Vec<DetectedLibraryFile>> {
        self.detect_library_files_with_optional_cache(game, Some(cache))
    }

    /// Fast scan mode that ONLY checks the file paths present in the cache.
    /// It completely skips full file system traversal (`collect_files`).
    pub fn detect_library_files_fast_cached(
        &self,
        game: &GameInstallation,
        cache: &FileHashCache,
    ) -> AppResult<Vec<DetectedLibraryFile>> {
        let root = install_root_path(game);
        let files = cached_existing_files_under_root(cache, &root);

        self.detect_library_files_from_paths(files, Some(cache))
    }

    fn detect_library_files_with_optional_cache(
        &self,
        game: &GameInstallation,
        cache: Option<&FileHashCache>,
    ) -> AppResult<Vec<DetectedLibraryFile>> {
        let root = install_root_path(game);
        let files = collect_files(&root, self.max_depth)?;

        self.detect_library_files_from_paths(files, cache)
    }

    fn detect_library_files_from_paths(
        &self,
        files: Vec<PathBuf>,
        cache: Option<&FileHashCache>,
    ) -> AppResult<Vec<DetectedLibraryFile>> {
        let mut detected = Vec::new();

        for file in sorted_unique_paths(files) {
            let Some(library) = self.detect_library_file(&file, cache)? else {
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
        cache: Option<&FileHashCache>,
    ) -> AppResult<Option<DetectedLibraryFile>> {
        let Some(file_name) = file_name_for_matching(file) else {
            return Ok(None);
        };

        let Some(classification) = self.classify_file_name(&file_name) else {
            return Ok(None);
        };

        let file_path = path_ref_from_path(file)?;
        let metadata = read_detected_file_metadata(file, file_path.clone(), cache)?;

        Ok(Some(DetectedLibraryFile::from_parts(
            file_name,
            file_path,
            classification,
            metadata,
        )))
    }

    fn classify_file_name(&self, file_name: &str) -> Option<LibraryFileClassification> {
        let matched = self
            .patterns
            .find_match_on_platform(file_name, self.platform)?;

        Some(LibraryFileClassification::new(
            matched.technology(),
            matched.kind(),
        ))
    }
}

impl ComponentDetector for LibraryPatternComponentDetector {
    fn name(&self) -> &str {
        DETECTOR_NAME
    }

    fn detect_components(&self, game: &GameInstallation) -> AppResult<Vec<GraphicsComponent>> {
        self.detect_library_files(game)?
            .into_iter()
            .map(|library| library.into_component(game))
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LibraryFileClassification {
    technology: GraphicsTechnology,
    kind: ComponentKind,
    confidence: DetectionConfidence,
    swappability: Swappability,
}

impl LibraryFileClassification {
    fn new(technology: GraphicsTechnology, pattern_kind: PatternKind) -> Self {
        Self {
            technology,
            kind: component_kind_for(technology),
            confidence: confidence_for(pattern_kind, technology),
            swappability: swappability_for(technology),
        }
    }
}

fn cached_existing_files_under_root(cache: &FileHashCache, root: &Path) -> Vec<PathBuf> {
    sorted_unique_paths(
        cache
            .keys()
            .map(PathBuf::from)
            .filter(|path| is_existing_file_under_root(path, root)),
    )
}

fn is_existing_file_under_root(path: &Path, root: &Path) -> bool {
    path.starts_with(root) && path.is_file()
}

fn sorted_unique_paths(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut paths: Vec<_> = paths.into_iter().collect();

    paths.sort_unstable();
    paths.dedup();

    paths
}

fn sort_detected_library_files(files: &mut Vec<DetectedLibraryFile>) {
    files.sort_by_cached_key(|file| file.file_path.to_string());
    files.dedup_by(|left, right| left.file_path == right.file_path);
}

fn install_root_path(game: &GameInstallation) -> PathBuf {
    PathBuf::from(game.install_path().as_str())
}

fn file_name_for_matching(path: &Path) -> Option<String> {
    path.file_name()?.to_str().map(str::to_owned)
}

fn path_ref_from_path(path: &Path) -> AppResult<PathRef> {
    let raw_path = path.to_string_lossy();
    let normalized_path = raw_path.replace('\\', "/");

    PathRef::new(normalized_path.as_str()).map_err(detection_error)
}

fn component_id_for(game: &GameInstallation, file_path: &PathRef) -> AppResult<ComponentId> {
    ComponentId::new(format!("component:{}:{file_path}", game.id())).map_err(detection_error)
}

fn component_file_from_detection(
    file_path: PathRef,
    sha256: Sha256Hash,
    version: Option<Version>,
) -> ComponentFile {
    let file = ComponentFile::new(file_path).with_sha256(sha256);

    match version {
        Some(version) => file.with_version(version),
        None => file,
    }
}

fn component_kind_for(technology: GraphicsTechnology) -> ComponentKind {
    match technology {
        GraphicsTechnology::NvidiaStreamline => ComponentKind::StreamlineComponent,
        _ => ComponentKind::NativeLibrary,
    }
}

fn confidence_for(
    pattern_kind: PatternKind,
    technology: GraphicsTechnology,
) -> DetectionConfidence {
    match (pattern_kind, technology) {
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
