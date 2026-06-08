//! Orchestration layer for RenderPilot.
//!
//! Provides typed feature results and `ServiceError` for use by both the CLI
//! and the GUI API facade. This crate owns heavy infrastructure dependencies
//! (network, filesystem, compression) and exposes purely typed Rust results —
//! no `serde_json::Value` responses are produced here.

pub mod catalog;
/// Application-wide orchestration context and state management.
pub mod context;
pub mod covers;
pub mod dlss;
mod error;
mod fs_sync;
pub mod libraries;
pub mod nvapi;
pub mod portable;
pub mod storage;

pub use context::Context;

pub use error::ServiceError;

pub use renderpilot_application as application;
pub use renderpilot_detection as detection;
pub use renderpilot_domain as domain;
