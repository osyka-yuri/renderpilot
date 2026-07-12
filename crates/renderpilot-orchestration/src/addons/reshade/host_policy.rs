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
    self, ReshadeContent, ReshadeHost, ReshadeHostAction, ReshadeIdentity, ReshadeScan,
    SlotActivity,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostConflictKind {
    MultipleHosts,
    InactiveSlot,
    WeakIdentity,
    KnownCustomBuild,
}

/// First-install ownership decision for a proxy ReShade runtime.
///
/// This is intentionally separate from the raw host action: a tracked install
/// may use that action for maintenance later, but an untracked runtime is
/// replaced only after its content was proved empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostLifecycle {
    /// No host exists; install a managed Add-on build.
    InstallNew,
    /// A compatible runtime has user content or could not be fully inspected.
    ReuseUser,
    /// A compatible empty Add-on build is kept but becomes tool-owned.
    AdoptEmpty,
    /// An empty but inadequate runtime is replaced with an Add-on build.
    RepairEmpty,
    /// Replacing or adopting the detected runtime is unsafe.
    Conflict,
}

impl HostLifecycle {
    #[must_use]
    pub(crate) const fn writes_host(self) -> bool {
        matches!(self, Self::InstallNew | Self::RepairEmpty)
    }

    #[must_use]
    pub(crate) const fn owns_host(self) -> bool {
        matches!(
            self,
            Self::InstallNew | Self::AdoptEmpty | Self::RepairEmpty
        )
    }

    #[must_use]
    pub(crate) const fn is_conflict(self) -> bool {
        matches!(self, Self::Conflict)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct HostAssessment {
    pub host: ReshadeHost,
    pub conflict: bool,
    pub action: ReshadeHostAction,
    /// Ownership decision for an initial add-on install.
    pub lifecycle: HostLifecycle,
    pub target_path: PathBuf,
    pub slot: String,
    content: ReshadeContent,
    conflict_kind: Option<HostConflictKind>,
    tool_name: &'static str,
}

impl HostAssessment {
    /// Whether the raw host action writes the DLL. Maintenance/update flows for
    /// an already tracked install use this, not the initial lifecycle.
    #[cfg(test)]
    pub(crate) fn writes_host(&self) -> bool {
        !self.conflict && self.action.writes_host()
    }

    #[must_use]
    pub(crate) const fn initial_writes_host(&self) -> bool {
        self.lifecycle.writes_host()
    }

    #[must_use]
    pub(crate) const fn initial_owns_host(&self) -> bool {
        self.lifecycle.owns_host()
    }

    #[must_use]
    pub(crate) const fn initial_is_conflict(&self) -> bool {
        self.lifecycle.is_conflict()
    }

    /// Returns pre-existing paths the initial install is allowed to remove.
    /// Newly written files are captured by the engine receipt; this supplements
    /// it for an adopted DLL and an existing empty `ReShade.ini`.
    #[must_use]
    pub(crate) fn initial_owned_existing_paths(
        &self,
        reshade_ini_path: Option<&Path>,
    ) -> Vec<PathBuf> {
        if !self.initial_owns_host() || !self.content.is_empty() {
            return Vec::new();
        }

        let mut paths = Vec::new();
        if self.lifecycle == HostLifecycle::AdoptEmpty {
            paths.push(self.target_path.clone());
        }
        if let Some(ini_path) = reshade_ini_path {
            paths.push(ini_path.to_path_buf());
        }
        paths
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

    /// Ensures the first-install lifecycle is safe. Empty, recognized runtimes
    /// may be repaired in-place; user content is never replaced automatically.
    pub(crate) fn ensure_initial_installable(
        &self,
        proxy_dll_name: &str,
    ) -> Result<(), ServiceError> {
        self.ensure_not_conflicting(proxy_dll_name)?;

        if self.initial_is_conflict() {
            return Err(ServiceError::InvalidInput(format!(
                "the existing '{proxy_dll_name}' ReShade host is not compatible with {} and has user content or could not be safely inspected",
                self.tool_name
            )));
        }

        Ok(())
    }
}

/// RenoDX-shaped assessment: tool name "RenoDX", no minimum host version.
pub(crate) fn assess(game_dir: &Path, proxy_dll_name: &str) -> HostAssessment {
    assess_for_tool(game_dir, proxy_dll_name, "RenoDX", None)
}

/// Recovery-shaped assessment. The caller may allow the exact add-on payload it
/// has already identified locally; every other add-on remains user content.
pub(crate) fn assess_for_tool_with_allowed_addons(
    game_dir: &Path,
    proxy_dll_name: &str,
    tool_name: &'static str,
    min_host_version: Option<&Version>,
    allowed_addon_names: &[&str],
) -> HostAssessment {
    let scan = scan::scan_reshade_hosts(game_dir, Some(proxy_dll_name));
    assess_scan_with_allowed_addons(
        game_dir,
        proxy_dll_name,
        &scan,
        tool_name,
        min_host_version,
        allowed_addon_names,
    )
}

/// Assessment for a named tool, optionally gating on a minimum ReShade host
/// version. For first install, a present host below `min_host_version` (or with
/// an unreadable version) is repaired only when its content is provably empty;
/// tracked-install maintenance continues to use the raw host action.
pub(crate) fn assess_for_tool(
    game_dir: &Path,
    proxy_dll_name: &str,
    tool_name: &'static str,
    min_host_version: Option<&Version>,
) -> HostAssessment {
    assess_for_tool_with_allowed_addons(game_dir, proxy_dll_name, tool_name, min_host_version, &[])
}

#[cfg(test)]
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

fn assess_scan_with_allowed_addons(
    game_dir: &Path,
    proxy_dll_name: &str,
    scan: &ReshadeScan,
    tool_name: &'static str,
    min_host_version: Option<&Version>,
    allowed_addon_names: &[&str],
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
    // otherwise be reused as-is. The initial lifecycle below decides whether an
    // under-min host is empty enough to repair safely.
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
    let content = if conflict_kind.is_none() {
        scan::assess_reshade_content(game_dir, allowed_addon_names)
    } else {
        ReshadeContent::Indeterminate
    };
    let lifecycle = if conflict_kind.is_some() || action == ReshadeHostAction::Conflict {
        HostLifecycle::Conflict
    } else if present.is_none() {
        HostLifecycle::InstallNew
    } else if action == ReshadeHostAction::UpToDate {
        if content.is_empty() {
            HostLifecycle::AdoptEmpty
        } else {
            HostLifecycle::ReuseUser
        }
    } else if content.is_empty() {
        HostLifecycle::RepairEmpty
    } else {
        HostLifecycle::Conflict
    };

    HostAssessment {
        host,
        conflict: conflict_kind.is_some(),
        action,
        lifecycle,
        target_path,
        slot,
        content,
        conflict_kind,
        tool_name,
    }
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::Version;

    use super::*;
    use crate::addons::renodx::test_support::{
        MACHINE_AMD64, PE32_PLUS_MAGIC, build_pe_with_exports,
    };
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
