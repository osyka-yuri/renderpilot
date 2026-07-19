//! Phase-3 revalidation after unlocked prepare.
//!
//! These guards refuse to apply a plan prepared against a snapshot that no
//! longer matches disk/DB or whose write targets have drifted.

use std::path::Path;

use renderpilot_domain::{InstalledAddon, TrackedSourceRole};

use super::layout::UpdateLayout;
use super::prepare::PreparedUpdate;
use crate::ServiceError;
use crate::addons::luma::errors;
use crate::addons::luma::use_cases::update_target::ResolvedUpdateTarget;
use crate::addons::records::source_with_role;
use crate::paths::same_path;

/// HostOnly plans skip the ZIP set-diff; refuse them when the tracked payload
/// is no longer intact after the unlocked prepare window.
pub(super) fn ensure_host_only_payload_still_intact(
    prepared: &PreparedUpdate,
    record: &InstalledAddon,
) -> Result<(), ServiceError> {
    if matches!(prepared, PreparedUpdate::HostOnly(_))
        && !crate::addons::luma::tracking::payload_disk_intact(record)
    {
        return Err(errors::state_changed_retry_update());
    }
    Ok(())
}

/// Fails when another install/update mutated the tracked record while network
/// prepare ran unlocked — applying a stale plan would desync disk and DB.
///
/// One compatible transition is allowed: concurrent deep-check / prepare may
/// promote advisory AddonPayload provenance to a real ZIP digest (same URL).
/// That rewrites only identity metadata, not disk layout.
pub(super) fn ensure_record_still_matches_snapshot(
    snapshot: &InstalledAddon,
    current: &InstalledAddon,
) -> Result<(), ServiceError> {
    if snapshot.game_id() != current.game_id()
        || snapshot.kind() != current.kind()
        || snapshot.addon_file() != current.addon_file()
        || snapshot.addon_version() != current.addon_version()
        || snapshot.created_files() != current.created_files()
        || snapshot.backed_up_files() != current.backed_up_files()
        || snapshot.managed_files() != current.managed_files()
        || snapshot.installed_at() != current.installed_at()
        || snapshot.host_kind() != current.host_kind()
        || snapshot.reshade_channel() != current.reshade_channel()
        || snapshot.registered_exe_path() != current.registered_exe_path()
        || snapshot.tracked_sources().len() != current.tracked_sources().len()
    {
        return Err(errors::state_changed_retry_update());
    }
    for snapshot_source in snapshot.tracked_sources() {
        let role = snapshot_source.role();
        let Some(current_source) = source_with_role(current, role) else {
            return Err(errors::state_changed_retry_update());
        };
        if snapshot_source != current_source
            && !payload_source_compatible(snapshot_source, current_source)
        {
            return Err(errors::state_changed_retry_update());
        }
    }
    Ok(())
}

/// Advisory→ZIP promote of AddonPayload (same URL) while prepare ran unlocked.
fn payload_source_compatible(
    snapshot: &renderpilot_domain::TrackedSource,
    current: &renderpilot_domain::TrackedSource,
) -> bool {
    snapshot.role() == TrackedSourceRole::AddonPayload
        && current.role() == TrackedSourceRole::AddonPayload
        && snapshot.is_advisory()
        && !current.is_advisory()
        && snapshot.url() == current.url()
}

/// Fails when the install target directory / proxy slot drifted while prepare
/// ran unlocked. Host and dgVoodoo writes use prepare-time absolute paths; if
/// the rendering-exe override (or analysis) now points elsewhere, applying the
/// plan would desync host writes from the payload set-diff root.
pub(super) fn ensure_prepared_target_still_matches(
    prepared: &PreparedUpdate,
    layout: &UpdateLayout,
    current_target: Option<&ResolvedUpdateTarget>,
) -> Result<(), ServiceError> {
    match prepared {
        PreparedUpdate::Full(full) => {
            let Some(current) = current_target else {
                return Err(errors::state_changed_retry_update());
            };
            if !same_path(&full.target.game_dir, &layout.game_dir)
                || !targets_match(&full.target, current)
            {
                return Err(errors::state_changed_retry_update());
            }
            ensure_component_paths_under_game_dir(
                &full.target.game_dir,
                full.host.write_path(),
                full.dgvoodoo.write_game_dir(),
            )?;
        }
        PreparedUpdate::HostOnly(host_only) => {
            let Some(current) = current_target else {
                return Err(errors::state_changed_retry_update());
            };
            if !same_path(&host_only.target.game_dir, &layout.game_dir)
                || !targets_match(&host_only.target, current)
            {
                return Err(errors::state_changed_retry_update());
            }
            let host_writes = host_only.host.writes();
            let dgv_writes = host_only.dgvoodoo.writes();
            if !host_writes && !dgv_writes {
                return Ok(());
            }
            if let Some(path) = host_only.host.write_path() {
                let expected = current.game_dir.join(&current.proxy_dll_name);
                if !same_path(path, &expected) {
                    return Err(errors::state_changed_retry_update());
                }
            }
            if let Some(game_dir) = host_only.dgvoodoo.write_game_dir()
                && !same_path(game_dir, &current.game_dir)
            {
                return Err(errors::state_changed_retry_update());
            }
        }
    }
    Ok(())
}

fn targets_match(prepared: &ResolvedUpdateTarget, current: &ResolvedUpdateTarget) -> bool {
    same_path(&prepared.game_dir, &current.game_dir)
        && prepared.asset == current.asset
        && prepared.addon_file == current.addon_file
        && prepared.arch == current.arch
        && prepared.proxy_dll_name == current.proxy_dll_name
        && prepared.external_requirement == current.external_requirement
}

fn ensure_component_paths_under_game_dir(
    game_dir: &Path,
    host_write: Option<&Path>,
    dgvoodoo_game_dir: Option<&Path>,
) -> Result<(), ServiceError> {
    if let Some(path) = host_write {
        let parent = path
            .parent()
            .ok_or_else(errors::state_changed_retry_update)?;
        if !same_path(parent, game_dir) {
            return Err(errors::state_changed_retry_update());
        }
    }
    if let Some(dir) = dgvoodoo_game_dir
        && !same_path(dir, game_dir)
    {
        return Err(errors::state_changed_retry_update());
    }
    Ok(())
}
