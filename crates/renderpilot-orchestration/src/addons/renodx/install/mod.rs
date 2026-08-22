//! Turns a fetched RenoDX install into a generic
//! [`InstallPlan`](crate::addons::engine::InstallPlan) for the shared
//! [`addons::engine`](crate::addons::engine) and maps the result back to a record.
//!
//! Given a [`PreparedInstall`] (already-fetched, already-verified bytes plus the
//! resolved file names), [`install`] builds the ordered file operations — the
//! add-on and ReShade host DLL via a no-backup
//! [`FileOp::Replace`](crate::addons::engine::FileOp::Replace) (both are
//! rolling snapshots or official redistributables RenoDX already PE-checked, so
//! nothing about a prior version is worth preserving), `ReShade.ini` via a
//! no-backup [`FileOp::UpdateText`](crate::addons::engine::FileOp::UpdateText)
//! merge (it may carry the user's own hand-tuned
//! ReShade settings) — hands them to the engine (which rolls back and journals
//! each op per its own backup policy) and assembles the reversible
//! [`InstalledAddon`](renderpilot_domain::InstalledAddon) with the upstream
//! sources to track for updates.
//! [`fn@uninstall`] reverses everything the engine's generic list-based reversal
//! covers, plus bespoke handling for `ReShade.ini` (see its own docs) since a
//! config merge is never something that reversal alone can undo correctly.
//! (In-place file replacement for updates/channel switches lives in
//! [`super::use_cases::reshade_update`], shared by the update and channel-switch
//! commands, which drive the engine directly only for a host moving to a new slot.)
//!
//! All ReShade/HDR specifics live here; the filesystem mechanics are the engine's.

use std::path::Path;

use renderpilot_domain::GameId;

use crate::ServiceError;

use crate::addons::reshade::proxy::HostKind;
use crate::addons::reshade::types::{ReshadeChannel, ReshadeIniTweaks};

mod ops;
mod plans;
mod recovery;
mod uninstall;

#[cfg(test)]
mod tests;

pub(crate) use plans::{build_vulkan_game_participants, build_vulkan_record};
pub use recovery::recover_torn_install;
pub(crate) use uninstall::PreparedRenoDxUninstall;
#[cfg(test)]
pub(crate) use uninstall::uninstall;

/// Everything the engine needs to lay down an install, with payloads already
/// downloaded and integrity-verified by the fetch layer.
#[derive(Debug, Clone)]
pub struct PreparedInstall {
    /// Game the install belongs to.
    pub game_id: GameId,
    /// How RenoDX hooks into this game: a per-game proxy DLL or the shared Vulkan
    /// layer. Selects which per-game file layout [`install`] lays down.
    pub host_kind: HostKind,
    /// Proxy DLL file name to install the ReShade host as (for example `dxgi.dll`).
    /// Used only for [`HostKind::Proxy`]; empty for a Vulkan install.
    pub proxy_dll_name: String,
    /// Add-on file name to place (for example `renodx-cp2077.addon64`).
    pub addon_file_name: String,
    /// Upstream URL the add-on was fetched from (empty for a file install).
    pub addon_source_url: String,
    /// SHA-256 of the add-on bytes — the durable change-detection digest.
    pub source_digest: String,
    /// HTTP cache validator for a cheap update pre-check.
    pub source_etag: Option<String>,
    /// Raw `Last-Modified` HTTP-date string of the add-on, when the host sent one.
    pub source_last_modified: Option<String>,
    /// Add-on payload bytes.
    pub addon_bytes: Vec<u8>,
    /// ReShade host DLL bytes; used only when no ReShade host is already present.
    pub reshade_dll_bytes: Vec<u8>,
    /// Upstream URL the ReShade host came from (empty when no host is installed).
    pub reshade_source_url: String,
    /// The ReShade host zip's cache validator (for a cheap host update pre-check).
    pub reshade_source_etag: Option<String>,
    /// Raw `Last-Modified` HTTP-date string of the ReShade host zip, when sent.
    pub reshade_last_modified: Option<String>,
    /// SHA-256 of the installed ReShade host DLL (empty when none installed).
    pub reshade_digest: String,
    /// Effective channel for a recorded ReShade host artifact.
    pub reshade_channel: Option<ReshadeChannel>,
    /// `ReShade.ini` tweaks RenoDX requires.
    pub ini_tweaks: ReshadeIniTweaks,
}

/// Installs RenoDX into `game_dir`, returning the record needed to reverse it.
///
/// Dispatches on the host kind: a [`HostKind::Proxy`] install lays down a per-game
/// ReShade proxy DLL (or reuses a compatible detected host); a [`HostKind::Vulkan`] install lays
/// down only per-game files — the shared Vulkan layer is handled separately by the
/// service. The engine rolls back on any failure.
/// Returns the install record plus a pending commit guard. Every host kind keeps
/// its crash-safety sentinel open until durable database persistence.
pub fn install(
    game_dir: &Path,
    prepared: &PreparedInstall,
) -> Result<
    (
        renderpilot_domain::InstalledAddon,
        crate::addons::engine::PendingInstallCommit,
    ),
    ServiceError,
> {
    match prepared.host_kind {
        HostKind::Proxy => plans::install_proxy(game_dir, prepared),
        HostKind::Vulkan => plans::install_vulkan(game_dir, prepared),
    }
}
