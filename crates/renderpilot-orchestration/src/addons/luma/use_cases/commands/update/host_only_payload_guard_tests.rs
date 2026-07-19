//! HostOnly plans must not apply when payload files disappeared during prepare.

use tempfile::tempdir;

use super::dgvoodoo::PreparedDgVoodooUpdate;
use super::ensure_host_only_payload_still_intact;
use super::host::PreparedHostUpdate;
use super::prepare::{PreparedFullUpdate, PreparedHostOnly, PreparedUpdate};
use super::test_fixtures::{empty_payload, record_with_addon, resolved_target};

fn host_only(record: &renderpilot_domain::InstalledAddon) -> PreparedUpdate {
    PreparedUpdate::HostOnly(Box::new(PreparedHostOnly {
        target: resolved_target(
            std::path::Path::new(record.addon_file().as_str())
                .parent()
                .expect("parent"),
        ),
        sources: Vec::new(),
        host: PreparedHostUpdate::unchanged(record),
        dgvoodoo: PreparedDgVoodooUpdate::unchanged(record),
        addon_version: record.addon_version().map(str::to_owned),
    }))
}

fn full_with_payload(
    game_dir: &std::path::Path,
    record: &renderpilot_domain::InstalledAddon,
) -> PreparedUpdate {
    PreparedUpdate::Full(Box::new(PreparedFullUpdate {
        target: resolved_target(game_dir),
        payload: empty_payload(),
        host: PreparedHostUpdate::unchanged(record),
        dgvoodoo: PreparedDgVoodooUpdate::unchanged(record),
        dependency_paths: Vec::new(),
    }))
}

#[test]
fn host_only_rejects_when_payload_disk_is_not_intact() {
    let dir = tempdir().expect("dir");
    let addon = dir.path().join("Luma-Game.addon");
    let record = record_with_addon(&addon);
    let prepared = host_only(&record);
    assert!(ensure_host_only_payload_still_intact(&prepared, &record).is_err());
}

#[test]
fn host_only_accepts_when_payload_disk_is_intact() {
    let dir = tempdir().expect("dir");
    let addon = dir.path().join("Luma-Game.addon");
    std::fs::write(&addon, b"addon").expect("write");
    let record = record_with_addon(&addon);
    let prepared = host_only(&record);
    assert!(ensure_host_only_payload_still_intact(&prepared, &record).is_ok());
}

#[test]
fn full_update_skips_payload_intact_guard() {
    let dir = tempdir().expect("dir");
    let addon = dir.path().join("Luma-Game.addon");
    let record = record_with_addon(&addon);
    let prepared = full_with_payload(dir.path(), &record);
    assert!(ensure_host_only_payload_still_intact(&prepared, &record).is_ok());
}
