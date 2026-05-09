use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
    thread,
};

use renderpilot_domain::GameId;
use renderpilot_platform_windows::game_libraries::DiscoveredGameSources;
use serde::Serialize;

use super::catalog::GameDetailsOutput;
use super::utils::{normalized_path_string, to_json, JsonResult};
use crate::{
    catalog::{self, AutoScanBatch},
    CliError,
};

/// Synthetic `root` label used in [`AutoScanOutput::errors`] for failures
/// that are not attached to a single install path (catalog-wide pruning
/// failure, batch open failure). Surfacing a stable, human-readable label
/// keeps the JSON payload self-explanatory and avoids a confusing empty
/// `root: ""` field.
const GLOBAL_ERROR_LABEL_PRUNE: &str = "auto-scan:prune";
const GLOBAL_ERROR_LABEL_OPEN_BATCH: &str = "auto-scan:open";

/// Hard cap on worker threads spawned for one auto-scan batch.
///
/// File walking is dominated by directory I/O syscalls and per-file
/// `symlink_metadata` reads, which on Windows / NTFS scale well across a
/// handful of threads but show diminishing returns past ~4. The actual worker
/// count is also bounded by the number of install paths being scanned (no
/// point spawning idle threads) and by the number of available CPU cores.
const AUTO_SCAN_MAX_WORKERS: usize = 4;

/// Scans a manually chosen folder.
///
/// JSON payload:
/// `{ "games": [ ... ] }`
pub fn scan_manual_folder(path: PathBuf) -> JsonResult {
    let games = catalog::scan_folder(path)?
        .into_iter()
        .map(|result| GameDetailsOutput::load(result.game.id().clone()))
        .collect::<Result<Vec<_>, _>>()?;

    to_json(GamesOutput { games })
}

/// Automatically discovers game install folders and scans each one.
///
/// Discovery returns per-game install directories, not launcher library roots,
/// so each scan operates on a real game folder rather than on a parent like
/// `steamapps/common` or `Program Files/EA Games`. Stale catalog rows that
/// were previously persisted at a launcher root or at a non-game direct
/// child of one (e.g. `steamapps/common/Steam Controller Configs`) are pruned
/// before the new scan begins.
///
/// The scan is best-effort across folders: one failing folder does not prevent
/// other folders from being scanned. Failures are returned in `errors` instead
/// of being silently discarded.
///
/// Performance: a single [`AutoScanBatch`] is opened up-front and reused for
/// every install directory. This avoids per-game SQLite open / WAL pragma /
/// migrations, per-game `SELECT FROM file_hash_cache`, and per-game
/// pattern-detector construction.
pub fn scan_auto_libraries() -> JsonResult {
    let DiscoveredGameSources {
        install_paths,
        library_roots,
    } = renderpilot_platform_windows::game_libraries::discover_game_sources();

    let library_root_keys = library_roots
        .iter()
        .map(|root| normalized_path_string(root))
        .collect::<Vec<_>>();
    let install_path_keys = install_paths
        .iter()
        .map(|path| normalized_path_string(path))
        .collect::<Vec<_>>();

    let mut scan = AutoScanAccumulator::default();

    if let Err(error) =
        catalog::prune_auto_scan_orphans_in_catalog(&library_root_keys, &install_path_keys)
    {
        scan.push_global_error(GLOBAL_ERROR_LABEL_PRUNE, error);
    }

    let batch = match catalog::open_auto_scan_batch() {
        Ok(batch) => batch,
        Err(error) => {
            scan.push_global_error(GLOBAL_ERROR_LABEL_OPEN_BATCH, error);
            return to_json(scan.into_output());
        }
    };

    let outcome = scan_install_paths_in_parallel(&batch, install_paths);
    scan.merge(outcome);

    to_json(scan.into_output())
}

/// Drives one auto-scan batch across multiple install directories in parallel.
///
/// Workers cooperatively pull install paths from a shared atomic index. Each
/// scan calls into the same [`AutoScanBatch`], whose `SqliteStorage` already
/// serialises concurrent writes through its internal `Mutex<Connection>`.
/// File walking and SHA-256 work fan out across workers; the only contention
/// points are (1) the SQLite mutex during `save_scan_result` /
/// `save_file_hash_cache`, and (2) the result accumulator, both held briefly
/// per scan.
fn scan_install_paths_in_parallel(
    batch: &AutoScanBatch,
    install_paths: Vec<PathBuf>,
) -> AutoScanAccumulator {
    if install_paths.is_empty() {
        return AutoScanAccumulator::default();
    }

    let worker_count = effective_worker_count(install_paths.len());
    let next_index = AtomicUsize::new(0);
    let accumulator = Mutex::new(AutoScanAccumulator::default());
    let paths: &[PathBuf] = install_paths.as_slice();

    thread::scope(|scope| {
        for _ in 0..worker_count {
            scope.spawn(|| {
                loop {
                    let index = next_index.fetch_add(1, Ordering::Relaxed);
                    let Some(path) = paths.get(index).cloned() else {
                        break;
                    };

                    let outcome = catalog::scan_auto_in_batch(batch, path.clone());

                    // Held only for the few microseconds it takes to record
                    // the per-scan outcome. The heavy work (file walk,
                    // SHA-256, DB writes inside the SQLite mutex) happens
                    // outside the lock.
                    let mut guard = accumulator
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    guard.record_scan_outcome(&path, outcome);
                }
            });
        }
    });

    mutex_into_inner_or_recover(accumulator)
}

fn mutex_into_inner_or_recover<T>(mutex: Mutex<T>) -> T {
    mutex
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Compile-time guard: parallel auto-scan shares [`AutoScanBatch`] across
/// `std::thread::scope` workers; these types must remain `Send + Sync`.
#[allow(dead_code)]
fn assert_auto_scan_shared_state_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<renderpilot_storage_sqlite::SqliteStorage>();
    assert_send_sync::<renderpilot_detection::FileHashCache>();
    assert_send_sync::<renderpilot_detection::LibraryPatternComponentDetector>();
    assert_send_sync::<crate::catalog::AutoScanBatch>();
}

fn effective_worker_count(install_path_count: usize) -> usize {
    let cpu_workers = thread::available_parallelism()
        .map(|nz| nz.get())
        .unwrap_or(1);

    install_path_count
        .min(AUTO_SCAN_MAX_WORKERS)
        .min(cpu_workers)
        .max(1)
}

#[derive(Debug, Default)]
struct AutoScanAccumulator {
    games: Vec<GameDetailsOutput>,
    errors: Vec<ScanErrorOutput>,
}

impl AutoScanAccumulator {
    fn record_scan_outcome(
        &mut self,
        root: &Path,
        outcome: Result<Vec<crate::catalog::ScanFolderCatalogResult>, CliError>,
    ) {
        match outcome {
            Ok(results) => {
                for result in results {
                    self.push_game(root, result.game.id().clone());
                }
            }
            Err(error) => self.push_error(root, error),
        }
    }

    fn merge(&mut self, other: AutoScanAccumulator) {
        self.games.extend(other.games);
        self.errors.extend(other.errors);
    }

    fn push_game(&mut self, root: &Path, game_id: GameId) {
        match GameDetailsOutput::load(game_id) {
            Ok(details) => self.games.push(details),
            Err(error) => self.push_error(root, error),
        }
    }

    fn push_error(&mut self, root: &Path, error: CliError) {
        self.errors.push(ScanErrorOutput::new(root, error));
    }

    /// Records a single global failure (catalog-wide pruning, batch open,
    /// etc.) under an explicit synthetic label. Used for errors that are
    /// not tied to a specific install root, so the JSON payload always
    /// carries a meaningful `root` value instead of an empty string.
    fn push_global_error(&mut self, label: &str, error: CliError) {
        self.errors.push(ScanErrorOutput::with_label(label, error));
    }

    fn into_output(self) -> AutoScanOutput {
        AutoScanOutput {
            games: self.games,
            errors: self.errors,
        }
    }
}

#[derive(Debug, Serialize)]
struct GamesOutput {
    games: Vec<GameDetailsOutput>,
}

#[derive(Debug, Serialize)]
struct AutoScanOutput {
    games: Vec<GameDetailsOutput>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    errors: Vec<ScanErrorOutput>,
}

#[derive(Debug, Serialize)]
struct ScanErrorOutput {
    root: String,
    message: String,
}

impl ScanErrorOutput {
    fn new(root: &Path, error: CliError) -> Self {
        Self {
            root: normalized_path_string(root),
            message: error.to_string(),
        }
    }

    fn with_label(label: &str, error: CliError) -> Self {
        Self {
            root: label.to_owned(),
            message: error.to_string(),
        }
    }
}
