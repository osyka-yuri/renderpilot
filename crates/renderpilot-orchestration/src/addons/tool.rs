//! Registered add-on tools: the single extension surface for cross-kind policy.
//!
//! Install / update / full availability stay in each tool's `use_cases` modules
//! (different engines and DTOs). What every framework path needs — kind identity,
//! exclusive peers, unmanaged signatures, catalog profile policy, catalog
//! capability probing, and the progress phase key — lives on
//! [`AddonTool`] and the [`TOOLS`] table.
//!
//! **New tool checklist**
//! 1. Add a variant to domain [`AddonKind`].
//! 2. Implement [`AddonTool`] in `your_tool/tool.rs`.
//! 3. Append `&YourTool` to [`TOOLS`].
//! 4. Add types / matcher / fetch / install / tracking / use_cases.

use std::path::{Path, PathBuf};

use renderpilot_application::ProxyTopologyRepository;
use renderpilot_domain::{AddonKind, InstalledAddon, InstalledAddonHostKind};

use super::capabilities::CapabilityProbeFuture;
use crate::game_mutation_lock::GameMutationGuard;
use crate::{Context, ServiceError};

#[path = "tool/active_binding.rs"]
mod active_binding;

/// Static policy and identity for one add-on tool RenderPilot can install.
///
/// Object-safe and sync on purpose: exclusivity and catalog cards must not depend
/// on typed install pipelines.
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

    /// Concrete files matching this tool's existing bounded unmanaged
    /// signature. The fallback keeps tools without path-level probes
    /// fail-closed by returning the directory as an opaque match.
    fn unmanaged_paths(&self, dir: &Path) -> Vec<PathBuf> {
        if self.unmanaged_present(dir) {
            vec![dir.to_path_buf()]
        } else {
            Vec::new()
        }
    }

    /// Whether a persisted install record still represents an active install.
    ///
    /// The default preserves the historical record-authoritative policy. Tools
    /// whose primary payload can be removed independently may tighten it using
    /// an inexpensive on-disk check. Mutation and cleanup flows still use the
    /// persisted record directly; this policy is for status, availability, and
    /// mutual-exclusion decisions.
    fn record_is_active(&self, _record: &InstalledAddon) -> bool {
        true
    }

    /// i18n key for the post-download finalizing progress phase.
    fn finalizing_phase(&self) -> &'static str;

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

    /// Loads the tool's capability source and wraps it in a type-erased
    /// catalog capability probe. A source may be a cached remote manifest or a
    /// local policy; see [`super::capabilities`].
    fn load_capability_probe(&self) -> CapabilityProbeFuture;
}

/// Every known tool. Adding a third tool means one more entry here.
pub(crate) static TOOLS: &[&dyn AddonTool] = &[
    &crate::addons::renodx::tool::RenoDxTool,
    &crate::addons::luma::tool::LumaTool,
    &crate::addons::optiscaler::tool::OptiScalerTool,
];

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
    tool(kind).map_or(&[], |registered| registered.exclusive_peers())
}

/// Returns the concrete matches from a registered tool's existing bounded
/// unmanaged probe. A caller may discount only exact receipt-owned paths.
#[must_use]
pub(crate) fn unmanaged_matching_paths(dir: &Path, kind: AddonKind) -> Vec<PathBuf> {
    tool(kind).map_or_else(Vec::new, |registered| registered.unmanaged_paths(dir))
}

/// Collects concrete unmanaged matches across the exact directories used by
/// install preflight.
#[must_use]
pub(crate) fn unmanaged_matching_paths_in_dirs(dirs: &[&Path], kind: AddonKind) -> Vec<PathBuf> {
    dirs.iter()
        .flat_map(|dir| unmanaged_matching_paths(dir, kind))
        .collect()
}

/// Returns whether `record` still represents an active install according to
/// the owning tool's policy.
#[must_use]
pub(crate) fn record_is_active(record: &InstalledAddon) -> bool {
    require_tool(record.kind()).record_is_active(record)
}

/// Whether a persisted record still describes an install bound to this game's
/// current loading chain.
///
/// The registered-tool predicate remains the inexpensive payload/local-state
/// check. Every record additionally requires a currently active game
/// registration. Proxy installs also require a compatible ReShade host in the
/// current executable root and a strict current ReShade `AddonPath` that
/// resolves to the record's payload directory. Shared Vulkan installs retain
/// their existing payload semantics for an active registration.
/// This is a read-only status query: it never repairs observations or changes
/// persisted ownership.
pub(crate) fn record_is_active_for_game(
    context: &Context,
    record: &InstalledAddon,
) -> Result<bool, ServiceError> {
    if !record_is_active(record) {
        return Ok(false);
    }

    let Some(game) = context.storage().find_active_game(record.game_id())? else {
        return Ok(false);
    };

    if !requires_proxy_host(record) {
        return Ok(true);
    }

    let override_path = crate::addons::game_context::executable_override(context, record.game_id());
    let analysis = crate::addons::game_analysis::analyze_game(&game, override_path.as_deref());
    let Ok(runtime_root) = crate::addons::game_analysis::install_target_dir(&analysis) else {
        return Ok(false);
    };

    let topology = context.storage().get_proxy_topology(record.game_id())?;
    Ok(active_binding::proxy_binding_matches_current_loading_chain(
        record,
        &runtime_root,
        analysis.facts.graphics.architecture(),
        topology.as_ref(),
    ))
}

fn requires_proxy_host(record: &InstalledAddon) -> bool {
    match record.kind() {
        AddonKind::Luma => true,
        AddonKind::RenoDx => record.host_kind() != Some(InstalledAddonHostKind::SharedVulkanLayer),
        _ => false,
    }
}

#[cfg(test)]
pub(super) fn proxy_binding_matches_current_loading_chain(
    record: &InstalledAddon,
    runtime_root: &Path,
    game_architecture: Option<renderpilot_domain::Architecture>,
    topology: Option<&renderpilot_domain::GameProxyTopology>,
) -> bool {
    active_binding::proxy_binding_matches_current_loading_chain(
        record,
        runtime_root,
        game_architecture,
        topology,
    )
}

/// Whether check-update supports a deep/advisory probe for `kind`.
/// Prefer the crate-public [`crate::addons::addon_supports_deep_check`].
#[must_use]
pub(crate) fn supports_deep_check(kind: AddonKind) -> bool {
    tool(kind).is_some_and(AddonTool::supports_deep_check)
}

/// Scans all possible install roots for a registered tool's on-disk signature.
#[must_use]
pub(crate) fn unmanaged_files_present_in_dirs(dirs: &[&Path], kind: AddonKind) -> bool {
    let Some(registered) = tool(kind) else {
        return false;
    };
    dirs.iter().any(|dir| registered.unmanaged_present(dir))
}

/// Lazily yields immediate regular files whose lowercased names match an
/// existing shallow unmanaged signature.
pub(crate) fn matching_regular_file_entries<'predicate>(
    dir: &Path,
    predicate: impl Fn(&str) -> bool + 'predicate,
) -> impl Iterator<Item = std::fs::DirEntry> + 'predicate {
    std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|file_type| file_type.is_file()))
        .filter(move |entry| predicate(&entry.file_name().to_string_lossy().to_ascii_lowercase()))
}

/// Collects matching paths for callers that need concrete receipt candidates.
pub(crate) fn matching_regular_file_paths(
    dir: &Path,
    predicate: impl Fn(&str) -> bool,
) -> Vec<PathBuf> {
    matching_regular_file_entries(dir, predicate)
        .map(|entry| entry.path())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_cover_every_addon_kind_exactly_once() {
        for kind in AddonKind::ALL {
            let matches: Vec<_> = TOOLS.iter().filter(|t| t.kind() == *kind).collect();
            assert_eq!(
                matches.len(),
                1,
                "expected exactly one AddonTool for {kind:?}, found {}",
                matches.len()
            );
        }
        assert_eq!(TOOLS.len(), AddonKind::ALL.len());
    }

    #[test]
    fn exclusive_peers_are_pairwise_registered() {
        for t in TOOLS {
            for &peer in t.exclusive_peers() {
                assert!(
                    tool(peer).is_some(),
                    "{:?} lists exclusive peer {peer:?} that is not registered",
                    t.kind()
                );
                assert!(
                    tool(peer)
                        .expect("peer")
                        .exclusive_peers()
                        .contains(&t.kind()),
                    "exclusive peer {peer:?} does not list {:?} back",
                    t.kind()
                );
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

    #[test]
    fn unmanaged_path_observation_matches_only_existing_shallow_signature_files() {
        let root = tempfile::tempdir().expect("tempdir");
        let addon = root.path().join("RenoDX-game.AddOn64");
        let unrelated = root.path().join("readme.txt");
        std::fs::write(&addon, b"addon").expect("addon");
        std::fs::write(&unrelated, b"notes").expect("unrelated file");

        let matches = unmanaged_matching_paths(root.path(), AddonKind::RenoDx);
        assert_eq!(matches, vec![addon]);
        assert!(unmanaged_matching_paths(root.path(), AddonKind::Luma).is_empty());
    }

    #[test]
    fn shallow_matching_entries_stop_when_the_consumer_finds_a_match() {
        use std::cell::Cell;

        let root = tempfile::tempdir().expect("tempdir");
        for index in 0..8 {
            std::fs::write(root.path().join(format!("file-{index}.txt")), b"file")
                .expect("write file");
        }

        let predicate_calls = Cell::new(0);
        let found = matching_regular_file_entries(root.path(), |_| {
            predicate_calls.set(predicate_calls.get() + 1);
            true
        })
        .next()
        .is_some();

        assert!(found);
        assert_eq!(predicate_calls.get(), 1);
    }
}
