import { setCatalogSetting, GAMES_FILTERS_CATALOG_SETTING_KEY } from '@entities/settings';
import { createGamesFilterPersistence, type PersistedGamesFilters } from './index-internal';
import {
  hasFilterIndicator as checkHasFilterIndicator,
  loadPersistedGamesFilters,
} from './games-filter-controller';
import { buildLibraryFilterOptions, groupLibraryFilterOptions } from './library-filter-options';
import { buildLauncherFilterOptions } from './launcher-filter-options';

import { GamesFiltersStore } from './games-filters-store.svelte';
import { setupGamesFiltersSync } from './games-filters-sync.svelte';

export type GamesFiltersModel = ReturnType<typeof createGamesFiltersModel>;

export type GamesFiltersModelInput = {
  getAvailableLibraries: () => readonly string[];
  getAvailableLaunchers: () => readonly string[];
};

export function createGamesFiltersModel(input: GamesFiltersModelInput) {
  const store = new GamesFiltersStore();

  let filterPreferenceLoaded = $state(false);
  let persistedFilters = $state<PersistedGamesFilters | null>(null);
  let filtersAnchorRef = $state<HTMLDivElement | null>(null);

  const filterPersistence = createGamesFilterPersistence({
    setCatalogSetting,
    settingKey: GAMES_FILTERS_CATALOG_SETTING_KEY,
  });

  const sync = setupGamesFiltersSync(store, filterPersistence, {
    getPreferenceLoaded: () => filterPreferenceLoaded,
    getPersistedFilters: () => persistedFilters,
    getAvailableLibraries: () => input.getAvailableLibraries(),
    getAvailableLaunchers: () => input.getAvailableLaunchers(),
  });

  const libraryFilterOptions = $derived(buildLibraryFilterOptions(input.getAvailableLibraries()));
  const groupedLibraryFilterOptions = $derived(groupLibraryFilterOptions(libraryFilterOptions));
  const launcherFilterOptions = $derived(buildLauncherFilterOptions(input.getAvailableLaunchers()));

  const hasFilterIndicator = $derived(
    checkHasFilterIndicator(
      store.state.searchQuery,
      store.state.appliedLibraries,
      input.getAvailableLibraries(),
      store.state.appliedLaunchers,
      input.getAvailableLaunchers(),
    ),
  );

  async function loadPreferences(isDisposed: () => boolean): Promise<void> {
    try {
      const value = await loadPersistedGamesFilters();

      if (isDisposed()) {
        return;
      }

      persistedFilters = value;
    } catch (error: unknown) {
      if (!isDisposed()) {
        console.error('Failed to load persisted game filters.', error);
      }
    } finally {
      if (!isDisposed()) {
        filterPreferenceLoaded = true;
      }
    }
  }

  return {
    get filtersState() {
      return store.state;
    },
    get filtersAnchorRef() {
      return filtersAnchorRef;
    },
    set filtersAnchorRef(value) {
      filtersAnchorRef = value;
    },
    get groupedLibraryFilterOptions() {
      return groupedLibraryFilterOptions;
    },
    get launcherFilterOptions() {
      return launcherFilterOptions;
    },
    get hasFilterIndicator() {
      return hasFilterIndicator;
    },
    loadPreferences,

    // Delegated Store Actions
    handleDialogOpenChange: (nextOpen: boolean) => {
      store.handleDialogOpenChange(nextOpen);
    },
    applyFilterSelection: () => {
      store.applyFilterSelection();
    },
    cancelFilterSelection: () => {
      store.cancelFilterSelection();
    },
    toggleFiltersDialog: () => {
      store.toggleFiltersDialog();
    },
    handleDraftLibrariesChange: (nextLibraries: readonly string[]) => {
      store.handleDraftLibrariesChange(nextLibraries);
    },
    handleDraftLaunchersChange: (nextLaunchers: readonly string[]) => {
      store.handleDraftLaunchersChange(nextLaunchers);
    },
    handleDraftLauncherOrderChange: (nextOrder: readonly string[]) => {
      store.handleDraftLauncherOrderChange(nextOrder);
    },
    quickToggleFavoritesOnly: () => {
      store.quickToggleFavoritesOnly();
    },
    quickToggleShowHidden: () => {
      store.quickToggleShowHidden();
    },
    resetFilters: () => {
      store.resetFilters();
    },
    setSearchQuery: (nextValue: string) => {
      store.setSearchQuery(nextValue);
    },

    // Delegated Sync Actions
    flushSearchPersist: () => {
      sync.flushSearchPersist();
    },
    dispose: () => {
      sync.dispose();
    },
  };
}
