use std::collections::HashSet;

use renderpilot_storage_sqlite::SqliteStorage;

use crate::ServiceError;

use super::paths;

/// Removes catalog rows that became orphans after auto-scan classification.
///
/// A row is treated as an orphan when its install path matches one of:
///
/// 1. **A launcher library root itself.** Earlier auto-scan revisions
///    persisted launcher container folders (`C:/Program Files (x86)/Steam/
///    steamapps/common`, `C:/Program Files/EA Games`, ...) as a single
///    catalog entry when the root produced zero or one library detections.
/// 2. **A direct child of a launcher library root that the current scan did
///    not retain.** This catches Steam runtime / SDK sub-folders such as
///    `Steam Controller Configs`, `Steamworks Common Redistributables`, or
///    `Steamworks Shared`, plus any previously-split orphan child that is
///    no longer recognized as a real game install.
///
/// Pruning is intentionally **launcher-agnostic**. `ManualFolderGameSource`
/// upgrades any folder under a Steam library to `Launcher::Steam` when an
/// `appmanifest_*.acf` is present, so Steamworks-style orphans land in the
/// catalog with `Launcher::Steam` rather than `Launcher::Manual`. Filtering
/// by launcher would leave those rows behind, which is the bug this prune
/// pass exists to fix; the safety net is `retained_install_paths`, which
/// shields every install path the current scan rediscovered.
///
/// Rows that lie deeper than a direct child of a library root (e.g.
/// `.../common/RealGame/Plugins/MyMod`) are preserved on purpose: those
/// belong to a scanned game and will be handled by the per-scan
/// `prune_stale_manual_games_under_scope` step.
///
/// All inputs are expected as PathRef-style normalized strings (forward
/// slashes). Comparison is case-insensitive (ASCII) and ignores trailing
/// separators. Returns the exact game ids removed.
pub fn prune_auto_scan_orphans(
    storage: &SqliteStorage,
    library_roots: &[String],
    authoritative_library_roots: &[String],
    retained_install_paths: &[String],
) -> Result<Vec<renderpilot_domain::GameId>, ServiceError> {
    if library_roots.is_empty() {
        return Ok(Vec::new());
    }

    let library_root_keys: HashSet<String> = library_roots
        .iter()
        .map(|root| paths::install_path_match_key(root))
        .collect();
    let retained_install_keys: HashSet<String> = retained_install_paths
        .iter()
        .map(|path| paths::install_path_match_key(path))
        .collect();
    let authoritative_root_keys: HashSet<String> = authoritative_library_roots
        .iter()
        .map(|root| paths::install_path_match_key(root))
        .collect();

    let games = storage.list_games().map_err(ServiceError::from)?;
    let mut stale_ids = Vec::new();

    for game in games {
        let install_key = paths::install_path_match_key(game.install_path().as_str());

        if is_auto_scan_orphan(
            &install_key,
            &library_root_keys,
            &authoritative_root_keys,
            &retained_install_keys,
        ) {
            stale_ids.push(game.id().clone());
        }
    }

    super::delete_games(storage, &stale_ids)?;

    Ok(stale_ids)
}

fn is_auto_scan_orphan(
    install_key: &str,
    library_root_keys: &HashSet<String>,
    authoritative_root_keys: &HashSet<String>,
    retained_install_keys: &HashSet<String>,
) -> bool {
    if library_root_keys.contains(install_key) {
        return true;
    }

    let Some(parent_key) = parent_install_path_key(install_key) else {
        return false;
    };

    if !authoritative_root_keys.contains(parent_key) {
        return false;
    }

    !retained_install_keys.contains(install_key)
}

/// Returns the parent component of a normalized install-path key, or `None`
/// when the key has no `/` separator (drive-relative roots like `c:` are
/// treated as having no parent).
///
/// Borrows from `install_key`; `HashSet<String>::contains::<str>` accepts
/// the slice directly via `Borrow<str>`, so no allocation is needed for
/// the lookup.
fn parent_install_path_key(install_key: &str) -> Option<&str> {
    let last_separator = install_key.rfind('/')?;
    let parent = &install_key[..last_separator];

    if parent.is_empty() {
        return None;
    }

    Some(parent)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::is_auto_scan_orphan;

    #[test]
    fn missing_children_are_pruned_only_after_authoritative_enumeration() {
        let library_roots = HashSet::from([String::from("c:/games")]);
        let retained = HashSet::new();

        assert!(!is_auto_scan_orphan(
            "c:/games/installed",
            &library_roots,
            &HashSet::new(),
            &retained,
        ));
        assert!(is_auto_scan_orphan(
            "c:/games/installed",
            &library_roots,
            &library_roots,
            &retained,
        ));
        assert!(is_auto_scan_orphan(
            "c:/games",
            &library_roots,
            &HashSet::new(),
            &retained,
        ));
    }
}
