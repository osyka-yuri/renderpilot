use std::collections::HashSet;

use renderpilot_application::{
    InstalledAddonRepository, OptiScalerStateRepository, ProxyTopologyRepository,
};
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
///
/// Only rows previously established by an authoritative launcher provider are
/// eligible. `UserConfirmed` and `Legacy` roots are never deleted by launcher
/// discovery: legacy false children are handled by evidence-based
/// consolidation during a full parent scan. Direct children omitted by a
/// provider are retained here because omission does not prove that an
/// installation disappeared; confirmed absence is handled by the lifecycle
/// coordinator.
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
    let games = context.storage().list_games().map_err(ServiceError::from)?;
    let mut stale_ids = Vec::new();

    for game in games {
        if game.root_authority() == RootAuthority::LauncherManifest
            && !game_has_auto_prune_protected_state(context, game.id())?
            && is_auto_scan_orphan(
                game.install_key(),
                &library_root_keys,
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
        crate::mutation_boundary::enter_game_observation_boundaries(stale_ids.iter());

    // State may have changed while locks were being acquired. Re-read and
    // repeat every eligibility check before entering the delete transaction.
    let candidates = stale_ids.into_iter().collect::<HashSet<GameId>>();
    let mut stale_ids = Vec::new();
    for game in context.storage().list_games()? {
        if candidates.contains(game.id())
            && game.root_authority() == RootAuthority::LauncherManifest
            && !game_has_auto_prune_protected_state(context, game.id())?
            && is_auto_scan_orphan(
                game.install_key(),
                &library_root_keys,
                &retained_install_keys,
            )
        {
            stale_ids.push(game.id().clone());
        }
    }
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

fn game_has_auto_prune_protected_state(
    context: &crate::Context,
    game_id: &GameId,
) -> Result<bool, ServiceError> {
    if context.storage().is_installation_absent(game_id)?
        || context
            .storage()
            .game_has_nvapi_durable_state(game_id.as_str())?
        || !context
            .storage()
            .pending_file_mutations_for_game(game_id)?
            .is_empty()
        || context.storage().get_installed_addon(game_id)?.is_some()
        || context
            .storage()
            .get_optiscaler_install_state(game_id)?
            .is_some()
        || context.storage().get_proxy_topology(game_id)?.is_some()
        || context.storage().has_component_backups_for_game(game_id)?
        || context
            .storage()
            .engine_config_journal_owner(game_id)?
            .is_some()
    {
        return Ok(true);
    }
    if context
        .storage()
        .get_nvapi_executable_override(game_id.as_str())?
        .is_some()
    {
        return Ok(true);
    }
    Ok(context
        .storage()
        .pending_shared_vulkan_mutation()?
        .is_some_and(|row| row.game_id.as_ref() == Some(game_id)))
}

fn is_auto_scan_orphan(
    install_key: &InstallKey,
    library_root_keys: &HashSet<InstallKey>,
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

    false
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use renderpilot_application::{GameRepository, InstalledAddonRepository};
    use renderpilot_domain::{
        AddonKind, GameId, GameIdentity, GameInstallation, GameRuntime, InstallKey, InstalledAddon,
        Launcher, PathRef, Platform, RootAuthority,
    };
    use renderpilot_storage_sqlite::BeginFileMutationPreparation;

    use super::{
        game_has_auto_prune_protected_state, is_auto_scan_orphan, prune_auto_scan_orphans,
    };

    #[test]
    fn provider_omission_never_prunes_a_direct_child_by_path_alone() {
        let key = |path: &str| InstallKey::from_path(&PathRef::new(path).expect("path"));
        let library_roots = HashSet::from([key("c:/games")]);
        let retained = HashSet::new();

        // Eligibility is independent of provider enumeration and filesystem
        // existence. A present child and a missing child both require the
        // separate presence/lifecycle path before catalog removal.
        for direct_child in ["c:/games/installed", "c:/games/missing"] {
            assert!(!is_auto_scan_orphan(
                &key(direct_child),
                &library_roots,
                &retained,
            ));
        }
        assert!(is_auto_scan_orphan(
            &key("c:/games"),
            &library_roots,
            &retained,
        ));
        assert!(!is_auto_scan_orphan(
            &key("c:/games"),
            &library_roots,
            &HashSet::from([key("c:/games")]),
        ));
    }

    #[test]
    fn provider_omission_preserves_present_launcher_child() -> Result<(), crate::ServiceError> {
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
        let removed = prune_auto_scan_orphans(&context, std::slice::from_ref(&library_text), &[])
            .expect("prune");

        assert!(removed.is_empty());
        assert!(context.storage().find_game(manual.id())?.is_some());
        assert!(context.storage().find_game(launcher.id())?.is_some());
        Ok(())
    }

    #[test]
    fn omitted_exact_launcher_library_root_is_still_pruned() -> Result<(), crate::ServiceError> {
        let temp = tempfile::tempdir().expect("temp");
        let library = temp.path().join("Epic Games");
        std::fs::create_dir_all(&library).expect("library root");
        let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        let launcher = game(
            "game:omitted-container",
            &library,
            RootAuthority::LauncherManifest,
        );
        context.storage().upsert_game(&launcher)?;

        let library_text = normalized(&library);
        let removed = prune_auto_scan_orphans(&context, std::slice::from_ref(&library_text), &[])
            .expect("prune omitted container");

        assert_eq!(removed, vec![launcher.id().clone()]);
        assert!(context.storage().find_game(launcher.id())?.is_none());
        Ok(())
    }

    #[test]
    fn omitted_library_root_with_unresolved_file_work_is_retained_without_recovery()
    -> Result<(), crate::ServiceError> {
        let temp = tempfile::tempdir().expect("temp");
        let library = temp.path().join("SteamLibrary");
        std::fs::create_dir_all(&library).expect("library root");
        let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        let game = game(
            "game:pending-container",
            &library,
            RootAuthority::LauncherManifest,
        );
        context.storage().upsert_game(&game)?;
        let unfinished_file = library.join("pending.dll");
        std::fs::write(&unfinished_file, b"current bytes").expect("seed file");
        let game_id = game.id().clone();
        context
            .storage()
            .begin_file_mutation_preparation(&BeginFileMutationPreparation {
                id: "orphan-prune-pending".to_owned(),
                game_id: game_id.clone(),
                feature: "test:pending-prune".to_owned(),
                subject_id: None,
                initial_manifest_json: "{}".to_owned(),
            })?;

        let removed = prune_auto_scan_orphans(&context, &[normalized(&library)], &[])?;

        assert!(removed.is_empty());
        assert!(context.storage().find_game(&game_id)?.is_some());
        assert_eq!(
            std::fs::read(&unfinished_file).expect("file unchanged"),
            b"current bytes"
        );
        assert_eq!(
            context
                .storage()
                .pending_file_mutations_for_game(&game_id)?
                .len(),
            1,
            "prune must neither recover nor erase the unresolved row"
        );
        Ok(())
    }

    #[test]
    fn retained_launcher_install_at_library_root_keeps_stable_card()
    -> Result<(), crate::ServiceError> {
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

    #[test]
    fn automatic_container_pruning_preserves_games_with_nvapi_state()
    -> Result<(), crate::ServiceError> {
        let temp = tempfile::tempdir().expect("temp");
        let library = temp.path().join("common");
        let game_root = library.clone();
        std::fs::create_dir_all(&game_root).expect("game root");
        let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        let launcher = game(
            "game:nvapi-protected",
            &game_root,
            RootAuthority::LauncherManifest,
        );
        context.storage().upsert_game(&launcher)?;
        context.storage().upsert_nvapi_executable_override(
            launcher.id().as_str(),
            "C:/Games/PendingNvidiaGame/Game.exe",
            "Game.exe",
        )?;
        assert!(
            game_has_auto_prune_protected_state(&context, launcher.id())
                .expect("query auto-prune NVAPI state")
        );
        assert!(
            !context
                .storage()
                .game_has_nvapi_durable_state(launcher.id().as_str())?,
            "an executable selection is auto-prune protection, not DRS mutation state",
        );

        let library_text = normalized(&library);
        let removed = prune_auto_scan_orphans(&context, std::slice::from_ref(&library_text), &[])
            .expect("prune");

        assert!(removed.is_empty());
        assert!(context.storage().find_game(launcher.id())?.is_some());
        assert!(
            context
                .storage()
                .get_nvapi_executable_override(launcher.id().as_str())?
                .is_some()
        );
        Ok(())
    }

    #[test]
    fn automatic_container_pruning_preserves_installed_addon_ownership()
    -> Result<(), crate::ServiceError> {
        let temp = tempfile::tempdir().expect("temp");
        let library = temp.path().join("Epic Games");
        std::fs::create_dir_all(&library).expect("library root");
        let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        let launcher = game(
            "game:addon-protected-container",
            &library,
            RootAuthority::LauncherManifest,
        );
        context.storage().upsert_game(&launcher)?;
        let addon_file = PathRef::new(format!("{}/renodx-game.addon64", normalized(&library)))
            .expect("add-on path");
        context
            .storage()
            .upsert_installed_addon(&InstalledAddon::new(
                launcher.id().clone(),
                AddonKind::RenoDx,
                addon_file,
            ))?;

        let removed = prune_auto_scan_orphans(&context, &[normalized(&library)], &[])?;

        assert!(removed.is_empty());
        assert!(context.storage().find_game(launcher.id())?.is_some());
        assert!(
            context
                .storage()
                .get_installed_addon(launcher.id())?
                .is_some()
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
