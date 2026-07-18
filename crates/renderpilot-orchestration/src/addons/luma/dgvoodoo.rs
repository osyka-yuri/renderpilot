//! Managed dgVoodoo2 dependency support for Luma profiles.
//!
//! The dependency has a stricter trust model than the Luma release payload:
//! its archive and every extracted file are pinned by size and SHA-256. The
//! facade keeps that fetch policy separate from local inspection, lifecycle
//! models, and the file-operation plan used by installation and updates.

mod fetch;
mod model;
mod plan;

pub(crate) use fetch::fetch;
