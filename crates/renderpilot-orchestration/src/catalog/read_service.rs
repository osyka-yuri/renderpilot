//! Coherent catalog read boundary shared by presentation transports.

use std::sync::Arc;

use renderpilot_domain::GameId;

use crate::ServiceError;

use super::{CatalogSnapshot, GameDetailsCatalogResult};

/// Orchestration-owned entry point for catalog projections.
///
/// The service keeps transport layers out of SQLite details and guarantees that
/// cards, facets, revisions, and details use the same process-wide caches.
#[derive(Clone, Copy)]
pub struct CatalogReadService<'context> {
    context: &'context crate::Context,
}

impl<'context> CatalogReadService<'context> {
    /// Creates a read service over the process-wide application context.
    #[must_use]
    pub const fn new(context: &'context crate::Context) -> Self {
        Self { context }
    }

    /// Returns the latency-sensitive immutable catalog snapshot.
    pub fn snapshot(&self) -> Result<Arc<CatalogSnapshot>, ServiceError> {
        super::cards::catalog_snapshot(self.context)
    }

    /// Waits for a snapshot matching the current authoritative generation.
    pub fn refresh_snapshot(&self) -> Result<Arc<CatalogSnapshot>, ServiceError> {
        super::cards::refresh_catalog_snapshot(self.context)
    }

    /// Performs the mandatory background validation of filesystem-sensitive
    /// card facts and returns the ids whose effective projections changed.
    pub fn refresh_validated_snapshot(
        &self,
    ) -> Result<(Arc<CatalogSnapshot>, Vec<GameId>), ServiceError> {
        super::cards::refresh_catalog_snapshot_validated(self.context)
    }

    /// Returns the typed details projection backed by the shared universe and
    /// generation-keyed details cache.
    pub fn game_details(&self, game_id: &GameId) -> Result<GameDetailsCatalogResult, ServiceError> {
        super::get_game_details(self.context, game_id)
    }
}

#[cfg(test)]
mod tests {
    use renderpilot_application::{
        ArtifactRepository, ComponentRepository, GameRepository, InstalledAddonRepository,
    };
    use renderpilot_domain::{
        AddonKind, ArtifactId, ArtifactTrustLevel, ComponentFile, ComponentId, ComponentKind,
        ComponentRollbackBaseline, GameIdentity, GameInstallation, GameRuntime, InstalledAddon,
        InstalledAddonHostKind, Launcher, LibraryArtifact, LibraryComponent, LibraryTechnology,
        PathRef, Platform, RootAuthority, Swappability,
    };
    use renderpilot_storage_sqlite::{AuthorityCas, BeginFileMutationPreparation};

    use super::*;

    #[test]
    fn absent_registration_disappears_from_cards_and_cached_details() {
        let temp = tempfile::tempdir().expect("temp");
        let install = temp.path().join("game");
        std::fs::create_dir_all(&install).expect("install");
        let id = GameId::new("manual:read-service-absent").expect("game id");
        let game = GameInstallation::new(
            GameIdentity::new(id.clone(), "Absent Read Service", Launcher::Manual)
                .expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(install.to_string_lossy().as_ref()).expect("install path"),
        )
        .with_root_authority(RootAuthority::UserConfirmed);
        let component_path = install.join("nvngx_dlss.dll");
        std::fs::write(&component_path, b"retained active component").expect("component file");
        let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        context.storage().upsert_game(&game).expect("game");
        let component_id =
            ComponentId::new("component:read-service-retained").expect("component id");
        let component_file = ComponentFile::new(
            PathRef::new(component_path.to_string_lossy().replace('\\', "/"))
                .expect("component path"),
        )
        .with_sha256(renderpilot_detection::sha256_file(&component_path).expect("component hash"));
        let component = LibraryComponent::new(
            component_id.clone(),
            id.clone(),
            ComponentKind::NativeLibrary,
            LibraryTechnology::DlssSuperResolution,
            Swappability::Swappable,
        )
        .with_file(component_file.clone());
        context
            .storage()
            .replace_components_for_game(&id, &[component])
            .expect("component");
        context
            .storage()
            .recover_component_rollback_baseline(
                &id,
                &component_id,
                &ComponentRollbackBaseline::new(vec![component_file]),
            )
            .expect("retained rollback baseline");
        context
            .storage()
            .begin_file_mutation_preparation(&BeginFileMutationPreparation {
                id: "mutation:read-service-retained".to_owned(),
                game_id: id.clone(),
                feature: "test_pending".to_owned(),
                subject_id: None,
                initial_manifest_json: "{}".to_owned(),
            })
            .expect("pending mutation");

        let reader = CatalogReadService::new(&context);
        assert_eq!(reader.snapshot().expect("snapshot").cards().len(), 1);
        assert!(
            !reader
                .snapshot()
                .expect("snapshot")
                .available_libraries()
                .is_empty()
        );
        assert!(
            !crate::catalog::distinct_game_libraries(&context)
                .expect("libraries")
                .is_empty()
        );
        assert!(
            !crate::catalog::distinct_game_launchers(&context)
                .expect("launchers")
                .is_empty()
        );
        assert!(
            crate::catalog::backup_component_ids(&context, &id)
                .expect("backup ids")
                .contains(component_id.as_str())
        );
        assert_eq!(reader.game_details(&id).expect("details").game.id(), &id);

        let epoch = context
            .storage()
            .catalog_readiness(&id)
            .expect("readiness")
            .authority_epoch();
        context
            .storage()
            .mark_installation_absent(&game, AuthorityCas::new(epoch))
            .expect("mark absent");

        assert!(
            context
                .storage()
                .find_game(&id)
                .expect("raw registration")
                .is_some()
        );
        assert!(
            crate::catalog::list_games(&context)
                .expect("active games")
                .is_empty()
        );
        assert!(
            reader.game_details(&id).is_err(),
            "cached details must be hidden before card refresh"
        );
        let snapshot = reader.refresh_snapshot().expect("active snapshot");
        assert!(snapshot.cards().is_empty());
        assert!(snapshot.available_launchers().is_empty());
        assert!(snapshot.available_libraries().is_empty());
        assert!(
            crate::catalog::distinct_game_libraries(&context)
                .expect("active libraries")
                .is_empty()
        );
        assert!(
            crate::catalog::distinct_game_launchers(&context)
                .expect("active launchers")
                .is_empty()
        );
        assert!(
            crate::catalog::backup_component_ids(&context, &id)
                .expect("active backup ids")
                .is_empty()
        );
        assert_eq!(
            context
                .storage()
                .list_components_for_game(&id)
                .expect("retained raw component")
                .len(),
            1,
            "pending recovery keeps raw rollback metadata available"
        );
    }

    #[test]
    fn replacement_cache_drops_retired_source_while_pending_keeps_raw_artifact() {
        let temp = tempfile::tempdir().expect("temp");
        let install = temp.path().join("game");
        std::fs::create_dir_all(&install).expect("install");
        let source = install.join("nvngx_dlss.dll");
        let bytes = b"retained local replacement source";
        std::fs::write(&source, bytes).expect("source file");
        let id = GameId::new("manual:replacement-cache-absent").expect("game id");
        let game = GameInstallation::new(
            GameIdentity::new(id.clone(), "Replacement Cache", Launcher::Manual).expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(install.to_string_lossy().as_ref()).expect("install path"),
        )
        .with_root_authority(RootAuthority::UserConfirmed);
        let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        context.storage().upsert_game(&game).expect("game");
        let artifact_id = ArtifactId::new("artifact:retained-local-source").expect("artifact id");
        let artifact = LibraryArtifact::new(
            artifact_id.clone(),
            LibraryTechnology::DlssSuperResolution,
            "nvngx_dlss.dll",
            vec![
                ComponentFile::new(
                    PathRef::new(source.to_string_lossy().replace('\\', "/")).expect("source path"),
                )
                .with_sha256(renderpilot_detection::sha256_file(&source).expect("source hash")),
            ],
            ArtifactTrustLevel::LocalObserved,
        )
        .expect("artifact")
        .with_source_game_id(id.clone());
        context
            .storage()
            .upsert_artifact(&artifact)
            .expect("artifact");
        context
            .storage()
            .begin_file_mutation_preparation(&BeginFileMutationPreparation {
                id: "mutation:retained-local-source".to_owned(),
                game_id: id.clone(),
                feature: "test_pending".to_owned(),
                subject_id: None,
                initial_manifest_json: "{}".to_owned(),
            })
            .expect("pending mutation");

        let cached = crate::catalog::load_replacement_universe(&context)
            .expect("initial replacement universe");
        assert!(
            cached
                .artifact_index
                .artifacts()
                .iter()
                .any(|candidate| candidate.id() == &artifact_id)
        );

        let epoch = context
            .storage()
            .catalog_readiness(&id)
            .expect("readiness")
            .authority_epoch();
        context
            .storage()
            .mark_installation_absent(&game, AuthorityCas::new(epoch))
            .expect("mark absent");
        assert_eq!(
            context
                .storage()
                .pending_file_mutations_for_game(&id)
                .expect("pending rows")
                .len(),
            1
        );
        assert!(
            context
                .storage()
                .list_artifacts()
                .expect("raw artifacts")
                .iter()
                .any(|stored| stored.id() == &artifact_id)
        );

        let refreshed = crate::catalog::load_replacement_universe(&context)
            .expect("refreshed replacement universe");
        assert!(
            !refreshed
                .artifact_index
                .artifacts()
                .iter()
                .any(|candidate| candidate.id() == &artifact_id)
        );
    }

    #[test]
    fn retained_shared_engine_owner_does_not_project_missing_local_addon_as_installed() {
        let temp = tempfile::tempdir().expect("temp");
        let install = temp.path().join("reappeared-game");
        std::fs::create_dir_all(&install).expect("install");
        let executable = install.join("Game.exe");
        std::fs::write(&executable, b"fresh executable").expect("executable");
        let engine_ini = temp.path().join("AppData").join("Engine.ini");
        std::fs::create_dir_all(engine_ini.parent().expect("Engine.ini parent"))
            .expect("Engine.ini directory");
        let id = GameId::new("manual:reappeared-external-engine-owner").expect("game id");
        let game = GameInstallation::new(
            GameIdentity::new(id.clone(), "Reappeared Game", Launcher::Manual).expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(install.to_string_lossy().replace('\\', "/")).expect("install path"),
        )
        .with_root_authority(RootAuthority::UserConfirmed)
        .with_executable_candidate(
            PathRef::new(executable.to_string_lossy().replace('\\', "/")).expect("executable path"),
        );
        let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        context.storage().upsert_game(&game).expect("game");
        let recipe = crate::addons::engine_config::EngineIniRecipe::new(
            "read.service.external.owner",
            1,
            vec![crate::addons::engine_config::EngineIniEntry {
                section: "SystemSettings".to_owned(),
                key: "r.AllowHDR".to_owned(),
                value: "1".to_owned(),
            }],
        )
        .expect("recipe");
        let recipes = crate::addons::engine_config::EngineIniRecipeSet::from_recipes([&recipe])
            .expect("recipes");
        crate::addons::engine_config::service::apply(
            context.storage(),
            &id,
            AddonKind::RenoDx,
            &engine_ini,
            &recipes,
            "read-service-external-owner",
        )
        .expect("external Engine.ini contribution");
        let local_addon = InstalledAddon::new(
            id.clone(),
            AddonKind::RenoDx,
            PathRef::new(
                install
                    .join("renodx.addon64")
                    .to_string_lossy()
                    .replace('\\', "/"),
            )
            .expect("missing local add-on path"),
        )
        .with_host_kind(InstalledAddonHostKind::SharedVulkanLayer)
        .with_registered_exe_path(
            PathRef::new(executable.to_string_lossy().replace('\\', "/"))
                .expect("registered executable"),
        );
        context
            .storage()
            .upsert_installed_addon(&local_addon)
            .expect("retained local metadata");

        let hydrated = context
            .storage()
            .get_installed_addon(&id)
            .expect("local row")
            .expect("retained row");
        assert!(hydrated.engine_config_journal().is_some());
        let snapshot = CatalogReadService::new(&context)
            .refresh_snapshot()
            .expect("snapshot");
        assert!(
            snapshot
                .card(&id)
                .expect("active game card")
                .addon_capabilities
                .is_empty()
        );
        assert!(
            context
                .storage()
                .engine_config_journal_owner(&id)
                .expect("external owner")
                .is_some()
        );
    }
}
