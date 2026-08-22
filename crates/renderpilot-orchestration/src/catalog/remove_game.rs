//! Safe removal of a user-managed game card from the catalog.

use renderpilot_application::{GameRepository, InstalledAddonRepository};
use renderpilot_domain::{GameId, RootAuthority};

use crate::ServiceError;
use crate::addons::renodx::use_cases::commands::uninstall as renodx_uninstall;

/// Result of removing one user-managed game from the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveGameFromCatalogResult {
    /// Stable identity that was removed.
    pub game_id: String,
}

/// Removes one user-managed card after executing its complete cleanup plan.
///
/// The catalog and game/shared guards cover the complete compound operation.
/// Component rollback uses the same durable mutation path as an explicit
/// rollback and the card is deleted only after a fresh inventory proves that
/// no managed state remains. Successful durable steps are intentionally
/// retained on failure so retrying continues from the remaining inventory.
pub fn remove_game_from_catalog(
    context: &crate::Context,
    game_id: &GameId,
) -> Result<RemoveGameFromCatalogResult, ServiceError> {
    let _catalog_guard = context.catalog_scan_guard();
    let storage = context.storage();
    require_removable_game(context, game_id)?;
    loop {
        let game_guard = crate::mutation_boundary::enter_game_mutation_boundary(context, game_id)?;
        let addon = storage.get_installed_addon(game_id)?;
        let Some(addon) = addon else {
            return remove_game_with_game_guard(context, game_id, &game_guard);
        };
        let is_shared_vulkan =
            renodx_uninstall::registered_vulkan_exe_for_uninstall(context, game_id, &addon)
                .is_some();
        if !is_shared_vulkan {
            return remove_game_with_game_guard(context, game_id, &game_guard);
        }

        // Shared Vulkan cleanup must re-enter through game -> shared. The
        // initial game-only snapshot is only a routing hint; the record is
        // read again after both guards are held before any mutation begins.
        drop(game_guard);
        let guards =
            crate::mutation_boundary::enter_game_shared_mutation_boundary(context, game_id)?;
        let current = storage.get_installed_addon(game_id)?;
        let Some(current) = current else {
            drop(guards);
            continue;
        };
        let still_shared_vulkan =
            renodx_uninstall::registered_vulkan_exe_for_uninstall(context, game_id, &current)
                .is_some();
        if !still_shared_vulkan {
            drop(guards);
            continue;
        }
        renodx_uninstall::uninstall_shared_locked(context, &guards, game_id, &current)?;
        let game_guard = guards.into_game();
        return remove_game_with_game_guard(context, game_id, &game_guard);
    }
}

fn remove_game_with_game_guard(
    context: &crate::Context,
    game_id: &GameId,
    game_guard: &crate::game_mutation_lock::GameMutationGuard,
) -> Result<RemoveGameFromCatalogResult, ServiceError> {
    let cleanup =
        super::managed_state::ManagedCleanupPlan::build_locked(context, game_guard, game_id)?;
    cleanup.execute_locked(context, game_guard, game_id)?;
    let remaining = super::managed_state::inventory(context, game_id)?;
    if !remaining.is_empty() {
        return Err(ServiceError::GameRemovalCleanupFailed {
            game_id: game_id.as_str().to_owned(),
            action: "cleanup verification".to_owned(),
            reason: "managed state remains after all planned inverse actions".to_owned(),
        });
    }

    let storage = context.storage();
    let deleted = storage.delete_game(game_id)?;
    if let Some(catalog_path) = storage.catalog_file_path()? {
        crate::covers::unlink_cover_file_best_effort(
            &catalog_path,
            deleted.old_cover_file_name.as_deref(),
        );
    }

    Ok(RemoveGameFromCatalogResult {
        game_id: game_id.as_str().to_owned(),
    })
}

fn require_removable_game(
    context: &crate::Context,
    game_id: &GameId,
) -> Result<renderpilot_domain::GameInstallation, ServiceError> {
    let game = context
        .storage()
        .find_game(game_id)?
        .ok_or_else(|| ServiceError::GameNotFound(game_id.as_str().to_owned()))?;
    if game.root_authority() == RootAuthority::LauncherManifest {
        return Err(ServiceError::invalid_input(
            "launcher-managed games cannot be removed from the catalog because launcher refresh would add them again",
        ));
    }
    Ok(game)
}

#[cfg(test)]
mod tests {
    use renderpilot_application::{ComponentRepository, GameRepository, InstalledAddonRepository};
    use renderpilot_domain::{
        AddonKind, ComponentFile, ComponentId, ComponentKind, ComponentRollbackBaseline,
        GameIdentity, GameInstallation, GameRuntime, InstalledAddon, Launcher, LibraryComponent,
        LibraryTechnology, PathRef, Platform, Swappability,
    };

    use super::*;

    #[test]
    fn removes_user_managed_card_without_touching_the_install_directory() {
        let temp = tempfile::tempdir().expect("temp");
        let install = temp.path().join("Manual Game");
        std::fs::create_dir_all(&install).expect("install");
        let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        let game = game(&install, RootAuthority::UserConfirmed);
        context.storage().upsert_game(&game).expect("seed");

        let result = remove_game_from_catalog(&context, game.id()).expect("remove");

        assert_eq!(result.game_id, game.id().as_str());
        assert!(
            install.is_dir(),
            "catalog removal must not delete game files"
        );
        assert!(
            context
                .storage()
                .find_game(game.id())
                .expect("read")
                .is_none()
        );
    }

    #[test]
    fn launcher_managed_card_is_not_presented_as_durable_removal() {
        let temp = tempfile::tempdir().expect("temp");
        let install = temp.path().join("Launcher Game");
        std::fs::create_dir_all(&install).expect("install");
        let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        let game = game(&install, RootAuthority::LauncherManifest);
        context.storage().upsert_game(&game).expect("seed");

        let error = remove_game_from_catalog(&context, game.id()).expect_err("must reject");

        assert!(error.to_string().contains("launcher-managed"));
        assert!(
            context
                .storage()
                .find_game(game.id())
                .expect("read")
                .is_some()
        );
    }

    #[test]
    fn active_component_replacement_is_rolled_back_before_removal() {
        let temp = tempfile::tempdir().expect("temp");
        let install = temp.path().join("Managed Game");
        std::fs::create_dir_all(&install).expect("install");
        let live = install.join("nvngx_dlss.dll");
        let backup = crate::fs::backup_path(&live).expect("backup path");
        std::fs::write(&live, b"replacement").expect("live");
        std::fs::write(&backup, b"original").expect("backup");
        let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        let game = game(&install, RootAuthority::UserConfirmed);
        context.storage().upsert_game(&game).expect("seed");
        let component_id = ComponentId::new("component:managed").expect("component");
        let component = LibraryComponent::new(
            component_id.clone(),
            game.id().clone(),
            ComponentKind::NativeLibrary,
            LibraryTechnology::DlssSuperResolution,
            Swappability::Swappable,
        )
        .with_file(
            ComponentFile::new(path_ref(&live))
                .with_sha256(renderpilot_detection::sha256_file(&live).expect("live hash")),
        );
        context
            .storage()
            .replace_components_for_game(game.id(), &[component])
            .expect("component");
        context
            .storage()
            .recover_component_rollback_baseline(
                game.id(),
                &component_id,
                &ComponentRollbackBaseline::new(vec![
                    ComponentFile::new(path_ref(&live)).with_sha256(
                        renderpilot_detection::sha256_file(&backup).expect("backup hash"),
                    ),
                ]),
            )
            .expect("baseline");

        let result = remove_game_from_catalog(&context, game.id()).expect("remove");

        assert_eq!(result.game_id, game.id().as_str());
        assert_eq!(
            std::fs::read(&live).expect("restored live file"),
            b"original"
        );
        assert!(
            !backup.exists(),
            "successful rollback must consume its sidecar"
        );
        assert!(
            context
                .storage()
                .find_game(game.id())
                .expect("read")
                .is_none()
        );
    }

    #[test]
    fn orphaned_component_baseline_is_rolled_back_from_recorded_provenance() {
        let temp = tempfile::tempdir().expect("temp");
        let install = temp.path().join("Managed Game");
        std::fs::create_dir_all(&install).expect("install");
        let live = install.join("nvngx_dlss.dll");
        let backup = crate::fs::backup_path(&live).expect("backup path");
        std::fs::write(&live, b"replacement").expect("live");
        std::fs::write(&backup, b"original").expect("backup");
        let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        let game = game(&install, RootAuthority::UserConfirmed);
        context.storage().upsert_game(&game).expect("seed");
        let component_id = ComponentId::new("component:orphaned").expect("component");
        let active_file = ComponentFile::new(path_ref(&live))
            .with_sha256(renderpilot_detection::sha256_file(&live).expect("live hash"));
        context
            .storage()
            .recover_component_rollback_baseline(
                game.id(),
                &component_id,
                &ComponentRollbackBaseline::new(vec![
                    ComponentFile::new(path_ref(&live)).with_sha256(
                        renderpilot_detection::sha256_file(&backup).expect("backup hash"),
                    ),
                ])
                .with_expected_active_files(vec![active_file]),
            )
            .expect("baseline");
        context
            .storage()
            .replace_components_for_game(game.id(), &[])
            .expect("component disappeared after rescan");

        remove_game_from_catalog(&context, game.id()).expect("remove");

        assert_eq!(
            std::fs::read(&live).expect("restored live file"),
            b"original"
        );
        assert!(!backup.exists(), "successful rollback consumes the sidecar");
        assert!(
            context
                .storage()
                .find_game(game.id())
                .expect("game lookup")
                .is_none()
        );
    }

    #[test]
    fn legacy_orphan_with_unproven_live_bytes_is_bundled_without_mutation() {
        let temp = tempfile::tempdir().expect("temp");
        let install = temp.path().join("Managed Game");
        std::fs::create_dir_all(&install).expect("install");
        let live = install.join("nvngx_dlss.dll");
        let backup = crate::fs::backup_path(&live).expect("backup path");
        std::fs::write(&live, b"unknown-active").expect("live");
        std::fs::write(&backup, b"original").expect("backup");
        let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        let game = game(&install, RootAuthority::UserConfirmed);
        context.storage().upsert_game(&game).expect("seed");
        let component_id = ComponentId::new("component:legacy-orphan").expect("component");
        context
            .storage()
            .recover_component_rollback_baseline(
                game.id(),
                &component_id,
                &ComponentRollbackBaseline::new(vec![
                    ComponentFile::new(path_ref(&live)).with_sha256(
                        renderpilot_detection::sha256_file(&backup).expect("backup hash"),
                    ),
                ]),
            )
            .expect("legacy baseline");

        let error = remove_game_from_catalog(&context, game.id())
            .expect_err("unproven live bytes must not be overwritten");
        let ServiceError::ManagedCleanupAmbiguous {
            recovery_bundle_path,
            ..
        } = error
        else {
            panic!("expected typed cleanup ambiguity");
        };

        assert!(std::path::Path::new(&recovery_bundle_path).is_dir());
        assert_eq!(std::fs::read(&live).expect("live"), b"unknown-active");
        assert_eq!(std::fs::read(&backup).expect("backup"), b"original");
        assert!(
            context
                .storage()
                .find_game(game.id())
                .expect("game lookup")
                .is_some()
        );
    }

    #[test]
    fn failed_automatic_rollback_keeps_the_card_and_recovery_state() {
        let temp = tempfile::tempdir().expect("temp");
        let install = temp.path().join("Managed Game");
        std::fs::create_dir_all(&install).expect("install");
        let live = install.join("nvngx_dlss.dll");
        std::fs::write(&live, b"replacement").expect("live");
        let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        let game = game(&install, RootAuthority::UserConfirmed);
        context.storage().upsert_game(&game).expect("seed");
        let component_id = ComponentId::new("component:managed").expect("component");
        let component = LibraryComponent::new(
            component_id.clone(),
            game.id().clone(),
            ComponentKind::NativeLibrary,
            LibraryTechnology::DlssSuperResolution,
            Swappability::Swappable,
        )
        .with_file(
            ComponentFile::new(path_ref(&live))
                .with_sha256(renderpilot_detection::sha256_file(&live).expect("live hash")),
        );
        context
            .storage()
            .replace_components_for_game(game.id(), &[component])
            .expect("component");
        context
            .storage()
            .recover_component_rollback_baseline(
                game.id(),
                &component_id,
                &ComponentRollbackBaseline::new(vec![
                    ComponentFile::new(path_ref(&live)).with_sha256(
                        renderpilot_detection::sha256_bytes(b"original").expect("original hash"),
                    ),
                ]),
            )
            .expect("baseline");

        let error = remove_game_from_catalog(&context, game.id()).expect_err("must preserve card");

        assert!(matches!(
            error,
            ServiceError::GameRemovalCleanupFailed { .. }
        ));
        assert_eq!(
            std::fs::read(&live).expect("unchanged live file"),
            b"replacement"
        );
        assert!(
            context
                .storage()
                .find_game(game.id())
                .expect("read")
                .is_some()
        );
        assert!(
            context
                .storage()
                .get_component_backup(&component_id)
                .expect("baseline lookup")
                .is_some(),
            "failed rollback must retain retryable recovery metadata"
        );
    }

    #[test]
    fn ambiguous_overlapping_component_history_is_bundled_without_mutation() {
        let temp = tempfile::tempdir().expect("temp");
        let install = temp.path().join("Managed Game");
        std::fs::create_dir_all(&install).expect("install");
        let live = install.join("nvngx_dlss.dll");
        let backup = crate::fs::backup_path(&live).expect("backup path");
        std::fs::write(&live, b"replacement").expect("live");
        std::fs::write(&backup, b"original").expect("backup");
        let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        let game = game(&install, RootAuthority::UserConfirmed);
        context.storage().upsert_game(&game).expect("seed");

        let component_ids = [
            ComponentId::new("component:overlap-a").expect("component"),
            ComponentId::new("component:overlap-b").expect("component"),
        ];
        let components = component_ids
            .iter()
            .map(|component_id| {
                LibraryComponent::new(
                    component_id.clone(),
                    game.id().clone(),
                    ComponentKind::NativeLibrary,
                    LibraryTechnology::DlssSuperResolution,
                    Swappability::Swappable,
                )
                .with_file(
                    ComponentFile::new(path_ref(&live))
                        .with_sha256(renderpilot_detection::sha256_file(&live).expect("live hash")),
                )
            })
            .collect::<Vec<_>>();
        context
            .storage()
            .replace_components_for_game(game.id(), &components)
            .expect("components");
        for component_id in &component_ids {
            context
                .storage()
                .recover_component_rollback_baseline(
                    game.id(),
                    component_id,
                    &ComponentRollbackBaseline::new(vec![
                        ComponentFile::new(path_ref(&live)).with_sha256(
                            renderpilot_detection::sha256_file(&backup).expect("backup hash"),
                        ),
                    ]),
                )
                .expect("baseline");
        }

        let error = remove_game_from_catalog(&context, game.id())
            .expect_err("ambiguous history must not be guessed");
        let ServiceError::ManagedCleanupAmbiguous {
            recovery_bundle_path,
            targets,
            ..
        } = error
        else {
            panic!("expected managed cleanup ambiguity");
        };

        assert!(!targets.is_empty());
        assert!(
            std::path::Path::new(&recovery_bundle_path).is_dir(),
            "pre-mutation recovery bundle must be published"
        );
        assert_eq!(
            std::fs::read(&live).expect("unchanged live file"),
            b"replacement"
        );
        assert_eq!(
            std::fs::read(&backup).expect("unchanged backup"),
            b"original"
        );
        assert!(
            context
                .storage()
                .find_game(game.id())
                .expect("read")
                .is_some()
        );
    }

    #[test]
    fn equivalent_overlapping_component_inverses_execute_once_without_ambiguity() {
        let temp = tempfile::tempdir().expect("temp");
        let install = temp.path().join("Managed Game");
        std::fs::create_dir_all(&install).expect("install");
        let live = install.join("nvngx_dlss.dll");
        let backup = crate::fs::backup_path(&live).expect("backup path");
        std::fs::write(&live, b"replacement").expect("live");
        std::fs::write(&backup, b"original").expect("backup");
        let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        let game = game(&install, RootAuthority::UserConfirmed);
        context.storage().upsert_game(&game).expect("seed");

        let active_file = ComponentFile::new(path_ref(&live))
            .with_sha256(renderpilot_detection::sha256_file(&live).expect("live hash"));
        let original_file = ComponentFile::new(path_ref(&live))
            .with_sha256(renderpilot_detection::sha256_file(&backup).expect("backup hash"));
        let component_ids = [
            ComponentId::new("component:duplicate-a").expect("component"),
            ComponentId::new("component:duplicate-b").expect("component"),
        ];
        let components = component_ids
            .iter()
            .map(|component_id| {
                LibraryComponent::new(
                    component_id.clone(),
                    game.id().clone(),
                    ComponentKind::NativeLibrary,
                    LibraryTechnology::DlssSuperResolution,
                    Swappability::Swappable,
                )
                .with_file(active_file.clone())
            })
            .collect::<Vec<_>>();
        context
            .storage()
            .replace_components_for_game(game.id(), &components)
            .expect("components");
        let inverse = ComponentRollbackBaseline::new(vec![original_file])
            .with_expected_active_files(vec![active_file]);
        for component_id in &component_ids {
            context
                .storage()
                .recover_component_rollback_baseline(game.id(), component_id, &inverse)
                .expect("baseline");
        }

        remove_game_from_catalog(&context, game.id()).expect("remove");

        assert_eq!(std::fs::read(&live).expect("restored"), b"original");
        assert!(!backup.exists());
        assert!(
            context
                .storage()
                .find_game(game.id())
                .expect("game lookup")
                .is_none()
        );
    }

    #[test]
    fn installed_addon_is_uninstalled_automatically_before_card_removal() {
        let temp = tempfile::tempdir().expect("temp");
        let install = temp.path().join("Managed Game");
        std::fs::create_dir_all(&install).expect("install");
        let addon_path = install.join("addon.addon64");
        std::fs::write(&addon_path, b"addon").expect("addon file");
        let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        let game = game(&install, RootAuthority::UserConfirmed);
        context.storage().upsert_game(&game).expect("seed");
        let addon = InstalledAddon::new(
            game.id().clone(),
            AddonKind::RenoDx,
            PathRef::new(addon_path.to_string_lossy()).expect("addon path"),
        );
        context
            .storage()
            .upsert_installed_addon(&addon)
            .expect("addon");

        remove_game_from_catalog(&context, game.id()).expect("remove");

        assert!(!addon_path.exists(), "durable add-on uninstall must run");
        assert!(
            context
                .storage()
                .get_installed_addon(game.id())
                .expect("addon lookup")
                .is_none()
        );
        assert!(
            context
                .storage()
                .find_game(game.id())
                .expect("game lookup")
                .is_none()
        );
    }

    fn game(path: &std::path::Path, authority: RootAuthority) -> GameInstallation {
        GameInstallation::new(
            GameIdentity::new(
                GameId::new("game:remove-test").expect("id"),
                "Game",
                Launcher::Manual,
            )
            .expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(path.to_string_lossy()).expect("path"),
        )
        .with_root_authority(authority)
    }

    fn path_ref(path: &std::path::Path) -> PathRef {
        PathRef::new(path.to_string_lossy()).expect("path")
    }
}
