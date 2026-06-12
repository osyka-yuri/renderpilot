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
    find_replacement_candidates, CandidateComparison, CandidateContext,
    ComponentReplacementCandidates, ReplacementCandidate,
};
pub use error::{invalid_operation_state_display_message, AppError, AppErrorKind, AppResult};
pub use info::{app_info, AppInfo};
pub use operation_plan::{
    build_swap_operation_plan, OperationPlan, OperationPlanBlocker, OperationPlanFile,
    OperationPlanFileAction, OperationPlanRiskLevel, OperationPlanWarning,
};

pub use persistence::{
    MetadataJson, OperationItemRecord, OperationJournalEntry, OperationKind, OperationRecord,
    OperationStatus, UnixTimestampMillis,
};

pub use ports::{
    ArtifactRepository, ComponentDetector, ComponentRepository, GameRepository, GameSourceProvider,
    OperationRepository,
};
