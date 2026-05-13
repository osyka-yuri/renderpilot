use std::{collections::HashSet, fs, path::Path};

use renderpilot_domain::GraphicsTechnology;
use serde::{Deserialize, Serialize};

use crate::{
    error::LibraryPatternError,
    glob::glob_matches,
    normalize::{normalize_file_name, normalize_pattern},
};

/// Whitelist of file extensions that any pattern in a [`LibraryPatternSet`] can
/// match.
///
/// Used by the file-system walker as a cheap pre-filter: a file whose
/// extension is not in this list cannot possibly match any pattern, so the
/// walker can skip it without paying the cost of `fs::symlink_metadata` and
/// the per-file pattern lookup. For RenderPilot's pattern catalog (every
/// pattern targets `*.dll`) this throws away the vast majority of files in
/// modern game installs (sounds, shaders, packed data, configs, ...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateFileExtensions {
    /// At least one pattern's extension cannot be statically determined (no
    /// `.` separator, or wildcard appears after it). The walker must not
    /// pre-filter by extension — every file has to flow through the full
    /// pattern matcher.
    Any,
    /// File is a candidate match only if its extension (case-insensitive) is
    /// in this set. Always non-empty when this variant is returned.
    Allowed(HashSet<String>),
}

impl CandidateFileExtensions {
    /// Returns `true` if a file with `file_name` could possibly match any
    /// pattern in the originating set.
    ///
    /// Callers should call this in tight loops (typically inside a directory
    /// walker), so it must not allocate on the common path.
    #[must_use]
    pub fn allows_file_name(&self, file_name: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Allowed(allowed) => extension_of_file_name(file_name).is_some_and(|ext| {
                allowed.contains(ext) || allowed.iter().any(|known| known.eq_ignore_ascii_case(ext))
            }),
        }
    }
}

/// ASCII extension slice of `file_name` (after the last `.`), or `None`.
///
/// This function borrows from the original file name and does not allocate.
fn extension_of_file_name(file_name: &str) -> Option<&str> {
    let (_, ext) = file_name.rsplit_once('.')?;

    if ext.is_empty() {
        return None;
    }

    Some(ext)
}

/// Set of library filename patterns loaded from JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryPatternSet {
    patterns: Vec<LibraryPattern>,
}

impl LibraryPatternSet {
    /// Creates a pattern set and validates pattern ordering metadata.
    pub fn new(patterns: Vec<LibraryPattern>) -> Result<Self, LibraryPatternError> {
        let set = Self { patterns };
        set.validate()?;
        Ok(set)
    }

    /// Parses a pattern set from JSON text.
    pub fn from_json_str(json: &str) -> Result<Self, LibraryPatternError> {
        serde_json::from_str(json).map_err(LibraryPatternError::Json)
    }

    /// Loads a pattern set from a JSON file.
    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, LibraryPatternError> {
        let json = fs::read_to_string(path).map_err(LibraryPatternError::Io)?;
        Self::from_json_str(&json)
    }

    /// Returns all configured patterns in matching order.
    pub fn patterns(&self) -> &[LibraryPattern] {
        &self.patterns
    }

    /// Returns the first graphics technology matching the given file name.
    pub fn match_file_name(&self, file_name: &str) -> Option<GraphicsTechnology> {
        self.match_file_name_on_platform(file_name, PatternPlatform::Any)
    }

    /// Returns the first matching pattern classification for the given file name.
    pub fn find_match(&self, file_name: &str) -> Option<LibraryPatternMatch> {
        self.find_match_on_platform(file_name, PatternPlatform::Any)
    }

    /// Returns the first graphics technology matching the given file name and platform.
    pub fn match_file_name_on_platform(
        &self,
        file_name: &str,
        platform: PatternPlatform,
    ) -> Option<GraphicsTechnology> {
        self.find_match_on_platform(file_name, platform)
            .map(|matched| matched.technology)
    }

    /// Returns the first matching pattern classification for the given file name and platform.
    pub fn find_match_on_platform(
        &self,
        file_name: &str,
        platform: PatternPlatform,
    ) -> Option<LibraryPatternMatch> {
        let file_name = normalize_file_name(file_name)?;

        self.patterns
            .iter()
            .find(|pattern| pattern.matches(file_name.as_str(), platform))
            .map(LibraryPatternMatch::from)
    }

    /// Computes the set of file-extensions any pattern (filtered by `platform`)
    /// could possibly match.
    ///
    /// Inspect the documentation on [`CandidateFileExtensions`] for the
    /// intended use as a walker pre-filter.
    pub fn candidate_file_extensions(&self, platform: PatternPlatform) -> CandidateFileExtensions {
        let mut allowed = HashSet::new();

        for pattern in &self.patterns {
            if !pattern.platform.matches(platform) {
                continue;
            }

            match static_extension_of_pattern(pattern.normalized_pattern.as_str()) {
                Some(ext) => {
                    allowed.insert(ext);
                }
                // A pattern with no static extension means we cannot safely
                // pre-filter — fall back to "no filter, evaluate every file".
                None => return CandidateFileExtensions::Any,
            }
        }

        if allowed.is_empty() {
            CandidateFileExtensions::Any
        } else {
            CandidateFileExtensions::Allowed(allowed)
        }
    }

    fn validate(&self) -> Result<(), LibraryPatternError> {
        let mut seen = HashSet::new();

        for pattern in &self.patterns {
            let key = (
                pattern.platform,
                pattern.kind,
                pattern.normalized_pattern.clone(),
            );

            if !seen.insert(key) {
                return Err(LibraryPatternError::DuplicatePattern {
                    pattern: pattern.normalized_pattern.clone(),
                    platform: pattern.platform,
                    kind: pattern.kind,
                });
            }
        }

        Ok(())
    }
}

/// Metadata for a successful library pattern match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LibraryPatternMatch {
    technology: GraphicsTechnology,
    kind: PatternKind,
    platform: PatternPlatform,
}

impl LibraryPatternMatch {
    /// Returns the classified graphics technology.
    pub fn technology(self) -> GraphicsTechnology {
        self.technology
    }

    /// Returns whether the match came from an exact or glob pattern.
    pub fn kind(self) -> PatternKind {
        self.kind
    }

    /// Returns the pattern platform that matched.
    pub fn platform(self) -> PatternPlatform {
        self.platform
    }
}

impl From<&LibraryPattern> for LibraryPatternMatch {
    fn from(pattern: &LibraryPattern) -> Self {
        Self {
            technology: pattern.technology,
            kind: pattern.kind,
            platform: pattern.platform,
        }
    }
}

impl<'de> Deserialize<'de> for LibraryPatternSet {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawLibraryPatternSet {
            patterns: Vec<LibraryPattern>,
        }

        let raw = RawLibraryPatternSet::deserialize(deserializer)?;
        Self::new(raw.patterns).map_err(serde::de::Error::custom)
    }
}

impl Serialize for LibraryPatternSet {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Serialize)]
        struct RawLibraryPatternSet<'a> {
            patterns: &'a [LibraryPattern],
        }

        RawLibraryPatternSet {
            patterns: &self.patterns,
        }
        .serialize(serializer)
    }
}

/// One filename pattern and its graphics technology classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryPattern {
    pattern: String,
    kind: PatternKind,
    platform: PatternPlatform,
    technology: GraphicsTechnology,
    normalized_pattern: String,
}

impl LibraryPattern {
    /// Creates a library pattern.
    pub fn new(
        pattern: impl Into<String>,
        kind: PatternKind,
        platform: PatternPlatform,
        technology: GraphicsTechnology,
    ) -> Result<Self, LibraryPatternError> {
        let pattern = pattern.into();
        let normalized_pattern =
            normalize_pattern(&pattern).ok_or(LibraryPatternError::EmptyPattern)?;

        Ok(Self {
            pattern,
            kind,
            platform,
            technology,
            normalized_pattern,
        })
    }

    /// Returns the source pattern text.
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// Returns whether the pattern is exact or glob.
    pub fn kind(&self) -> PatternKind {
        self.kind
    }

    /// Returns the platform this pattern applies to.
    pub fn platform(&self) -> PatternPlatform {
        self.platform
    }

    /// Returns the classified graphics technology.
    pub fn technology(&self) -> GraphicsTechnology {
        self.technology
    }

    fn matches(&self, file_name: &str, platform: PatternPlatform) -> bool {
        self.platform.matches(platform)
            && self
                .kind
                .matches(self.normalized_pattern.as_str(), file_name)
    }
}

impl<'de> Deserialize<'de> for LibraryPattern {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawLibraryPattern {
            pattern: String,
            #[serde(default = "default_pattern_kind")]
            kind: PatternKind,
            #[serde(default = "default_pattern_platform")]
            platform: PatternPlatform,
            technology: GraphicsTechnology,
        }

        let raw = RawLibraryPattern::deserialize(deserializer)?;
        Self::new(raw.pattern, raw.kind, raw.platform, raw.technology)
            .map_err(serde::de::Error::custom)
    }
}

impl Serialize for LibraryPattern {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Serialize)]
        struct RawLibraryPattern<'a> {
            pattern: &'a str,
            kind: PatternKind,
            platform: PatternPlatform,
            technology: GraphicsTechnology,
        }

        RawLibraryPattern {
            pattern: &self.pattern,
            kind: self.kind,
            platform: self.platform,
            technology: self.technology,
        }
        .serialize(serializer)
    }
}

/// Platform scope for a library pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternPlatform {
    /// Pattern applies to any platform.
    Any,
    /// Pattern applies to Windows libraries.
    Windows,
    /// Pattern applies to Linux libraries.
    Linux,
    /// Pattern applies to macOS libraries.
    MacOs,
}

impl PatternPlatform {
    fn matches(self, requested: Self) -> bool {
        self == Self::Any || requested == Self::Any || self == requested
    }
}

/// Matching strategy for a library pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternKind {
    /// Match the normalized file name exactly.
    Exact,
    /// Match the normalized file name using `*` and `?` wildcards.
    Glob,
}

impl PatternKind {
    fn matches(self, pattern: &str, file_name: &str) -> bool {
        match self {
            Self::Exact => pattern == file_name,
            Self::Glob => glob_matches(pattern, file_name),
        }
    }
}

fn default_pattern_kind() -> PatternKind {
    PatternKind::Exact
}

fn default_pattern_platform() -> PatternPlatform {
    PatternPlatform::Any
}

/// Returns the static (no-wildcard) extension of a normalized pattern, or
/// `None` when the pattern's extension cannot be inferred without running the
/// matcher (no `.`, wildcard after the last `.`).
///
/// Inputs are already lower-case (see `normalize_pattern`), so the result is
/// safe to compare with [`extension_of_file_name`] using `==`.
fn static_extension_of_pattern(normalized_pattern: &str) -> Option<String> {
    let (_, ext) = normalized_pattern.rsplit_once('.')?;

    if ext.is_empty() {
        return None;
    }

    if ext.contains('*') || ext.contains('?') {
        return None;
    }

    Some(ext.to_owned())
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::GraphicsTechnology;

    use super::{LibraryPattern, LibraryPatternSet, PatternKind, PatternPlatform};
    use crate::LibraryPatternError;

    const PATTERNS_JSON: &str = include_str!("../../../data/library_patterns.json");

    #[test]
    fn loads_library_patterns_json() {
        let patterns = LibraryPatternSet::from_json_str(PATTERNS_JSON).expect("valid patterns");

        assert!(!patterns.patterns().is_empty());
    }

    #[test]
    fn detects_dlss_super_resolution() {
        let patterns = pattern_set();

        assert_eq!(
            patterns.match_file_name("nvngx_dlss.dll"),
            Some(GraphicsTechnology::DlssSuperResolution)
        );
    }

    #[test]
    fn detects_dlss_frame_generation() {
        let patterns = pattern_set();

        assert_eq!(
            patterns.match_file_name("nvngx_dlssg.dll"),
            Some(GraphicsTechnology::DlssFrameGeneration)
        );
    }

    #[test]
    fn detects_dlss_ray_reconstruction() {
        let patterns = pattern_set();

        assert_eq!(
            patterns.match_file_name("nvngx_dlssd.dll"),
            Some(GraphicsTechnology::DlssRayReconstruction)
        );
    }

    #[test]
    fn detects_streamline_glob() {
        let patterns = pattern_set();

        assert_eq!(
            patterns.match_file_name("sl.interposer.dll"),
            Some(GraphicsTechnology::NvidiaStreamline)
        );
    }

    #[test]
    fn detects_intel_xess() {
        let patterns = pattern_set();

        assert_eq!(
            patterns.match_file_name("libxess.dll"),
            Some(GraphicsTechnology::IntelXeSs)
        );
    }

    #[test]
    fn detects_amd_frame_generation_before_general_fsr() {
        let patterns = pattern_set();

        assert_eq!(
            patterns.match_file_name("amd_fidelityfx_framegeneration.dll"),
            Some(GraphicsTechnology::AmdFsrFrameGeneration)
        );
    }

    #[test]
    fn detects_amd_fsr_runtime_variants_before_unknown_fsr_globs() {
        let patterns = pattern_set();

        assert_eq!(
            patterns.match_file_name("amd_fidelityfx_dx12.dll"),
            Some(GraphicsTechnology::AmdFsr)
        );
        assert_eq!(
            patterns.match_file_name("amd_fidelityfx_upscaler.dll"),
            Some(GraphicsTechnology::AmdFsr)
        );
        assert_eq!(
            patterns.match_file_name("amd_fidelityfx_upscaler_dx12.dll"),
            Some(GraphicsTechnology::AmdFsr)
        );
        assert_eq!(
            patterns.match_file_name("amd_fidelityfx_vk.dll"),
            Some(GraphicsTechnology::AmdFsr)
        );
    }

    #[test]
    fn detects_amd_fsr_denoiser_and_loader() {
        let patterns = pattern_set();

        assert_eq!(
            patterns.match_file_name("amd_fidelityfx_denoiser.dll"),
            Some(GraphicsTechnology::AmdFsrRayRegeneration)
        );
        assert_eq!(
            patterns.match_file_name("amd_fidelityfx_denoiser_dx12.dll"),
            Some(GraphicsTechnology::AmdFsrRayRegeneration)
        );
        assert_eq!(
            patterns.match_file_name("amd_fidelityfx_loader_dx12.dll"),
            Some(GraphicsTechnology::Unknown)
        );
    }

    #[test]
    fn detects_fsr3_runtime_dlls_before_broad_unknown_fsr_globs() {
        let patterns = pattern_set();

        assert_eq!(
            patterns.match_file_name("ffx_fsr3_x64.dll"),
            Some(GraphicsTechnology::AmdFsrFrameGeneration)
        );
        assert_eq!(
            patterns.match_file_name("ffx_fsr3upscaler_x64.dll"),
            Some(GraphicsTechnology::AmdFsr)
        );
        assert_eq!(
            patterns.match_file_name("dlssg_to_fsr3_amd_is_better.dll"),
            Some(GraphicsTechnology::AmdFsrFrameGeneration)
        );
    }

    #[test]
    fn detects_fsr2_runtime_dlls_before_broad_unknown_fsr_globs() {
        let patterns = pattern_set();

        assert_eq!(
            patterns.match_file_name("ffx_fsr2_api_dx12_x64.dll"),
            Some(GraphicsTechnology::AmdFsr)
        );
        assert_eq!(
            patterns.match_file_name("ffx_fsr2_api_vk_x64.dll"),
            Some(GraphicsTechnology::AmdFsr)
        );
        assert_eq!(
            patterns.match_file_name("ffx_fsr2_api_x64.dll"),
            Some(GraphicsTechnology::AmdFsr)
        );
        assert_eq!(
            patterns.match_file_name("fsr2-unity-plugin.dll"),
            Some(GraphicsTechnology::AmdFsr)
        );
        assert_eq!(
            patterns.match_file_name("fsr2-unity-plugind.dll"),
            Some(GraphicsTechnology::AmdFsr)
        );
    }

    #[test]
    fn detects_direct_storage_runtimes() {
        let patterns = pattern_set();

        assert_eq!(
            patterns.match_file_name("dstorage.dll"),
            Some(GraphicsTechnology::DirectStorage)
        );
        assert_eq!(
            patterns.match_file_name("dstoragecore.dll"),
            Some(GraphicsTechnology::DirectStorage)
        );
    }

    #[test]
    fn detects_additional_xess_runtime_variants_before_unknown_xess_glob() {
        let patterns = pattern_set();

        assert_eq!(
            patterns.match_file_name("libxess_dx11.dll"),
            Some(GraphicsTechnology::IntelXeSs)
        );
        assert_eq!(
            patterns.match_file_name("libxell.dll"),
            Some(GraphicsTechnology::IntelXeLl)
        );
        assert_eq!(
            patterns.match_file_name("libxell_dx11.dll"),
            Some(GraphicsTechnology::IntelXeLl)
        );
    }

    #[test]
    fn broad_fsr_patterns_are_unknown() {
        let patterns = pattern_set();

        assert_eq!(
            patterns.match_file_name("some_fsr_unknown.dll"),
            Some(GraphicsTechnology::Unknown)
        );
    }

    #[test]
    fn broad_xess_patterns_are_unknown() {
        let patterns = pattern_set();

        assert_eq!(
            patterns.match_file_name("custom_xess_bridge.dll"),
            Some(GraphicsTechnology::Unknown)
        );
    }

    #[test]
    fn matching_is_case_insensitive_and_accepts_paths() {
        let patterns = pattern_set();

        assert_eq!(
            patterns.match_file_name(r"C:\Games\Game\NVNGX_DLSS.DLL"),
            Some(GraphicsTechnology::DlssSuperResolution)
        );
    }

    #[test]
    fn platform_filter_can_exclude_patterns() {
        let patterns = pattern_set();

        assert_eq!(
            patterns.match_file_name_on_platform("nvngx_dlss.dll", PatternPlatform::Linux),
            None
        );
    }

    #[test]
    fn duplicate_patterns_are_rejected() {
        let pattern = LibraryPattern::new(
            "nvngx_dlss.dll",
            PatternKind::Exact,
            PatternPlatform::Windows,
            GraphicsTechnology::DlssSuperResolution,
        )
        .expect("valid pattern");

        let error = LibraryPatternSet::new(vec![pattern.clone(), pattern])
            .expect_err("duplicate pattern should fail");

        assert!(matches!(
            error,
            LibraryPatternError::DuplicatePattern { .. }
        ));
    }

    #[test]
    fn direct_deserialization_rejects_duplicate_patterns() {
        let json = r#"
        {
          "patterns": [
            {
              "pattern": "nvngx_dlss.dll",
              "kind": "exact",
              "platform": "windows",
              "technology": "dlss_super_resolution"
            },
            {
              "pattern": "NVNGX_DLSS.DLL",
              "kind": "exact",
              "platform": "windows",
              "technology": "dlss_super_resolution"
            }
          ]
        }
        "#;

        let error =
            serde_json::from_str::<LibraryPatternSet>(json).expect_err("duplicate should fail");

        assert!(error.to_string().contains("duplicate library pattern"));
    }

    fn pattern_set() -> LibraryPatternSet {
        LibraryPatternSet::from_json_str(PATTERNS_JSON).expect("valid patterns")
    }

    #[test]
    fn candidate_extensions_for_default_windows_set_is_only_dll() {
        let patterns = pattern_set();

        let candidates = patterns.candidate_file_extensions(PatternPlatform::Windows);

        assert!(candidates.allows_file_name("nvngx_dlss.dll"));
        assert!(candidates.allows_file_name("MyGame.DLL"));

        assert!(!candidates.allows_file_name("config.ini"));
        assert!(!candidates.allows_file_name("textures.pak"));
        assert!(!candidates.allows_file_name("video.bik"));
        assert!(!candidates.allows_file_name("README"));
    }

    #[test]
    fn candidate_extensions_falls_back_to_any_when_pattern_has_wildcard_extension() {
        let patterns = LibraryPatternSet::new(vec![LibraryPattern::new(
            "weird.*",
            super::PatternKind::Glob,
            super::PatternPlatform::Windows,
            renderpilot_domain::GraphicsTechnology::Unknown,
        )
        .expect("valid pattern")])
        .expect("valid set");

        let candidates = patterns.candidate_file_extensions(super::PatternPlatform::Windows);

        assert!(matches!(candidates, super::CandidateFileExtensions::Any));
        assert!(candidates.allows_file_name("anything.txt"));
    }

    #[test]
    fn candidate_extensions_returns_any_when_pattern_set_filters_to_empty() {
        let patterns = pattern_set();

        // Linux platform has no Windows-only patterns.
        let candidates = patterns.candidate_file_extensions(super::PatternPlatform::Linux);

        assert!(matches!(candidates, super::CandidateFileExtensions::Any));
    }

    #[test]
    fn specific_amd_and_fsr_patterns_are_ordered_before_broad_unknown_globs() {
        let patterns = pattern_set();
        let all = patterns.patterns();

        let exact_dx12_idx = pattern_index(all, "amd_fidelityfx_dx12.dll", PatternKind::Exact);
        let exact_framegen_idx = pattern_index(
            all,
            "amd_fidelityfx_framegeneration_dx12.dll",
            PatternKind::Exact,
        );
        let exact_fsr2_idx = pattern_index(all, "ffx_fsr2_api_dx12_x64.dll", PatternKind::Exact);
        let broad_unknown_idx = pattern_index(all, "*fsr*.dll", PatternKind::Glob);

        assert!(exact_dx12_idx < broad_unknown_idx);
        assert!(exact_framegen_idx < broad_unknown_idx);
        assert!(exact_fsr2_idx < broad_unknown_idx);
    }

    fn pattern_index(patterns: &[LibraryPattern], value: &str, kind: PatternKind) -> usize {
        patterns
            .iter()
            .position(|pattern| pattern.pattern() == value && pattern.kind() == kind)
            .expect("pattern should exist")
    }
}
