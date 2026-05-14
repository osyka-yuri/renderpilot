import { setCatalogSetting, GAMES_FILTERS_CATALOG_SETTING_KEY } from '@entities/settings';
import {
  applyDraftFilters,
  cancelFilterDialog,
  createGamesFilterPersistence,
  createInitialGamesFilterState,
  openFilterDialog,
  setDraftLibraries,
  setDraftLaunchers,
  setDraftLauncherOrder,
  type PersistedGamesFilters,
  withSearchQuery,
} from './index-internal';
import {
  hasFilterIndicator as checkHasFilterIndicator,
  loadPersistedGamesFilters,
  shouldQueueAvailabilityPersist,
  syncGamesFilterState,
} from './games-filter-controller';
import { buildLibraryFilterOptions, groupLibraryFilterOptions } from './library-filter-options';
import { buildLauncherFilterOptions } from './launcher-filter-options';

export type GamesFiltersModel = ReturnType<typeof createGamesFiltersModel>;

export type GamesFiltersModelInput = {
  getAvailableLibraries: () => readonly string[];
  getAvailableLaunchers: () => readonly string[];
};

export function createGamesFiltersModel(input: GamesFiltersModelInput) {
  let filtersState = $state(createInitialGamesFilterState());
  let filterPreferenceLoaded = $state(false);
  let persistedFilters = $state<PersistedGamesFilters | null>(null);
  let filtersAnchorRef = $state<HTMLDivElement | null>(null);
  let availabilityPersistSnapshot = $state('');

  const filterPersistence = createGamesFilterPersistence({
    setCatalogSetting,
    settingKey: GAMES_FILTERS_CATALOG_SETTING_KEY,
  });

  const libraryFilterOptions = $derived(buildLibraryFilterOptions(input.getAvailableLibraries()));

  const groupedLibraryFilterOptions = $derived(groupLibraryFilterOptions(libraryFilterOptions));

  const launcherFilterOptions = $derived(buildLauncherFilterOptions(input.getAvailableLaunchers()));

  const hasFilterIndicator = $derived(
    checkHasFilterIndicator(
      filtersState.searchQuery,
      filtersState.appliedLibraries,
      input.getAvailableLibraries(),
      filtersState.appliedLaunchers,
      input.getAvailableLaunchers(),
    ),
  );

  $effect(() => {
    const syncResult = syncGamesFilterState(
      filtersState,
      filterPreferenceLoaded,
      persistedFilters,
      input.getAvailableLibraries(),
      input.getAvailableLaunchers(),
    );

    if (syncResult.state !== filtersState) {
      filtersState = syncResult.state;
    }

    if (!syncResult.didAdjustApplied) {
      return;
    }

    const persistResult = shouldQueueAvailabilityPersist(
      syncResult.state,
      filterPreferenceLoaded,
      availabilityPersistSnapshot,
    );

    if (!persistResult.shouldQueue) {
      return;
    }

    availabilityPersistSnapshot = persistResult.nextSnapshot;

    void filterPersistence
      .persistFilters(createPersistenceContext())
      .catch((error: unknown) => {
        console.error('Failed to persist adjusted game filters.', error);
      })
      .finally(() => {
        if (availabilityPersistSnapshot === persistResult.nextSnapshot) {
          availabilityPersistSnapshot = '';
        }
      });
  });

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

  function createPersistenceContext() {
    return {
      getState: () => filtersState,
      setState: (next: typeof filtersState) => {
        filtersState = next;
      },
    };
  }

  function handleDialogOpenChange(nextOpen: boolean): void {
    if (nextOpen) {
      filtersState = openFilterDialog(filtersState);
      return;
    }

    filtersState = cancelFilterDialog(filtersState);
  }

  async function commitFilterSelection(): Promise<void> {
    filtersState = applyDraftFilters(filtersState);

    try {
      await filterPersistence.persistFilters(createPersistenceContext());
    } catch (error: unknown) {
      console.error('Failed to persist selected game filters.', error);
    }
  }

  function applyFilterSelection(): void {
    void commitFilterSelection();
  }

  function cancelFilterSelection(): void {
    filtersState = cancelFilterDialog(filtersState);
  }

  function toggleFiltersDialog(): void {
    handleDialogOpenChange(!filtersState.isDialogOpen);
  }

  function handleDraftLibrariesChange(nextLibraries: readonly string[]): void {
    filtersState = setDraftLibraries(filtersState, nextLibraries);
  }

  function handleDraftLaunchersChange(nextLaunchers: readonly string[]): void {
    filtersState = setDraftLaunchers(filtersState, nextLaunchers);
  }

  function handleDraftLauncherOrderChange(nextOrder: readonly string[]): void {
    filtersState = setDraftLauncherOrder(filtersState, nextOrder);
  }

  function setSearchQuery(nextValue: string): void {
    const nextState = withSearchQuery(filtersState, nextValue);

    if (nextState === filtersState) {
      return;
    }

    filtersState = nextState;
    filterPersistence.queueSearchPersist(createPersistenceContext());
  }

  function flushSearchPersist(): void {
    filterPersistence.flushQueuedSearchPersist(createPersistenceContext());
  }

  function dispose(): void {
    filterPersistence.dispose();
  }

  return {
    get filtersState() {
      return filtersState;
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
    handleDialogOpenChange,
    applyFilterSelection,
    cancelFilterSelection,
    toggleFiltersDialog,
    handleDraftLibrariesChange,
    handleDraftLaunchersChange,
    handleDraftLauncherOrderChange,
    setSearchQuery,
    flushSearchPersist,
    dispose,
  };
}
