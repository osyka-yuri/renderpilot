use serde::{Deserialize, Serialize};

/// Public leftover category in the frozen desktop contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LeftoverCategory {
    /// A per-game Vulkan application registration.
    VulkanRegistration,
    /// A recorded Engine.ini configuration receipt.
    EngineConfig,
    /// An OptiScaler file with current canonical ownership.
    OptiscalerFile,
    /// An installed add-on file with current canonical ownership.
    AddonFile,
    /// A library component file with current canonical ownership.
    ComponentFile,
    /// Private custody data retained for safe restoration.
    PrivateCustody,
    /// A recorded directory eligible only for exact empty-directory removal.
    Directory,
}

/// Why an item cannot currently be safely handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LeftoverIssueCode {
    /// No supported canonical owner identifies the item.
    UnsupportedOwner,
    /// Available evidence does not prove ownership of the item.
    UnprovenOwnership,
    /// The file differs from its recorded owned contents.
    ModifiedFile,
    /// The original file is retained in private custody.
    PrivateOriginalCustody,
    /// A pending operation belongs to another feature or owner.
    ForeignPending,
    /// A pending operation conflicts with this cleanup action.
    PendingConflict,
    /// The target needed to carry out an owned operation is unavailable.
    UnavailableTarget,
    /// Native state could not be observed through an authorized source.
    NativeAuthorityUnavailable,
    /// The installation is active or has another current owner.
    ActiveOwnershipConflict,
    /// The action no longer matches its listed proposal.
    StaleIntent,
    /// An operation failed while handling this item.
    OperationFailed,
}

/// Optional localized-independent diagnostic for a proposal item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeftoverIssue {
    /// Machine-readable reason the item is blocked or an operation failed.
    pub code: LeftoverIssueCode,
    /// Optional human-readable context for the issue.
    pub detail: Option<String>,
}

/// One concrete canonical ownership item shown to the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeftoverItem {
    /// Opaque identifier accepted only with its current proposal intent.
    pub item_id: String,
    /// Kind of owned item represented by this entry.
    pub category: LeftoverCategory,
    /// Owned filesystem path or executable path, when applicable.
    pub path: Option<String>,
    /// Whether the current item can be cleaned, retried, or is blocked.
    pub disposition: LeftoverDisposition,
    /// Optional reason the item cannot be handled now.
    pub issue: Option<LeftoverIssue>,
}

/// Whether a future explicit action can handle an item now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LeftoverDisposition {
    /// Current ownership evidence permits exact cleanup.
    Cleanable,
    /// The exact owned pending operation may use its existing recovery path.
    Retryable,
    /// The item must remain untouched under current evidence.
    Blocked,
}

/// Root classification admitted for an absent registered installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LeftoverRootState {
    /// The registered installation root is confirmed absent.
    Missing,
    /// The root contains only residue tied to current canonical owners.
    ResidueOnly,
    /// The root is empty after the installation has retired.
    RetiredEmpty,
}

/// One opaque proposal. Only `gameId` and `intent` are accepted for actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeftoverProposal {
    /// Registered game identity for this proposal.
    pub game_id: String,
    /// Display name captured for the current proposal.
    pub game_name: String,
    /// Registered installation path captured for the proposal.
    pub install_path: String,
    /// Current classification of the registered installation root.
    pub root_state: LeftoverRootState,
    /// Opaque token binding Clean and Leave requests to the latest proposal.
    pub intent: String,
    /// Semantic revision used to suppress this exact set of leftovers.
    pub leave_revision: String,
    /// Current canonical residue and supported external registrations.
    pub items: Vec<LeftoverItem>,
    /// Whether at least one listed item is currently eligible for Clean.
    pub can_clean: bool,
}

/// One isolated per-game issue from list discovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeftoverGameIssue {
    /// Registered game identity, when one could be established.
    pub game_id: Option<String>,
    /// Issue that prevented or qualified discovery for this game.
    pub issue: LeftoverIssue,
}

/// Read-only list result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListRetiredGameLeftoversOutput {
    /// Actionable proposals for registered retired installations.
    pub proposals: Vec<LeftoverProposal>,
    /// Per-game discovery issues and unbound owner records.
    pub issues: Vec<LeftoverGameIssue>,
}

/// Frozen request shape shared by Clean and Leave transports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetiredGameLeftoversRequest {
    /// Registered game identity named by the current proposal.
    pub game_id: String,
    /// Opaque intent token from the current List result.
    pub intent: String,
}

/// Result of one concrete cleanup action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupStep {
    /// Opaque item identifier from the reviewed proposal.
    pub item_id: String,
    /// Kind of item handled by this step.
    pub category: LeftoverCategory,
    /// Exact result of this item-level operation.
    pub outcome: CleanupStepOutcome,
    /// Optional reason the operation was blocked or failed.
    pub issue: Option<LeftoverIssue>,
}

/// Exact finite outcome for one cleanup step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CleanupStepOutcome {
    /// The exact owned local file or directory was removed.
    Removed,
    /// The exact current external owner association was released.
    Released,
    /// The target was already absent under current ownership evidence.
    AlreadyAbsent,
    /// The matching existing recovery operation completed.
    Recovered,
    /// A foreign operation was preserved without recovery.
    PreservedForeign,
    /// Current evidence did not permit the requested operation.
    Blocked,
    /// The item operation failed.
    Failed,
}

/// Result of a concrete cleanup action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupOutcome {
    /// Registered game identity named by the request.
    pub game_id: String,
    /// Overall result across the attempted item steps.
    pub status: CleanupStatus,
    /// Per-item results from this cleanup attempt.
    pub steps: Vec<CleanupStep>,
    /// Fresh proposal for residue that remains after the attempt.
    pub remaining_proposal: Option<LeftoverProposal>,
    /// Optional issue affecting the overall action.
    pub issue: Option<LeftoverIssue>,
}

/// Finite overall cleanup status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CleanupStatus {
    /// Every currently actionable item was handled or is gone.
    Complete,
    /// Some items were handled and actionable residue remains.
    Partial,
    /// Current evidence prevented cleanup.
    Blocked,
    /// The request no longer matches the current proposal.
    Stale,
}

/// Result of a Leave action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaveOutcome {
    /// Registered game identity named by the request.
    pub game_id: String,
    /// Whether the current proposal was left or had changed.
    pub status: LeaveStatus,
    /// Semantic revision suppressed by this action, when successful.
    pub leave_revision: Option<String>,
    /// Fresh proposal when the request was stale.
    pub remaining_proposal: Option<LeftoverProposal>,
    /// Optional reason Leave could not be applied.
    pub issue: Option<LeftoverIssue>,
}

/// Finite Leave status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LeaveStatus {
    /// The current semantic revision was suppressed.
    Left,
    /// The request no longer matches current residue.
    Stale,
}
