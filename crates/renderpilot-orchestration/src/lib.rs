//! Orchestration layer for RenderPilot.
//!
//! Provides typed feature results and `ServiceError` for use by both the CLI
//! and the GUI API facade. This crate owns heavy infrastructure dependencies
//! (network, filesystem, compression) and exposes purely typed Rust results —
//! no `serde_json::Value` responses are produced here.

pub mod addons;
mod app_dir;
pub mod catalog;
mod cdn;
/// Application-wide orchestration context and state management.
pub mod context;
mod coordinated_files;
pub mod covers;
pub mod dlss;
mod error;
mod file_mutation;
pub mod file_safety;
mod fs;
pub mod game_executable;
mod game_mutation_lock;
pub mod libraries;
/// Coordinated CDN manifest refresh (passive TTL vs forced + cooldown).
pub mod manifests;
pub(crate) mod mutation_boundary;
pub mod net;
pub mod nvapi;
mod paths;
pub mod portable;
pub mod storage;
mod util;

pub use context::Context;
pub use file_safety::{
    FileSafetyAuthority, GameFileSafetyAssessment, GameMutationSafetyPermits, GameSafetyPermit,
    SafetyScope, SharedVulkanSafetyAssessment, SharedVulkanSafetyPermit,
};

pub(crate) use error::failed;
pub use error::{InvalidInstallRootReason, ServiceError};

pub use renderpilot_application as application;
pub use renderpilot_detection as detection;
pub use renderpilot_domain as domain;
