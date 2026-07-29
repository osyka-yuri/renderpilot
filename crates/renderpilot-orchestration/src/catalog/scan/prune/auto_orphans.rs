use std::collections::HashSet;

use renderpilot_domain::{GameId, InstallKey, RootAuthority};

use crate::ServiceError;

use crate::catalog::install_paths as paths;

/// Removes catalog rows that became orphans after auto-scan classification.
///
/// A row is treated as an orphan when its install path matches one of:
///
/// 1. **A launcher library root itself that the current provider did not
///    retain as an installation.** Earlier auto-scan revisions
///    persisted launcher container folders (`C:/Program Files (x86)/Steam/
///    steamapps/common`, `C:/Program Files/EA Games`, ...) as a single
///    catalog entry when the root produced zero or one library detections.
/// 2. **A direct child of a launcher library root that the current scan did
///    not retain.** This catches Steam runtime / SDK sub-folders such as
///    `Steam Controller Configs`, `Steamworks Common Redistributables`, or
///    `Steamworks Shared`, plus any previously-split orphan child that is
///    no longer recognized as a real game install.
///
/// Only rows previously established by an authoritative launcher provider are
/// eligible. `UserConfirmed` and `Legacy` roots are never deleted by launcher
/// discovery: legacy false children are handled by evidence-based
/// consolidation during a full parent scan.
///
/// Rows that lie deeper than a direct child of a library root (e.g.
/// `.../common/RealGame/Plugins/MyMod`) are preserved on purpose: those
/// belong to a scanned game and will be handled by the per-scan
/// `prune_stale_manual_games_under_scope` step.
///
/// All inputs are expected as PathRef-style normalized strings (forward
/// slashes). Comparison is case-insensitive (ASCII) and ignores trailing
/// separators. Returns the exact game ids removed.
pub(crate) fn prune_auto_scan_orphans(
    context: &crate::Context,
    library_roots: &[String],
    authoritative_library_roots: &[String],
    retained_install_paths: &[String],
) -> Result<Vec<renderpilot_domain::GameId>, ServiceError> {
    if library_roots.is_empty() {
        return Ok(Vec::new());
    }

    let library_root_keys: HashSet<InstallKey> = library_roots
        .iter()
        .filter_map(|root| paths::install_path_match_key(root))
        .collect();
    let retained_install_keys: HashSet<InstallKey> = retained_install_paths
        .iter()
        .filter_map(|path| paths::install_path_match_key(path))
        .collect();
    let authoritative_root_keys: HashSet<InstallKey> = authoritative_library_roots
        .iter()
        .filter_map(|root| paths::install_path_match_key(root))
        .collect();

    let games = context.storage().list_games().map_err(ServiceError::from)?;
    let mut stale_ids = Vec::new();

    for game in games {
        if game.root_authority() == RootAuthority::LauncherManifest
            && is_auto_scan_orphan(
                game.install_key(),
                &library_root_keys,
                &authoritative_root_keys,
                &retained_install_keys,
            )
        {
            stale_ids.push(game.id().clone());
        }
    }

    stale_ids.sort();
    stale_ids.dedup();
    if stale_ids.is_empty() {
        return Ok(stale_ids);
    }

    let _game_guards =
        crate::game_mutation_lock::enter_game_mutation_boundaries(context, stale_ids.clone())?;

    // State may have changed while locks were being acquired. Re-read and
    // repeat every eligibility check before entering the delete transaction.
    let candidates = stale_ids.into_iter().collect::<HashSet<GameId>>();
    let mut stale_ids = context
        .storage()
        .list_games()?
        .into_iter()
        .filter(|game| candidates.contains(game.id()))
        .filter(|game| game.root_authority() == RootAuthority::LauncherManifest)
        .filter(|game| {
            is_auto_scan_orphan(
                game.install_key(),
                &library_root_keys,
                &authoritative_root_keys,
                &retained_install_keys,
            )
        })
        .map(|game| game.id().clone())
        .collect::<Vec<_>>();
    stale_ids.sort();

    let deleted = context.storage().delete_games(&stale_ids)?;
    if let Some(catalog_path) = context.storage().catalog_file_path()? {
        for deleted in deleted {
            crate::covers::unlink_cover_file_best_effort(
                &catalog_path,
                deleted.old_cover_file_name.as_deref(),
            );
        }
    }

    Ok(stale_ids)
}

fn is_auto_scan_orphan(
    install_key: &InstallKey,
    library_root_keys: &HashSet<InstallKey>,
    authoritative_root_keys: &HashSet<InstallKey>,
    retained_install_keys: &HashSet<InstallKey>,
) -> bool {
    // Provider evidence always wins over cleanup scope. Although launcher
    // libraries normally contain one directory per game, registry records and
    // manifests may legitimately point at the container path itself. Deleting
    // that retained row here would discard its stable GameId and scoped state
    // immediately before the same install is scanned again.
    if retained_install_keys.contains(install_key) {
        return false;
    }

    if library_root_keys.contains(install_key) {
        return true;
    }

    let Some(parent_key) = parent_install_path_key(install_key.as_str()) else {
        return false;
    };

    authoritative_root_keys
        .iter()
        .any(|key| key.as_str() == parent_key)
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

    use renderpilot_application::{AppResult, GameRepository};
    use renderpilot_domain::{
        GameId, GameIdentity, GameInstallation, GameRuntime, InstallKey, Launcher, PathRef,
        Platform, RootAuthority,
    };

    use super::{is_auto_scan_orphan, prune_auto_scan_orphans};

    #[test]
    fn missing_children_are_pruned_only_after_authoritative_enumeration() {
        let key = |path: &str| InstallKey::from_path(&PathRef::new(path).expect("path"));
        let library_roots = HashSet::from([key("c:/games")]);
        let retained = HashSet::new();

        assert!(!is_auto_scan_orphan(
            &key("c:/games/installed"),
            &library_roots,
            &HashSet::new(),
            &retained,
        ));
        assert!(is_auto_scan_orphan(
            &key("c:/games/installed"),
            &library_roots,
            &library_roots,
            &retained,
        ));
        assert!(is_auto_scan_orphan(
            &key("c:/games"),
            &library_roots,
            &HashSet::new(),
            &retained,
        ));
        assert!(!is_auto_scan_orphan(
            &key("c:/games"),
            &library_roots,
            &HashSet::new(),
            &HashSet::from([key("c:/games")]),
        ));
    }

    #[test]
    fn pruning_removes_only_launcher_owned_rows() -> AppResult<()> {
        let temp = tempfile::tempdir().expect("temp");
        let library = temp.path().join("common");
        let manual_root = library.join("ManualGame");
        let launcher_root = library.join("RemovedLauncherGame");
        std::fs::create_dir_all(&manual_root).expect("manual root");
        std::fs::create_dir_all(&launcher_root).expect("launcher root");
        let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        let manual = game("game:manual", &manual_root, RootAuthority::UserConfirmed);
        let launcher = game(
            "game:launcher",
            &launcher_root,
            RootAuthority::LauncherManifest,
        );
        context.storage().upsert_game(&manual)?;
        context.storage().upsert_game(&launcher)?;

        let library_text = normalized(&library);
        let removed = prune_auto_scan_orphans(
            &context,
            std::slice::from_ref(&library_text),
            std::slice::from_ref(&library_text),
            &[],
        )
        .expect("prune");

        assert_eq!(removed, vec![launcher.id().clone()]);
        assert!(context.storage().find_game(manual.id())?.is_some());
        assert!(context.storage().find_game(launcher.id())?.is_none());
        Ok(())
    }

    #[test]
    fn retained_launcher_install_at_library_root_keeps_stable_card() -> AppResult<()> {
        let temp = tempfile::tempdir().expect("temp");
        let library = temp.path().join("Epic Games");
        std::fs::create_dir_all(&library).expect("library root");
        let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        let launcher = game(
            "game:stable-launcher-root",
            &library,
            RootAuthority::LauncherManifest,
        );
        context.storage().upsert_game(&launcher)?;

        let library_text = normalized(&library);
        let removed = prune_auto_scan_orphans(
            &context,
            std::slice::from_ref(&library_text),
            &[],
            std::slice::from_ref(&library_text),
        )
        .expect("prune");

        assert!(removed.is_empty());
        assert_eq!(
            context.storage().find_game(launcher.id())?,
            Some(launcher),
            "provider-retained installs must keep their GameId and scoped state",
        );
        Ok(())
    }

    fn game(id: &str, root: &std::path::Path, authority: RootAuthority) -> GameInstallation {
        GameInstallation::new(
            GameIdentity::new(GameId::new(id).expect("id"), "Game", Launcher::Manual)
                .expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(normalized(root)).expect("path"),
        )
        .with_root_authority(authority)
    }

    fn normalized(path: &std::path::Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }
}
