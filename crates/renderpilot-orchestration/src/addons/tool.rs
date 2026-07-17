//! Registered add-on tools: the single extension surface for cross-kind policy.
//!
//! Install / update / full availability stay in each tool's `use_cases` modules
//! (different engines and DTOs). What every framework path needs — kind identity,
//! exclusive peers, unmanaged signatures, catalog profile policy, catalog
//! capability probing, progress phase key, torn recovery — lives on
//! [`AddonTool`] and the [`TOOLS`] table.
//!
//! **New tool checklist**
//! 1. Add a variant to domain [`AddonKind`].
//! 2. Implement [`AddonTool`] in `your_tool/tool.rs`.
//! 3. Append `&YourTool` to [`TOOLS`].
//! 4. Add types / matcher / fetch / install / tracking / use_cases.

use std::path::Path;

use renderpilot_domain::{AddonKind, InstalledAddon};

use super::capabilities::CapabilityProbeFuture;
use crate::game_mutation_lock::GameMutationGuard;
use crate::{Context, ServiceError};

/// Static policy and identity for one add-on tool RenderPilot can install.
///
/// Object-safe and sync on purpose: exclusivity, catalog cards, and torn recovery
/// must not depend on typed install pipelines.
pub(crate) trait AddonTool: Send + Sync {
    /// Domain kind this tool owns.
    fn kind(&self) -> AddonKind;

    /// Other kinds mutually exclusive with this tool.
    fn exclusive_peers(&self) -> &'static [AddonKind];

    /// User-facing message when an exclusive peer blocks install of this tool.
    ///
    /// `unmanaged` is true when the block came from on-disk peer files rather
    /// than a managed install record — callers must not tell the user to
    /// "uninstall" something that has no record.
    fn exclusive_block_message(&self, unmanaged: bool) -> &'static str;

    /// On-disk signature when no DB record exists (shallow / bounded scan).
    fn unmanaged_present(&self, dir: &Path) -> bool;

    /// i18n key for the post-download finalizing progress phase.
    fn finalizing_phase(&self) -> &'static str;

    /// Best-effort cleanup of torn-install debris (caller detected the sentinel).
    fn recover_torn(&self, scan_dirs: &[&Path]);

    /// Lazily upgrades legacy install-record fields under the game mutation
    /// guard. Default: return the record unchanged. Tools that once stored
    /// coordinated ownership in generic engine sets implement a real migration.
    fn reconcile_legacy_locked(
        &self,
        _context: &Context,
        _guard: &GameMutationGuard,
        record: &InstalledAddon,
    ) -> Result<InstalledAddon, ServiceError> {
        Ok(record.clone())
    }

    /// Whether check-update supports an expensive deep/advisory probe flag.
    /// Default: false. Luma's multi-file ZIP tree can promote advisory sources.
    fn supports_deep_check(&self) -> bool {
        false
    }

    /// Loads (or reuses the cached) manifest and wraps it in a type-erased
    /// catalog capability probe. See [`super::capabilities`].
    fn load_capability_probe(&self) -> CapabilityProbeFuture;
}

/// Every known tool. Adding a third tool means one more entry here.
pub(crate) static TOOLS: &[&dyn AddonTool] = &[&crate::addons::renodx::tool::RenoDxTool];

/// Lookup by kind. Returns `None` only if domain and registration have drifted
/// (covered by the exhaustiveness test below).
#[must_use]
pub(crate) fn tool(kind: AddonKind) -> Option<&'static dyn AddonTool> {
    TOOLS.iter().copied().find(|t| t.kind() == kind)
}

/// Required tool for a kind used by framework paths that already know the kind
/// is valid (e.g. exclusivity for a requesting install).
#[must_use]
pub(crate) fn require_tool(kind: AddonKind) -> &'static dyn AddonTool {
    tool(kind).unwrap_or_else(|| {
        panic!(
            "AddonKind::{kind:?} is not registered in addons::tool::TOOLS — \
             implement AddonTool and add it to the table"
        )
    })
}

/// Returns the registered peers that are mutually exclusive with `kind`.
#[must_use]
pub(crate) fn exclusive_peers(kind: AddonKind) -> &'static [AddonKind] {
    match kind {
        AddonKind::Luma => &[AddonKind::RenoDx],
        _ => tool(kind).map_or(&[], |registered| registered.exclusive_peers()),
    }
}

/// Returns whether a registered tool's bounded on-disk signature is present.
#[must_use]
pub(crate) fn unmanaged_files_present(dir: &Path, kind: AddonKind) -> bool {
    match kind {
        AddonKind::Luma => legacy_luma_unmanaged_present(dir),
        _ => tool(kind).is_some_and(|registered| registered.unmanaged_present(dir)),
    }
}

/// Transitional Luma detector used before its full `AddonTool` registration.
/// It preserves the RenoDX exclusivity backstop without exposing Luma commands.
fn legacy_luma_unmanaged_present(game_dir: &Path) -> bool {
    if super::any_file_name_matches(game_dir, |name| {
        name.starts_with("luma-")
            && (name.ends_with(".addon")
                || name.ends_with(".addon32")
                || name.ends_with(".addon64"))
    }) {
        return true;
    }
    let luma_dir = game_dir.join("Luma");
    luma_dir.is_dir() && legacy_luma_dir_has_framework_content(&luma_dir, 0)
}

fn legacy_luma_dir_has_framework_content(dir: &Path, depth: u8) -> bool {
    const MAX_DEPTH: u8 = 3;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let Ok(file_type) = entry.file_type() else {
            return false;
        };
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        (file_type.is_file()
            && (name.ends_with(".hlsl")
                || name.ends_with(".fx")
                || name.ends_with(".fxh")
                || name.ends_with(".addon")
                || name.ends_with(".addon32")
                || name.ends_with(".addon64")))
            || (file_type.is_dir()
                && depth < MAX_DEPTH
                && legacy_luma_dir_has_framework_content(&entry.path(), depth + 1))
    })
}

/// Whether check-update supports a deep/advisory probe for `kind`.
/// Prefer the crate-public [`crate::addons::addon_supports_deep_check`].
#[must_use]
pub(crate) fn supports_deep_check(kind: AddonKind) -> bool {
    tool(kind).is_some_and(|registered| registered.supports_deep_check())
}

/// Scans all possible install roots for a registered tool's on-disk signature.
#[must_use]
pub(crate) fn unmanaged_files_present_in_dirs(dirs: &[&Path], kind: AddonKind) -> bool {
    dirs.iter().any(|dir| unmanaged_files_present(dir, kind))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_tools_have_distinct_known_kinds() {
        for (index, tool) in TOOLS.iter().enumerate() {
            assert!(
                AddonKind::ALL.contains(&tool.kind()),
                "registered tool {:?} has no domain AddonKind",
                tool.kind()
            );
            assert!(
                TOOLS[..index]
                    .iter()
                    .all(|previous| previous.kind() != tool.kind()),
                "AddonTool for {:?} is registered more than once",
                tool.kind()
            );
        }
    }

    #[test]
    fn exclusive_peers_are_pairwise_registered() {
        for t in TOOLS {
            for &peer in t.exclusive_peers() {
                assert!(AddonKind::ALL.contains(&peer));
            }
        }
    }

    #[test]
    fn exclusive_block_messages_are_nonempty_for_record_and_unmanaged() {
        for t in TOOLS {
            for unmanaged in [false, true] {
                assert!(
                    !t.exclusive_block_message(unmanaged).is_empty(),
                    "{:?} must provide exclusive_block_message(unmanaged={unmanaged})",
                    t.kind()
                );
            }
            assert_ne!(
                t.exclusive_block_message(false),
                t.exclusive_block_message(true),
                "{:?} should distinguish record vs unmanaged block copy",
                t.kind()
            );
        }
    }
}
