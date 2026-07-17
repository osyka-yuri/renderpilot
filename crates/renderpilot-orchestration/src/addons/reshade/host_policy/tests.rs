use renderpilot_domain::Version;

use super::*;
use crate::addons::renodx::test_support::{MACHINE_AMD64, PE32_PLUS_MAGIC, build_pe_with_exports};
use crate::addons::reshade::scan::{ActiveSlotReason, ActiveSlotState, ReshadeAddonSupport};

fn present_host(
    slot: &str,
    addon_support: ReshadeAddonSupport,
    identity: ReshadeIdentity,
    state: SlotActivity,
) -> ReshadeHost {
    ReshadeHost::Present {
        path: PathBuf::from(format!(r"C:\Games\Test\{slot}")),
        slot: slot.to_owned(),
        version: Some(Version::parse("6.7.3").expect("version")),
        addon_support,
        identity,
        active: ActiveSlotState {
            state,
            reason: ActiveSlotReason::DetectedByMatcher,
        },
    }
}

fn assess_scan(
    game_dir: &Path,
    proxy_dll_name: &str,
    scan: &ReshadeScan,
    tool_name: &'static str,
    min_host_version: Option<&Version>,
) -> HostAssessment {
    assess_scan_with_allowed_addons(
        game_dir,
        proxy_dll_name,
        scan,
        tool_name,
        min_host_version,
        &[],
    )
}

fn assess_hosts(hosts: Vec<ReshadeHost>) -> HostAssessment {
    assess_scan(
        Path::new(r"C:\Games\Test"),
        "dxgi.dll",
        &ReshadeScan { hosts },
        "RenoDX",
        None,
    )
}

#[test]
fn absent_host_writes_requested_slot() {
    let assessment = assess_hosts(vec![]);

    assert_eq!(assessment.action, ReshadeHostAction::UpdateHost);
    assert!(!assessment.conflict);
    assert!(assessment.writes_host());
    assert_eq!(assessment.slot, "dxgi.dll");
    assert!(assessment.target_path.ends_with("dxgi.dll"));
}

#[test]
fn active_full_host_is_reused() {
    let assessment = assess_hosts(vec![present_host(
        "dxgi.dll",
        ReshadeAddonSupport::Full,
        ReshadeIdentity::Confirmed,
        SlotActivity::Active,
    )]);

    assert_eq!(assessment.action, ReshadeHostAction::UpToDate);
    assert!(!assessment.conflict);
    assert!(!assessment.writes_host());
    assert_eq!(assessment.lifecycle, HostLifecycle::AdoptEmpty);
    assert_eq!(assessment.lifecycle, HostLifecycle::AdoptEmpty);
}

#[test]
fn active_host_without_addon_support_is_reinstalled() {
    let assessment = assess_hosts(vec![present_host(
        "dxgi.dll",
        ReshadeAddonSupport::None,
        ReshadeIdentity::Confirmed,
        SlotActivity::Active,
    )]);

    assert_eq!(
        assessment.action,
        ReshadeHostAction::ReinstallWithAddonSupport
    );
    assert!(!assessment.conflict);
    assert!(assessment.writes_host());
}

#[test]
fn empty_present_host_without_addon_support_is_repaired_on_first_install() {
    let assessment = assess_hosts(vec![present_host(
        "dxgi.dll",
        ReshadeAddonSupport::None,
        ReshadeIdentity::Confirmed,
        SlotActivity::Active,
    )]);

    assert_eq!(assessment.lifecycle, HostLifecycle::RepairEmpty);
    assert!(assessment.ensure_initial_installable("dxgi.dll").is_ok());
}

#[test]
fn foreign_addon_preserves_a_compatible_host_and_blocks_repair() {
    let dir = tempfile::tempdir().expect("tempdir");
    let full = build_pe_with_exports(
        MACHINE_AMD64,
        PE32_PLUS_MAGIC,
        &[
            "ReShadeVersion",
            "ReShadeRegisterAddon",
            "ReShadeUnregisterAddon",
            "ReShadeRegisterEvent",
            "ReShadeUnregisterEvent",
        ],
    );
    std::fs::write(dir.path().join("dxgi.dll"), &full).expect("host");
    std::fs::write(dir.path().join("foreign.addon64"), b"foreign").expect("addon");

    let compatible = assess(dir.path(), "dxgi.dll");
    assert_eq!(compatible.lifecycle, HostLifecycle::ReuseUser);
    assert!(compatible.ensure_initial_installable("dxgi.dll").is_ok());

    let ordinary = build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &["ReShadeVersion"]);
    std::fs::write(dir.path().join("dxgi.dll"), ordinary).expect("ordinary host");
    let incompatible = assess(dir.path(), "dxgi.dll");
    assert_eq!(incompatible.lifecycle, HostLifecycle::Conflict);
    assert!(incompatible.ensure_initial_installable("dxgi.dll").is_err());
}

#[test]
fn active_host_with_unknown_addon_support_is_repaired() {
    let assessment = assess_hosts(vec![present_host(
        "dxgi.dll",
        ReshadeAddonSupport::Unknown,
        ReshadeIdentity::Probable,
        SlotActivity::Active,
    )]);

    assert_eq!(assessment.action, ReshadeHostAction::RepairHost);
    assert!(!assessment.conflict);
    assert!(assessment.writes_host());
}

#[test]
fn weak_active_slot_is_conflict() {
    let assessment = assess_hosts(vec![present_host(
        "dxgi.dll",
        ReshadeAddonSupport::Unknown,
        ReshadeIdentity::Weak,
        SlotActivity::Active,
    )]);

    assert_eq!(assessment.action, ReshadeHostAction::Conflict);
    assert!(assessment.conflict);
    assert!(!assessment.writes_host());
}

#[test]
fn min_version_gate_rewrites_an_older_but_otherwise_reusable_host() {
    // A confirmed, active, full-support host that would normally be reused,
    // but whose version is below the tool's minimum -> rewrite, not reuse.
    let host = present_host(
        "dxgi.dll",
        ReshadeAddonSupport::Full,
        ReshadeIdentity::Confirmed,
        SlotActivity::Active,
    );
    let min = Version::parse("6.7.5").expect("version");
    let assessment = assess_scan(
        Path::new(r"C:\Games\Test"),
        "dxgi.dll",
        &ReshadeScan { hosts: vec![host] },
        "Luma",
        Some(&min),
    );

    assert_eq!(
        assessment.action,
        ReshadeHostAction::ReinstallWithAddonSupport
    );
    assert!(!assessment.conflict);
    assert!(assessment.writes_host());
}

#[test]
fn min_version_gate_reuses_a_new_enough_host() {
    let host = present_host(
        "dxgi.dll",
        ReshadeAddonSupport::Full,
        ReshadeIdentity::Confirmed,
        SlotActivity::Active,
    );
    let min = Version::parse("6.7.0").expect("version");
    let assessment = assess_scan(
        Path::new(r"C:\Games\Test"),
        "dxgi.dll",
        &ReshadeScan { hosts: vec![host] },
        "Luma",
        Some(&min),
    );

    assert_eq!(assessment.action, ReshadeHostAction::UpToDate);
    assert!(!assessment.writes_host());
}

#[test]
fn recognized_custom_build_is_conflict_even_with_confirmed_identity() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("GShade64.dll"), b"gshade-runtime").expect("write");
    let host = ReshadeHost::Present {
        path: dir.path().join("dxgi.dll"),
        slot: "dxgi.dll".to_owned(),
        version: Some(Version::parse("6.7.3").expect("version")),
        addon_support: ReshadeAddonSupport::Full,
        identity: ReshadeIdentity::Confirmed,
        active: ActiveSlotState {
            state: SlotActivity::Active,
            reason: ActiveSlotReason::DetectedByMatcher,
        },
    };

    let assessment = assess_scan(
        dir.path(),
        "dxgi.dll",
        &ReshadeScan { hosts: vec![host] },
        "RenoDX",
        None,
    );

    assert!(assessment.conflict);
    assert!(assessment.is_known_custom_build());
    assert!(!assessment.writes_host());
}

#[test]
fn recognized_custom_build_wins_over_multiple_hosts() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("GShade64.dll"), b"gshade-runtime").expect("write");
    let confirmed = |slot: &str| ReshadeHost::Present {
        path: dir.path().join(slot),
        slot: slot.to_owned(),
        version: Some(Version::parse("6.7.3").expect("version")),
        addon_support: ReshadeAddonSupport::Full,
        identity: ReshadeIdentity::Confirmed,
        active: ActiveSlotState {
            state: SlotActivity::Inactive,
            reason: ActiveSlotReason::DetectedByMatcher,
        },
    };

    let assessment = assess_scan(
        dir.path(),
        "dxgi.dll",
        &ReshadeScan {
            hosts: vec![confirmed("dxgi.dll"), confirmed("d3d11.dll")],
        },
        "RenoDX",
        None,
    );

    assert!(assessment.is_known_custom_build());
}

#[test]
fn recognized_custom_build_wins_over_weak_identity() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("GShade64.dll"), b"gshade-runtime").expect("write");
    let weak = ReshadeHost::Present {
        path: dir.path().join("dxgi.dll"),
        slot: "dxgi.dll".to_owned(),
        version: None,
        addon_support: ReshadeAddonSupport::Unknown,
        identity: ReshadeIdentity::Weak,
        active: ActiveSlotState {
            state: SlotActivity::Active,
            reason: ActiveSlotReason::DetectedByMatcher,
        },
    };

    let assessment = assess_scan(
        dir.path(),
        "dxgi.dll",
        &ReshadeScan { hosts: vec![weak] },
        "RenoDX",
        None,
    );

    assert!(assessment.is_known_custom_build());
}

#[test]
fn custom_build_marker_alone_does_not_block_a_normal_fresh_install() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("GShade64.dll"), b"gshade-runtime").expect("write");

    let assessment = assess_scan(
        dir.path(),
        "dxgi.dll",
        &ReshadeScan { hosts: Vec::new() },
        "RenoDX",
        None,
    );

    assert!(!assessment.is_known_custom_build());
    assert!(!assessment.conflict);
    assert!(assessment.writes_host());
}

#[test]
fn inactive_reshade_slot_is_conflict() {
    let assessment = assess_hosts(vec![present_host(
        "ReShade64.dll",
        ReshadeAddonSupport::Full,
        ReshadeIdentity::Confirmed,
        SlotActivity::Inactive,
    )]);

    assert_eq!(assessment.action, ReshadeHostAction::Conflict);
    assert!(assessment.conflict);
    assert!(!assessment.writes_host());
}

#[test]
fn multiple_reshade_hosts_are_conflict_even_with_active_full_host() {
    let assessment = assess_hosts(vec![
        present_host(
            "dxgi.dll",
            ReshadeAddonSupport::Full,
            ReshadeIdentity::Confirmed,
            SlotActivity::Active,
        ),
        present_host(
            "ReShade64.dll",
            ReshadeAddonSupport::Full,
            ReshadeIdentity::Probable,
            SlotActivity::Inactive,
        ),
    ]);

    assert_eq!(assessment.action, ReshadeHostAction::UpToDate);
    assert!(assessment.conflict);
    assert!(!assessment.writes_host());
}
