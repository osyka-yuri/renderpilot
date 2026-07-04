//! Deciding what to do about a detected ReShade host for a specific tool.
//!
//! [`assess`] is the RenoDX-shaped entry point (tool name "RenoDX", no minimum
//! host version); [`assess_for_tool`] generalizes it so Luma can name itself and
//! gate on a minimum ReShade version (a host older than Luma's add-on API would
//! silently refuse to load its add-on, so an under-min host is rewritten rather
//! than reused). RenoDX passing `None` yields byte-identical decisions and
//! messages.

use std::path::{Path, PathBuf};

use renderpilot_domain::Version;

use crate::ServiceError;

use super::scan::{
    self, ReshadeHost, ReshadeHostAction, ReshadeIdentity, ReshadeScan, SlotActivity,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostConflictKind {
    MultipleHosts,
    InactiveSlot,
    WeakIdentity,
    KnownCustomBuild,
}

#[derive(Debug, Clone)]
pub(crate) struct HostAssessment {
    pub host: ReshadeHost,
    pub conflict: bool,
    pub action: ReshadeHostAction,
    pub target_path: PathBuf,
    pub slot: String,
    conflict_kind: Option<HostConflictKind>,
    tool_name: &'static str,
}

impl HostAssessment {
    pub(crate) fn writes_host(&self) -> bool {
        !self.conflict && self.action.writes_host()
    }

    /// Whether the active slot is occupied by a recognized non-ReShade build
    /// (e.g. GShade) a tool must never silently replace. See
    /// [`scan::is_known_custom_build`].
    pub(crate) fn is_known_custom_build(&self) -> bool {
        self.conflict_kind == Some(HostConflictKind::KnownCustomBuild)
    }

    pub(crate) fn ensure_not_conflicting(&self, proxy_dll_name: &str) -> Result<(), ServiceError> {
        let Some(kind) = self.conflict_kind else {
            return Ok(());
        };
        let tool = self.tool_name;
        let message = match kind {
            HostConflictKind::MultipleHosts => {
                "multiple ReShade hosts were found; resolve the active proxy slot before installing or updating"
                    .to_owned()
            }
            HostConflictKind::InactiveSlot => {
                "ReShade is present, but not in the proxy slot this game will load; refusing to place a second host automatically"
                    .to_owned()
            }
            HostConflictKind::WeakIdentity => format!(
                "the '{proxy_dll_name}' slot {tool} needs is occupied by a non-ReShade file; remove or relocate it before installing"
            ),
            HostConflictKind::KnownCustomBuild => format!(
                "a recognized custom ReShade build (e.g. GShade) occupies this slot; {tool} never replaces it automatically"
            ),
        };
        Err(ServiceError::InvalidInput(message))
    }
}

/// RenoDX-shaped assessment: tool name "RenoDX", no minimum host version.
pub(crate) fn assess(game_dir: &Path, proxy_dll_name: &str) -> HostAssessment {
    assess_for_tool(game_dir, proxy_dll_name, "RenoDX", None)
}

/// Assessment for a named tool, optionally gating on a minimum ReShade host
/// version. A present, active, full-support host below `min_host_version` (or with
/// an unreadable version) is not a conflict — it is rewritten with the tool's
/// channel build so its add-on will actually load.
pub(crate) fn assess_for_tool(
    game_dir: &Path,
    proxy_dll_name: &str,
    tool_name: &'static str,
    min_host_version: Option<&Version>,
) -> HostAssessment {
    let scan = scan::scan_reshade_hosts(game_dir, Some(proxy_dll_name));
    assess_scan(game_dir, proxy_dll_name, &scan, tool_name, min_host_version)
}

fn assess_scan(
    game_dir: &Path,
    proxy_dll_name: &str,
    scan: &ReshadeScan,
    tool_name: &'static str,
    min_host_version: Option<&Version>,
) -> HostAssessment {
    let multiple_hosts = scan.has_multiple_reshade_hosts();
    let host = scan.primary_host();
    let mut action = scan::host_action(&host);
    let present = host.as_present();

    // Checked first, ahead of every other conflict kind: a recognized custom
    // build's own proxy stub can otherwise read as `WeakIdentity` (its identity
    // can't be trusted) or, with more than one aliased slot, `MultipleHosts` —
    // either would report the wrong reason. Gated on at least one scanned
    // candidate so an unrelated, leftover `GShade64.dll` with nothing at all
    // resembling a host in the folder doesn't block a normal fresh install.
    let conflict_kind = if !scan.hosts.is_empty() && scan::is_known_custom_build(game_dir, None) {
        Some(HostConflictKind::KnownCustomBuild)
    } else if multiple_hosts {
        Some(HostConflictKind::MultipleHosts)
    } else if let Some(present) = present {
        if present.active.state != SlotActivity::Active {
            Some(HostConflictKind::InactiveSlot)
        } else if present.identity < ReshadeIdentity::Probable {
            Some(HostConflictKind::WeakIdentity)
        } else {
            None
        }
    } else {
        None
    };

    // Minimum-version gate: only when there is no conflict and the host would
    // otherwise be reused as-is. An under-min (or unreadable) version means the
    // tool's add-on would refuse to load, so rewrite the host instead of reusing.
    if conflict_kind.is_none()
        && action == ReshadeHostAction::UpToDate
        && let Some(min) = min_host_version
        && let Some(present) = present
    {
        action = match present.version {
            Some(version) if version >= min => ReshadeHostAction::UpToDate,
            Some(_) => ReshadeHostAction::ReinstallWithAddonSupport,
            None => ReshadeHostAction::RepairHost,
        };
    }

    let target_path = present
        .map(|host| host.path.to_path_buf())
        .unwrap_or_else(|| game_dir.join(proxy_dll_name));
    let slot = present
        .map(|host| host.slot.to_owned())
        .unwrap_or_else(|| proxy_dll_name.to_owned());

    HostAssessment {
        host,
        conflict: conflict_kind.is_some(),
        action,
        target_path,
        slot,
        conflict_kind,
        tool_name,
    }
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::Version;

    use super::*;
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
        // but whose version is below the tool's minimum → rewrite, not reuse.
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
}
