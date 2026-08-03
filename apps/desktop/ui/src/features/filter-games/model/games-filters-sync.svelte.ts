import { reportClientError } from '@shared/errors';
import { shouldQueueAvailabilityPersist, syncGamesFilterState } from './games-filter-controller';
import type { GamesFiltersStore } from './games-filters-store.svelte';
import type { PersistedGamesFilters } from './index-internal';
import type { createGamesFilterPersistence } from './games-filter-persistence';

export type GamesFiltersSyncOptions = {
  getPreferenceLoaded: () => boolean;
  getPersistedFilters: () => PersistedGamesFilters | null;
  getAvailableLibraries: () => readonly string[];
  getAvailableAddons: () => readonly string[];
  getAvailableLaunchers: () => readonly string[];
};

export function setupGamesFiltersSync(
  store: GamesFiltersStore,
  persistence: ReturnType<typeof createGamesFilterPersistence>,
  options: GamesFiltersSyncOptions,
) {
  let availabilityPersistSnapshot = '';
  let prevSearchQuery = store.state.searchQuery;
  let prevAppliedLibraries = store.state.appliedLibraries;
  let prevAppliedAddons = store.state.appliedAddons;
  let prevAppliedLaunchers = store.state.appliedLaunchers;
  let prevAppliedLauncherOrder = store.state.appliedLauncherOrder;
  let prevShowHidden = store.state.appliedShowHidden;
  let prevFavoritesOnly = store.state.appliedFavoritesOnly;

  function rememberUserState(state: typeof store.state): void {
    prevSearchQuery = state.searchQuery;
    prevAppliedLibraries = state.appliedLibraries;
    prevAppliedAddons = state.appliedAddons;
    prevAppliedLaunchers = state.appliedLaunchers;
    prevAppliedLauncherOrder = state.appliedLauncherOrder;
    prevShowHidden = state.appliedShowHidden;
    prevFavoritesOnly = state.appliedFavoritesOnly;
  }

  function synchronizeAvailability(): void {
    const syncResult = syncGamesFilterState(
      store.state,
      options.getPreferenceLoaded(),
      options.getPersistedFilters(),
      options.getAvailableLibraries(),
      options.getAvailableLaunchers(),
      options.getAvailableAddons(),
    );

    if (syncResult.state !== store.state) {
      // Hydration and availability reconciliation are system transitions, not
      // user actions. Advance the user-action baseline before publishing the
      // state so the second effect does not write the same preferences back.
      rememberUserState(syncResult.state);
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
        reportClientError('persist_adjusted_game_filters', error);
      })
      .finally(() => {
        if (availabilityPersistSnapshot === persistResult.nextSnapshot) {
          availabilityPersistSnapshot = '';
        }
      });
  }

  // Effect 1: Availability & Hydration Sync
  $effect(synchronizeAvailability);

  // Effect 2: User Action Sync
  $effect(() => {
    const s = store.state;

    const searchChanged = s.searchQuery !== prevSearchQuery;
    const appliedChanged =
      s.appliedLibraries !== prevAppliedLibraries ||
      s.appliedAddons !== prevAppliedAddons ||
      s.appliedLaunchers !== prevAppliedLaunchers ||
      s.appliedLauncherOrder !== prevAppliedLauncherOrder ||
      s.appliedShowHidden !== prevShowHidden ||
      s.appliedFavoritesOnly !== prevFavoritesOnly;

    prevSearchQuery = s.searchQuery;
    prevAppliedLibraries = s.appliedLibraries;
    prevAppliedAddons = s.appliedAddons;
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
        reportClientError('persist_user_filter_action', error);
      });
    } else if (searchChanged) {
      persistence.queueSearchPersist(ctx);
    }
  });

  return {
    synchronizeAvailability,
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
