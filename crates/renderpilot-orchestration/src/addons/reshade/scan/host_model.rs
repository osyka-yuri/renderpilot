//! The ReShade host data model: what a scan found in a game folder, and the
//! policy action it implies. The scan algorithm itself lives in [`super::hosts`].

use std::path::{Path, PathBuf};

use renderpilot_domain::Version;
use serde::Serialize;

/// Current ReShade host state in a game folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ReshadeHost {
    /// No ReShade-looking host was found.
    Absent,
    /// A host or occupied proxy slot is present.
    Present {
        /// Full path to the DLL.
        path: PathBuf,
        /// Proxy slot/file name where it was found.
        slot: String,
        /// Version-resource file version, if readable.
        version: Option<Version>,
        /// Whether the host exports the add-on API a tool requires.
        addon_support: ReshadeAddonSupport,
        /// Confidence that the DLL is actually ReShade.
        identity: ReshadeIdentity,
        /// Whether this slot is the one the resolved game executable will load.
        active: ActiveSlotState,
    },
}

impl ReshadeHost {
    /// Returns the present-host details, if any.
    #[must_use]
    pub fn as_present(&self) -> Option<ReshadeHostRef<'_>> {
        match self {
            Self::Absent => None,
            Self::Present {
                path,
                slot,
                version,
                addon_support,
                identity,
                active,
            } => Some(ReshadeHostRef {
                path,
                slot,
                version: version.as_ref(),
                addon_support: *addon_support,
                identity: *identity,
                active: *active,
            }),
        }
    }

    /// Whether the host is present and confidently identified as ReShade.
    #[must_use]
    pub fn is_usable_reshade(&self) -> bool {
        self.as_present()
            .is_some_and(|host| host.identity >= ReshadeIdentity::Probable)
    }

    /// Whether this host sits in the active proxy slot.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.as_present()
            .is_some_and(|host| host.active.state == SlotActivity::Active)
    }
}

/// Borrowed view over a [`ReshadeHost::Present`] payload.
#[derive(Debug, Clone, Copy)]
pub struct ReshadeHostRef<'a> {
    /// Full path to the DLL.
    pub path: &'a Path,
    /// Proxy slot/file name where it was found.
    pub slot: &'a str,
    /// Version-resource file version, if readable.
    pub version: Option<&'a Version>,
    /// Whether the host exports the add-on API a tool requires.
    pub addon_support: ReshadeAddonSupport,
    /// Confidence that the DLL is actually ReShade.
    pub identity: ReshadeIdentity,
    /// Whether this slot is the one the resolved game executable will load.
    pub active: ActiveSlotState,
}

/// Add-on API capability of the detected host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReshadeAddonSupport {
    /// The host exports the ReShade add-on API.
    Full,
    /// The host is ReShade but does not export the add-on API.
    None,
    /// Capability could not be determined.
    Unknown,
}

/// Confidence that a candidate DLL is ReShade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReshadeIdentity {
    /// A proxy slot is occupied, but ReShade identity is not established.
    Weak,
    /// Version-resource metadata or supporting files strongly point to ReShade.
    Probable,
    /// Export table contains `ReShadeVersion`.
    Confirmed,
}

/// Active-slot classification for a host candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ActiveSlotState {
    /// Whether this host is in the slot the resolved executable should load.
    pub state: SlotActivity,
    /// Why that classification was chosen.
    pub reason: ActiveSlotReason,
}

/// Whether a host slot is expected to load.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotActivity {
    /// The slot matches the resolved proxy DLL.
    Active,
    /// Another slot is expected to load instead.
    Inactive,
    /// The active slot is not known.
    Ambiguous,
}

/// Explanation for active-slot classification. Kept minimal to the cases the
/// resolver can actually distinguish today (the active proxy slot is supplied by
/// the matcher, which already folds in import detection and bootstrap-exe
/// resolution); richer provenance can be added if a flow ever needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActiveSlotReason {
    /// The active slot came from the resolved-executable/matcher result.
    DetectedByMatcher,
    /// Dynamic loading or missing context left the active slot unknown.
    DynamicLoadUnknown,
}

/// Read-only scan of the game folder's ReShade-related DLLs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReshadeScan {
    /// Every ReShade-looking host or occupied active proxy slot found.
    pub hosts: Vec<ReshadeHost>,
}

impl ReshadeScan {
    /// Returns the single active host, if one can be identified.
    #[must_use]
    pub fn active_host(&self) -> Option<&ReshadeHost> {
        self.hosts.iter().find(|host| host.is_active())
    }

    /// Returns the hosts with at least probable ReShade identity.
    #[must_use]
    pub fn reshade_hosts(&self) -> Vec<&ReshadeHost> {
        self.hosts
            .iter()
            .filter(|host| host.is_usable_reshade())
            .collect()
    }

    /// Whether more than one ReShade host was found.
    #[must_use]
    pub fn has_multiple_reshade_hosts(&self) -> bool {
        self.reshade_hosts().len() > 1
    }

    /// Returns a compact host state for UI/DTO use.
    #[must_use]
    pub fn primary_host(&self) -> ReshadeHost {
        self.active_host()
            .cloned()
            .or_else(|| self.reshade_hosts().into_iter().next().cloned())
            .unwrap_or(ReshadeHost::Absent)
    }
}

/// Policy action for a detected ReShade host relative to the desired version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReshadeHostAction {
    /// No safe automatic host action is available.
    Conflict,
    /// Replace with the full add-on-support build.
    ReinstallWithAddonSupport,
    /// Repair an unidentifiable or partially readable host.
    RepairHost,
    /// Update the active host to the desired version.
    UpdateHost,
    /// Host is suitable as-is.
    UpToDate,
}

impl ReshadeHostAction {
    /// Whether applying this policy action writes or replaces the ReShade host DLL.
    #[must_use]
    pub const fn writes_host(self) -> bool {
        matches!(
            self,
            Self::ReinstallWithAddonSupport | Self::RepairHost | Self::UpdateHost
        )
    }
}
