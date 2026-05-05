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
    find_replacement_candidates, CandidateComparison, CandidateWarning,
    ComponentFileReplacementCandidates, ReplacementCandidate,
};
pub use error::{AppError, AppErrorKind, AppResult};
pub use info::{app_info, AppInfo};
pub use operation_plan::{
    build_swap_operation_plan, OperationPlan, OperationPlanBlocker, OperationPlanRiskLevel,
    OperationPlanWarning,
};

pub use persistence::{
    BackupId, BackupRecord, MetadataJson, OperationItemRecord, OperationKind, OperationRecord,
    OperationStatus, UnixTimestampMillis,
};

pub use ports::{
    ArtifactRepository, BackupRepository, ComponentDetector, ComponentRepository, GameRepository,
    GameSourceProvider, OperationRepository,
};
