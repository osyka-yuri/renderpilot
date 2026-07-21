//! RenoDX as a registered [`crate::addons::tool::AddonTool`].

use std::path::Path;

use renderpilot_domain::{AddonKind, InstalledAddon};

use crate::addons::capabilities::{CapabilityProbe, CapabilityProbeFuture};
use crate::addons::matching::MatchFacts;
use crate::addons::tool::AddonTool;

use super::RENODX_PHASE_FINALIZING;
use super::install::recover_torn_install;
use super::manifest_store;
use super::matcher::{self, RenoDxResolution};
use super::types::RenoDxManifest;

/// RenoDX tool registration handle (zero-sized).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RenoDxTool;

/// Matches `renodx-<slug>.addon64` / `.addon32` (case-insensitive input).
///
/// Shared by unmanaged-presence detection and on-disk discovery. DLSS-fix files
/// (`renodx-dlssg_to_fsr3_*`) also match; callers that must exclude them filter.
#[must_use]
pub(crate) fn is_renodx_addon_file_name(lower: &str) -> bool {
    lower.starts_with("renodx-") && (lower.ends_with(".addon64") || lower.ends_with(".addon32"))
}

/// Pure catalog probe over an already-loaded manifest — tests (and any other
/// caller that already holds a manifest) skip the network/cache round trip.
#[must_use]
pub(crate) fn capability_probe(manifest: RenoDxManifest) -> CapabilityProbe {
    CapabilityProbe::new(AddonKind::RenoDx, move |facts: &MatchFacts| {
        let resolution = matcher::resolve(&manifest, facts);
        matches!(
            &resolution,
            RenoDxResolution::Installable(_) | RenoDxResolution::External { .. }
        )
    })
}

fn unmanaged_present(game_dir: &Path) -> bool {
    crate::addons::any_file_name_matches(game_dir, is_renodx_addon_file_name)
}

impl AddonTool for RenoDxTool {
    fn kind(&self) -> AddonKind {
        AddonKind::RenoDx
    }

    fn exclusive_peers(&self) -> &'static [AddonKind] {
        &[AddonKind::Luma]
    }

    fn exclusive_block_message(&self, unmanaged: bool) -> &'static str {
        if unmanaged {
            "Luma Framework files are present for this game; remove them before installing RenoDX"
        } else {
            "Luma Framework is installed for this game; uninstall it before installing RenoDX"
        }
    }

    fn unmanaged_present(&self, dir: &Path) -> bool {
        unmanaged_present(dir)
    }

    fn record_is_active(&self, record: &InstalledAddon) -> bool {
        crate::fs::is_readable_non_empty_file(Path::new(record.addon_file().as_str()))
    }

    fn finalizing_phase(&self) -> &'static str {
        RENODX_PHASE_FINALIZING
    }

    fn recover_torn(&self, scan_dirs: &[&Path]) {
        recover_torn_install(scan_dirs);
    }

    fn load_capability_probe(&self) -> CapabilityProbeFuture {
        Box::pin(async {
            Ok(capability_probe(
                manifest_store::get_or_fetch_manifest().await?,
            ))
        })
    }
}
