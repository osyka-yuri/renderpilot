//! Application layer for RenderPilot.
//!
//! This crate contains application-level errors, ports, persistence-facing
//! records, and small shared application metadata.
//!
//! The application layer depends on `renderpilot-domain`, but does not depend
//! on infrastructure details such as SQLite, filesystem access, launchers,
//! network APIs, or UI frameworks.

mod candidates;
mod error;
mod info;
mod operation_plan;
mod persistence;
mod ports;

pub use candidates::{
    CandidateComparison, CandidateContext, ComponentReplacementCandidates, ReplacementCandidate,
    find_replacement_candidates,
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
    InstalledAddonRepository, OperationRepository,
};
