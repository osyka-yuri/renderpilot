//! Stable feature labels for durable game-file transactions.
//!
//! These strings are persisted on `pending_file_mutations.feature` and used in
//! recovery diagnostics. Keep them stable across releases.
//!
//! This module is the source of truth. Orchestration re-exports the same labels
//! at `crate::addons::mutation_features` for a short path; other crates import
//! domain directly.

/// Catalog component swap (overlay apply).
pub const CATALOG_SWAP: &str = "catalog_swap";
/// Catalog component rollback to baseline.
pub const CATALOG_ROLLBACK: &str = "catalog_rollback";

/// Luma install (engine + managed DLSS + optional host).
pub const LUMA_INSTALL: &str = "luma_install";
/// Luma uninstall (engine reverse + managed release + cascade).
pub const LUMA_UNINSTALL: &str = "luma_uninstall";
/// Luma update (multi-step sentinel + durable transaction).
pub const LUMA_UPDATE: &str = "luma_update";

/// RenoDX install from catalog/CDN.
pub const RENODX_INSTALL: &str = "renodx_install";
/// RenoDX install from a local add-on file.
pub const RENODX_INSTALL_FROM_FILE: &str = "renodx_install_from_file";
/// RenoDX uninstall.
pub const RENODX_UNINSTALL: &str = "renodx_uninstall";
/// RenoDX add-on update.
pub const RENODX_UPDATE: &str = "renodx_update";
/// RenoDX proxy ReShade channel switch.
pub const RENODX_SWITCH_RESHADE_CHANNEL: &str = "renodx_switch_reshade_channel";
/// Settings-side apply/update of the process-wide shared Vulkan layer.
pub const SHARED_VULKAN_APPLY: &str = "shared_vulkan_apply";
/// RenoDX DLSS-Fix companion install.
pub const RENODX_DLSS_FIX_INSTALL: &str = "renodx_dlss_fix_install";
/// RenoDX DLSS-Fix companion uninstall.
pub const RENODX_DLSS_FIX_UNINSTALL: &str = "renodx_dlss_fix_uninstall";
/// RenoDX DLSS-Fix companion update or payload-only repair.
pub const RENODX_DLSS_FIX_UPDATE: &str = "renodx_dlss_fix_update";

/// Safety authority required before a mutation feature may write.
///
/// This classification is deliberately exhaustive for the durable feature
/// registry. Callers must not replace it with an ad-hoc bypass boolean: adding
/// a feature requires adding its policy here and extending the tests below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafetyRequirement {
    /// No fresh safety permit is required (rollback, uninstall, or recovery).
    None,
    /// The current game's safety assessment is required.
    Game,
    /// The current game's assessment is required; a shared-Vulkan permit is
    /// additionally required when the resolved operation mutates that layer.
    GameWithOptionalSharedVulkan,
    /// The process-wide shared Vulkan-layer assessment is required.
    SharedVulkan,
}

/// Returns the safety policy for a persisted mutation feature.
///
/// `None` means the feature name is not part of the registry and must not be
/// silently treated as an ungated mutation.
#[must_use]
pub fn safety_requirement(feature: &str) -> Option<SafetyRequirement> {
    use SafetyRequirement::{Game, GameWithOptionalSharedVulkan, None, SharedVulkan};
    match feature {
        CATALOG_SWAP
        | LUMA_INSTALL
        | LUMA_UPDATE
        | RENODX_DLSS_FIX_INSTALL
        | RENODX_DLSS_FIX_UPDATE => Some(Game),
        RENODX_INSTALL
        | RENODX_INSTALL_FROM_FILE
        | RENODX_UPDATE
        | RENODX_SWITCH_RESHADE_CHANNEL => Some(GameWithOptionalSharedVulkan),
        SHARED_VULKAN_APPLY => Some(SharedVulkan),
        CATALOG_ROLLBACK | LUMA_UNINSTALL | RENODX_UNINSTALL | RENODX_DLSS_FIX_UNINSTALL => {
            Some(None)
        }
        _ => Option::None,
    }
}

/// Whether a persisted durable-mutation feature belongs to the RenoDX DLSS-Fix
/// companion lifecycle. This exact allow-list deliberately excludes generic
/// RenoDX work and all other tools' pending rows.
#[must_use]
pub fn is_renodx_dlss_fix_feature(feature: &str) -> bool {
    matches!(
        feature,
        RENODX_DLSS_FIX_INSTALL | RENODX_DLSS_FIX_UNINSTALL | RENODX_DLSS_FIX_UPDATE
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dlss_fix_feature_allow_list_is_exact() {
        assert!(is_renodx_dlss_fix_feature(RENODX_DLSS_FIX_INSTALL));
        assert!(is_renodx_dlss_fix_feature(RENODX_DLSS_FIX_UNINSTALL));
        assert!(is_renodx_dlss_fix_feature(RENODX_DLSS_FIX_UPDATE));
        assert!(!is_renodx_dlss_fix_feature(RENODX_UPDATE));
        assert!(!is_renodx_dlss_fix_feature(LUMA_UPDATE));
    }

    #[test]
    fn every_registered_feature_has_an_explicit_safety_requirement() {
        let features = [
            CATALOG_SWAP,
            CATALOG_ROLLBACK,
            LUMA_INSTALL,
            LUMA_UNINSTALL,
            LUMA_UPDATE,
            RENODX_INSTALL,
            RENODX_INSTALL_FROM_FILE,
            RENODX_UNINSTALL,
            RENODX_UPDATE,
            RENODX_SWITCH_RESHADE_CHANNEL,
            SHARED_VULKAN_APPLY,
            RENODX_DLSS_FIX_INSTALL,
            RENODX_DLSS_FIX_UNINSTALL,
            RENODX_DLSS_FIX_UPDATE,
        ];
        assert!(
            features
                .iter()
                .all(|feature| safety_requirement(feature).is_some())
        );
        assert!(safety_requirement("future_feature").is_none());
    }

    #[test]
    fn feature_registry_has_explicit_requirements_for_every_mutation() {
        use SafetyRequirement::{Game, GameWithOptionalSharedVulkan, None, SharedVulkan};

        let cases = [
            (CATALOG_SWAP, Game),
            (CATALOG_ROLLBACK, None),
            (LUMA_INSTALL, Game),
            (LUMA_UNINSTALL, None),
            (LUMA_UPDATE, Game),
            (RENODX_INSTALL, GameWithOptionalSharedVulkan),
            (RENODX_INSTALL_FROM_FILE, GameWithOptionalSharedVulkan),
            (RENODX_UNINSTALL, None),
            (RENODX_UPDATE, GameWithOptionalSharedVulkan),
            (RENODX_SWITCH_RESHADE_CHANNEL, GameWithOptionalSharedVulkan),
            (SHARED_VULKAN_APPLY, SharedVulkan),
            (RENODX_DLSS_FIX_INSTALL, Game),
            (RENODX_DLSS_FIX_UNINSTALL, None),
            (RENODX_DLSS_FIX_UPDATE, Game),
        ];

        for (feature, expected) in cases {
            assert_eq!(
                safety_requirement(feature),
                Some(expected),
                "mutation feature {feature} must keep an explicit safety policy"
            );
        }
        assert!(
            safety_requirement("future_feature").is_none(),
            "unknown mutation features must not receive an implicit bypass"
        );
    }
}
