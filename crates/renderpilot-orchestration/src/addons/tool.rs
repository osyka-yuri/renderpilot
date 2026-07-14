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

use renderpilot_domain::AddonKind;

use super::capabilities::CapabilityProbeFuture;

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
    fn exclusive_block_message(&self) -> &'static str;

    /// On-disk signature when no DB record exists (shallow / bounded scan).
    fn unmanaged_present(&self, dir: &Path) -> bool;

    /// Whether a pure matcher outcome should appear as catalog-available.
    fn profile_available_from_resolution(&self, installable: bool, external: bool) -> bool;

    /// i18n key for the post-download finalizing progress phase.
    fn finalizing_phase(&self) -> &'static str;

    /// Best-effort cleanup of torn-install debris (caller detected the sentinel).
    fn recover_torn(&self, scan_dirs: &[&Path]);

    /// Loads (or reuses the cached) manifest and wraps it in a type-erased
    /// catalog capability probe. See [`super::capabilities`].
    fn load_capability_probe(&self) -> CapabilityProbeFuture;
}

/// Every registered tool. Adding an implementation means one more entry here.
pub(crate) static TOOLS: &[&dyn AddonTool] = &[&crate::addons::renodx::tool::RenoDxTool];

/// Lookup by kind. Returns `None` when a domain kind has not yet been registered.
/// This supports introducing shared domain primitives before a tool implementation.
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
    fn exclusive_block_messages_are_nonempty() {
        for t in TOOLS {
            assert!(
                !t.exclusive_block_message().is_empty(),
                "{:?} must provide an exclusive_block_message",
                t.kind()
            );
        }
    }
}
