use serde::{Deserialize, Serialize};

/// UI-facing host mechanism used by an installed RenoDX add-on.
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
        /// Whether the install includes the DLSS-Fix companion add-on. Surfaced
        /// directly on the state so the UI does not have to infer it from the
        /// update report (which is `null` while the update probe is in flight or
        /// after a network failure).
        #[serde(default)]
        dlss_fix_installed: bool,
        /// Whether the add-on has a tracked upstream source (a normal install).
        /// `false` for a user-file install, which records no upstream URL.
        /// Surfaced directly on the state for the same reason as
        /// `dlss_fix_installed`, so the "installed from a file" hint stays correct
        /// while the update probe is in flight or after it fails (the report's
        /// `addon` is `null` in those cases too).
        #[serde(default)]
        addon_tracked: bool,
    },
}

impl RenoDxInstallState {
    /// Returns whether this state is `Installed` **and** includes the DLSS-Fix
    /// companion add-on. A thin pattern-match helper so callers need not repeat
    /// the `match`/`if let` boilerplate.
    #[must_use]
    pub fn is_dlss_fix_installed(&self) -> bool {
        matches!(
            self,
            Self::Installed {
                dlss_fix_installed: true,
                ..
            }
        )
    }

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
