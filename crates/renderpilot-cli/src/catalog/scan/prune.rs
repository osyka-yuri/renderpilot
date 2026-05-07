use std::collections::HashSet;

use renderpilot_domain::{GameId, Launcher};
use renderpilot_storage_sqlite::SqliteStorage;

use crate::{catalog::ScanFolderCatalogResult, error::CliError};

use super::paths;

/// Game IDs that were produced by the current scan.
///
/// On a successful scan this set should normally be non-empty because even an empty
/// library scan still corresponds to a selected manual game installation.
pub(super) fn game_ids_from_scan_results(results: &[ScanFolderCatalogResult]) -> HashSet<GameId> {
    results
        .iter()
        .map(|result| result.game.id().clone())
        .collect()
}

/// Removes stale manual-folder catalog rows under `scope_root`.
///
/// Safety rules:
///
/// - Only [`Launcher::Manual`] games are considered.
/// - Only games whose install path lies inside `scope_root` are considered.
/// - Games refreshed by the current scan are retained.
/// - If `retained_ids` is empty, pruning is skipped entirely.
///
/// The empty `retained_ids` guard is intentional. It prevents a failed, partial,
/// or incorrectly wired scan from being interpreted as "delete every manual game
/// under this folder".
pub(super) fn prune_stale_manual_games_under_scope(
    storage: &SqliteStorage,
    scope_root: &str,
    retained_ids: &HashSet<GameId>,
) -> Result<(), CliError> {
    if should_skip_prune(retained_ids) {
        return Ok(());
    }

    let stale_ids = collect_stale_manual_game_ids(storage, scope_root, retained_ids)?;

    delete_games(storage, stale_ids)
}

fn should_skip_prune(retained_ids: &HashSet<GameId>) -> bool {
    retained_ids.is_empty()
}

fn collect_stale_manual_game_ids(
    storage: &SqliteStorage,
    scope_root: &str,
    retained_ids: &HashSet<GameId>,
) -> Result<Vec<GameId>, CliError> {
    let games = storage.list_games().map_err(CliError::from)?;
    let mut stale_ids = Vec::new();

    for game in games {
        let game_id = game.id();

        if retained_ids.contains(game_id) {
            continue;
        }

        if game.identity().launcher() != Launcher::Manual {
            continue;
        }

        if !paths::normalized_path_within_scope(game.install_path().as_str(), scope_root) {
            continue;
        }

        stale_ids.push(game_id.clone());
    }

    Ok(stale_ids)
}

fn delete_games(storage: &SqliteStorage, game_ids: Vec<GameId>) -> Result<(), CliError> {
    for game_id in game_ids {
        storage.delete_game(&game_id)?;
    }

    Ok(())
}
