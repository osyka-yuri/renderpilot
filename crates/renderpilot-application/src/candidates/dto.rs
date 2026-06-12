//! Output data types for replacement-candidate lookup.
//!
//! Pure data and accessors: the per-component candidate group, the individual
//! candidate, and the version-comparison verdict. Construction takes a
//! precomputed [`CandidateComparison`] so this module carries no matching logic
//! (that lives in [`super::matcher`]).

use renderpilot_domain::{
    fsr, ArtifactId, ComponentId, GameId, GraphicsComponent, GraphicsTechnology, LibraryArtifact,
    PathRef, Version,
};

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
    /// `current_version` describes the component's version representative
    /// ([`fsr::version_representative`]: for cohesive FSR the upscaler
    /// carries the FSR 4.x version; next to a real unified FSR 3.1 the entry
    /// point does), while `file_path` is the user-facing display path — the dx12
    /// entry point is still the path the user expects to see.
    pub fn new(component: &GraphicsComponent, candidates: Vec<ReplacementCandidate>) -> Self {
        let representative = fsr::version_representative(component.files())
            .expect("component candidate group requires at least one file");
        let display = fsr::display_component_file(component.files())
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

    /// Returns the currently detected version, when available.
    pub fn current_version(&self) -> Option<&Version> {
        self.current_version.as_ref()
    }

    /// Returns replacement candidates in stable presentation order: newest
    /// version first with unknown versions last, ties broken by file name and
    /// build type.
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

/// Named sort key for [`ReplacementCandidate`] ordering — every field is named
/// so the sort semantics are obvious at the call site instead of requiring the
/// reader to decode a positional tuple. Field order IS the sort order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct CandidateSortKey<'a> {
    /// Version descending: `Reverse` makes `Some(3.0)` sort before `Some(2.0)`,
    /// and `Reverse(None)` sorts after every `Some(..)` — unknown versions last.
    version: std::cmp::Reverse<Option<&'a Version>>,
    /// Secondary key: file name in lexical order.
    file_name: &'a str,
    /// Release builds before debug builds at the same version.
    is_debug: bool,
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
        manifest_entry_id: Option<String>,
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
            version: artifact.version().cloned(),
            sha256: artifact.sha256().as_str().to_owned(),
            source_game_id: artifact.source_game_id().cloned(),
            comparison,
            manifest_entry_id,
            is_downloaded,
            is_debug,
        }
    }

    /// Stable presentation order: always version-descending, independent of the
    /// installed version and of any mutable state.
    ///
    /// Deliberately excluded from the key: the comparison verdict (it shifts
    /// every candidate's position whenever the installed version changes) and
    /// the local file path (it appears when a download completes). The one
    /// mutable field kept, `is_downloaded`, sits *after* every field that
    /// distinguishes two post-dedup rows — it only orders identical
    /// `(version, file_name, is_debug)` twins so the downloaded one survives
    /// deduplication, and such twins collapse to a single visible row.
    pub(super) fn ordering_key(&self) -> CandidateSortKey<'_> {
        CandidateSortKey {
            version: std::cmp::Reverse(self.version.as_ref()),
            file_name: self.file_name.as_str(),
            is_debug: self.is_debug,
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
}
