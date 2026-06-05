import { shouldQueueAvailabilityPersist, syncGamesFilterState } from './games-filter-controller';
import type { GamesFiltersStore } from './games-filters-store.svelte';
import type { PersistedGamesFilters } from './index-internal';
import type { createGamesFilterPersistence } from './games-filter-persistence';

export type GamesFiltersSyncOptions = {
  getPreferenceLoaded: () => boolean;
  getPersistedFilters: () => PersistedGamesFilters | null;
  getAvailableLibraries: () => readonly string[];
  getAvailableLaunchers: () => readonly string[];
};

export function setupGamesFiltersSync(
  store: GamesFiltersStore,
  persistence: ReturnType<typeof createGamesFilterPersistence>,
  options: GamesFiltersSyncOptions,
) {
  let availabilityPersistSnapshot = '';

  // Effect 1: Availability & Hydration Sync
  $effect(() => {
    const syncResult = syncGamesFilterState(
      store.state,
      options.getPreferenceLoaded(),
      options.getPersistedFilters(),
      options.getAvailableLibraries(),
      options.getAvailableLaunchers(),
    );

    if (syncResult.state !== store.state) {
      store.setState(syncResult.state);
    }

    if (!syncResult.didAdjustApplied) {
      return;
    }

    const persistResult = shouldQueueAvailabilityPersist(
      syncResult.state,
      options.getPreferenceLoaded(),
      availabilityPersistSnapshot,
    );

    if (!persistResult.shouldQueue) {
      return;
    }

    availabilityPersistSnapshot = persistResult.nextSnapshot;

    void persistence
      .persistFilters({
        getState: () => store.state,
        setState: (next) => {
          store.setState(next);
        },
      })
      .catch((error: unknown) => {
        console.error('Failed to persist adjusted game filters.', error);
      })
      .finally(() => {
        if (availabilityPersistSnapshot === persistResult.nextSnapshot) {
          availabilityPersistSnapshot = '';
        }
      });
  });

  // Effect 2: User Action Sync
  let prevSearchQuery = store.state.searchQuery;
  let prevAppliedLibraries = store.state.appliedLibraries;
  let prevAppliedLaunchers = store.state.appliedLaunchers;
  let prevAppliedLauncherOrder = store.state.appliedLauncherOrder;
  let prevShowHidden = store.state.appliedShowHidden;
  let prevFavoritesOnly = store.state.appliedFavoritesOnly;

  $effect(() => {
    const s = store.state;

    const searchChanged = s.searchQuery !== prevSearchQuery;
    const appliedChanged =
      s.appliedLibraries !== prevAppliedLibraries ||
      s.appliedLaunchers !== prevAppliedLaunchers ||
      s.appliedLauncherOrder !== prevAppliedLauncherOrder ||
      s.appliedShowHidden !== prevShowHidden ||
      s.appliedFavoritesOnly !== prevFavoritesOnly;

    prevSearchQuery = s.searchQuery;
    prevAppliedLibraries = s.appliedLibraries;
    prevAppliedLaunchers = s.appliedLaunchers;
    prevAppliedLauncherOrder = s.appliedLauncherOrder;
    prevShowHidden = s.appliedShowHidden;
    prevFavoritesOnly = s.appliedFavoritesOnly;

    const ctx = {
      getState: () => store.state,
      setState: (next: typeof store.state) => {
        store.setState(next);
      },
    };

    if (appliedChanged) {
      void persistence.persistFilters(ctx).catch((error: unknown) => {
        console.error('Failed to persist user filter action.', error);
      });
    } else if (searchChanged) {
      persistence.queueSearchPersist(ctx);
    }
  });

  return {
    flushSearchPersist() {
      persistence.flushQueuedSearchPersist({
        getState: () => store.state,
        setState: (next) => {
          store.setState(next);
        },
      });
    },
    dispose() {
      persistence.dispose();
    },
  };
}
