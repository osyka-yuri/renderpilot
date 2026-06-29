use std::path::{Path, PathBuf};

use crate::ServiceError;

use super::errors;
use super::reshade::{
    self, ReshadeHost, ReshadeHostAction, ReshadeIdentity, ReshadeScan, SlotActivity,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostConflictKind {
    MultipleHosts,
    InactiveSlot,
    WeakIdentity,
}

#[derive(Debug, Clone)]
pub(super) struct HostAssessment {
    pub host: ReshadeHost,
    pub conflict: bool,
    pub action: ReshadeHostAction,
    pub target_path: PathBuf,
    pub slot: String,
    conflict_kind: Option<HostConflictKind>,
}

impl HostAssessment {
    pub(super) fn writes_host(&self) -> bool {
        !self.conflict && self.action.writes_host()
    }

    pub(super) fn ensure_not_conflicting(&self, proxy_dll_name: &str) -> Result<(), ServiceError> {
        let Some(kind) = self.conflict_kind else {
            return Ok(());
        };
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
                "the '{proxy_dll_name}' slot RenoDX needs is occupied by a non-ReShade file; remove or relocate it before installing"
            ),
        };
        Err(errors::invalid(message))
    }
}

pub(super) fn assess(game_dir: &Path, proxy_dll_name: &str) -> HostAssessment {
    let scan = reshade::scan_reshade_hosts(game_dir, Some(proxy_dll_name));
    assess_scan(game_dir, proxy_dll_name, &scan)
}

fn assess_scan(game_dir: &Path, proxy_dll_name: &str, scan: &ReshadeScan) -> HostAssessment {
    let multiple_hosts = scan.has_multiple_reshade_hosts();
    let host = scan.primary_host();
    let action = reshade::host_action(&host);
    let present = host.as_present();

    let conflict_kind = if multiple_hosts {
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
    }
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::Version;

    use super::*;
    use crate::addons::renodx::reshade::{ActiveSlotReason, ActiveSlotState, ReshadeAddonSupport};

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
