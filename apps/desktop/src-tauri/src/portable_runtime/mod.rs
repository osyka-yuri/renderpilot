//! Practical portable updater runtime.
//!
//! The stable raw supervisor owns all mutation. A generated App only receives
//! authenticated startup and two private permits; no copied helper, probe,
//! resume mode, UI confirmation, or self-replacement path exists here.

pub mod activation;
pub mod app_process;
pub mod app_protocol;
pub mod bootstrap;
pub mod cleanup;
pub mod epoch_namespace;
pub mod error;
pub mod generation;
pub mod health;
pub mod journal;
pub mod migration;
pub mod process_admission;
mod provenance;
pub mod publication;
mod random;
pub mod recovery;
pub mod request_gate;
pub mod rpu;
pub mod runtime_paths;
pub mod selection;
pub mod signature;
pub mod snapshot;
pub mod staging;
pub mod supervisor;
pub mod supervisor_activation;
pub mod supervisor_updates;
pub mod win32;

#[cfg(test)]
mod tests;
