use crate::{ServiceError, storage::open_catalog_storage};
use renderpilot_storage_sqlite::SqliteStorage;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

mod caches;
mod snapshot_cache;

use self::caches::{BackgroundRefreshGate, GameDetailsCache, ReplacementUniverseCache};
use self::snapshot_cache::CatalogSnapshotCache;

/// Shared application context holding the catalog storage and configuration.
pub struct Context {
    storage: SqliteStorage,
    file_mutation_root: PathBuf,
    catalog_snapshots: CatalogSnapshotCache,
    catalog_scan: Mutex<()>,
    replacement_universe_cache: ReplacementUniverseCache,
    game_details_cache: GameDetailsCache,
    background_refresh_gate: BackgroundRefreshGate,
}

impl Context {
    /// Opens the application context and initializes shared storage.
    pub fn open() -> Result<Self, ServiceError> {
        let storage = open_catalog_storage()?;
        let root = crate::app_dir::app_dir()?.join("file-transactions");
        Ok(Self::from_storage_with_mutation_root(storage, root))
    }

    /// Opens the application context using a custom database path (useful for testing).
    pub fn open_at(path: impl AsRef<std::path::Path>) -> Result<Self, ServiceError> {
        let path = path.as_ref();
        let storage =
            SqliteStorage::open(path).map_err(|e| ServiceError::command_failed(e.to_string()))?;
        let root = mutation_root_for_catalog(path);
        Ok(Self::from_storage_with_mutation_root(storage, root))
    }

    /// Creates a [`Context`] from an existing storage connection, for tests.
    ///
    /// The file-mutation root is a fresh, nondeterministic temp directory
    /// (`<temp>/renderpilot-file-transactions/<pid>/<ulid>`), so each call is
    /// isolated. Only available under `#[cfg(test)]`; production code must use
    /// [`Context::open`] / [`Context::open_at`], which derive a stable
    /// mutation root from the catalog path.
    #[cfg(test)]
    pub fn from_storage(storage: SqliteStorage) -> Self {
        let root = std::env::temp_dir()
            .join("renderpilot-file-transactions")
            .join(std::process::id().to_string())
            .join(ulid::Ulid::generate().to_string());
        Self::from_storage_with_mutation_root(storage, root)
    }

    fn from_storage_with_mutation_root(
        storage: SqliteStorage,
        file_mutation_root: PathBuf,
    ) -> Self {
        Self {
            storage,
            file_mutation_root,
            catalog_snapshots: CatalogSnapshotCache::default(),
            catalog_scan: Mutex::new(()),
            replacement_universe_cache: ReplacementUniverseCache::default(),
            game_details_cache: GameDetailsCache::default(),
            background_refresh_gate: BackgroundRefreshGate::default(),
        }
    }

    /// Exposes the underlying SQLite storage for orchestration internal use.
    ///
    /// Intentionally `pub(crate)`: only orchestration feature modules may drive
    /// the storage ports. Front-ends (`renderpilot-api`, `renderpilot-cli`) must
    /// go through the typed feature functions, keeping the
    /// orchestration↔presentation boundary compiler-enforced. Tests that need
    /// raw storage open their own [`SqliteStorage`] on the same database path.
    pub(crate) fn storage(&self) -> &SqliteStorage {
        &self.storage
    }

    pub(crate) fn file_mutation_root(&self) -> &Path {
        &self.file_mutation_root
    }

    /// Serializes complete catalog scan sessions while allowing the workers
    /// within one auto-scan session to run concurrently.
    pub(crate) fn catalog_scan_guard(&self) -> std::sync::MutexGuard<'_, ()> {
        self.catalog_scan
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn catalog_snapshot(
        &self,
    ) -> Result<Arc<crate::catalog::CatalogSnapshot>, ServiceError> {
        self.catalog_snapshots.snapshot(self)
    }

    /// Returns a snapshot for the current storage generation.
    ///
    /// Unlike the latency-sensitive read path, this waits for an in-flight
    /// rebuild and never reports the retained stale snapshot as a completed
    /// refresh. Event publishers use it before announcing a new revision.
    pub(crate) fn refresh_catalog_snapshot(
        &self,
    ) -> Result<Arc<crate::catalog::CatalogSnapshot>, ServiceError> {
        self.catalog_snapshots.refresh(self)
    }

    /// Rebuilds the card projection with fresh filesystem-sensitive facts.
    ///
    /// This path is intentionally reserved for background maintenance. The
    /// latency-sensitive catalog path uses durable facts and never waits for
    /// `.bak`, tracked component-file, or D3D12 executable probes. A completed
    /// validation is installed atomically only after it has been built in full.
    pub(crate) fn refresh_catalog_snapshot_validated(
        &self,
    ) -> Result<
        (
            Arc<crate::catalog::CatalogSnapshot>,
            Vec<renderpilot_domain::GameId>,
        ),
        ServiceError,
    > {
        self.catalog_snapshots.refresh_validated(self)
    }

    pub(crate) fn replacement_universe_cache(
        &self,
    ) -> Option<(
        crate::catalog::ReplacementUniverseRevision,
        Arc<crate::catalog::ReplacementUniverse>,
    )> {
        self.replacement_universe_cache.get()
    }

    pub(crate) fn cache_replacement_universe(
        &self,
        revision: crate::catalog::ReplacementUniverseRevision,
        universe: Arc<crate::catalog::ReplacementUniverse>,
    ) {
        self.replacement_universe_cache.store(revision, universe);
    }

    pub(crate) fn replacement_universe_rebuild_guard(&self) -> std::sync::MutexGuard<'_, ()> {
        self.replacement_universe_cache.rebuild_guard()
    }

    pub(crate) fn game_details_cache(
        &self,
        game_id: &renderpilot_domain::GameId,
        catalog_generation: u64,
    ) -> Option<Arc<crate::catalog::GameDetailsCatalogResult>> {
        self.game_details_cache.get(game_id, catalog_generation)
    }

    pub(crate) fn cache_game_details(
        &self,
        game_id: renderpilot_domain::GameId,
        catalog_generation: u64,
        details: Arc<crate::catalog::GameDetailsCatalogResult>,
    ) {
        self.game_details_cache
            .store(game_id, catalog_generation, details);
    }

    pub(crate) fn game_details_rebuild_lock(
        &self,
        game_id: &renderpilot_domain::GameId,
    ) -> Arc<Mutex<()>> {
        self.game_details_cache.rebuild_lock(game_id)
    }

    /// Applies a cover-only mutation without invalidating or rebuilding card
    /// facts that are unrelated to artwork. A retained stale snapshot is still
    /// patched: its generation marker remains stale, while a concurrent rebuild
    /// observes the revision change and preserves the newer cover projection.
    pub(crate) fn patch_catalog_cover(
        &self,
        game_id: &renderpilot_domain::GameId,
        updated_at_ms: Option<i64>,
    ) {
        self.catalog_snapshots.patch_cover(game_id, updated_at_ms);
    }

    /// Applies an exact favorite/hidden mutation without rebuilding unrelated
    /// card facts. The generation bounds ensure no concurrent catalog write is
    /// accidentally hidden by the patch.
    pub(crate) fn patch_catalog_ui_state(
        &self,
        game_id: &renderpilot_domain::GameId,
        is_favorite: bool,
        is_hidden: bool,
        generation_before: u64,
        generation_after: u64,
    ) {
        self.catalog_snapshots.patch_ui_state(
            self,
            game_id,
            is_favorite,
            is_hidden,
            generation_before,
            generation_after,
        );
    }

    /// Claims the one process-wide background refresh start for this context.
    #[must_use]
    pub fn begin_background_refresh(&self) -> bool {
        self.background_refresh_gate.claim()
    }

    /// Marks the projection stale while retaining it for readers during rebuild.
    pub fn invalidate_catalog_snapshot(&self) {
        self.storage.invalidate_catalog_projection();
        self.catalog_snapshots.invalidate();
    }
}

fn mutation_root_for_catalog(path: &Path) -> PathBuf {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let catalog_name = path
        .file_name()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| std::ffi::OsStr::new("catalog.sqlite"));

    parent.join("file-transactions").join(catalog_name)
}

#[cfg(test)]
mod tests {
    use super::{Context, mutation_root_for_catalog};
    use renderpilot_application::{ComponentRepository, GameRepository};
    use renderpilot_domain::{
        ComponentFile, ComponentId, ComponentKind, GameId, GameIdentity, GameInstallation,
        GameRuntime, GraphicsComponent, GraphicsTechnology, Launcher, PathRef, Platform,
        Sha256Hash, Swappability,
    };
    use renderpilot_storage_sqlite::SqliteStorage;
    use std::path::Path;
    use std::sync::Arc;

    #[test]
    fn custom_catalogs_get_stable_isolated_transaction_namespaces() {
        let first = mutation_root_for_catalog(Path::new("C:/temp/first.sqlite"));
        let first_again = mutation_root_for_catalog(Path::new("C:/temp/first.sqlite"));
        let second = mutation_root_for_catalog(Path::new("C:/temp/second.sqlite"));

        assert_eq!(first, first_again);
        assert_ne!(first, second);
        assert_eq!(first, Path::new("C:/temp/file-transactions/first.sqlite"));
    }

    #[test]
    fn background_refresh_gate_can_be_claimed_only_once() {
        let context = Context::from_storage(SqliteStorage::in_memory().expect("storage"));

        assert!(context.begin_background_refresh());
        assert!(!context.begin_background_refresh());
    }

    #[test]
    fn details_single_flight_reuses_active_locks_and_prunes_idle_ids() {
        let context = Context::from_storage(SqliteStorage::in_memory().expect("storage"));
        let first_id = GameId::new("manual:first").expect("first id");
        let second_id = GameId::new("manual:second").expect("second id");

        let first = context.game_details_rebuild_lock(&first_id);
        let concurrent = context.game_details_rebuild_lock(&first_id);
        assert!(Arc::ptr_eq(&first, &concurrent));

        drop(first);
        drop(concurrent);
        let _second = context.game_details_rebuild_lock(&second_id);
        assert!(!context.game_details_cache.has_rebuild_lock(&first_id));
        assert!(context.game_details_cache.has_rebuild_lock(&second_id));
    }

    #[test]
    fn invalidation_keeps_previous_snapshot_available_during_single_flight_rebuild() {
        let context = Context::from_storage(SqliteStorage::in_memory().expect("storage"));
        let first = context.catalog_snapshot().expect("first snapshot");
        context.invalidate_catalog_snapshot();

        let rebuild = context.catalog_snapshots.rebuild_guard();
        let stale = context
            .catalog_snapshot()
            .expect("stale snapshot remains readable");
        assert!(Arc::ptr_eq(&first, &stale));
        drop(rebuild);

        let rebuilt = context.catalog_snapshot().expect("rebuilt snapshot");
        assert!(rebuilt.revision() > first.revision());
    }

    #[test]
    fn authoritative_refresh_waits_for_single_flight_and_returns_current_revision() {
        let context = Arc::new(Context::from_storage(
            SqliteStorage::in_memory().expect("storage"),
        ));
        let first = context.catalog_snapshot().expect("first snapshot");
        context.invalidate_catalog_snapshot();

        let rebuild = context.catalog_snapshots.rebuild_guard();
        let (sender, receiver) = std::sync::mpsc::channel();
        let worker_context = Arc::clone(&context);
        let worker = std::thread::spawn(move || {
            sender
                .send(worker_context.refresh_catalog_snapshot())
                .expect("send refresh result");
        });

        assert!(
            receiver
                .recv_timeout(std::time::Duration::from_millis(25))
                .is_err(),
            "authoritative refresh must not return the retained stale snapshot",
        );
        drop(rebuild);

        let refreshed = receiver
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("refresh completed")
            .expect("refreshed snapshot");
        worker.join().expect("refresh worker");
        assert!(refreshed.revision() > first.revision());
    }

    #[test]
    fn preferences_keep_snapshot_hot_while_card_fact_writes_invalidate_it() {
        let context = Context::from_storage(SqliteStorage::in_memory().expect("storage"));
        let first = context.catalog_snapshot().expect("first snapshot");

        context
            .storage()
            .set_setting("games_filters_v3", r#"{"searchQuery":"doom"}"#)
            .expect("preference");
        let after_preference = context.catalog_snapshot().expect("snapshot hit");
        assert!(Arc::ptr_eq(&first, &after_preference));

        let game_id = GameId::new("manual:generation").expect("game id");
        let identity =
            GameIdentity::new(game_id, "Generation", Launcher::Manual).expect("game identity");
        let game = GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new("C:/Games/Generation").expect("path"),
        );
        context.storage().upsert_game(&game).expect("game write");

        let after_game = context.catalog_snapshot().expect("rebuilt snapshot");
        assert!(after_game.revision() > first.revision());
        assert_eq!(after_game.cards().len(), 1);
    }

    #[test]
    fn cover_mutation_patches_one_card_without_invalidating_the_snapshot_generation() {
        let context = context_with_games(2);
        let game_id = GameId::new("manual:generated-00000").expect("game id");
        let first = context.catalog_snapshot().expect("first snapshot");
        let generation = context.storage().catalog_generation();

        context
            .storage()
            .upsert_game_cover(&game_id, "cover.webp")
            .expect("cover row");
        let updated_at_ms = context
            .storage()
            .find_game_cover(&game_id)
            .expect("cover lookup")
            .expect("cover record")
            .updated_at_ms;
        assert_eq!(context.storage().catalog_generation(), generation);

        context.patch_catalog_cover(&game_id, Some(updated_at_ms));
        let patched = context.catalog_snapshot().expect("patched snapshot");

        assert!(patched.revision() > first.revision());
        assert_eq!(
            patched
                .card(&game_id)
                .and_then(|card| card.cover_updated_at_ms),
            Some(updated_at_ms),
        );
        assert!(Arc::ptr_eq(&patched, &context.catalog_snapshot().unwrap()));
    }

    #[test]
    fn cover_patch_is_preserved_while_an_invalidated_snapshot_waits_for_rebuild() {
        let context = context_with_games(1);
        let game_id = GameId::new("manual:generated-00000").expect("game id");
        let initial = context.catalog_snapshot().expect("initial snapshot");
        context.invalidate_catalog_snapshot();

        let rebuild = context.catalog_snapshots.rebuild_guard();
        context.patch_catalog_cover(&game_id, Some(42));
        let retained = context.catalog_snapshot().expect("retained snapshot");
        drop(rebuild);

        assert!(retained.revision() > initial.revision());
        assert_eq!(
            retained
                .card(&game_id)
                .and_then(|card| card.cover_updated_at_ms),
            Some(42),
        );
    }

    #[test]
    fn live_validation_corrects_durable_rollback_projection_in_the_background() {
        let context = context_with_games(1);
        let game_id = GameId::new("manual:generated-00000").expect("game id");
        let component_id = ComponentId::new("component:missing-backup").expect("component id");
        let missing_path =
            PathRef::new("C:/renderpilot-tests/live-validation/missing-renderpilot-runtime.dll")
                .expect("missing path");
        let hash = Sha256Hash::new("a".repeat(64)).expect("hash");
        let file = ComponentFile::new(missing_path).with_sha256(hash);
        let component = GraphicsComponent::new(
            component_id.clone(),
            game_id.clone(),
            ComponentKind::NativeLibrary,
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
        )
        .with_file(file.clone());
        context
            .storage()
            .replace_components_for_game(&game_id, std::slice::from_ref(&component))
            .expect("component");
        context
            .storage()
            .recover_component_backup(&game_id, &component_id, &[file])
            .expect("durable baseline");

        let fast = context.catalog_snapshot().expect("durable snapshot");
        assert!(
            fast.card(&game_id).expect("fast card").rollback_available,
            "the latency-sensitive projection must not probe the missing path",
        );

        let (validated, changed_game_ids) = context
            .refresh_catalog_snapshot_validated()
            .expect("live validation");
        assert_eq!(changed_game_ids, vec![game_id.clone()]);
        assert!(
            !validated
                .card(&game_id)
                .expect("validated card")
                .rollback_available,
            "background validation must reject a baseline with no readable byte source",
        );
        assert_eq!(
            context
                .catalog_snapshot()
                .expect("installed validation")
                .revision(),
            validated.revision(),
        );

        crate::catalog::set_game_favorite(&context, &game_id, true).expect("favorite mutation");
        let patched = context.catalog_snapshot().expect("patched snapshot");
        let patched_card = patched.card(&game_id).expect("patched card");
        assert!(patched_card.is_favorite);
        assert!(
            !patched_card.rollback_available,
            "an unrelated UI-state patch must retain validated live facts",
        );
        assert!(patched.revision() > validated.revision());
    }

    #[test]
    fn unchanged_live_validation_retains_the_existing_snapshot_revision() {
        let context = context_with_games(1);
        let initial = context.catalog_snapshot().expect("initial snapshot");

        let (validated, changed_game_ids) = context
            .refresh_catalog_snapshot_validated()
            .expect("live validation");

        assert!(changed_game_ids.is_empty());
        assert!(
            Arc::ptr_eq(&initial, &validated),
            "an equivalent live projection must not create a visible catalog revision",
        );
    }

    #[test]
    fn live_validation_advances_revision_after_authoritative_invalidation() {
        let context = context_with_games(1);
        let initial = context.catalog_snapshot().expect("initial snapshot");
        context.invalidate_catalog_snapshot();

        let (validated, changed_game_ids) = context
            .refresh_catalog_snapshot_validated()
            .expect("live validation");

        assert!(changed_game_ids.is_empty());
        assert!(validated.revision() > initial.revision());
        assert!(!Arc::ptr_eq(&initial, &validated));
    }

    #[test]
    fn cold_snapshot_uses_a_fixed_number_of_sqlite_selects() {
        let mut expected_selects = None;
        for game_count in [10, 1_000, 10_000] {
            let context = context_with_games(game_count);
            let (snapshot, select_count) = context
                .storage()
                .with_select_statement_count(|_| context.refresh_catalog_snapshot())
                .expect("generated snapshot");

            assert_eq!(snapshot.cards().len(), game_count);
            match expected_selects {
                Some(expected) => assert_eq!(select_count, expected),
                None => {
                    assert!(select_count > 0);
                    expected_selects = Some(select_count);
                }
            }
        }
    }

    #[test]
    fn repeated_game_details_hit_the_generation_keyed_projection_cache() {
        let context = context_with_games(1);
        let game_id = GameId::new("manual:generated-00000").expect("game id");

        let (_, cold_selects) = context
            .storage()
            .with_select_statement_count(|_| crate::catalog::get_game_details(&context, &game_id))
            .expect("cold details");
        let (_, hot_selects) = context
            .storage()
            .with_select_statement_count(|_| crate::catalog::get_game_details(&context, &game_id))
            .expect("cached details");

        assert!(cold_selects > 0);
        assert_eq!(hot_selects, 0);
    }

    fn context_with_games(game_count: usize) -> Context {
        let storage = SqliteStorage::in_memory().expect("storage");
        let games = (0..game_count)
            .map(|index| {
                let game_id =
                    GameId::new(format!("manual:generated-{index:05}")).expect("generated game id");
                let identity = GameIdentity::new(
                    game_id,
                    format!("Generated Game {index:05}"),
                    Launcher::Manual,
                )
                .expect("generated identity");
                GameInstallation::new(
                    identity,
                    Platform::Windows,
                    GameRuntime::NativeWindows,
                    PathRef::new(format!("C:/Games/Generated-{index:05}")).expect("generated path"),
                )
            })
            .collect::<Vec<_>>();
        storage.upsert_games(&games).expect("seed generated games");
        Context::from_storage(storage)
    }
}
