//! Classic `.bak` sidecar path naming and filesystem operations.
//!
//! - [`naming`] -- pure path syntax (no I/O, no kind validation)
//! - [`ops`] -- verify / create / restore against the live file

pub(super) mod naming;
pub(super) mod ops;
