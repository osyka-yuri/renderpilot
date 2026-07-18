//! Managed dgVoodoo2 dependency support for Luma profiles.
//!
//! The dependency has a stricter trust model than the Luma release payload:
//! its archive and every extracted file are pinned by size and SHA-256. The
//! facade keeps that fetch policy separate from local inspection, lifecycle
//! models, and the file-operation plan used by installation and updates.

mod fetch;
mod inspection;
mod model;
mod plan;

pub(crate) use fetch::fetch;
pub(crate) use inspection::{
    adopted_existing, advisory_wrapper_source, assess_existing, game_file_names,
    historical_dependency_basenames, is_dependency_basename, map_needs_ownership_sync,
    owned_status, record_can_manage_runtime, record_owns_any_map_dest, requirement, reused_config,
};
// Used by unit tests in this module; not needed on the production install path
// (management authority uses `record_can_manage_runtime` instead).
#[cfg(test)]
pub(crate) use inspection::record_owns_runtime;
pub(crate) use model::{
    DgVoodooInstall, DgVoodooPreparation, ExistingDgVoodoo, OwnedDgVoodooStatus, PreparedDgVoodoo,
};
pub(crate) use plan::{install_ops, merged_config, reuse_ops};

#[cfg(test)]
use fetch::{read_mapped_file, verify_archive_identity};
#[cfg(test)]
use inspection::{
    config_is_adoptable, is_compatible_inspection, normalized_requirement_version,
    owned_status_from_inspection,
};
#[cfg(test)]
pub(crate) use model::{AdoptedDgVoodoo, PreparedDgVoodooFile, ReusedDgVoodoo};
#[cfg(test)]
use plan::{config_sections, managed_config_default};

#[cfg(test)]
mod tests;
