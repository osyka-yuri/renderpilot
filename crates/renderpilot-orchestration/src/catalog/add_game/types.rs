//! Public value types for the inspect-and-confirm add-game contract.

use std::path::PathBuf;

use renderpilot_domain::{InstallRoot, RootAuthority};

use crate::catalog::RootCorrectionAssessment;

/// Confidence assigned specifically to a root recommendation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootRecommendationConfidence {
    /// The root is backed by a launcher manifest.
    Authoritative,
    /// Filesystem evidence makes this root a useful recommendation.
    Suggested,
}

/// Physical role of the selected directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallBoundaryKind {
    /// The selected directory is the boundary of exactly one installation.
    SingleInstall,
    /// The selected directory is an engine project below its distribution root.
    EngineProjectSubtree,
    /// The selected directory is an executable-only subtree below the game root.
    BinarySubtree,
    /// The selected directory is a container for exactly one nested installation.
    SingleInstallContainer,
    /// The selected directory contains multiple independent installations.
    MultipleInstallContainer,
    /// Available evidence does not establish one safe installation boundary.
    Ambiguous,
    /// Traversal faults or limits prevented a reliable boundary decision.
    Incomplete,
}

/// Completeness of evidence used for a boundary decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraversalCompleteness {
    /// Every path required for the decision was inspected.
    Complete,
    /// At least one required path could not be inspected.
    Incomplete,
}

/// Stable evidence category suitable for transport mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InstallBoundaryEvidence {
    /// Exact installation root supplied by a supported launcher manifest.
    LauncherManifest,
    /// Engine topology identifies an outer distribution root.
    EngineDistributionRoot,
    /// A playable PE executable is located directly in the candidate root.
    RootExecutable,
    /// Engine-specific directory topology contributed to the decision.
    EngineStructure,
    /// Known component layout contributed non-authoritative context.
    ComponentContext,
    /// An independent accepted-executable branch contributed to the boundary.
    ExecutableBranch,
}

/// Source that produced a recommendation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootRecommendationSource {
    /// Exact installation root supplied by a launcher manifest.
    LauncherManifest,
    /// Existing catalog identity used only to preserve an already confirmed root.
    ExistingCatalog,
    /// Outer distribution root established by engine topology.
    EngineDistributionRoot,
    /// Candidate root established by its own playable executable.
    RootExecutable,
    /// Component layout supplied supporting context.
    ComponentContext,
}

/// Structured selected-root boundary facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallBoundaryInspection {
    /// Classified physical role of the selected directory.
    pub kind: InstallBoundaryKind,
    /// Reliability of the filesystem evidence.
    pub completeness: TraversalCompleteness,
    /// Canonical roots of independent installations found below the selection.
    pub candidate_roots: Vec<InstallRoot>,
    /// Evidence categories that produced the classification.
    pub evidence: Vec<InstallBoundaryEvidence>,
}

/// Structured alternative root recommendation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootRecommendationInspection {
    /// Canonical recommended installation root.
    pub root: InstallRoot,
    /// Highest-priority evidence source for the recommendation.
    pub source: RootRecommendationSource,
    /// Whether the recommendation is exact or heuristic.
    pub confidence: RootRecommendationConfidence,
    /// Reliability of the traversal used for the recommendation.
    pub completeness: TraversalCompleteness,
    /// Evidence categories supporting the recommendation.
    pub evidence: Vec<InstallBoundaryEvidence>,
    /// Semantic fingerprint of the recommended root's own boundary,
    /// relationship, correction state, and executable identities.
    pub(crate) effective_fingerprint: String,
}

/// Relationship between a candidate root and the current catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallRelationshipKind {
    /// No catalog installation overlaps the candidate.
    New,
    /// The candidate is already cataloged.
    ExactExisting,
    /// The candidate sits inside an existing installation.
    InsideExisting,
    /// The candidate is a parent correction for one manual installation.
    ExpandsExisting,
    /// The candidate is a proven child correction for one overly broad manual
    /// installation root.
    NarrowsExisting,
    /// The candidate contains one launcher-proven installation whose exact
    /// root must be used instead.
    ContainsProvenInstall,
    /// The candidate contains multiple catalog installations.
    ContainsMultiple,
}

/// Catalog relationship evidence returned by inspection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallRelationship {
    /// Relationship category.
    pub kind: InstallRelationshipKind,
    /// Existing game identities involved in the relationship.
    pub game_ids: Vec<String>,
    /// Launcher-manifest roots involved even when no card exists yet.
    pub proven_install_roots: Vec<InstallRoot>,
}

/// One executable presented by add-game inspection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutableInspection {
    /// Absolute normalized path.
    pub path: String,
    /// Path relative to the inspected root.
    pub relative_path: String,
    /// File size observed during inspection.
    pub size_bytes: u64,
    /// Ranking score from executable detection.
    pub rank_score: i32,
    /// Whether DOS and PE signatures were readable.
    pub valid_windows_pe: bool,
    /// Stable rejection category for launcher/setup/helper candidates.
    pub rejection_kind: Option<String>,
    /// Exact heuristic token that caused rejection.
    pub rejection_token: Option<String>,
}

/// Non-fatal inspection or add-game diagnostic with variant-specific data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddGameWarning {
    /// Proven false legacy cards were consolidated.
    LegacyCardsConsolidated {
        /// Number of removed false cards.
        count: usize,
    },
    /// Ambiguous legacy cards were retained.
    LegacyCardsRetained {
        /// Number of retained ambiguous cards.
        count: usize,
    },
    /// A durable consolidation recovery bundle was published.
    RecoveryBundleCreated {
        /// Published bundle path.
        path: String,
    },
    /// Root-correction history was archived before narrowing.
    RootCorrectionHistoryArchived {
        /// Published bundle path.
        path: String,
    },
    /// The selected directory could not be traversed completely.
    FilesystemProbeError,
    /// The selected directory is inside an existing installation.
    InsideExistingInstall,
    /// The selected directory safely narrows one legacy manual root.
    NarrowsExistingInstall,
    /// Multiple proven installs overlap the selection.
    MultipleProvenInstalls,
    /// One proven installation lies below the selection.
    ContainsProvenInstall,
    /// All readable executables were rejected by the ranking policy.
    ExplicitExecutableRequired,
    /// The selection has no readable Windows PE executable.
    NoReadableExecutable,
}

/// Root path selected from an inspection decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AddGameRootChoice {
    /// Use the folder originally selected by the user.
    Selected,
    /// Use the backend's recommended installation root.
    Recommended,
}

/// Catalog mutation derived by the backend from current catalog facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AddGameCatalogAction {
    /// Add a new catalog installation.
    Add,
    /// Re-scan an exact existing installation.
    Rescan,
    /// Correct the root of one existing manual installation.
    CorrectExistingRoot,
}

/// One currently valid add-game choice and its backend-derived action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct AddGameOption {
    /// Which inspected root will be used.
    pub root_choice: AddGameRootChoice,
    /// Action derived from current catalog state.
    pub catalog_action: AddGameCatalogAction,
}

/// Stable reason why inspection produced no valid action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AddGameUnavailableReason {
    /// The selected directory contains multiple independent installations.
    MultipleInstalls,
    /// The selection contains one proven installation whose exact root is required.
    ContainsProvenInstall,
    /// The selection overlaps multiple existing or launcher-proven installs.
    ContainsMultipleCatalogInstalls,
    /// The selection is inside an existing root and cannot safely correct it.
    InsideExistingInstall,
    /// No readable game executable established an addable installation.
    NoReadableExecutable,
    /// Managed state blocks the only possible root correction.
    RootCorrectionBlocked,
}

/// Complete backend decision for the inspected selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddGameDecision {
    /// No user review is required.
    Automatic {
        /// The single option safe to execute without review.
        option: AddGameOption,
    },
    /// The user must choose one of a non-empty set of options.
    Review(AddGameReview),
    /// No safe catalog action is currently available.
    Unavailable {
        /// Stable reasons no option can be accepted.
        reasons: Vec<AddGameUnavailableReason>,
    },
}

impl AddGameDecision {
    /// Returns the backend-derived option for a root choice, when that choice is allowed.
    #[must_use]
    pub fn option_for(&self, root_choice: AddGameRootChoice) -> Option<AddGameOption> {
        match self {
            Self::Automatic { option } => (option.root_choice == root_choice).then_some(*option),
            Self::Review(review) => review
                .options()
                .iter()
                .copied()
                .find(|option| option.root_choice == root_choice),
            Self::Unavailable { .. } => None,
        }
    }
}

/// Validated review state with a non-empty option set and an in-set default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddGameReview {
    default_option: AddGameOption,
    options: Vec<AddGameOption>,
}

impl AddGameReview {
    /// Creates a review only when at least one option exists and the default belongs to it.
    ///
    /// Options are canonicalized and the default is placed first so every
    /// transport presents the same initial action without leaking enum order.
    pub fn new(default_option: AddGameOption, mut options: Vec<AddGameOption>) -> Option<Self> {
        options.sort();
        options.dedup();
        let default_index = options
            .iter()
            .position(|option| option == &default_option)?;
        options.swap(0, default_index);
        Some(Self {
            default_option,
            options,
        })
    }

    /// Backend-selected initial choice.
    pub const fn default_option(&self) -> AddGameOption {
        self.default_option
    }

    /// Non-empty set of choices accepted by confirmation.
    pub fn options(&self) -> &[AddGameOption] {
        &self.options
    }
}

/// Read-only result shown before an installation is accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddGameInspection {
    /// Canonical user-selected root.
    pub selected_root: InstallRoot,
    /// Deterministic token binding confirmation to the inspected filesystem and
    /// catalog assessment.
    pub inspection_fingerprint: String,
    /// Process-local catalog generation used by this decision.
    pub catalog_generation: u64,
    /// Filesystem role and evidence of the selected directory.
    pub boundary: InstallBoundaryInspection,
    /// Better root suggested by authoritative or heuristic evidence.
    pub recommendation: Option<RootRecommendationInspection>,
    /// Catalog overlap classification.
    pub relationship: InstallRelationship,
    /// Ranked executable candidates.
    pub executables: Vec<ExecutableInspection>,
    /// True when a rejected executable must be explicitly selected.
    pub requires_explicit_executable: bool,
    /// Structured safety assessment for changing one manual installation root.
    pub root_correction: Option<RootCorrectionAssessment>,
    /// Backend-owned review and action policy.
    pub decision: AddGameDecision,
    /// Non-fatal diagnostics.
    pub warnings: Vec<AddGameWarning>,
}

/// Confirmed request to add exactly one game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddGameRequest {
    /// Folder originally selected in the directory picker.
    pub selected_root: PathBuf,
    /// Which backend-provided root option the caller accepted.
    pub root_choice: AddGameRootChoice,
    /// Explicit permission for a backend-derived root correction.
    pub allow_root_correction: bool,
    /// Explicit executable override when every candidate was rejected by ranking.
    pub chosen_executable: Option<PathBuf>,
    /// Fingerprint returned by the exact inspection being confirmed.
    pub inspection_fingerprint: String,
}

/// Observable result of an add-game command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddGameDisposition {
    /// A new game was added.
    Added,
    /// The existing game already matched the filesystem.
    Unchanged,
    /// Existing catalog facts changed.
    Updated,
    /// One manual game's root was corrected without changing its identity.
    RootCorrected,
}

/// Singular add-game command result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddGameResult {
    /// Stable game identity.
    pub game_id: String,
    /// Root actually scanned.
    pub effective_root: String,
    /// High-level mutation outcome.
    pub disposition: AddGameDisposition,
    /// Evidence persisted for the root.
    pub root_authority: RootAuthority,
    /// Number of detected component files.
    pub detected_library_count: usize,
    /// Proven false legacy cards consolidated into this game.
    pub consolidated_game_ids: Vec<String>,
    /// Durable recovery bundle created before destination-wins conflicts.
    pub recovery_bundle_path: Option<String>,
    /// Non-fatal diagnostics.
    pub warnings: Vec<AddGameWarning>,
}

impl AddGameResult {
    /// Stable transport/storage spelling of the persisted root authority.
    pub const fn root_authority_name(&self) -> &'static str {
        match self.root_authority {
            RootAuthority::LauncherManifest => "launcher_manifest",
            RootAuthority::UserConfirmed => "user_confirmed",
            RootAuthority::Legacy => "legacy",
        }
    }
}
