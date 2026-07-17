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
        Err(ServiceError::invalid_input(message))
    }

    /// Ensures the first-install lifecycle is safe. Empty, recognized runtimes
    /// may be repaired in-place; user content is never replaced automatically.
    pub(crate) fn ensure_initial_installable(
        &self,
        proxy_dll_name: &str,
    ) -> Result<(), ServiceError> {
        self.ensure_not_conflicting(proxy_dll_name)?;

        if self.initial_is_conflict() {
            return Err(ServiceError::invalid_input(format!(
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

pub(super) fn assess_scan_with_allowed_addons(
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
    // can't be trusted) or, with more than one aliased slot, `MultipleHosts` --
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
mod tests;
