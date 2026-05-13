import { setCatalogSetting, GAMES_FILTERS_CATALOG_SETTING_KEY } from '@entities/settings';
import {
  applyDraftFilters,
  cancelFilterPopover,
  createGamesFilterPersistence,
  createInitialGamesFilterState,
  openFilterPopover,
  setDraftLibraries,
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

export type GamesFiltersModel = ReturnType<typeof createGamesFiltersModel>;

export type GamesFiltersModelInput = {
  getAvailableLibraries: () => readonly string[];
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

  const hasFilterIndicator = $derived(
    checkHasFilterIndicator(
      filtersState.searchQuery,
      filtersState.appliedLibraries,
      input.getAvailableLibraries(),
    ),
  );

  $effect(() => {
    const syncResult = syncGamesFilterState(
      filtersState,
      filterPreferenceLoaded,
      persistedFilters,
      input.getAvailableLibraries(),
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

  function handlePopoverOpenChange(nextOpen: boolean): void {
    if (nextOpen) {
      filtersState = openFilterPopover(filtersState);
      return;
    }

    filtersState = cancelFilterPopover(filtersState);
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
    filtersState = cancelFilterPopover(filtersState);
  }

  function toggleFiltersPopover(): void {
    handlePopoverOpenChange(!filtersState.isPopoverOpen);
  }

  function handleDraftLibrariesChange(nextLibraries: readonly string[]): void {
    filtersState = setDraftLibraries(filtersState, nextLibraries);
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
    get hasFilterIndicator() {
      return hasFilterIndicator;
    },
    loadPreferences,
    handlePopoverOpenChange,
    applyFilterSelection,
    cancelFilterSelection,
    toggleFiltersPopover,
    handleDraftLibrariesChange,
    setSearchQuery,
    flushSearchPersist,
    dispose,
  };
}
