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

/// Initial OptiScaler installation.
pub const OPTISCALER_INSTALL: &str = "optiscaler_install";
/// OptiScaler release/module update or repair.
pub const OPTISCALER_UPDATE: &str = "optiscaler_update";
/// OptiScaler relocation to a newly selected executable or directory.
pub const OPTISCALER_RELOCATE: &str = "optiscaler_relocate";
/// OptiScaler uninstall and proxy-topology restoration.
pub const OPTISCALER_UNINSTALL: &str = "optiscaler_uninstall";

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
/// Per-game unregister of an app from the settings-owned shared Vulkan layer.
pub const RETIRED_GAME_LEFTOVERS_UNREGISTER: &str = "retired_game_leftovers_unregister";
/// RenoDX DLSS-Fix companion install.
pub const RENODX_DLSS_FIX_INSTALL: &str = "renodx_dlss_fix_install";
/// RenoDX DLSS-Fix companion uninstall.
pub const RENODX_DLSS_FIX_UNINSTALL: &str = "renodx_dlss_fix_uninstall";
/// RenoDX DLSS-Fix companion update or payload-only repair.
pub const RENODX_DLSS_FIX_UPDATE: &str = "renodx_dlss_fix_update";

/// Closed owner classification for every durable mutation feature.
///
/// This is intentionally distinct from [`SafetyRequirement`]: file-safety
/// authority describes which live assessment is required, while the owner
/// identifies which product aggregate is allowed to mutate a coordinated
/// proxy chain. Unknown features receive no implicit owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationFeatureOwner {
    /// Graphics-component catalog replacement lifecycle.
    Catalog,
    /// Luma add-on lifecycle.
    Luma,
    /// OptiScaler closed aggregate lifecycle.
    OptiScaler,
    /// RenoDX and its DLSS-Fix companion lifecycle.
    RenoDx,
    /// Process-wide shared Vulkan-layer lifecycle.
    SharedVulkan,
}

/// Returns the closed product owner for a persisted mutation feature.
///
/// `None` means the feature is not registered and must fail closed before a
/// durable row or live file mutation is created.
#[must_use]
pub fn feature_owner(feature: &str) -> Option<MutationFeatureOwner> {
    use MutationFeatureOwner::{Catalog, Luma, OptiScaler, RenoDx, SharedVulkan};
    match feature {
        CATALOG_SWAP | CATALOG_ROLLBACK => Some(Catalog),
        LUMA_INSTALL | LUMA_UNINSTALL | LUMA_UPDATE => Some(Luma),
        OPTISCALER_INSTALL | OPTISCALER_UPDATE | OPTISCALER_RELOCATE | OPTISCALER_UNINSTALL => {
            Some(OptiScaler)
        }
        RENODX_INSTALL
        | RENODX_INSTALL_FROM_FILE
        | RENODX_UNINSTALL
        | RENODX_UPDATE
        | RENODX_SWITCH_RESHADE_CHANNEL
        | RENODX_DLSS_FIX_INSTALL
        | RENODX_DLSS_FIX_UNINSTALL
        | RENODX_DLSS_FIX_UPDATE => Some(RenoDx),
        SHARED_VULKAN_APPLY | RETIRED_GAME_LEFTOVERS_UNREGISTER => Some(SharedVulkan),
        _ => None,
    }
}

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
        | OPTISCALER_INSTALL
        | OPTISCALER_UPDATE
        | OPTISCALER_RELOCATE
        | RENODX_DLSS_FIX_INSTALL
        | RENODX_DLSS_FIX_UPDATE => Some(Game),
        RENODX_INSTALL
        | RENODX_INSTALL_FROM_FILE
        | RENODX_UPDATE
        | RENODX_SWITCH_RESHADE_CHANNEL => Some(GameWithOptionalSharedVulkan),
        SHARED_VULKAN_APPLY => Some(SharedVulkan),
        CATALOG_ROLLBACK
        | LUMA_UNINSTALL
        | OPTISCALER_UNINSTALL
        | RENODX_UNINSTALL
        | RENODX_DLSS_FIX_UNINSTALL
        | RETIRED_GAME_LEFTOVERS_UNREGISTER => Some(None),
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
            OPTISCALER_INSTALL,
            OPTISCALER_UPDATE,
            OPTISCALER_RELOCATE,
            OPTISCALER_UNINSTALL,
            RENODX_INSTALL,
            RENODX_INSTALL_FROM_FILE,
            RENODX_UNINSTALL,
            RENODX_UPDATE,
            RENODX_SWITCH_RESHADE_CHANNEL,
            SHARED_VULKAN_APPLY,
            RETIRED_GAME_LEFTOVERS_UNREGISTER,
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
    fn every_registered_feature_has_an_explicit_owner() {
        use MutationFeatureOwner::{Catalog, Luma, OptiScaler, RenoDx, SharedVulkan};

        let cases = [
            (CATALOG_SWAP, Catalog),
            (CATALOG_ROLLBACK, Catalog),
            (LUMA_INSTALL, Luma),
            (LUMA_UNINSTALL, Luma),
            (LUMA_UPDATE, Luma),
            (OPTISCALER_INSTALL, OptiScaler),
            (OPTISCALER_UPDATE, OptiScaler),
            (OPTISCALER_RELOCATE, OptiScaler),
            (OPTISCALER_UNINSTALL, OptiScaler),
            (RENODX_INSTALL, RenoDx),
            (RENODX_INSTALL_FROM_FILE, RenoDx),
            (RENODX_UNINSTALL, RenoDx),
            (RENODX_UPDATE, RenoDx),
            (RENODX_SWITCH_RESHADE_CHANNEL, RenoDx),
            (SHARED_VULKAN_APPLY, SharedVulkan),
            (RETIRED_GAME_LEFTOVERS_UNREGISTER, SharedVulkan),
            (RENODX_DLSS_FIX_INSTALL, RenoDx),
            (RENODX_DLSS_FIX_UNINSTALL, RenoDx),
            (RENODX_DLSS_FIX_UPDATE, RenoDx),
        ];

        for (feature, expected) in cases {
            assert_eq!(feature_owner(feature), Some(expected), "feature {feature}");
        }
        assert!(feature_owner("future_feature").is_none());
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
            (OPTISCALER_INSTALL, Game),
            (OPTISCALER_UPDATE, Game),
            (OPTISCALER_RELOCATE, Game),
            (OPTISCALER_UNINSTALL, None),
            (RENODX_INSTALL, GameWithOptionalSharedVulkan),
            (RENODX_INSTALL_FROM_FILE, GameWithOptionalSharedVulkan),
            (RENODX_UNINSTALL, None),
            (RENODX_UPDATE, GameWithOptionalSharedVulkan),
            (RENODX_SWITCH_RESHADE_CHANNEL, GameWithOptionalSharedVulkan),
            (SHARED_VULKAN_APPLY, SharedVulkan),
            (RETIRED_GAME_LEFTOVERS_UNREGISTER, None),
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
