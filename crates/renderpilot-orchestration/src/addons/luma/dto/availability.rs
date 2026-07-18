//! Availability data transfer objects.
//!
//! Wire install state lives in domain as a shared serde contract; re-exported
//! here as the tool DTO boundary preferred by use-cases.
//!
//! [`crate::addons::luma::types::LumaFeatures`],
//! [`crate::addons::luma::types::LumaGuidance`], and
//! [`crate::addons::luma::types::LumaProfile`] are intentional public wire
//! vocabulary (same serde shapes as user-facing catalogue fields). Install
//! recipes stay private via projections such as
//! [`crate::addons::luma::dto::availability::ManagedDependencySummary`].

use renderpilot_domain::AddonKind;
use serde::Serialize;

pub use renderpilot_domain::LumaInstallState;

use crate::addons::CatalogMessage;
use crate::addons::anticheat::RiskAssessment;
use crate::addons::luma::types::{
    LumaExternalRequirement, LumaFeatures, LumaGuidance, LumaProfile,
};
use crate::addons::matching::{IncompatibilityReason, MatchConfidence};
// The observable ReShade host DTOs are shared; re-exported so the Luma
// availability report keeps addressing them here.
pub use crate::addons::reshade::dto::{HostActions, HostDetection, HostFacts};

/// Backend-derived Luma actions (shared host-action wire shape; `switch_channel` always unset).
pub type LumaActions = HostActions;

/// Public identity of a managed dependency. Download sources, digests and the
/// install recipe remain private implementation details of the backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ManagedDependencySummary {
    /// Managed dgVoodoo2 runtime required by an older Direct3D title.
    Dgvoodoo2 {
        /// Required upstream runtime version.
        version: String,
    },
}

impl From<LumaExternalRequirement> for ManagedDependencySummary {
    fn from(value: LumaExternalRequirement) -> Self {
        match value {
            LumaExternalRequirement::Dgvoodoo2 { version, .. } => Self::Dgvoodoo2 { version },
        }
    }
}

/// Read-only preview of whether Luma can be installed for a game.
#[derive(Debug, Clone, Serialize)]
pub struct AvailabilityReport {
    /// Current install state for the game.
    pub state: LumaInstallState,
    /// Detection state of the Direct3D ReShade proxy host.
    pub host_detection: HostDetection,
    /// Observable Direct3D ReShade host facts, without private install records.
    pub host_facts: HostFacts,
    /// Backend-derived actions the UI may render. Unlike RenoDX, Luma never
    /// offers a channel switch — every host it writes is nightly.
    pub actions: LumaActions,
    /// Minimum ReShade host version Luma's current builds require, for display
    /// alongside a host-update action.
    pub min_reshade_version: String,
    /// Advisory Visual C++ Redistributable presence check. `None` when it could
    /// not be determined. Never blocks an install — informational only.
    pub vcredist_present: Option<bool>,
    /// Official installer URL for the redistributable this game's detected
    /// architecture needs (x64 or x86) — the advisory callout's download link.
    pub vcredist_installer_url: String,
    /// Whether a prior install or rollback for this game did not complete
    /// cleanly (the engine's crash-safety sentinel is still present). `false`
    /// when the install directory can't be resolved. Reinstalling clears it.
    pub install_torn: bool,
    /// Whether and how Luma can be installed.
    pub outcome: AvailabilityOutcome,
}

/// The installability verdict for a game.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AvailabilityOutcome {
    /// A compatible game matched; the add-on can be installed.
    Installable {
        /// Confidence shown to the user (verified / experimental / untested).
        confidence: MatchConfidence,
        /// Ban/stability risk and whether explicit confirmation is required.
        risk: RiskAssessment,
        /// Required launch arguments, shown to the user as a copyable callout.
        launch_args: Vec<String>,
        /// Dedicated game profile vs shared engine payload (UI badge source).
        profile: LumaProfile,
        /// Per-game HDR and DLSS/FSR availability, when this is a Generic UE
        /// profile with an explicit UE-matrix entry.
        features: Option<LumaFeatures>,
        /// Reviewed per-game instructions. These are available both before and
        /// after installation because availability is re-resolved on query.
        guidance: Vec<LumaGuidance>,
        /// Runtime dependency this profile requires. RenderPilot installs it
        /// only when it is absent; a compatible existing runtime is reused.
        external_requirement: Option<ManagedDependencySummary>,
    },
    /// A game matched but cannot be installed for it.
    Incompatible {
        /// Why it cannot be installed.
        reason: IncompatibilityReason,
    },
    /// The game is blacklisted / known-broken, or needs an external
    /// prerequisite this installer doesn't automate.
    Blacklisted {
        /// Localizable explanation supplied by the catalogue.
        message: CatalogMessage,
    },
    /// No Luma profile matched the game.
    Unsupported,
    /// A different addon tool (RenoDX) is already installed — or unmanaged
    /// files belonging to it were found on disk — for this game. Luma and
    /// RenoDX are mutually exclusive per game; uninstall the other one first.
    BlockedByOtherAddon {
        /// The other addon tool occupying this game.
        other_kind: AddonKind,
        /// Whether the block came from an unmanaged on-disk install (`true`)
        /// rather than a tracked database record (`false`).
        unmanaged: bool,
    },
    /// Luma-shaped files are on disk for this game with no tracked database
    /// record (e.g. after a database loss). The availability path now attempts
    /// to adopt such installs (like RenoDX); this outcome is only reached when
    /// adoption could not claim the files (locked, no roots, etc.). The UI
    /// prompts manual cleanup as a last resort.
    UnmanagedPresent,
}

#[cfg(test)]
mod tests {
    use super::ManagedDependencySummary;

    #[test]
    fn dependency_summary_does_not_expose_the_install_recipe() {
        let value = serde_json::to_value(ManagedDependencySummary::Dgvoodoo2 {
            version: "2.87.3".to_owned(),
        })
        .expect("summary serializes");

        assert_eq!(
            value,
            serde_json::json!({ "kind": "dgvoodoo2", "version": "2.87.3" })
        );
        assert!(value.get("source").is_none());
        assert!(value.get("install_map").is_none());
        assert!(value.get("config").is_none());
    }
}
