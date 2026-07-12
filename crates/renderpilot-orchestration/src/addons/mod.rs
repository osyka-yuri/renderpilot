//! Tool-agnostic framework for add-ons RenderPilot *introduces* into a game folder
//! (a proxy DLL plus a config, tracked for reversal and upstream updates), with each
//! tool a thin module over the shared mechanics.
//!
//! ## Obligatory call graph (install / availability)
//!
//! ```text
//! operation_lock(game_id)
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
//! * [`record`] / `records` — receipt → `InstalledAddon`; kind-aware row access
//! * `tracking` — dated display, host proxy path, shared record rebuild
//! * [`update`] / `file_update` — update verdict + in-place replace
//! * `operation_lock` — per-game mutex (keyed by game_id alone)
//! * `exclusivity` + `registry` — mutual exclusion facade over tools
//! * `install_guard` — shared exclusivity + torn recovery on install roots
//! * `availability_pipeline` — shared availability front half
//! * `progress` — sequential stages + finalizing phase
//! * `game_analysis` / `game_context` / `matching` — facts + match rules
//! * `reshade` — shared host subsystem (scan, policy, report, fetch, types)
//! * `anticheat` — risk gate
//! * [`capabilities`] — catalog profile snapshot
//! * [`renodx`] — tool-specific types, matching, fetch, install, tracking, and use cases
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

pub(crate) mod anticheat;
pub(crate) mod availability_pipeline;
pub mod capabilities;
pub mod engine;
pub(crate) mod errors;
pub(crate) mod exclusivity;
pub(crate) mod file_update;
pub(crate) mod game_analysis;
pub(crate) mod game_context;
mod ini;
pub(crate) mod install_guard;
pub(crate) mod manifest_validate;
pub(crate) mod matching;
pub(crate) mod operation_lock;
pub(crate) mod progress;
pub mod record;
pub(crate) mod records;
pub(crate) mod registry;
pub mod renodx;
pub(crate) mod reshade;
pub(crate) mod tool;
pub(crate) mod tracking;
pub mod update;

#[cfg(test)]
pub(crate) mod test_support;

use std::path::{Path, PathBuf};

/// UTF-8 byte-order mark some publishing tools prepend to JSON, which `serde_json`
/// rejects; stripped at the parse boundary.
pub(crate) const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// Best-effort path canonicalization: resolves symlinks and normalizes when the
/// path exists on disk, falls back to the input path otherwise.
#[must_use]
pub(crate) fn canonicalize_best_effort(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

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
