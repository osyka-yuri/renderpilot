//! Output data types for replacement-candidate lookup.
//!
//! Pure data and accessors: the per-component candidate group, the individual
//! candidate, and the version-comparison verdict. Construction takes a
//! precomputed [`CandidateComparison`] so this module carries no matching logic
//! (that lives in [`super::matcher`]).

use renderpilot_domain::{
    ArtifactId, ArtifactTrustLevel, ComponentId, ComponentVersionReport, GameId, GraphicsComponent,
    GraphicsTechnology, LibraryArtifact, PathRef, Version, component_version_report, fsr,
};

/// Version state used for both the DTO and matcher comparison baseline.
pub(super) fn component_version_state(component: &GraphicsComponent) -> ComponentVersionReport {
    component_version_report(component.files(), component.technology())
}

/// Replacement candidates applicable to one detected component (bundle).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentReplacementCandidates {
    component_id: ComponentId,
    game_id: GameId,
    technology: GraphicsTechnology,
    file_path: PathRef,
    version_report: ComponentVersionReport,
    candidates: Vec<ReplacementCandidate>,
}

impl ComponentReplacementCandidates {
    /// Creates a per-component candidate group.
    ///
    /// `version_report` distinguishes known, mixed, and unknown state instead
    /// of overloading `None`. `file_path` is the user-facing display path
    /// (dx12 entry point for FSR, name-min for Streamline).
    pub fn new(
        component: &GraphicsComponent,
        version_report: ComponentVersionReport,
        candidates: Vec<ReplacementCandidate>,
    ) -> Self {
        let display = fsr::display_component_file(component.files())
            .expect("component candidate group requires at least one display file");

        Self {
            component_id: component.id().clone(),
            game_id: component.game_id().clone(),
            technology: component.technology(),
            file_path: display.path().clone(),
            version_report,
            candidates,
        }
    }

    pub(super) fn sort_key(&self) -> (&'static str, &str, &str) {
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

    /// Returns the honest installed-version state for this component.
    pub fn version_report(&self) -> &ComponentVersionReport {
        &self.version_report
    }

    /// Returns replacement candidates in stable presentation order.
    ///
    /// Sort keys, in priority order:
    /// 1. version descending (unknown versions last);
    /// 2. multi-file packages ahead of single-file twins of the same version;
    /// 3. file name (lexical);
    /// 4. release before debug;
    /// 5. preferred trust level (CDN/cache ahead of game-folder observations);
    /// 6. downloaded twin last among identity ties (so the local copy wins
    ///    deduplication while remaining a single visible row).
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
    catalog_package_id: Option<String>,
    is_downloaded: bool,
    is_debug: bool,
    /// Sort-only: prefer CDN/cache over game-folder observations.
    trust_level: ArtifactTrustLevel,
    /// Sort-only: multi-file packages sort ahead of single-file twins.
    file_count: usize,
}

/// Named sort key for [`ReplacementCandidate`] ordering — every field is named
/// so the sort semantics are obvious at the call site instead of requiring the
/// reader to decode a positional tuple. Field order IS the sort order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct CandidateSortKey<'a> {
    /// Version descending: `Reverse` makes `Some(3.0)` sort before `Some(2.0)`,
    /// and `Reverse(None)` sorts after every `Some(..)` — unknown versions last.
    version: std::cmp::Reverse<Option<&'a Version>>,
    /// Multi-file packages (FSR / Streamline) sort ahead of single-file twins of
    /// the same version so bulk swaps pick the full set.
    file_count: std::cmp::Reverse<usize>,
    /// Secondary key: file name in lexical order.
    file_name: &'a str,
    /// Release builds before debug builds at the same version.
    is_debug: bool,
    /// Prefer CDN/cache artifacts over game-folder observations when both
    /// survive as candidates for the same version key.
    trust_rank: u8,
    /// Downloaded twins sort before their non-downloaded counterpart so the
    /// downloaded copy survives deduplication.
    downloaded: std::cmp::Reverse<bool>,
    /// Content-identity tie-break that never changes.
    sha256: &'a str,
}

impl ReplacementCandidate {
    /// Builds a candidate from an artifact and the already-computed comparison
    /// verdict. The matcher computes [`CandidateComparison`] (and rejects an
    /// incompatible artifact) before calling this, so this constructor is pure.
    pub(super) fn new(
        artifact: &LibraryArtifact,
        comparison: CandidateComparison,
        is_downloaded: bool,
        catalog_package_id: Option<String>,
        is_debug: bool,
    ) -> Self {
        Self {
            artifact_id: artifact.id().clone(),
            file_name: artifact.file_name().to_owned(),
            file_path: if is_downloaded {
                Some(artifact.path().clone())
            } else {
                None
            },
            // PE FileVersion remains the artifact's technical version, while
            // Microsoft NuGet runtimes are presented and ordered by their
            // actual release version. Some packages intentionally contain
            // members whose PE versions differ from the package version
            // (notably historical DXIL builds).
            version: artifact.release_version().cloned(),
            sha256: artifact.sha256().as_str().to_owned(),
            source_game_id: artifact.source_game_id().cloned(),
            comparison,
            catalog_package_id,
            is_downloaded,
            is_debug,
            trust_level: artifact.trust_level(),
            // Domain forbids empty file lists; len is the package size for sort.
            file_count: artifact.files().len(),
        }
    }

    /// Stable presentation / dedup order used by the matcher.
    ///
    /// Field order of [`CandidateSortKey`] *is* the sort order and must stay
    /// aligned with the public description on
    /// [`ComponentReplacementCandidates::candidates`].
    ///
    /// Deliberately excluded: comparison verdict (shifts whenever the installed
    /// version changes) and local path (appears after download). Trust and
    /// `is_downloaded` sit after identity-distinguishing fields so the preferred
    /// twin survives deduplication without reordering distinct candidates.
    pub(super) fn ordering_key(&self) -> CandidateSortKey<'_> {
        CandidateSortKey {
            version: std::cmp::Reverse(self.version.as_ref()),
            file_count: std::cmp::Reverse(self.file_count),
            file_name: self.file_name.as_str(),
            is_debug: self.is_debug,
            trust_rank: self.trust_level.candidate_preference_rank(),
            downloaded: std::cmp::Reverse(self.is_downloaded),
            sha256: self.sha256.as_str(),
        }
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

    /// Returns the user-facing release version, when known.
    ///
    /// For package-backed artifacts this is the upstream package version;
    /// otherwise it falls back to the primary file's PE version.
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

    /// Returns the catalog package id if this candidate is curated remotely.
    pub fn catalog_package_id(&self) -> Option<&str> {
        self.catalog_package_id.as_deref()
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
}
