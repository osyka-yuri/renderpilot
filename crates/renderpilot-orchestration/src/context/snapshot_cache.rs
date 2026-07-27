use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock, TryLockError};

use renderpilot_domain::GameId;

use crate::catalog::CatalogSnapshot;
use crate::{Context, ServiceError};

pub(super) struct CatalogSnapshotCache {
    entry: RwLock<Option<CatalogSnapshotEntry>>,
    rebuild: Mutex<()>,
    revision: AtomicU64,
}

struct CatalogSnapshotEntry {
    catalog_generation: Option<u64>,
    snapshot: Arc<CatalogSnapshot>,
}

impl Default for CatalogSnapshotCache {
    fn default() -> Self {
        Self {
            entry: RwLock::new(None),
            rebuild: Mutex::new(()),
            revision: AtomicU64::new(0),
        }
    }
}

impl CatalogSnapshotCache {
    #[cfg(test)]
    pub(super) fn rebuild_guard(&self) -> std::sync::MutexGuard<'_, ()> {
        self.rebuild.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn snapshot(&self, context: &Context) -> Result<Arc<CatalogSnapshot>, ServiceError> {
        self.snapshot_with_consistency(context, false)
    }

    pub(super) fn refresh(&self, context: &Context) -> Result<Arc<CatalogSnapshot>, ServiceError> {
        self.snapshot_with_consistency(context, true)
    }

    pub(super) fn refresh_validated(
        &self,
        context: &Context,
    ) -> Result<(Arc<CatalogSnapshot>, Vec<GameId>), ServiceError> {
        let _rebuild_guard = self.rebuild.lock().unwrap_or_else(PoisonError::into_inner);

        loop {
            let build_generation = context.storage().catalog_generation();
            let build_start_revision = self.current_revision();
            let revision = self.next_revision();
            let mut validated = CatalogSnapshot::build_validated(context, revision)?;
            let final_generation = context.storage().catalog_generation();
            if final_generation != build_generation {
                log::debug!(
                    "catalog changed during live validation; retrying from revision={revision}"
                );
                continue;
            }

            let mut current_entry = self.entry.write().unwrap_or_else(PoisonError::into_inner);
            if context.storage().catalog_generation() != final_generation {
                drop(current_entry);
                log::debug!(
                    "catalog changed before live validation install; retrying from revision={revision}"
                );
                continue;
            }

            if let Some(current) = current_entry.as_ref()
                && Some(current.snapshot.revision()) != build_start_revision
            {
                validated = validated.preserving_cover_projection(&current.snapshot);
            }

            let changed_game_ids = current_entry.as_ref().map_or_else(
                || {
                    validated
                        .cards()
                        .iter()
                        .map(|card| card.game.id().clone())
                        .collect()
                },
                |current| current.snapshot.changed_game_ids(&validated),
            );
            let generation_changed = current_entry
                .as_ref()
                .is_none_or(|current| current.catalog_generation != Some(final_generation));

            context
                .game_details_cache
                .retain_generation(final_generation);

            if changed_game_ids.is_empty()
                && !generation_changed
                && let Some(current) = current_entry.as_mut()
            {
                current.catalog_generation = Some(final_generation);
                return Ok((Arc::clone(&current.snapshot), changed_game_ids));
            }

            let snapshot = Arc::new(validated);
            *current_entry = Some(CatalogSnapshotEntry {
                catalog_generation: Some(final_generation),
                snapshot: Arc::clone(&snapshot),
            });
            return Ok((snapshot, changed_game_ids));
        }
    }

    fn snapshot_with_consistency(
        &self,
        context: &Context,
        require_current_generation: bool,
    ) -> Result<Arc<CatalogSnapshot>, ServiceError> {
        let catalog_generation = context.storage().catalog_generation();
        if let Some(snapshot) = self.current(catalog_generation) {
            return Ok(snapshot);
        }

        let previous = self
            .entry
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|entry| Arc::clone(&entry.snapshot));

        let rebuild_guard = if require_current_generation {
            self.rebuild.lock().unwrap_or_else(PoisonError::into_inner)
        } else {
            match self.rebuild.try_lock() {
                Ok(guard) => guard,
                Err(TryLockError::Poisoned(error)) => error.into_inner(),
                Err(TryLockError::WouldBlock) => {
                    if let Some(previous) = previous {
                        return Ok(previous);
                    }
                    self.rebuild.lock().unwrap_or_else(PoisonError::into_inner)
                }
            }
        };

        let catalog_generation = context.storage().catalog_generation();
        if let Some(snapshot) = self.current(catalog_generation) {
            drop(rebuild_guard);
            return Ok(snapshot);
        }

        loop {
            let build_generation = context.storage().catalog_generation();
            let build_start_revision = self.current_revision();
            let revision = self.next_revision();
            let mut snapshot = match CatalogSnapshot::build(context, revision) {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    if !require_current_generation && let Some(previous) = previous.as_ref() {
                        log::warn!(
                            "catalog snapshot rebuild failed; serving revision={} until retry: {error}",
                            previous.revision()
                        );
                        return Ok(Arc::clone(previous));
                    }
                    return Err(error);
                }
            };
            let final_generation = context.storage().catalog_generation();

            if final_generation == build_generation {
                let mut current_entry = self.entry.write().unwrap_or_else(PoisonError::into_inner);
                if context.storage().catalog_generation() != final_generation {
                    drop(current_entry);
                    continue;
                }
                if let Some(current) = current_entry.as_ref()
                    && Some(current.snapshot.revision()) != build_start_revision
                {
                    snapshot = snapshot.preserving_cover_projection(&current.snapshot);
                }
                let snapshot = Arc::new(snapshot);
                context
                    .game_details_cache
                    .retain_generation(final_generation);
                *current_entry = Some(CatalogSnapshotEntry {
                    catalog_generation: Some(final_generation),
                    snapshot: Arc::clone(&snapshot),
                });
                return Ok(snapshot);
            }

            log::debug!("catalog changed during snapshot build; retrying from revision={revision}");
            if !require_current_generation && let Some(previous) = previous.as_ref() {
                return Ok(Arc::clone(previous));
            }
        }
    }

    fn current(&self, catalog_generation: u64) -> Option<Arc<CatalogSnapshot>> {
        self.entry
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .filter(|entry| entry.catalog_generation == Some(catalog_generation))
            .map(|entry| Arc::clone(&entry.snapshot))
    }

    fn current_revision(&self) -> Option<u64> {
        self.entry
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|entry| entry.snapshot.revision())
    }

    fn next_revision(&self) -> u64 {
        self.revision.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub(super) fn patch_cover(&self, game_id: &GameId, updated_at_ms: Option<i64>) {
        let mut entry = self.entry.write().unwrap_or_else(PoisonError::into_inner);
        let Some(current) = entry.as_mut() else {
            return;
        };
        if current.snapshot.card(game_id).is_none() {
            return;
        }
        let Some(snapshot) =
            current
                .snapshot
                .with_cover_patch(self.next_revision(), game_id, updated_at_ms)
        else {
            return;
        };
        current.snapshot = Arc::new(snapshot);
    }

    pub(super) fn patch_ui_state(
        &self,
        context: &Context,
        game_id: &GameId,
        is_favorite: bool,
        is_hidden: bool,
        generation_before: u64,
        generation_after: u64,
    ) {
        let mut entry = self.entry.write().unwrap_or_else(PoisonError::into_inner);
        let Some(current) = entry.as_mut() else {
            return;
        };
        if current.catalog_generation != Some(generation_before)
            || context.storage().catalog_generation() != generation_after
        {
            return;
        }
        let Some(snapshot) = current.snapshot.with_ui_state_patch(
            self.next_revision(),
            game_id,
            is_favorite,
            is_hidden,
        ) else {
            return;
        };
        current.catalog_generation = Some(generation_after);
        current.snapshot = Arc::new(snapshot);
    }

    pub(super) fn invalidate(&self) {
        if let Some(entry) = self
            .entry
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
        {
            entry.catalog_generation = None;
        }
    }
}
