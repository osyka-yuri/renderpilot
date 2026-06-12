mod scan;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use renderpilot_application::{AppResult, ComponentDetector};
use renderpilot_domain::{
    fsr, ArtifactId, ArtifactTrustLevel, ComponentFile, ComponentId, ComponentKind, GameId,
    GameInstallation, GraphicsComponent, GraphicsTechnology, LibraryArtifact, PathRef, Sha256Hash,
    Swappability, Version,
};
use serde::Serialize;

use crate::{
    error::detection_error,
    file_metadata::{try_read_detected_file_metadata, DetectedFileMetadata, FileHashCache},
    FileCacheKey, LibraryPatternError, LibraryPatternSet, PatternKind, PatternPlatform,
    VersionDetectionStatus,
};

use self::scan::collect_files_filtered;

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
        let Some(metadata) = try_read_detected_file_metadata(file, file_path.clone(), cache)?
        else {
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

/// Returns cached file paths that lexically belong under `root`.
///
/// Existence is **not** verified here. [`try_read_detected_file_metadata`]
/// returns `Ok(None)` for missing files (stale cache entries), so those paths
/// are skipped without failing the whole scan.
fn cached_files_under_root(cache: &FileHashCache, root: &Path) -> Vec<PathBuf> {
    let scope = normalized_scope_prefix(root);

    sorted_unique_paths(
        cache
            .keys()
            .map(PathBuf::from)
            .filter(|path| path_in_scope(path, &scope)),
    )
}

fn path_in_scope(path: &Path, scope: &str) -> bool {
    let normalized = path.to_string_lossy().replace('\\', "/");

    normalized == scope
        || normalized
            .strip_prefix(scope)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn normalized_scope_prefix(root: &Path) -> String {
    let normalized = root.to_string_lossy().replace('\\', "/");

    if normalized.ends_with('/') && normalized.len() > 1 {
        normalized.trim_end_matches('/').to_owned()
    } else {
        normalized
    }
}

fn sorted_unique_paths(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut paths: Vec<_> = paths.into_iter().collect();

    paths.sort_unstable();
    paths.dedup();

    paths
}

fn sort_detected_library_files(files: &mut Vec<DetectedLibraryFile>) {
    files.sort_by(|left, right| left.file_path.as_str().cmp(right.file_path.as_str()));
    files.dedup_by(|left, right| left.file_path == right.file_path);
}

fn install_root_path(game: &GameInstallation) -> PathBuf {
    PathBuf::from(game.install_path().as_str())
}

fn file_name_for_matching(path: &Path) -> Option<&str> {
    path.file_name()?.to_str()
}

fn path_ref_from_path(path: &Path) -> AppResult<PathRef> {
    let raw_path = path.to_string_lossy();
    let normalized_path = raw_path.replace('\\', "/");

    PathRef::new(normalized_path.as_str()).map_err(detection_error)
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

/// Groups detected library files into one [`GraphicsComponent`] per
/// `(directory, grouping technology)`.
///
/// Files normally group by technology family inside one directory. Native FSR 4
/// directories are the exception: when a directory contains an
/// [`GraphicsTechnology::AmdFsrLoader`] and no [`GraphicsTechnology::AmdFsr`]
/// entry point, each FSR DLL keeps its exact technology and becomes its own
/// single-file component.
pub fn group_into_components(
    game: &GameInstallation,
    libraries: &[DetectedLibraryFile],
) -> AppResult<Vec<GraphicsComponent>> {
    group_detected_files(libraries)
        .into_iter()
        .map(|group| build_grouped_component(game, group))
        .collect()
}

/// Groups detected library files into one locally-observed [`LibraryArtifact`]
/// bundle per `(directory, grouping technology)`, mirroring
/// [`group_into_components`].
pub fn group_into_artifacts(
    game_id: &GameId,
    libraries: &[DetectedLibraryFile],
) -> AppResult<Vec<LibraryArtifact>> {
    group_detected_files(libraries)
        .into_iter()
        .map(|group| build_grouped_artifact(game_id, group))
        .collect()
}

#[derive(Debug)]
struct GroupedDetectedFiles<'a> {
    technology: GraphicsTechnology,
    files: Vec<&'a DetectedLibraryFile>,
}

/// Partitions detected files into groups keyed by `(parent_dir, grouping
/// technology)`, preserving first-seen order so component/artifact ordering is
/// deterministic.
fn group_detected_files(libraries: &[DetectedLibraryFile]) -> Vec<GroupedDetectedFiles<'_>> {
    let native_fsr_directories = native_fsr_directories(libraries);
    let mut groups: Vec<GroupedDetectedFiles<'_>> = Vec::new();
    let mut index: HashMap<(String, &'static str), usize> = HashMap::new();

    for library in libraries {
        let parent_dir = parent_directory(library.file_path());
        let technology = grouping_technology(library, &parent_dir, &native_fsr_directories);
        let key = (parent_dir, technology.as_slug());

        if let Some(&existing) = index.get(&key) {
            groups[existing].files.push(library);
        } else {
            index.insert(key, groups.len());
            groups.push(GroupedDetectedFiles {
                technology,
                files: vec![library],
            });
        }
    }

    groups
}

fn build_grouped_component(
    game: &GameInstallation,
    group: GroupedDetectedFiles<'_>,
) -> AppResult<GraphicsComponent> {
    let ordered = order_with_primary_first(&group.files);
    let parent_dir = parent_directory(ordered[0].file_path());
    let component_id = grouped_component_id(game, group.technology, &parent_dir)?;

    let mut component = GraphicsComponent::new(
        component_id,
        game.id().clone(),
        group_kind(&ordered),
        group.technology,
        group_swappability(&ordered),
    );

    for file in &ordered {
        component = component.with_file(component_file_from_detection(
            file.file_path().clone(),
            file.sha256().clone(),
            file.version().cloned(),
        ));
    }

    Ok(component)
}

fn build_grouped_artifact(
    game_id: &GameId,
    group: GroupedDetectedFiles<'_>,
) -> AppResult<LibraryArtifact> {
    let ordered = order_with_primary_first(&group.files);
    let artifact_id = ArtifactId::for_bundle(ordered.iter().map(|file| file.sha256()));

    let files: Vec<ComponentFile> = ordered
        .iter()
        .map(|file| {
            component_file_from_detection(
                file.file_path().clone(),
                file.sha256().clone(),
                file.version().cloned(),
            )
        })
        .collect();

    LibraryArtifact::new(
        artifact_id,
        group.technology,
        ordered[0].file_name(),
        files,
        ArtifactTrustLevel::LocalObserved,
    )
    .map_err(detection_error)?
    .with_source("scan-folder")
    .map_err(detection_error)
    .map(|artifact| artifact.with_source_game_id(game_id.clone()))
}

/// Orders a group's files so the representative comes first, then alphabetically
/// for determinism. The first file becomes the bundle's primary/display file.
fn order_with_primary_first<'a>(group: &[&'a DetectedLibraryFile]) -> Vec<&'a DetectedLibraryFile> {
    let family = group
        .first()
        .map(|file| file.technology().family())
        .unwrap_or_default();

    // FSR sets arbitrate the representative by release-build cohesion: a leftover
    // upscaler next to a real unified FSR 3.1 must not hijack the version display.
    let fsr_upscaler_represents = family == GraphicsTechnology::AmdFsr
        && fsr::upscaler_represents_set(
            group.iter().map(|file| (file.file_name(), file.version())),
        );

    let mut ordered = group.to_vec();
    ordered.sort_by(|left, right| {
        primary_rank(left, family, fsr_upscaler_represents)
            .cmp(&primary_rank(right, family, fsr_upscaler_represents))
            .then_with(|| left.file_name().cmp(right.file_name()))
    });

    ordered
}

/// Lower rank = more representative. AMD FSR delegates to the shared
/// [`fsr::primary_rank`] (the upscaler carries the FSR version only in a
/// cohesive set); otherwise the file whose technology equals the family is.
fn primary_rank(
    file: &DetectedLibraryFile,
    family: GraphicsTechnology,
    fsr_upscaler_represents: bool,
) -> u8 {
    if family == GraphicsTechnology::AmdFsr {
        return fsr::primary_rank(file.file_name(), fsr_upscaler_represents);
    }

    if file.technology() == family {
        0
    } else {
        1
    }
}

fn group_kind(ordered: &[&DetectedLibraryFile]) -> ComponentKind {
    if ordered
        .iter()
        .any(|file| file.kind() == ComponentKind::StreamlineComponent)
    {
        ComponentKind::StreamlineComponent
    } else {
        ordered[0].kind()
    }
}

/// A multi-file bundle must be swapped as a unit ([`Swappability::BundleOnly`]);
/// a single file keeps its own detected policy. (A single restrictive sibling no
/// longer blocks an otherwise-swappable bundle.)
fn group_swappability(ordered: &[&DetectedLibraryFile]) -> Swappability {
    if ordered.len() > 1 {
        return Swappability::BundleOnly;
    }

    ordered
        .first()
        .map(|file| file.swappability())
        .unwrap_or(Swappability::Unknown)
}

fn grouped_component_id(
    game: &GameInstallation,
    technology: GraphicsTechnology,
    parent_dir: &str,
) -> AppResult<ComponentId> {
    ComponentId::new(format!(
        "component:{}:{}:{parent_dir}",
        game.id(),
        technology.as_slug()
    ))
    .map_err(detection_error)
}

fn native_fsr_directories(libraries: &[DetectedLibraryFile]) -> HashSet<String> {
    let mut directories = HashMap::<String, (bool, bool)>::new();

    for library in libraries {
        if library.technology().family() != GraphicsTechnology::AmdFsr {
            continue;
        }

        let summary = directories
            .entry(parent_directory(library.file_path()))
            .or_insert((false, false));
        match library.technology() {
            GraphicsTechnology::AmdFsrLoader => summary.0 = true,
            GraphicsTechnology::AmdFsr => summary.1 = true,
            _ => {}
        }
    }

    directories
        .into_iter()
        .filter_map(|(directory, (has_loader, has_anchor))| {
            (has_loader && !has_anchor).then_some(directory)
        })
        .collect()
}

fn grouping_technology(
    library: &DetectedLibraryFile,
    parent_dir: &str,
    native_fsr_directories: &HashSet<String>,
) -> GraphicsTechnology {
    if library.technology().family() == GraphicsTechnology::AmdFsr
        && native_fsr_directories.contains(parent_dir)
    {
        return library.technology();
    }

    library.technology().family()
}

/// Returns the normalized parent directory of a file path (forward slashes).
fn parent_directory(path: &PathRef) -> String {
    path.parent().unwrap_or_default().to_owned()
}
