//! Tool-agnostic framework for add-ons RenderPilot *introduces* into a game folder
//! (a proxy DLL plus a config, tracked for reversal and upstream updates), with each
//! tool a thin module over the shared mechanics.
//!
//! ## Obligatory call graph (install / availability)
//!
//! ```text
//! game_mutation_lock::lock(game_id)
//!   → records / require_game
//!   → analyze (+ install_target_dir / InstallRoots via game_analysis + reshade)
//!   → exclusivity / install_guard (scan_dir_paths) + torn recovery
//!   → tool resolve / risk / host_policy
//!   → fetch + engine / run_split_install
//!   → tracking::rebuild_install_record + records persist (or revert)
//! ```
//!
//! Availability shares the same scan roots via `availability_pipeline` so the
//! unmanaged exclusivity backstop cannot disagree with install.
//!
//! ## Extension surface: `tool::AddonTool`
//!
//! Cross-kind static policy (exclusive peers, unmanaged signatures, catalog
//! profile availability, finalizing phase key, torn recovery) lives on
//! `tool::AddonTool`. Registered tools sit in `tool::TOOLS`. Adding a third
//! tool means: domain `AddonKind` variant → `impl AddonTool` → one `TOOLS` entry
//! → thin module (types / matcher / fetch / install / tracking / use_cases).
//! Install and update **do not** go on the trait (different engines and DTOs).
//!
//! ## Module map
//!
//! Private modules are listed as plain identifiers (not rustdoc links) so public
//! crate docs stay free of private-intra-doc-link noise.
//!
//! * `tool` — `AddonTool` trait + registration table
//! * [`engine`] — serializable install plan ops, sentinel, rollback
//! * `durable` — addon ceremony over `file_mutation` (closed:
//!   install/targets/uninstall; multi-step: prepare + finish_sentinel)
//! * `mutation_targets` — live/sidecar path sets for durable transactions
//! * [`record`] / `records` — receipt → `InstalledAddon`; kind-aware row access
//! * `tracking` — dated display, host proxy path, canonical `RebuildParts` rebuild
//! * [`update`] / `file_update` — update verdict + in-place replace
//! * `vulkan_lock` — cross-game shared-resource mutex (Vulkan layer only)
//! * `exclusivity` — mutual exclusion policy over registered tools
//! * `install_guard` — shared exclusivity + torn recovery on install roots
//! * `availability_pipeline` — shared availability front half
//! * `progress` — sequential stages + finalizing phase
//! * `game_analysis` / `game_context` / `matching` — facts + match rules
//! * `reshade` — shared host subsystem (scan, policy, report, fetch, types)
//! * `anticheat` — risk gate
//! * [`capabilities`] — catalog profile snapshot
//! * [`renodx`] / [`luma`] — thin tools (types → matcher → fetch → install → tracking → use_cases)
//!
//! A sibling tool implements `tool::AddonTool`, registers in `tool::TOOLS`,
//! and reuses the shared modules; it must never import another tool module.
//!
//! ## Update strategy matrix
//!
//! Tools deliberately use different update engines; do not unify them casually:
//!
//! * **RenoDX** — replace tracked host/add-on files in place via
//!   `file_update` (single PE / addon payload identity is unambiguous).
//! * **Luma** — re-fetch a ZIP tree and apply a set-diff
//!   (`luma::use_cases::commands::update`) because the payload is a multi-file
//!   directory tree with nested assets (and optional dgVoodoo).
//!
//! ## Intentional asymmetries (not missing features)
//!
//! * **DLSS ownership** — Luma coordinates game DLSS via `managed_files` + catalog
//!   cascade. RenoDX DLSS-Fix is a separate companion tracked source, not a
//!   coordinated catalog binding.
//! * **Repair** — Luma repair is `force_full` update (set-diff reconverge). RenoDX
//!   repair reuses install/channel paths (flat PE identity).
//! * **Reinstall** — RenoDX proxy install is safely re-runnable; Luma tree install
//!   refuses an existing same-kind record (`records::ensure_no_record`) to avoid
//!   `.bak` litter.
//! * **RenoDX-only surface** — install-from-file, ReShade channel switch, shared
//!   Vulkan layer, DLSS-Fix UI/settings.

pub(crate) mod anticheat;
pub(crate) mod availability_pipeline;
pub mod capabilities;
mod catalog_message;
pub(crate) mod durable;
pub mod engine;
pub(crate) mod errors;
pub(crate) mod exclusivity;
pub(crate) mod file_update;
pub(crate) mod game_analysis;
pub(crate) mod game_context;
mod ini;
pub(crate) mod install_guard;
pub mod luma;
pub(crate) mod manifest_validate;
pub(crate) mod matching;
pub(crate) mod mutation_features;
pub(crate) mod mutation_targets;
pub(crate) mod progress;
pub mod record;
pub(crate) mod records;
pub mod renodx;
pub(crate) mod reshade;
pub(crate) mod tool;
pub(crate) mod tracking;
pub mod update;
pub(crate) mod vulkan_lock;

pub use catalog_message::CatalogMessage;

/// Whether check-update supports a deep/advisory probe for the given kind.
///
/// Single public entry for CLI/API; the trait method on `tool::AddonTool` is
/// the per-tool extension point.
#[must_use]
pub fn addon_supports_deep_check(kind: renderpilot_domain::AddonKind) -> bool {
    tool::supports_deep_check(kind)
}

#[cfg(test)]
pub(crate) mod test_support;

use std::path::{Path, PathBuf};

/// UTF-8 byte-order mark some publishing tools prepend to JSON, which `serde_json`
/// rejects; stripped at the parse boundary.
pub(crate) const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// Iterates a directory's immediate children, converts each regular-file name
/// to ASCII lowercase, and returns `true` the moment the predicate accepts one.
/// Returns `false` when the directory can't be read or no match is found.
pub(crate) fn any_file_name_matches(dir: &Path, predicate: impl Fn(&str) -> bool) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry.file_type().is_ok_and(|ft| ft.is_file())
            && predicate(&entry.file_name().to_string_lossy().to_ascii_lowercase())
    })
}

/// Converts a slice of [`renderpilot_domain::PathRef`]s into owned [`PathBuf`]s.
#[must_use]
pub(crate) fn path_bufs(paths: &[renderpilot_domain::PathRef]) -> Vec<PathBuf> {
    paths.iter().map(|p| PathBuf::from(p.as_str())).collect()
}

/// Composition-root hook for lazy managed-file reconciliation. Feature
/// coordinators call this without importing a concrete add-on implementation;
/// the registered `tool::AddonTool` owns the migration body.
pub(crate) fn reconcile_legacy_managed_files_locked(
    context: &crate::Context,
    guard: &crate::game_mutation_lock::GameMutationGuard,
    game_id: &renderpilot_domain::GameId,
) -> Result<Option<renderpilot_domain::InstalledAddon>, crate::ServiceError> {
    use renderpilot_application::InstalledAddonRepository;

    let Some(record) = context.storage().get_installed_addon(game_id)? else {
        return Ok(None);
    };
    let Some(registered) = tool::tool(record.kind()) else {
        return Ok(Some(record));
    };
    registered
        .reconcile_legacy_locked(context, guard, &record)
        .map(Some)
}

#[cfg(test)]
mod tests {
    use super::any_file_name_matches;
    use tempfile::tempdir;

    #[test]
    fn file_name_matching_lowercases_regular_files_and_ignores_directories() {
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join("RenoDX-Cp2077.Addon64")).expect("create directory");
        assert!(!any_file_name_matches(dir.path(), |name| name == "renodx-cp2077.addon64"));

        std::fs::write(dir.path().join("RenoDX-Other.Addon64"), b"x").expect("write match");

        assert!(any_file_name_matches(dir.path(), |name| name == "renodx-other.addon64"));
    }

    #[test]
    fn file_name_matching_returns_false_for_a_missing_directory() {
        let dir = tempdir().expect("tempdir");
        assert!(!any_file_name_matches(&dir.path().join("missing"), |_| {
            true
        }));
    }
}
