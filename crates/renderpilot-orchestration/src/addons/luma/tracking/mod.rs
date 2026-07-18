//! Derives UI-facing Luma install state from a persisted record, and shared
//! record-rebuild helpers the update flow uses to fold a fresh receipt/refreshed
//! sources back into an existing record while preserving its identity and
//! persisted timestamps.

mod install_state;
mod paths;
mod sources;

#[cfg(test)]
mod tests;

use renderpilot_domain::InstalledAddon;

use crate::ServiceError;
use crate::addons::tracking;

pub(crate) use install_state::advisory_nightly_host_source;
pub(super) use install_state::install_state_from_record;
pub(crate) use paths::{
    owned_dependency_paths, owned_host_adjacent_paths, owns_path, payload_disk_intact,
    payload_owned_paths,
};
pub(crate) use sources::{
    payload_needs_provenance_bind, promote_advisory_payload_source, resolved_addon_version,
    source_has_bind_mark, try_mark_advisory_payload_checked, try_promote_advisory_payload,
};

#[cfg(test)]
pub(crate) use sources::{ADVISORY_PAYLOAD_CHECKED_MARK, mark_advisory_payload_source};

/// Rebuilds `record` from a fresh file/source list after an update's set-diff
/// apply, preserving its persisted timestamps. `parts.addon_file` is supplied by
/// the caller rather than re-derived, since an update that renames the main
/// `.addon` must point the rebuilt record at the new path.
pub(crate) fn rebuild(
    record: &InstalledAddon,
    parts: tracking::RebuildParts,
) -> Result<InstalledAddon, ServiceError> {
    tracking::rebuild_install_record(record, parts, tracking::PreserveMetadata::luma())
}
