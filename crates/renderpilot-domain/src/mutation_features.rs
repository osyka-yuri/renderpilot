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
/// RenoDX DLSS-Fix companion install.
pub const RENODX_DLSS_FIX_INSTALL: &str = "renodx_dlss_fix_install";
/// RenoDX DLSS-Fix companion uninstall.
pub const RENODX_DLSS_FIX_UNINSTALL: &str = "renodx_dlss_fix_uninstall";
/// RenoDX DLSS-Fix companion update or payload-only repair.
pub const RENODX_DLSS_FIX_UPDATE: &str = "renodx_dlss_fix_update";

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
}
