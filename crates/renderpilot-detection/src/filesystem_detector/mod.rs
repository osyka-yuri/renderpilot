mod classification;
mod grouping;
mod paths;
mod scan;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use renderpilot_application::{AppResult, ComponentDetector};
use renderpilot_domain::{
    ComponentKind, GameInstallation, GraphicsComponent, GraphicsTechnology, PathRef, Sha256Hash,
    Swappability, Version,
};
use serde::Serialize;

use crate::{
    file_metadata::{try_read_detected_file_metadata, DetectedFileMetadata, FileHashCache},
    FileCacheKey, LibraryPatternError, LibraryPatternSet, PatternPlatform, VersionDetectionStatus,
};

use self::classification::LibraryFileClassification;
use self::paths::{
    cached_files_under_root, file_name_for_matching, install_root_path, path_ref_from_path,
    sort_detected_library_files, sorted_unique_paths,
};
use self::scan::collect_files_filtered;

pub use self::grouping::{group_into_artifacts, group_into_components};

const DEFAULT_MAX_RECURSION_DEPTH: usize = 12;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FastCachedDetectionReport {
    libraries: Vec<DetectedLibraryFile>,
    detectable_count: usize,
}

impl FastCachedDetectionReport {
    pub fn libraries(&self) -> &[DetectedLibraryFile] {
        &self.libraries
    }

    pub fn into_libraries(self) -> Vec<DetectedLibraryFile> {
        self.libraries
    }

    pub fn detectable_count(&self) -> usize {
        self.detectable_count
    }
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

    /// Creates a Windows detector from the bundled RenderPilot pattern catalog.
    pub fn windows_default() -> Result<Self, LibraryPatternError> {
        let patterns = LibraryPatternSet::bundled_defaults()?;
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
    ///
    /// Cached entries whose underlying file no longer exists are silently
    /// dropped — the cache is treated as a hint, not a source of truth.
    pub fn detect_library_files_fast_cached(
        &self,
        game: &GameInstallation,
        cache: &FileHashCache,
    ) -> AppResult<Vec<DetectedLibraryFile>> {
        let root = install_root_path(game);
        let files = cached_files_under_root(cache, &root);

        self.detect_library_files_from_paths(files, Some(cache))
    }

    /// Fast scan with completeness evidence for fallback decisions in higher layers.
    pub fn detect_library_files_fast_cached_with_evidence(
        &self,
        game: &GameInstallation,
        cache: &FileHashCache,
    ) -> AppResult<FastCachedDetectionReport> {
        let libraries = self.detect_library_files_fast_cached(game, cache)?;
        let detectable_count = self.count_detectable_library_files(game)?;

        Ok(FastCachedDetectionReport {
            libraries,
            detectable_count,
        })
    }

    /// Returns how many on-disk files under `game` currently match any
    /// configured library pattern for this detector platform.
    ///
    /// This check does not read PE metadata or hash file contents, so it is
    /// cheap enough to validate whether a fast cache-only result is complete.
    pub fn count_detectable_library_files(&self, game: &GameInstallation) -> AppResult<usize> {
        let files = self.collect_candidate_library_paths(game)?;
        Ok(self.count_detectable_from_paths(files))
    }

    fn detect_library_files_with_optional_cache(
        &self,
        game: &GameInstallation,
        cache: Option<&FileHashCache>,
    ) -> AppResult<Vec<DetectedLibraryFile>> {
        let files = self.collect_candidate_library_paths(game)?;

        self.detect_library_files_from_paths(files, cache)
    }

    fn collect_candidate_library_paths(&self, game: &GameInstallation) -> AppResult<Vec<PathBuf>> {
        let root = install_root_path(game);
        let candidate_extensions = self.patterns.candidate_file_extensions(self.platform);
        let files = collect_files_filtered(&root, self.max_depth, |file_name: &str| {
            candidate_extensions.allows_file_name(file_name)
        })?;

        Ok(sorted_unique_paths(files))
    }

    fn count_detectable_from_paths(&self, paths: Vec<PathBuf>) -> usize {
        paths
            .into_iter()
            .filter(|path| {
                file_name_for_matching(path)
                    .and_then(|name| self.classify_file_name(name))
                    .is_some()
            })
            .count()
    }

    fn detect_library_files_from_paths(
        &self,
        files: Vec<PathBuf>,
        cache: Option<&FileHashCache>,
    ) -> AppResult<Vec<DetectedLibraryFile>> {
        let mut detected = Vec::new();

        for file in files {
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

        let Some(classification) = self.classify_file_name(file_name) else {
            return Ok(None);
        };

        let file_path = path_ref_from_path(file)?;
        let Some(metadata) = try_read_detected_file_metadata(file, &file_path, cache)? else {
            return Ok(None);
        };

        Ok(Some(DetectedLibraryFile::from_parts(
            file_name.to_owned(),
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
        let libraries = self.detect_library_files(game)?;
        group_into_components(game, &libraries)
    }
}
