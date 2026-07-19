//! Target / layout revalidation after unlocked prepare.

use tempfile::tempdir;

use super::dgvoodoo::PreparedDgVoodooUpdate;
use super::ensure_prepared_target_still_matches;
use super::host::PreparedHostUpdate;
use super::layout::UpdateLayout;
use super::prepare::{PreparedFullUpdate, PreparedHostOnly, PreparedUpdate};
use super::test_fixtures::{empty_payload, empty_record, resolved_target_with_proxy};
use crate::addons::reshade::InstallRoots;

#[test]
fn prepared_full_rejects_game_dir_drift() {
    let old = tempdir().expect("old");
    let new = tempdir().expect("new");
    let record = empty_record(new.path());
    let prepared = PreparedUpdate::Full(Box::new(PreparedFullUpdate {
        target: resolved_target_with_proxy(old.path(), "dxgi.dll"),
        payload: empty_payload(),
        host: PreparedHostUpdate::unchanged(&record),
        dgvoodoo: PreparedDgVoodooUpdate::unchanged(&record),
        dependency_paths: Vec::new(),
    }));
    let layout = UpdateLayout {
        game_dir: new.path().to_path_buf(),
        payload_dir: new.path().to_path_buf(),
        roots: InstallRoots::resolve_from_ini(new.path()),
    };
    let current = resolved_target_with_proxy(new.path(), "dxgi.dll");
    assert!(ensure_prepared_target_still_matches(&prepared, &layout, Some(&current)).is_err());
}

#[test]
fn prepared_full_accepts_matching_target() {
    let dir = tempdir().expect("dir");
    let record = empty_record(dir.path());
    let prepared = PreparedUpdate::Full(Box::new(PreparedFullUpdate {
        target: resolved_target_with_proxy(dir.path(), "dxgi.dll"),
        payload: empty_payload(),
        host: PreparedHostUpdate::unchanged(&record),
        dgvoodoo: PreparedDgVoodooUpdate::unchanged(&record),
        dependency_paths: Vec::new(),
    }));
    let layout = UpdateLayout {
        game_dir: dir.path().to_path_buf(),
        payload_dir: dir.path().to_path_buf(),
        roots: InstallRoots::resolve_from_ini(dir.path()),
    };
    let current = resolved_target_with_proxy(dir.path(), "dxgi.dll");
    assert!(ensure_prepared_target_still_matches(&prepared, &layout, Some(&current)).is_ok());
}

#[test]
fn prepared_host_only_without_writes_still_requires_live_target() {
    let dir = tempdir().expect("dir");
    let record = empty_record(dir.path());
    let prepared = PreparedUpdate::HostOnly(Box::new(PreparedHostOnly {
        target: resolved_target_with_proxy(dir.path(), "dxgi.dll"),
        sources: Vec::new(),
        host: PreparedHostUpdate::unchanged(&record),
        dgvoodoo: PreparedDgVoodooUpdate::unchanged(&record),
        addon_version: record.addon_version().map(str::to_owned),
    }));
    let layout = UpdateLayout {
        game_dir: dir.path().to_path_buf(),
        payload_dir: dir.path().to_path_buf(),
        roots: InstallRoots::resolve_from_ini(dir.path()),
    };
    assert!(ensure_prepared_target_still_matches(&prepared, &layout, None).is_err());
    let current = resolved_target_with_proxy(dir.path(), "dxgi.dll");
    assert!(ensure_prepared_target_still_matches(&prepared, &layout, Some(&current)).is_ok());
}
