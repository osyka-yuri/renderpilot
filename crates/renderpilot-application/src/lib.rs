//! Application layer for RenderPilot.
//!
//! This crate contains application-level errors, ports, persistence-facing
//! records, and small shared application metadata.
//!
//! The application layer depends on `renderpilot-domain`, but does not depend
//! on infrastructure details such as SQLite, filesystem access, launchers,
//! network APIs, or UI frameworks.

mod candidates;
mod compatibility;
mod dxc;
mod error;
mod info;
mod operation_plan;
mod persistence;
mod ports;
mod transition;

pub use candidates::{
    CandidateComparison, CandidateContext, ComponentReplacementCandidates, InstalledReleaseState,
    ReplacementCandidate, find_replacement_candidates,
};
pub use compatibility::{
    SwapCompatibilityError, SwapTargetProfile, ensure_replacement_compatible,
    ensure_swap_compatible, validate_runtime_artifact,
};
pub use error::{AppError, AppErrorKind, AppResult, invalid_operation_state_display_message};
pub use info::{AppInfo, app_info};
pub use operation_plan::{
    OperationPlan, OperationPlanBlocker, OperationPlanFile, OperationPlanFileAction,
    OperationPlanRiskLevel, OperationPlanWarning, build_swap_operation_plan,
};

pub use persistence::{
    MetadataJson, OperationItemRecord, OperationJournalEntry, OperationKind, OperationRecord,
    OperationStatus, UnixTimestampMillis,
};

pub use ports::{
    ArtifactRepository, ComponentDetector, ComponentRepository, GameRepository, GameSourceProvider,
    InstalledAddonRepository, OperationRepository, SharedArtifactRepository,
};
pub use transition::{resolve_transition_members, resolve_transition_removals};
