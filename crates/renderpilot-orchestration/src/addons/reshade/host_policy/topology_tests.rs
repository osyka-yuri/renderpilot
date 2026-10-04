use std::{
    fs::{self, FileTimes},
    path::Path,
    time::{Duration, SystemTime},
};

use renderpilot_domain::{PathRef, Version};

use super::{
    HostLifecycle, TopologyHostAssessment, assess_topology_downstream_for_tool,
    assess_topology_downstream_from_snapshot, probe_topology_host_download,
};
use crate::addons::reshade::scan::{
    ReshadeAddonSupport, ReshadeContent, ReshadeHostAction, ReshadeIdentity,
};
use crate::peer_mutation_executor::{PeerPathSnapshot, observe_peer_path_snapshot};

fn topology_assess(game_root: &Path, min_host_version: Option<&Version>) -> TopologyHostAssessment {
    assess_topology_downstream_for_tool(
        game_root,
        &game_root.join("ReShade64.dll"),
        "Luma",
        min_host_version,
    )
    .expect("topology assessment")
}

fn compatible_reshade_host() -> Vec<u8> {
    crate::addons::test_support::build_pe_with_exports(
        crate::addons::test_support::MACHINE_AMD64,
        crate::addons::test_support::PE32_PLUS_MAGIC,
        &[
            "ReShadeVersion",
            "ReShadeRegisterAddon",
            "ReShadeUnregisterAddon",
            "ReShadeRegisterEvent",
        ],
    )
}

fn path_ref(path: &Path) -> PathRef {
    PathRef::new(path.to_string_lossy().into_owned()).expect("path")
}

fn retained_host(root: &Path, bytes: &[u8]) -> (PathRef, PeerPathSnapshot) {
    let host = root.join("ReShade64.dll");
    std::fs::write(&host, bytes).expect("host");
    let root_ref = path_ref(root);
    let host_ref = path_ref(&host);
    let snapshot = observe_peer_path_snapshot(&host_ref, &root_ref).expect("snapshot");
    (host_ref, snapshot)
}

#[test]
fn topology_ignores_outer_proxy_and_missing_exact_downstream_is_install_new() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("dxgi.dll"), b"OptiScaler outer").expect("outer");
    std::fs::write(dir.path().join("ReShade.ini"), b"[GENERAL]\r\n").expect("ini");

    let assessment = topology_assess(dir.path(), None);

    assert_eq!(assessment.snapshot().lifecycle, HostLifecycle::InstallNew);
    assert_eq!(assessment.snapshot().action, ReshadeHostAction::UpdateHost);
    assert!(!assessment.snapshot().present);
    assert!(assessment.requires_host_download());
    assert!(!assessment.initial_is_conflict());
}

#[test]
fn topology_external_addon_path_does_not_reintroduce_outer_host_conflict() {
    let dir = tempfile::tempdir().expect("tempdir");
    let external = tempfile::tempdir().expect("external");
    std::fs::write(dir.path().join("dxgi.dll"), b"OptiScaler outer").expect("outer");
    std::fs::write(
        dir.path().join("ReShade.ini"),
        format!("[ADDON]\r\nAddonPath={}\r\n", external.path().display()),
    )
    .expect("ini");

    let first = topology_assess(dir.path(), None);
    let second = topology_assess(dir.path(), None);

    assert_eq!(first.snapshot().lifecycle, HostLifecycle::InstallNew);
    assert_eq!(first, second);
}

#[test]
fn topology_present_compatible_host_is_adopted_without_a_write() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bytes = compatible_reshade_host();
    std::fs::write(dir.path().join("dxgi.dll"), b"OptiScaler outer").expect("outer");
    std::fs::write(dir.path().join("ReShade64.dll"), &bytes).expect("host");

    let assessment = topology_assess(dir.path(), None);
    let digest = renderpilot_detection::sha256_bytes(&bytes).expect("digest");

    assert_eq!(assessment.snapshot().lifecycle, HostLifecycle::AdoptEmpty);
    assert_eq!(assessment.snapshot().action, ReshadeHostAction::UpToDate);
    assert!(!assessment.requires_host_download());
    assert_eq!(assessment.snapshot().digest.as_ref(), Some(&digest));
    assert_eq!(assessment.snapshot().length, Some(bytes.len() as u64));
    assert_eq!(
        assessment.snapshot().identity,
        Some(ReshadeIdentity::Confirmed)
    );
    assert_eq!(
        assessment.snapshot().addon_support,
        Some(ReshadeAddonSupport::Full)
    );
}

#[test]
fn topology_minimum_version_repairs_empty_and_conflicts_with_user_content() {
    let empty = tempfile::tempdir().expect("empty");
    std::fs::write(
        empty.path().join("ReShade64.dll"),
        compatible_reshade_host(),
    )
    .expect("host");
    let minimum = Version::parse("99.0.0").expect("version");
    let repaired = topology_assess(empty.path(), Some(&minimum));
    assert_eq!(repaired.snapshot().lifecycle, HostLifecycle::RepairEmpty);
    assert!(repaired.requires_host_download());

    let user = tempfile::tempdir().expect("user");
    std::fs::write(user.path().join("ReShade64.dll"), compatible_reshade_host()).expect("host");
    std::fs::write(user.path().join("ReShadePreset.ini"), b"preset").expect("preset");
    let conflict = topology_assess(user.path(), Some(&minimum));
    assert_eq!(conflict.snapshot().lifecycle, HostLifecycle::Conflict);
    assert!(conflict.initial_is_conflict());
    assert!(!conflict.requires_host_download());
}

#[test]
fn topology_ignores_neighboring_custom_runtime_for_exact_file_decision() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("ReShade64.dll"), compatible_reshade_host()).expect("host");
    std::fs::write(dir.path().join("GShade64.dll"), b"custom runtime").expect("custom");

    let assessment = topology_assess(dir.path(), None);

    assert_eq!(assessment.snapshot().lifecycle, HostLifecycle::AdoptEmpty);
    assert!(!assessment.assessment().is_known_custom_build());
}

#[test]
fn topology_unidentified_exact_file_is_weak_conflict_without_download() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("ReShade64.dll"), b"not a PE").expect("host");

    let assessment = topology_assess(dir.path(), None);

    assert_eq!(assessment.snapshot().identity, Some(ReshadeIdentity::Weak));
    assert_eq!(assessment.snapshot().lifecycle, HostLifecycle::Conflict);
    assert!(assessment.initial_is_conflict());
    assert!(!assessment.requires_host_download());
}

#[test]
fn topology_snapshot_changes_for_bytes_but_not_mtime() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("ReShade64.dll");
    fs::write(&path, compatible_reshade_host()).expect("host");
    let first = topology_assess(dir.path(), None);

    let mtime = SystemTime::UNIX_EPOCH
        .checked_add(Duration::from_secs(1))
        .expect("representable mtime");
    fs::set_times(&path, FileTimes::new().set_modified(mtime)).expect("mtime");
    let same_bytes = topology_assess(dir.path(), None);
    assert_eq!(first, same_bytes);

    fs::write(&path, b"changed bytes").expect("changed host");
    let changed = topology_assess(dir.path(), None);
    assert_ne!(first.snapshot(), changed.snapshot());
}

#[test]
fn topology_rejects_wrong_or_traversing_paths() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let cases = [
        root.join("dxgi.dll"),
        root.join("nested").join("ReShade64.dll"),
        root.join(".").join("ReShade64.dll"),
        root.join("nested").join("..").join("ReShade64.dll"),
    ];

    for path in cases {
        assert!(
            assess_topology_downstream_for_tool(root, &path, "Luma", None).is_err(),
            "path should be rejected: {}",
            path.display()
        );
    }
}

#[test]
fn topology_rejects_non_regular_exact_downstream() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("ReShade64.dll");
    std::fs::create_dir(&path).expect("directory");
    assert!(assess_topology_downstream_for_tool(dir.path(), &path, "Luma", None).is_err());

    #[cfg(unix)]
    {
        let link_named = dir.path().join("ReShade64.dll");
        std::fs::remove_dir(&link_named).expect("remove directory");
        std::os::unix::fs::symlink(dir.path().join("target.dll"), &link_named)
            .expect("exact symlink");
        assert!(
            assess_topology_downstream_for_tool(dir.path(), &link_named, "Luma", None).is_err()
        );
    }
}

#[test]
fn retained_download_probe_distinguishes_absent_full_and_repair() {
    let absent = tempfile::tempdir().expect("absent");
    let absent_path = path_ref(&absent.path().join("ReShade64.dll"));
    let absent_snapshot = observe_peer_path_snapshot(&absent_path, &path_ref(absent.path()))
        .expect("absent snapshot");
    assert!(
        probe_topology_host_download(absent.path(), &absent_path, &absent_snapshot, None,)
            .expect("absent probe")
    );

    let full = tempfile::tempdir().expect("full");
    let (full_path, full_snapshot) = retained_host(full.path(), &compatible_reshade_host());
    assert!(
        !probe_topology_host_download(full.path(), &full_path, &full_snapshot, None,)
            .expect("full probe")
    );

    let repair = tempfile::tempdir().expect("repair");
    let repair_bytes = crate::addons::test_support::build_pe_with_exports(
        crate::addons::test_support::MACHINE_AMD64,
        crate::addons::test_support::PE32_PLUS_MAGIC,
        &["ReShadeVersion"],
    );
    let (repair_path, repair_snapshot) = retained_host(repair.path(), &repair_bytes);
    assert!(
        probe_topology_host_download(repair.path(), &repair_path, &repair_snapshot, None,)
            .expect("repair probe")
    );
}

#[test]
fn retained_download_probe_rejects_weak_identity_and_wrong_exact_path() {
    let root = tempfile::tempdir().expect("root");
    let (host_path, snapshot) = retained_host(root.path(), b"not a PE");
    assert!(probe_topology_host_download(root.path(), &host_path, &snapshot, None).is_err());

    let wrong = path_ref(&root.path().join("dxgi.dll"));
    assert!(probe_topology_host_download(root.path(), &wrong, &snapshot, None).is_err());

    let custom = tempfile::tempdir().expect("custom");
    let mut custom_bytes = crate::addons::test_support::build_nvidia_dlss_pe([6, 7, 0, 0]);
    let nvidia_utf16 = b"N\0V\0I\0D\0I\0A\0";
    let gshade_utf16 = b"G\0S\0h\0a\0d\0e\0";
    if custom_bytes.len() >= nvidia_utf16.len() {
        for offset in 0..=custom_bytes.len() - nvidia_utf16.len() {
            if custom_bytes[offset..offset + nvidia_utf16.len()] == nvidia_utf16[..] {
                custom_bytes[offset..offset + nvidia_utf16.len()].copy_from_slice(gshade_utf16);
            }
        }
    }
    let (custom_path, custom_snapshot) = retained_host(custom.path(), &custom_bytes);
    assert!(
        probe_topology_host_download(custom.path(), &custom_path, &custom_snapshot, None,).is_err()
    );
}

#[test]
fn retained_assessment_uses_supplied_content_without_reading_configuration() {
    let root = tempfile::tempdir().expect("root");
    let (host_path, snapshot) = retained_host(root.path(), &compatible_reshade_host());

    let empty = assess_topology_downstream_from_snapshot(
        root.path(),
        &host_path,
        &snapshot,
        ReshadeContent::Empty,
        "Luma",
        None,
    )
    .expect("empty assessment");
    assert_eq!(empty.snapshot().lifecycle, HostLifecycle::AdoptEmpty);

    std::fs::write(
        root.path().join("ReShade.ini"),
        b"[GENERAL]\r\nCurrentPresetPath=ReShadePreset.ini\r\n",
    )
    .expect("ini");
    let still_empty = assess_topology_downstream_from_snapshot(
        root.path(),
        &host_path,
        &snapshot,
        ReshadeContent::Empty,
        "Luma",
        None,
    )
    .expect("retained empty assessment");
    assert_eq!(still_empty.snapshot().lifecycle, HostLifecycle::AdoptEmpty);

    let user = assess_topology_downstream_from_snapshot(
        root.path(),
        &host_path,
        &snapshot,
        ReshadeContent::UserContent,
        "Luma",
        None,
    )
    .expect("user assessment");
    assert_eq!(user.snapshot().lifecycle, HostLifecycle::ReuseUser);
}

#[test]
fn retained_assessment_marks_repair_empty_and_user_content_conflict() {
    let root = tempfile::tempdir().expect("root");
    let bytes = crate::addons::test_support::build_pe_with_exports(
        crate::addons::test_support::MACHINE_AMD64,
        crate::addons::test_support::PE32_PLUS_MAGIC,
        &["ReShadeVersion"],
    );
    let (host_path, snapshot) = retained_host(root.path(), &bytes);
    let minimum = Version::parse("99.0.0").expect("minimum");

    let repair = assess_topology_downstream_from_snapshot(
        root.path(),
        &host_path,
        &snapshot,
        ReshadeContent::Empty,
        "Luma",
        Some(&minimum),
    )
    .expect("repair assessment");
    assert_eq!(repair.snapshot().lifecycle, HostLifecycle::RepairEmpty);

    let conflict = assess_topology_downstream_from_snapshot(
        root.path(),
        &host_path,
        &snapshot,
        ReshadeContent::UserContent,
        "Luma",
        Some(&minimum),
    )
    .expect("conflict assessment");
    assert_eq!(conflict.snapshot().lifecycle, HostLifecycle::Conflict);
}

#[test]
fn retained_x86_host_is_rejected_by_probe_and_phase_three_assessment() {
    let root = tempfile::tempdir().expect("root");
    let bytes = crate::addons::test_support::build_pe_with_exports(
        crate::addons::test_support::MACHINE_I386,
        crate::addons::test_support::PE32_MAGIC,
        &[
            "ReShadeVersion",
            "ReShadeRegisterAddon",
            "ReShadeUnregisterAddon",
            "ReShadeRegisterEvent",
        ],
    );
    let (host_path, snapshot) = retained_host(root.path(), &bytes);

    assert!(probe_topology_host_download(root.path(), &host_path, &snapshot, None).is_err());
    assert!(
        assess_topology_downstream_from_snapshot(
            root.path(),
            &host_path,
            &snapshot,
            ReshadeContent::Empty,
            "Luma",
            None,
        )
        .is_err()
    );
}
