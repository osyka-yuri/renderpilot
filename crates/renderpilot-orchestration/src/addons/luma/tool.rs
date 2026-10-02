//! Luma as a registered [`crate::addons::tool::AddonTool`].

use std::ops::ControlFlow;
use std::path::Path;

use renderpilot_domain::{AddonKind, InstalledAddon, LibraryComponent, TrackedSourceRole};

use crate::addons::capabilities::{CapabilityProbe, CapabilityProbeFuture};
use crate::addons::matching::MatchFacts;
use crate::addons::tool::AddonTool;

use super::LUMA_PHASE_FINALIZING;
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
        && Path::new(lower).extension().is_some_and(|ext| {
            ext.eq_ignore_ascii_case("addon")
                || ext.eq_ignore_ascii_case("addon64")
                || ext.eq_ignore_ascii_case("addon32")
        })
}

/// Matches a `.bak` sibling left by a torn install or engine backup.
/// Not counted as unmanaged — recovery removes these.
#[must_use]
pub(crate) fn is_luma_addon_backup_file_name(lower: &str) -> bool {
    Path::new(lower)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("bak"))
        && is_luma_addon_file_name(lower.strip_suffix(".bak").unwrap_or(lower))
}

/// Whether this Luma receipt identifies `path` as one of its dgVoodoo support
/// files. This keeps the read-side host classifier from treating wrappers as
/// ReShade hosts while leaving the dgVoodoo profile's filename policy local.
#[must_use]
pub(crate) fn is_recorded_dgvoodoo_dependency(record: &InstalledAddon, path: &Path) -> bool {
    record.kind() == AddonKind::Luma
        && record
            .tracked_sources()
            .iter()
            .any(|source| source.role() == TrackedSourceRole::DgVoodooWrapper)
        && super::dgvoodoo::is_dependency_basename(path)
}

/// Pure catalog probe over an already-loaded manifest — tests (and any other
/// caller that already holds a manifest) skip the network/cache round trip.
#[must_use]
pub(crate) fn capability_probe(manifest: LumaManifest) -> CapabilityProbe {
    let source_revision = manifest.generated_at.clone();
    CapabilityProbe::new(
        AddonKind::Luma,
        source_revision,
        move |facts: &MatchFacts, _components: &[LibraryComponent]| {
            let resolution = matcher::resolve(&manifest, facts);
            matches!(&resolution, LumaResolution::Installable(_))
        },
    )
}

fn unmanaged_present(game_dir: &Path) -> bool {
    if crate::addons::tool::matching_regular_file_entries(game_dir, is_luma_addon_file_name)
        .next()
        .is_some()
    {
        return true;
    }

    let luma_dir = game_dir.join("Luma");
    luma_dir.is_dir()
        && visit_luma_framework_paths(&luma_dir, 0, &mut |_| ControlFlow::Break(())).is_break()
}

fn unmanaged_paths(game_dir: &Path) -> Vec<std::path::PathBuf> {
    let mut matches =
        crate::addons::tool::matching_regular_file_paths(game_dir, is_luma_addon_file_name);
    let luma_dir = game_dir.join("Luma");
    if luma_dir.is_dir() {
        let _ = visit_luma_framework_paths(&luma_dir, 0, &mut |entry| {
            matches.push(entry.path());
            ControlFlow::Continue(())
        });
    }
    matches
}

const MAX_LUMA_FRAMEWORK_DEPTH: u8 = 3;

fn visit_luma_framework_paths(
    dir: &Path,
    depth: u8,
    visitor: &mut impl FnMut(&std::fs::DirEntry) -> ControlFlow<()>,
) -> ControlFlow<()> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return ControlFlow::Continue(());
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if file_type.is_file() && is_luma_framework_file_name(&name) {
            visitor(&entry)?;
        } else if file_type.is_dir() && depth < MAX_LUMA_FRAMEWORK_DEPTH {
            visit_luma_framework_paths(&entry.path(), depth + 1, visitor)?;
        }
    }
    ControlFlow::Continue(())
}

/// Depth-capped walk: any `.hlsl`/`.fx`/`.fxh`/`.addon*` under `Luma/` counts.
fn is_luma_framework_file_name(lower: &str) -> bool {
    Path::new(lower).extension().is_some_and(|ext| {
        ext.eq_ignore_ascii_case("hlsl")
            || ext.eq_ignore_ascii_case("fx")
            || ext.eq_ignore_ascii_case("fxh")
            || ext.eq_ignore_ascii_case("addon")
            || ext.eq_ignore_ascii_case("addon32")
            || ext.eq_ignore_ascii_case("addon64")
    })
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

    fn unmanaged_paths(&self, dir: &Path) -> Vec<std::path::PathBuf> {
        unmanaged_paths(dir)
    }

    fn record_is_active(&self, record: &InstalledAddon) -> bool {
        crate::fs::is_readable_non_empty_file(Path::new(record.addon_file().as_str()))
    }

    fn finalizing_phase(&self) -> &'static str {
        LUMA_PHASE_FINALIZING
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

#[cfg(test)]
mod tests {
    use super::{unmanaged_paths, unmanaged_present};

    #[test]
    fn luma_probe_is_case_insensitive_regular_file_only_and_depth_capped() {
        let root = tempfile::tempdir().expect("tempdir");
        let shallow_addon = root.path().join("LUMA-game.ADDON64");
        std::fs::write(&shallow_addon, b"addon").expect("shallow addon");

        let luma = root.path().join("Luma");
        let depth_three = luma.join("one/two/three/SHADER.FX");
        let depth_four = luma.join("one/two/three/four/too-deep.fx");
        let directory_with_addon_name = luma.join("looks-like.addon");
        std::fs::create_dir_all(depth_three.parent().expect("depth three parent"))
            .expect("create depth three");
        std::fs::create_dir_all(depth_four.parent().expect("depth four parent"))
            .expect("create depth four");
        std::fs::write(&depth_three, b"shader").expect("depth three shader");
        std::fs::write(&depth_four, b"shader").expect("depth four shader");
        std::fs::create_dir(&directory_with_addon_name).expect("directory with signature name");

        let paths = unmanaged_paths(root.path());
        assert_eq!(paths, vec![shallow_addon, depth_three]);
        assert_eq!(unmanaged_present(root.path()), !paths.is_empty());
    }

    #[test]
    fn luma_boolean_probe_matches_framework_enumeration_without_shallow_addon() {
        let root = tempfile::tempdir().expect("tempdir");
        let framework_file = root.path().join("Luma/SHADERS/effect.FXH");
        std::fs::create_dir_all(framework_file.parent().expect("framework parent"))
            .expect("create framework directory");
        std::fs::write(&framework_file, b"shader").expect("framework file");

        let paths = unmanaged_paths(root.path());
        assert_eq!(paths, vec![framework_file]);
        assert_eq!(unmanaged_present(root.path()), !paths.is_empty());
    }
}
