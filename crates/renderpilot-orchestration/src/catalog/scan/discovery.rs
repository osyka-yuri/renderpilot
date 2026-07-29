use std::{
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};

use crate::ServiceError;
use renderpilot_domain::GameId;

use crate::catalog::auto_scan::{AutoScanBatch, open_auto_scan_batch, scan_auto_in_batch};

/// Synthetic `root` label used in [`AutoScanDiscoveryResult::errors`] for failures
/// that are not attached to a single install path (catalog-wide pruning
/// failure, batch open failure). Surfacing a stable, human-readable label
/// keeps the JSON payload self-explanatory and avoids a confusing empty
/// `root: ""` field.
#[derive(Debug, Clone, Copy)]
pub enum GlobalErrorLabel {
    /// Pruning failure.
    Prune,
    /// Batch open failure.
    OpenBatch,
}

impl std::fmt::Display for GlobalErrorLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Prune => write!(f, "auto-scan:prune"),
            Self::OpenBatch => write!(f, "auto-scan:open"),
        }
    }
}

/// Hard cap on worker threads spawned for one auto-scan batch.
///
/// File walking is dominated by directory I/O syscalls and per-file
/// `symlink_metadata` reads, which on Windows / NTFS scale well across a
/// handful of threads but show diminishing returns past ~4. The actual worker
/// count is also bounded by the number of install paths being scanned (no
/// point spawning idle threads) and by the number of available CPU cores.
const AUTO_SCAN_MAX_WORKERS: usize = 4;

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
pub fn scan_auto_libraries(context: &crate::Context) -> AutoScanDiscoveryResult {
    scan_auto_libraries_with_checkpoints(context, false)
}

/// Background variant that may skip Steam installs whose authoritative
/// appmanifest fingerprint is unchanged. All other sources remain full scans.
pub fn scan_auto_libraries_background(context: &crate::Context) -> AutoScanDiscoveryResult {
    scan_auto_libraries_with_checkpoints(context, true)
}

fn scan_auto_libraries_with_checkpoints(
    context: &crate::Context,
    allow_checkpoint_skip: bool,
) -> AutoScanDiscoveryResult {
    let _scan_guard = context.catalog_scan_guard();
    let (library_root_keys, authoritative_root_keys, install_path_keys, installs) =
        discover_normalized_auto_scan_inputs(allow_checkpoint_skip);

    let mut scan = AutoScanAccumulator::default();

    match super::prune_auto_scan_orphans(
        context,
        &library_root_keys,
        &authoritative_root_keys,
        &install_path_keys,
    ) {
        Ok(removed) => scan.delta.removed_game_ids = removed,
        Err(error) => scan.push_global_error(GlobalErrorLabel::Prune, &error),
    }

    let batch = match open_auto_scan_batch(context) {
        Ok(batch) => batch,
        Err(error) => {
            scan.push_global_error(GlobalErrorLabel::OpenBatch, &error);
            return scan.into_output();
        }
    };

    let outcome = scan_install_paths_in_parallel(&batch, &installs);
    scan.merge(outcome);
    refresh_confirmed_manual_roots(context, &mut scan);

    scan.into_output()
}

fn refresh_confirmed_manual_roots(context: &crate::Context, scan: &mut AutoScanAccumulator) {
    let mut games = match context.storage().list_games() {
        Ok(games) => games
            .into_iter()
            .filter(|game| {
                game.root_authority() == renderpilot_domain::RootAuthority::UserConfirmed
                    || (game.root_authority() == renderpilot_domain::RootAuthority::Legacy
                        && game.identity().launcher() == renderpilot_domain::Launcher::Manual)
            })
            .collect::<Vec<_>>(),
        Err(error) => {
            scan.push_global_error(GlobalErrorLabel::OpenBatch, &error.into());
            return;
        }
    };
    games.sort_by_key(|game| game.install_path().as_str().matches('/').count());

    for game in games {
        let root = PathBuf::from(game.install_path().as_str());
        if !root.is_dir() {
            scan.errors.push(ScanErrorOutput {
                root: normalized_path_string(&root),
                message: "confirmed manual installation is currently unavailable".to_owned(),
            });
            continue;
        }
        let outcome = super::scan_explicit_install(
            context,
            root.clone(),
            game.id().clone(),
            game.root_authority(),
            game.confirmed_executable()
                .map(|relative| root.join(relative.as_str())),
            super::ExplicitRootChange::Unchanged,
            &[],
        )
        .map(|result| vec![result]);
        scan.record_scan_outcome(&root, outcome);
    }
}

#[derive(Debug)]
struct AutoScanInstall {
    install: renderpilot_platform_windows::game_libraries::DiscoveredInstall,
    allow_checkpoint_skip: bool,
}

fn discover_normalized_auto_scan_inputs(
    use_checkpoints: bool,
) -> (Vec<String>, Vec<String>, Vec<String>, Vec<AutoScanInstall>) {
    use renderpilot_platform_windows::game_libraries::DiscoveredGameSources;

    let DiscoveredGameSources {
        installs,
        library_roots,
        authoritative_library_roots,
    } = renderpilot_platform_windows::game_libraries::discover_game_sources();

    let library_root_keys = library_roots
        .iter()
        .map(normalized_path_string)
        .collect::<Vec<_>>();
    let install_path_keys = installs
        .iter()
        .map(|install| normalized_path_string(&install.install_path))
        .collect::<Vec<_>>();
    let authoritative_root_keys = authoritative_library_roots
        .iter()
        .map(normalized_path_string)
        .collect::<Vec<_>>();
    let installs = installs
        .into_iter()
        .map(|install| AutoScanInstall {
            allow_checkpoint_skip: use_checkpoints,
            install,
        })
        .collect();

    (
        library_root_keys,
        authoritative_root_keys,
        install_path_keys,
        installs,
    )
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
    batch: &AutoScanBatch<'_>,
    installs: &[AutoScanInstall],
) -> AutoScanAccumulator {
    if installs.is_empty() {
        return AutoScanAccumulator::default();
    }

    let worker_count = effective_worker_count(installs.len());
    let next_index = AtomicUsize::new(0);
    let accumulator = Mutex::new(AutoScanAccumulator::default());
    let installs: &[AutoScanInstall] = installs;

    thread::scope(|scope| {
        for _ in 0..worker_count {
            scope.spawn(|| {
                loop {
                    let index = next_index.fetch_add(1, Ordering::Relaxed);
                    let Some(install) = installs.get(index) else {
                        break;
                    };

                    let outcome =
                        scan_auto_in_batch(batch, &install.install, install.allow_checkpoint_skip);

                    // Held only for the few microseconds it takes to record
                    // the per-scan outcome. The heavy work (file walk,
                    // SHA-256, DB writes inside the SQLite mutex) happens
                    // outside the lock.
                    let mut guard = accumulator
                        .lock()
                        // Intentionally recovers from a poisoned mutex so that
                        // one panicking worker does not silently discard results
                        // from all other workers. The panic itself is not logged
                        // here; for a best-effort bulk scan we prefer partial
                        // results over total failure.
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    guard.record_scan_outcome(&install.install.install_path, outcome);
                }
            });
        }
    });

    accumulator
        .into_inner()
        // Same as the inner lock: we prefer partial results over losing
        // everything because one worker panicked.
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn effective_worker_count(install_path_count: usize) -> usize {
    let cpu_workers = thread::available_parallelism()
        .map(|nz| nz.get())
        .unwrap_or(1);

    effective_worker_count_with_cpu(install_path_count, cpu_workers)
}

fn effective_worker_count_with_cpu(install_path_count: usize, cpu_workers: usize) -> usize {
    install_path_count
        .min(AUTO_SCAN_MAX_WORKERS)
        .min(cpu_workers)
        .max(1)
}

#[derive(Debug, Default)]
struct AutoScanAccumulator {
    delta: crate::catalog::CatalogScanDelta,
    errors: Vec<ScanErrorOutput>,
}

impl AutoScanAccumulator {
    fn record_scan_outcome(
        &mut self,
        root: &Path,
        outcome: Result<Vec<crate::catalog::ScanFolderCatalogResult>, ServiceError>,
    ) {
        match outcome {
            Ok(results) => {
                for result in results {
                    match result.change {
                        crate::catalog::CatalogScanChange::Unchanged => {}
                        crate::catalog::CatalogScanChange::Added => {
                            self.delta.added_game_ids.push(result.game.id().clone());
                        }
                        crate::catalog::CatalogScanChange::Updated => {
                            self.delta.updated_game_ids.push(result.game.id().clone());
                        }
                    }
                }
            }
            Err(error) => self.push_error(root, &error),
        }
    }

    fn merge(&mut self, other: AutoScanAccumulator) {
        self.delta.added_game_ids.extend(other.delta.added_game_ids);
        self.delta
            .updated_game_ids
            .extend(other.delta.updated_game_ids);
        self.delta
            .removed_game_ids
            .extend(other.delta.removed_game_ids);
        self.errors.extend(other.errors);
    }

    fn push_error(&mut self, root: &Path, error: &ServiceError) {
        self.errors.push(ScanErrorOutput::new(root, error));
    }

    /// Records a single global failure (catalog-wide pruning, batch open,
    /// etc.) under an explicit synthetic label. Used for errors that are
    /// not tied to a specific install root, so the JSON payload always
    /// carries a meaningful `root` value instead of an empty string.
    fn push_global_error(&mut self, label: GlobalErrorLabel, error: &ServiceError) {
        self.errors.push(ScanErrorOutput::with_label(label, error));
    }

    fn into_output(mut self) -> AutoScanDiscoveryResult {
        sort_and_dedup_game_ids(&mut self.delta.added_game_ids);
        sort_and_dedup_game_ids(&mut self.delta.updated_game_ids);
        sort_and_dedup_game_ids(&mut self.delta.removed_game_ids);
        self.errors
            .sort_by(|left, right| (&left.root, &left.message).cmp(&(&right.root, &right.message)));
        AutoScanDiscoveryResult {
            delta: self.delta,
            errors: self.errors,
        }
    }
}

fn sort_and_dedup_game_ids(game_ids: &mut Vec<GameId>) {
    game_ids.sort();
    game_ids.dedup();
}

/// Result of auto-discovery.
pub struct AutoScanDiscoveryResult {
    /// Stable added/updated/removed identity delta.
    pub delta: crate::catalog::CatalogScanDelta,
    /// Errors encountered.
    pub errors: Vec<ScanErrorOutput>,
}

#[derive(Debug)]
/// An error encountered during auto-scan.
pub struct ScanErrorOutput {
    /// The root path of the error.
    pub root: String,
    /// The error message.
    pub message: String,
}

impl ScanErrorOutput {
    fn new(root: &Path, error: &ServiceError) -> Self {
        Self {
            root: normalized_path_string(root),
            message: error.to_string(),
        }
    }

    fn with_label(label: GlobalErrorLabel, error: &ServiceError) -> Self {
        Self {
            root: label.to_string(),
            message: error.to_string(),
        }
    }
}

fn normalized_path_string(path: impl AsRef<Path>) -> String {
    path.as_ref().to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::{
        AUTO_SCAN_MAX_WORKERS, AutoScanAccumulator, ScanErrorOutput,
        effective_worker_count_with_cpu,
    };
    use renderpilot_domain::GameId;

    #[test]
    fn worker_count_is_bounded_by_install_count_cpu_and_cap() {
        assert_eq!(effective_worker_count_with_cpu(0, 16), 1);
        assert_eq!(effective_worker_count_with_cpu(1, 16), 1);
        assert_eq!(effective_worker_count_with_cpu(2, 1), 1);
        assert_eq!(effective_worker_count_with_cpu(2, 8), 2);
        assert_eq!(
            effective_worker_count_with_cpu(16, 16),
            AUTO_SCAN_MAX_WORKERS
        );
    }

    /// Compile-time guard: parallel auto-scan shares [`AutoScanBatch`] across
    /// `std::thread::scope` workers; these types must remain `Send + Sync`.
    #[test]
    fn assert_auto_scan_shared_state_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<crate::Context>();
        assert_send_sync::<renderpilot_detection::FileHashCache>();
        assert_send_sync::<renderpilot_detection::LibraryPatternComponentDetector>();
        assert_send_sync::<crate::catalog::auto_scan::AutoScanBatch<'_>>();
    }

    #[test]
    fn accumulated_delta_and_partial_errors_have_deterministic_order() {
        let game_a = GameId::new("manual:a").expect("game id");
        let game_b = GameId::new("manual:b").expect("game id");
        let output = AutoScanAccumulator {
            delta: crate::catalog::CatalogScanDelta {
                added_game_ids: vec![game_b.clone(), game_a.clone(), game_b],
                updated_game_ids: vec![game_a.clone(), game_a],
                removed_game_ids: Vec::new(),
            },
            errors: vec![
                ScanErrorOutput {
                    root: "z".to_owned(),
                    message: "second".to_owned(),
                },
                ScanErrorOutput {
                    root: "a".to_owned(),
                    message: "first".to_owned(),
                },
            ],
        }
        .into_output();

        assert_eq!(
            output
                .delta
                .added_game_ids
                .iter()
                .map(GameId::as_str)
                .collect::<Vec<_>>(),
            vec!["manual:a", "manual:b"]
        );
        assert_eq!(output.delta.updated_game_ids.len(), 1);
        assert_eq!(output.errors[0].root, "a");
        assert_eq!(output.errors[1].root, "z");
    }
}
