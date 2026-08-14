//! Narrow Windows ownership wrappers used only by the portable supervisor.

pub mod directory;
mod error_dialog;
pub mod file;
pub mod handle;
pub mod job;
pub mod object;
pub mod process;

pub(crate) use error_dialog::show_portable_supervisor_failure;
