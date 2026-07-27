//! Luma as a registered [`crate::addons::tool::AddonTool`].

use std::path::Path;

use renderpilot_domain::AddonKind;

use crate::addons::capabilities::{CapabilityProbe, CapabilityProbeFuture};
use crate::addons::matching::MatchFacts;
use crate::addons::tool::AddonTool;

use super::LUMA_PHASE_FINALIZING;
use super::install::recover_torn_install;
use super::manifest_store;
use super::matcher::{self, LumaResolution};
use super::types::LumaManifest;

/// Luma tool registration handle (zero-sized).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct LumaTool;

/// Matches `Luma-<Game>.addon`/`.addon64`/`.addon32` (case-insensitive input).
#[must_use]
pub(crate) fn is_luma_addon_file_name(lower: &str) -> bool {
    lower.starts_with("luma-")
        && (lower.ends_with(".addon") || lower.ends_with(".addon64") || lower.ends_with(".addon32"))
}

/// Matches a `.bak` sibling left by a torn install or engine backup.
/// Not counted as unmanaged — recovery removes these.
#[must_use]
pub(crate) fn is_luma_addon_backup_file_name(lower: &str) -> bool {
    lower.ends_with(".bak") && is_luma_addon_file_name(lower.strip_suffix(".bak").unwrap_or(lower))
}

/// Pure catalog probe over an already-loaded manifest — tests (and any other
/// caller that already holds a manifest) skip the network/cache round trip.
#[must_use]
pub(crate) fn capability_probe(manifest: LumaManifest) -> CapabilityProbe {
    let source_revision = manifest.generated_at.clone();
    CapabilityProbe::new(
        AddonKind::Luma,
        source_revision,
        move |facts: &MatchFacts| {
            let resolution = matcher::resolve(&manifest, facts);
            matches!(&resolution, LumaResolution::Installable(_))
        },
    )
}

fn unmanaged_present(game_dir: &Path) -> bool {
    // Luma marker: luma-*.addon/.addon64/.addon32, or a Luma/ tree with
    // framework-shaped content (shaders / nested addons). A non-empty Luma/
    // full of unrelated junk alone is not treated as unmanaged — that would
    // false-positive games with a coincidental content folder of the same name.
    if crate::addons::any_file_name_matches(game_dir, is_luma_addon_file_name) {
        return true;
    }

    let luma_dir = game_dir.join("Luma");
    if luma_dir.is_dir() {
        return luma_dir_has_framework_shaped_content(&luma_dir, 0);
    }
    false
}

/// Depth-capped walk: any `.hlsl`/`.fx`/`.fxh`/`.addon*` under `Luma/` counts.
fn luma_dir_has_framework_shaped_content(dir: &Path, depth: u8) -> bool {
    const MAX_DEPTH: u8 = 3;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if file_type.is_file() && is_luma_framework_file_name(&name) {
            return true;
        }
        if file_type.is_dir()
            && depth < MAX_DEPTH
            && luma_dir_has_framework_shaped_content(&entry.path(), depth + 1)
        {
            return true;
        }
    }
    false
}

fn is_luma_framework_file_name(lower: &str) -> bool {
    lower.ends_with(".hlsl")
        || lower.ends_with(".fx")
        || lower.ends_with(".fxh")
        || lower.ends_with(".addon")
        || lower.ends_with(".addon32")
        || lower.ends_with(".addon64")
}

impl AddonTool for LumaTool {
    fn kind(&self) -> AddonKind {
        AddonKind::Luma
    }

    fn exclusive_peers(&self) -> &'static [AddonKind] {
        &[AddonKind::RenoDx]
    }

    fn exclusive_block_message(&self, unmanaged: bool) -> &'static str {
        if unmanaged {
            "RenoDX files are present for this game; remove them before installing Luma Framework"
        } else {
            "RenoDX is installed for this game; uninstall it before installing Luma Framework"
        }
    }

    fn unmanaged_present(&self, dir: &Path) -> bool {
        unmanaged_present(dir)
    }

    fn finalizing_phase(&self) -> &'static str {
        LUMA_PHASE_FINALIZING
    }

    fn recover_torn(&self, scan_dirs: &[&Path]) {
        recover_torn_install(scan_dirs);
    }

    fn supports_deep_check(&self) -> bool {
        true
    }

    fn reconcile_legacy_locked(
        &self,
        context: &crate::Context,
        guard: &crate::game_mutation_lock::GameMutationGuard,
        record: &renderpilot_domain::InstalledAddon,
    ) -> Result<renderpilot_domain::InstalledAddon, crate::ServiceError> {
        super::reconciliation::reconcile_legacy_dlss_binding_locked(context, guard, record)
    }

    fn load_capability_probe(&self) -> CapabilityProbeFuture {
        Box::pin(async {
            Ok(capability_probe(
                manifest_store::get_or_fetch_manifest().await?,
            ))
        })
    }
}
