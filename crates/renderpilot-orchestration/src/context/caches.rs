use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};

use renderpilot_domain::GameId;

use crate::catalog::{GameDetailsCatalogResult, ReplacementUniverse, ReplacementUniverseRevision};

#[derive(Default)]
pub(super) struct ReplacementUniverseCache {
    entry: RwLock<Option<ReplacementUniverseCacheEntry>>,
    rebuild: Mutex<()>,
}

struct ReplacementUniverseCacheEntry {
    revision: ReplacementUniverseRevision,
    universe: Arc<ReplacementUniverse>,
}

impl ReplacementUniverseCache {
    pub(super) fn get(&self) -> Option<(ReplacementUniverseRevision, Arc<ReplacementUniverse>)> {
        self.entry
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|entry| (entry.revision, Arc::clone(&entry.universe)))
    }

    pub(super) fn store(
        &self,
        revision: ReplacementUniverseRevision,
        universe: Arc<ReplacementUniverse>,
    ) {
        *self.entry.write().unwrap_or_else(PoisonError::into_inner) =
            Some(ReplacementUniverseCacheEntry { revision, universe });
    }

    pub(super) fn rebuild_guard(&self) -> MutexGuard<'_, ()> {
        self.rebuild.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Default)]
pub(super) struct GameDetailsCache {
    entries: RwLock<HashMap<GameId, GameDetailsCacheEntry>>,
    rebuilds: Mutex<HashMap<GameId, Arc<Mutex<()>>>>,
}

struct GameDetailsCacheEntry {
    catalog_generation: u64,
    details: Arc<GameDetailsCatalogResult>,
}

impl GameDetailsCache {
    pub(super) fn get(
        &self,
        game_id: &GameId,
        catalog_generation: u64,
    ) -> Option<Arc<GameDetailsCatalogResult>> {
        self.entries
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(game_id)
            .filter(|entry| entry.catalog_generation == catalog_generation)
            .map(|entry| Arc::clone(&entry.details))
    }

    pub(super) fn store(
        &self,
        game_id: GameId,
        catalog_generation: u64,
        details: Arc<GameDetailsCatalogResult>,
    ) {
        self.entries
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                game_id,
                GameDetailsCacheEntry {
                    catalog_generation,
                    details,
                },
            );
    }

    pub(super) fn retain_generation(&self, catalog_generation: u64) {
        self.entries
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|_, entry| entry.catalog_generation == catalog_generation);
    }

    pub(super) fn rebuild_lock(&self, game_id: &GameId) -> Arc<Mutex<()>> {
        let mut rebuilds = self.rebuilds.lock().unwrap_or_else(PoisonError::into_inner);
        // The map owns one strong reference. Entries without an active user or
        // waiter can be dropped so invalid ids cannot grow process memory.
        rebuilds.retain(|_, rebuild| Arc::strong_count(rebuild) > 1);
        Arc::clone(
            rebuilds
                .entry(game_id.clone())
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        )
    }

    #[cfg(test)]
    pub(super) fn has_rebuild_lock(&self, game_id: &GameId) -> bool {
        self.rebuilds
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains_key(game_id)
    }
}

#[derive(Default)]
pub(super) struct BackgroundRefreshGate(AtomicBool);

impl BackgroundRefreshGate {
    pub(super) fn claim(&self) -> bool {
        !self.0.swap(true, Ordering::AcqRel)
    }
}
