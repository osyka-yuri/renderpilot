//! Manual-folder and auto-scan orchestration for the game catalog.
//!
//! ## Modules
//!
//! - `detect` -- library-file detection modes + hash-cache I/O glue
//! - `roots` -- multi-install root derivation and library bucketing
//! - `reconcile` -- catalog identity merge for stable game ids
//! - `persist` -- write scan units and prune stale parent rows
//! - `hash_cache` -- populate/load/save `file_hash_cache` (crate-visible for
//!   auto_scan batch prefetch)
//! - existing: `discovery`, `paths`, `prune`, `recovery`, `scan_plan`, `auto`
//!
//! ## Dependency rules
//!
//! ```text
//! mod (scan_impl) -> detect, roots (via install_partitioner), persist, prune
//! detect          -> hash_cache, scan_plan
//! persist         -> reconcile, recovery, roots
//! roots           -> paths (lexical scope)
//! ```

mod detect;
// crate-visible for auto_scan batch prefetch (hard path).
pub(crate) mod hash_cache;
mod install_partitioner;
mod paths;
mod persist;
mod prune;
mod reconcile;
mod recovery;
mod roots;
mod scan_plan;

#[cfg(windows)]
mod auto;
#[cfg(windows)]
/// Auto-discovery logic.
pub mod discovery;

#[cfg(windows)]
pub(crate) use auto::scan_auto_in_shared_batch;
#[cfg(windows)]
pub use prune::prune_auto_scan_orphans;
#[cfg(windows)]
pub(crate) use reconcile::CatalogInstallIndex;

use std::path::PathBuf;

use install_partitioner::derive_install_roots;
use renderpilot_application::AppError;
use renderpilot_detection::{FileHashCache, LibraryPatternComponentDetector};
use renderpilot_platform_windows::ManualFolderGameSource;
use scan_plan::{DetectionMode, InstallRootStrategy, resolve_install_root_strategy};

use crate::ServiceError;

use super::ScanFolderCatalogResult;

use self::detect::detect_libraries;
use self::persist::persist_scan_results;

/// Scans `path` for manual-folder game installations.
///
/// The scan intentionally performs one filesystem detection pass over the selected root.
/// If multiple sibling game installs are detected under the selected path, detected
/// library files are reassigned to the best matching sub-installation by longest path
/// prefix.
///
/// Example:
///
/// ```text
/// D:/SteamLibrary/
///   steamapps/common/GameA/nvngx_dlss.dll
///   steamapps/common/GameB/bin/x64/nvngx_dlss.dll
/// ```
///
/// Shared prefix:
///
/// ```text
/// steamapps/common
/// ```
///
/// First diverging components:
///
/// ```text
/// GameA
/// GameB
/// ```
///
/// Result: two separate game installations (launcher-tagged when metadata is present).
///
/// When the selected folder itself is already a known launcher install (Steam /
/// GOG / Epic), the scan keeps a single install root. Splitting those trees by
/// diverging DLL paths would re-tag subfolders as `Manual` and drop the store
/// identity that auto-scan established.
pub(super) fn scan_folder_impl(
    context: &crate::Context,
    path: PathBuf,
) -> Result<Vec<ScanFolderCatalogResult>, ServiceError> {
    let detector = LibraryPatternComponentDetector::windows_default()
        .map_err(|error| AppError::detection_failed(error.to_string()))?;

    // Strategy is resolved after a single discover inside `scan_impl` so
    // launcher-owned installs never get split into Manual sub-roots.
    scan_impl(
        ScanInputs {
            context,
            detector: &detector,
        },
        &ManualFolderGameSource::new(path),
        DetectionMode::FullCached,
        InstallRootStrategy::FromSelectedIdentity,
        None,
        None,
    )
}

/// Borrowed storage + detector for one [`scan_impl`] invocation.
#[derive(Clone, Copy)]
struct ScanInputs<'a> {
    context: &'a crate::Context,
    detector: &'a LibraryPatternComponentDetector,
}

fn scan_impl(
    inputs: ScanInputs<'_>,
    source: &ManualFolderGameSource,
    detection_mode: DetectionMode,
    install_root_strategy: InstallRootStrategy,
    prefetched_cache: Option<&FileHashCache>,
    catalog_index: Option<&reconcile::CatalogInstallIndex>,
) -> Result<Vec<ScanFolderCatalogResult>, ServiceError> {
    let storage = inputs.context.storage();
    let detector = inputs.detector;

    let selected_game = source.discover_game()?;
    let _guard = crate::game_mutation_lock::enter_game_mutation_boundary(
        inputs.context,
        selected_game.id(),
    )?;
    let scope_root = selected_game.install_path().as_str().to_owned();
    let install_root_strategy =
        resolve_install_root_strategy(install_root_strategy, &selected_game);

    let libraries = detect_libraries(
        storage,
        detector,
        &selected_game,
        detection_mode,
        prefetched_cache,
    )?;

    let install_roots = derive_install_roots(&selected_game, &libraries, install_root_strategy);

    let results = persist_scan_results(
        storage,
        selected_game,
        libraries,
        install_roots,
        catalog_index,
    )?;

    prune::prune_stale_manual_games_under_scope(
        storage,
        &scope_root,
        &prune::game_ids_from_scan_results(&results),
    )?;

    Ok(results)
}
