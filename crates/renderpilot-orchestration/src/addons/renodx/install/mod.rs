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

use renderpilot_domain::{Architecture, GameId};

use crate::ServiceError;

use crate::addons::reshade::proxy::HostKind;
use crate::addons::reshade::types::{ReshadeChannel, ReshadeIniTweaks};

mod ops;
mod plans;
mod recovery;
mod uninstall;

#[cfg(test)]
mod tests;

pub use recovery::recover_torn_install;
pub use uninstall::uninstall;

/// The DLSS-Fix companion add-on file name prefix (`renodx-dlssfix.`).
pub(crate) const DLSS_FIX_FILE_PREFIX: &str = "renodx-dlssfix.";

/// The DLSS-Fix add-on file name for `arch` (e.g. `renodx-dlssfix.addon64`).
#[must_use]
pub(crate) fn dlss_fix_file_name(arch: Architecture) -> String {
    format!("{DLSS_FIX_FILE_PREFIX}{}", arch.addon_extension())
}

/// The path of the DLSS-Fix companion add-on within the record's `created_files`,
/// if one is installed (matched by the [`DLSS_FIX_FILE_PREFIX`] file name).
#[must_use]
pub(crate) fn dlss_fix_file_path(
    record: &renderpilot_domain::InstalledAddon,
) -> Option<std::path::PathBuf> {
    record
        .created_files()
        .iter()
        .find(|f| {
            f.file_name()
                .is_some_and(|n| n.starts_with(DLSS_FIX_FILE_PREFIX))
        })
        .map(|f| std::path::PathBuf::from(f.as_str()))
}

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
pub fn install(
    game_dir: &Path,
    prepared: &PreparedInstall,
) -> Result<renderpilot_domain::InstalledAddon, ServiceError> {
    match prepared.host_kind {
        HostKind::Proxy => plans::install_proxy(game_dir, prepared),
        HostKind::Vulkan => plans::install_vulkan(game_dir, prepared),
    }
}
