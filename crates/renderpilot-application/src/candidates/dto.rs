//! Output data types for replacement-candidate lookup.
//!
//! Pure data and accessors: the per-component candidate group, the individual
//! candidate, and the version-comparison verdict. Construction takes a
//! precomputed [`CandidateComparison`] so this module carries no matching logic
//! (that lives in [`super::matcher`]).

use crate::D3d12ExecutableAction;
use renderpilot_domain::{
    Architecture, ArtifactId, ArtifactTrustLevel, CatalogLegalDocumentReceipt,
    CatalogPackageAvailability, CatalogPackageProvenanceReceipt, CatalogPackageReceipt,
    ComponentId, ComponentVersionReport, GameId, LibraryArtifact, LibraryComponent,
    LibraryTechnology, PackageRelease, PathRef, ReleaseChannel, Version, component_version_report,
    fsr,
};

use super::identity::{IntrinsicPackageIdentity, ResolvedTransitionIdentity};

/// Active catalog identity used to enrich replacement candidates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveCatalogPackage {
    package_id: String,
    release: PackageRelease,
    automatic_selection_allowed: bool,
    presentation: Option<CatalogCandidatePresentation>,
}

impl ActiveCatalogPackage {
    /// Creates an active package descriptor.
    pub fn new(
        package_id: impl Into<String>,
        release: PackageRelease,
        automatic_selection_allowed: bool,
    ) -> Self {
        Self {
            package_id: package_id.into(),
            automatic_selection_allowed,
            release,
            presentation: None,
        }
    }

    /// Creates an active descriptor from its canonical immutable receipt.
    pub fn from_receipt(
        receipt: &CatalogPackageReceipt,
        automatic_selection_allowed: bool,
    ) -> Self {
        Self {
            package_id: receipt.package_id().to_owned(),
            release: receipt.release().clone(),
            automatic_selection_allowed,
            presentation: Some(CatalogCandidatePresentation::from_receipt(receipt)),
        }
    }

    /// Returns the stable catalog package identifier.
    pub fn package_id(&self) -> &str {
        &self.package_id
    }

    /// Returns the exact package release.
    pub const fn release(&self) -> &PackageRelease {
        &self.release
    }

    /// Returns whether unattended selection is allowed for this active package.
    pub const fn automatic_selection_allowed(&self) -> bool {
        self.automatic_selection_allowed
    }

    /// Returns immutable facts that may be rendered by a candidate UI.
    pub const fn presentation(&self) -> Option<&CatalogCandidatePresentation> {
        self.presentation.as_ref()
    }
}

/// Immutable package facts attached to catalog candidates for presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogCandidatePresentation {
    variant: String,
    architecture: Architecture,
    unsigned: bool,
    provenance: Option<CatalogPackageProvenanceReceipt>,
    legal_documents: Vec<CatalogLegalDocumentReceipt>,
}

impl CatalogCandidatePresentation {
    fn from_receipt(receipt: &CatalogPackageReceipt) -> Self {
        Self {
            variant: receipt.variant().to_owned(),
            architecture: receipt.target().architecture,
            unsigned: receipt.has_unsigned_members(),
            provenance: receipt.composite_provenance().cloned(),
            legal_documents: receipt.legal_documents().to_vec(),
        }
    }

    /// Returns the package variant; for Xiph this is `<topology>.<naming profile>`.
    pub fn variant(&self) -> &str {
        &self.variant
    }

    /// Returns the verified target architecture.
    pub const fn architecture(&self) -> Architecture {
        self.architecture
    }

    /// Returns whether package members are unsigned.
    pub const fn unsigned(&self) -> bool {
        self.unsigned
    }

    /// Returns immutable composite provenance when present.
    pub const fn provenance(&self) -> Option<&CatalogPackageProvenanceReceipt> {
        self.provenance.as_ref()
    }

    /// Returns validated legal-document links.
    pub fn legal_documents(&self) -> &[CatalogLegalDocumentReceipt] {
        &self.legal_documents
    }
}

/// Catalog identity and availability attached to a replacement candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogCandidatePackage {
    package_id: String,
    release: PackageRelease,
    availability: CatalogPackageAvailability,
    automatic_selection_allowed: bool,
    presentation: Option<CatalogCandidatePresentation>,
}

impl CatalogCandidatePackage {
    #[cfg(test)]
    pub(super) fn new(
        package_id: impl Into<String>,
        release: PackageRelease,
        availability: CatalogPackageAvailability,
        automatic_selection_allowed: bool,
    ) -> Self {
        Self {
            package_id: package_id.into(),
            release,
            availability,
            automatic_selection_allowed,
            presentation: None,
        }
    }

    pub(super) fn from_receipt(
        receipt: &CatalogPackageReceipt,
        availability: CatalogPackageAvailability,
        automatic_selection_allowed: bool,
    ) -> Option<Self> {
        receipt.is_valid().then(|| Self {
            package_id: receipt.package_id().to_owned(),
            release: receipt.release().clone(),
            availability,
            automatic_selection_allowed,
            presentation: Some(CatalogCandidatePresentation::from_receipt(receipt)),
        })
    }

    pub(super) fn from_active(
        active: &ActiveCatalogPackage,
        availability: CatalogPackageAvailability,
    ) -> Self {
        // ActiveCatalogPackage is built from the authoritative active catalog
        // snapshot. It is a canonical source in its own right; a downloaded
        // artifact does not need a second local receipt to retain that status.
        Self {
            package_id: active.package_id().to_owned(),
            release: active.release().clone(),
            availability,
            automatic_selection_allowed: active.automatic_selection_allowed(),
            presentation: active.presentation().cloned(),
        }
    }

    /// Returns the stable catalog package identifier.
    pub fn package_id(&self) -> &str {
        &self.package_id
    }

    /// Returns the exact package release.
    pub const fn release(&self) -> &PackageRelease {
        &self.release
    }

    /// Returns whether the package is active or local-only.
    pub const fn availability(&self) -> CatalogPackageAvailability {
        self.availability
    }

    /// Returns the backend-computed unattended-selection capability.
    pub const fn automatic_selection_allowed(&self) -> bool {
        self.automatic_selection_allowed
    }

    /// Returns immutable facts that may be rendered by a candidate UI.
    pub const fn presentation(&self) -> Option<&CatalogCandidatePresentation> {
        self.presentation.as_ref()
    }

    pub(super) const fn canonical_source_rank(&self) -> u8 {
        match self.availability {
            CatalogPackageAvailability::Available => 0,
            CatalogPackageAvailability::LocalOnly => 1,
        }
    }
}

/// Version state used for both the DTO and matcher comparison baseline.
pub(super) fn component_version_state(component: &LibraryComponent) -> ComponentVersionReport {
    component_version_report(component.files(), component.technology())
}

/// Installed release information prepared for catalog presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstalledReleaseState {
    /// One trustworthy release is installed.
    Known {
        /// Canonical technical PE FileVersion, when the binary exposes one.
        technical_version: Option<Version>,
        /// Optional supplemental catalog annotation.
        release_label: Option<String>,
        /// Exact catalog release when installed content matches a known package.
        catalog_release: Option<PackageRelease>,
    },
    /// Installed members prove more than one version is present.
    Mixed {
        /// Lowest known member technical version.
        min_technical_version: Version,
        /// Highest known member technical version.
        max_technical_version: Version,
    },
    /// No trustworthy installed release can be established.
    Unknown,
}

impl InstalledReleaseState {
    pub(super) fn from_version_report(report: ComponentVersionReport) -> Self {
        match report {
            ComponentVersionReport::Known(version) => Self::Known {
                technical_version: Some(version),
                release_label: None,
                catalog_release: None,
            },
            ComponentVersionReport::Mixed { min, max } => Self::Mixed {
                min_technical_version: min,
                max_technical_version: max,
            },
            ComponentVersionReport::Unknown => Self::Unknown,
        }
    }

    pub(super) fn known_catalog(
        technical_version: Option<Version>,
        release: PackageRelease,
    ) -> Self {
        Self::Known {
            technical_version,
            release_label: release.label.clone(),
            catalog_release: Some(release),
        }
    }

    /// Returns the installed version when the state is known.
    pub const fn known_version(&self) -> Option<&Version> {
        match self {
            Self::Known {
                technical_version, ..
            } => technical_version.as_ref(),
            Self::Mixed { .. } | Self::Unknown => None,
        }
    }
}

/// Replacement candidates applicable to one detected component (bundle).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentReplacementCandidates {
    component_id: ComponentId,
    game_id: GameId,
    technology: LibraryTechnology,
    file_path: PathRef,
    installed_release: InstalledReleaseState,
    candidates: Vec<ReplacementCandidate>,
    automatic_candidate_artifact_id: Option<ArtifactId>,
}

impl ComponentReplacementCandidates {
    /// Creates a per-component candidate group.
    ///
    /// `version_report` distinguishes known, mixed, and unknown state instead
    /// of overloading `None`. `file_path` is the user-facing display path
    /// (dx12 entry point for FSR, name-min for Streamline).
    pub(crate) fn new(
        component: &LibraryComponent,
        installed_release: InstalledReleaseState,
        candidates: Vec<ReplacementCandidate>,
    ) -> Option<Self> {
        let display = fsr::display_component_file(component.files())?;

        Some(Self {
            component_id: component.id().clone(),
            game_id: component.game_id().clone(),
            technology: component.technology(),
            file_path: display.path().clone(),
            installed_release,
            candidates,
            automatic_candidate_artifact_id: None,
        })
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

    /// Returns the library technology of the component.
    pub fn technology(&self) -> LibraryTechnology {
        self.technology
    }

    /// Returns the detected file path of the component.
    pub fn file_path(&self) -> &PathRef {
        &self.file_path
    }

    /// Returns the honest installed-release state for this component.
    pub const fn installed_release(&self) -> &InstalledReleaseState {
        &self.installed_release
    }

    /// Returns replacement candidates in stable presentation order.
    ///
    /// Sort keys, in priority order:
    /// 1. version descending (unknown versions last);
    /// 2. multi-file packages ahead of single-file twins of the same version;
    /// 3. file name (lexical);
    /// 4. release before debug;
    /// 5. preferred trust level (CDN/cache ahead of game-folder observations);
    /// 6. downloaded twin first among identity ties (so the local copy wins
    ///    deduplication while remaining a single visible row);
    /// 7. intrinsic package identity, then component-resolved transition
    ///    identity as stable content tie-breaks.
    pub fn candidates(&self) -> &[ReplacementCandidate] {
        &self.candidates
    }

    /// Returns the unique backend-selected unattended candidate, when one
    /// maximal eligible package exists.
    pub fn automatic_candidate_artifact_id(&self) -> Option<&ArtifactId> {
        self.automatic_candidate_artifact_id.as_ref()
    }

    pub(super) fn set_automatic_candidate_artifact_id(&mut self, artifact_id: Option<ArtifactId>) {
        self.automatic_candidate_artifact_id = artifact_id;
    }
}

/// One exact component/artifact pair within a coordinated manual option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinatedCandidateItem {
    component_id: ComponentId,
    artifact_id: ArtifactId,
}

impl CoordinatedCandidateItem {
    pub(super) const fn new(component_id: ComponentId, artifact_id: ArtifactId) -> Self {
        Self {
            component_id,
            artifact_id,
        }
    }

    /// Returns the component selected by this item.
    pub fn component_id(&self) -> &ComponentId {
        &self.component_id
    }

    /// Returns the exact selected artifact.
    pub fn artifact_id(&self) -> &ArtifactId {
        &self.artifact_id
    }
}

/// A backend-coordinated, manually selectable composite release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinatedCandidateOption {
    option_id: String,
    release: PackageRelease,
    items: Vec<CoordinatedCandidateItem>,
}

impl CoordinatedCandidateOption {
    pub(super) fn new(
        option_id: String,
        release: PackageRelease,
        items: Vec<CoordinatedCandidateItem>,
    ) -> Self {
        Self {
            option_id,
            release,
            items,
        }
    }

    /// Returns the stable SHA-256 identity of this option.
    pub fn option_id(&self) -> &str {
        &self.option_id
    }

    /// Returns the user-facing release metadata.
    pub const fn release(&self) -> &PackageRelease {
        &self.release
    }

    /// Returns selected component/artifact pairs in component-id order.
    pub fn items(&self) -> &[CoordinatedCandidateItem] {
        &self.items
    }
}

/// Complete candidate projection for one game, including coordinated options
/// that span multiple component groups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateSelection {
    groups: Vec<ComponentReplacementCandidates>,
    streamline_options: Vec<CoordinatedCandidateOption>,
}

impl CandidateSelection {
    pub(super) fn new(
        groups: Vec<ComponentReplacementCandidates>,
        streamline_options: Vec<CoordinatedCandidateOption>,
    ) -> Self {
        Self {
            groups,
            streamline_options,
        }
    }

    /// Returns per-component replacement candidates.
    pub fn groups(&self) -> &[ComponentReplacementCandidates] {
        &self.groups
    }

    /// Returns backend-coordinated Streamline manual options.
    pub fn streamline_options(&self) -> &[CoordinatedCandidateOption] {
        &self.streamline_options
    }

    /// Consumes the projection when only per-component candidates are needed.
    pub fn into_groups(self) -> Vec<ComponentReplacementCandidates> {
        self.groups
    }

    /// Consumes the projection into its transport-facing parts.
    pub fn into_parts(
        self,
    ) -> (
        Vec<ComponentReplacementCandidates>,
        Vec<CoordinatedCandidateOption>,
    ) {
        (self.groups, self.streamline_options)
    }
}

/// One replacement artifact that can be applied to a component file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacementCandidate {
    artifact_id: ArtifactId,
    file_name: String,
    file_path: Option<PathRef>,
    technical_version: Option<Version>,
    release_label: Option<String>,
    sha256: String,
    source_game_id: Option<GameId>,
    comparison: CandidateComparison,
    catalog_package: Option<CatalogCandidatePackage>,
    is_downloaded: bool,
    is_debug: bool,
    /// Sort-only: prefer CDN/cache over game-folder observations.
    trust_level: ArtifactTrustLevel,
    /// Sort-only: multi-file packages sort ahead of single-file twins.
    file_count: usize,
    /// Context-free package identity used for deterministic ordering and deduplication.
    intrinsic_identity: Option<IntrinsicPackageIdentity>,
    /// Actual transition identity after component-aware target resolution.
    resolved_identity: Option<ResolvedTransitionIdentity>,
    d3d12_executable_action: Option<D3d12ExecutableAction>,
}

impl ReplacementCandidate {
    /// Builds a candidate from an artifact and the already-computed comparison
    /// verdict. The matcher computes [`CandidateComparison`] (and rejects an
    /// incompatible artifact) before calling this, so this constructor is pure.
    pub(super) fn new(
        artifact: &LibraryArtifact,
        comparison: CandidateComparison,
        is_downloaded: bool,
        catalog_package: Option<CatalogCandidatePackage>,
        intrinsic_identity: Option<IntrinsicPackageIdentity>,
        resolved_identity: Option<ResolvedTransitionIdentity>,
    ) -> Self {
        let is_debug = catalog_package
            .as_ref()
            .is_some_and(|package| package.release().channel == ReleaseChannel::Debug);
        Self {
            artifact_id: artifact.id().clone(),
            file_name: artifact.file_name().to_owned(),
            file_path: if is_downloaded {
                Some(artifact.path().clone())
            } else {
                None
            },
            // Technical comparison is strictly PE FileVersion. Exact package
            // identity is carried independently in `catalog_package`.
            technical_version: artifact.version().cloned(),
            release_label: artifact.metadata().release_label().map(str::to_owned),
            sha256: artifact.sha256().as_str().to_owned(),
            source_game_id: artifact.source_game_id().cloned(),
            comparison,
            catalog_package,
            is_downloaded,
            is_debug,
            trust_level: artifact.trust_level(),
            // Domain forbids empty file lists; len is the package size for sort.
            file_count: artifact.files().len(),
            intrinsic_identity,
            resolved_identity,
            d3d12_executable_action: None,
        }
    }

    /// Attaches the shared D3D12 executable assessment computed by the matcher.
    #[must_use]
    pub(super) fn with_d3d12_executable_action(
        mut self,
        action: Option<D3d12ExecutableAction>,
    ) -> Self {
        self.d3d12_executable_action = action;
        self
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

    /// Returns the primary DLL's technical PE FileVersion, when known.
    pub fn technical_version(&self) -> Option<&Version> {
        self.technical_version.as_ref()
    }

    /// Returns the supplemental upstream release label.
    pub fn release_label(&self) -> Option<&str> {
        self.release_label.as_deref()
    }

    /// Returns the source game where the candidate was observed, when known.
    pub fn source_game_id(&self) -> Option<&GameId> {
        self.source_game_id.as_ref()
    }

    /// Returns how confidently the candidate can be compared to the current component.
    pub fn comparison(&self) -> CandidateComparison {
        self.comparison
    }

    /// Returns the internally consistent catalog package projection.
    pub const fn catalog_package(&self) -> Option<&CatalogCandidatePackage> {
        self.catalog_package.as_ref()
    }

    pub(super) const fn trust_level(&self) -> ArtifactTrustLevel {
        self.trust_level
    }

    pub(super) fn canonical_source_rank(&self) -> u8 {
        match self.catalog_package.as_ref() {
            Some(package) => package.canonical_source_rank(),
            None => 2,
        }
    }

    pub(super) const fn file_count(&self) -> usize {
        self.file_count
    }

    /// Returns the executable action required by this D3D12 candidate.
    pub const fn d3d12_executable_action(&self) -> Option<&D3d12ExecutableAction> {
        self.d3d12_executable_action.as_ref()
    }

    pub(super) const fn intrinsic_identity(&self) -> Option<&IntrinsicPackageIdentity> {
        self.intrinsic_identity.as_ref()
    }

    pub(super) const fn resolved_identity(&self) -> Option<&ResolvedTransitionIdentity> {
        self.resolved_identity.as_ref()
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
    /// Every replaced component version is equal.
    EqualVersion,
    /// Composite component versions move in opposite directions.
    MixedVersion,
}

impl CandidateComparison {
    /// Returns the stable CLI text for this comparison result.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NewerVersion => "newer_version",
            Self::UnknownVersion => "unknown_version",
            Self::OlderVersion => "older_version",
            Self::EqualVersion => "equal_version",
            Self::MixedVersion => "mixed_version",
        }
    }
}
