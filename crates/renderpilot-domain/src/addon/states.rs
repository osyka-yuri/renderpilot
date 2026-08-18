//! Wire/API install-state **projections** for status and availability DTOs.
//!
//! These are not core domain entities: they are serde shapes emitted to CLI,
//! desktop, and other clients. Core ownership lives on [`super::InstalledAddon`]
//! (+ managed files / tracked sources). Prefer constructing these only at the
//! tool query boundary (`tracking::install_state_from_record`).
//!
//! Kept in `renderpilot-domain` so every facade shares one wire contract without
//! a third crate. Do not grow per-tool UI fields here unless they are part of
//! the stable status JSON.

use serde::{Deserialize, Serialize};

/// UI wire host mechanism for RenoDX availability/status DTOs.
///
/// Distinct from persisted [`super::InstalledAddonHostKind`] (`shared_vulkan_layer`)
/// so the frontend can keep the short `vulkan` wire name. Map at the tool DTO
/// boundary via `From<InstalledAddonHostKind>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenoDxHostKind {
    /// A per-game ReShade proxy DLL.
    Proxy,
    /// The shared ReShade Vulkan implicit layer.
    Vulkan,
}

/// Current RenoDX installation state for a game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RenoDxInstallState {
    /// RenoDX is not installed for the game.
    NotInstalled,
    /// RenoDX is installed.
    Installed {
        /// Host mechanism used by this install, mapped to a UI-facing stable
        /// vocabulary. `None` for legacy records that predate host metadata.
        #[serde(default)]
        host_kind: Option<RenoDxHostKind>,
        /// Installed add-on version label, when known (free-form, e.g.
        /// `snapshot-2026.06`). RenoDX add-ons are rolling snapshots with no version
        /// number, so this is effectively always `null`; the UI uses `addon_dated`
        /// as the concrete anchor instead.
        version: Option<String>,
        /// The add-on's upstream `Last-Modified` HTTP-date string (its publish-date
        /// proxy), when the host sent one — the UI's "Add-on dated …" anchor.
        #[serde(default)]
        addon_dated: Option<String>,
        /// When the add-on was first installed (Unix epoch milliseconds).
        /// Always a concrete number for emitted `installed` states.
        installed_at: i64,
        /// When the install record was last updated (Unix epoch milliseconds) —
        /// bumped by an add-on/host/DLSS-Fix update.
        /// Always a concrete number for emitted `installed` states.
        updated_at: i64,
        /// Whether the install record contains any DLSS-Fix ownership or source
        /// evidence. This does not claim that a safe companion file is currently
        /// present; the dedicated availability projection determines that exact
        /// relationship and its allowed actions. Surfaced directly so the UI can
        /// retain the component row while that projection is in flight or failed.
        #[serde(default)]
        dlss_fix_evidence_present: bool,
        /// Whether the add-on has a tracked upstream source (a normal install).
        /// `false` for a user-file install, which records no upstream URL.
        /// Surfaced directly on the state for the same reason as
        /// `dlss_fix_evidence_present`, so the "installed from a file" hint stays correct
        /// while the update probe is in flight or after it fails (the report's
        /// `addon` is `null` in those cases too).
        #[serde(default)]
        addon_tracked: bool,
    },
}

impl RenoDxInstallState {
    /// Returns whether this state is `Installed` and its add-on payload has a
    /// non-empty upstream source URL.
    #[must_use]
    pub fn is_addon_tracked(&self) -> bool {
        matches!(
            self,
            Self::Installed {
                addon_tracked: true,
                ..
            }
        )
    }
}

/// Current Luma Framework installation state for a game.
///
/// Deliberately narrower than [`RenoDxInstallState`]: Luma only ever hooks in via
/// a per-game ReShade proxy DLL (no Vulkan alternative), so there is no `host_kind`
/// to surface. It has no RenoDX-style DLSS-Fix companion add-on; optional
/// coordinated `nvngx_dlss.dll` ownership is modeled on [`crate::InstalledAddon::managed_files`]
/// instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum LumaInstallState {
    /// Luma is not installed for the game.
    NotInstalled,
    /// Luma is installed.
    Installed {
        /// Installed build label, when known (e.g. `Build 515`, parsed from the
        /// upstream rolling release's redirect target). `None` when the build
        /// number could not be recovered (e.g. an adopted on-disk install).
        version: Option<String>,
        /// The add-on's upstream `Last-Modified` HTTP-date string (its publish-date
        /// proxy), when the host sent one — the UI's "Add-on dated …" anchor.
        #[serde(default)]
        addon_dated: Option<String>,
        /// When the add-on was first installed (Unix epoch milliseconds).
        /// Always a concrete number for emitted `installed` states.
        installed_at: i64,
        /// When the install record was last updated (Unix epoch milliseconds) —
        /// bumped by an add-on/host update.
        /// Always a concrete number for emitted `installed` states.
        updated_at: i64,
        /// Effective ReShade channel of the host artifact, when known. Informational
        /// only — unlike RenoDX, Luma has no channel-switch action: every host it
        /// writes is nightly, and this simply reports whatever a reused foreign host
        /// happens to be.
        #[serde(default)]
        reshade_channel: Option<String>,
        /// Launch arguments this title requires (e.g. `-dx11`), re-resolved from the
        /// manifest at query time rather than persisted on the install record —
        /// showing the user's current copy-paste callout even if the catalogue's
        /// guidance changes after install.
        #[serde(default)]
        launch_args: Vec<String>,
    },
}
