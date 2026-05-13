import { shallowStringArrayEqual } from '@shared/text';
import { hasPartialLibrarySelection } from '@entities/game';
import { getCatalogSetting, GAMES_FILTERS_CATALOG_SETTING_KEY } from '@entities/settings';
import {
  createPersistedSnapshot,
  hydrateGamesFilterState,
  parsePersistedGamesFilters,
  type GamesFilterState,
  type PersistedGamesFilters,
  withAvailableLibraries,
} from './index-internal';

export type GamesFilterSyncResult = {
  state: GamesFilterState;
  didAdjustApplied: boolean;
};

export type AvailabilityPersistDecision = {
  shouldQueue: boolean;
  nextSnapshot: string;
};

export function syncGamesFilterState(
  state: GamesFilterState,
  preferenceLoaded: boolean,
  nextPersisted: PersistedGamesFilters | null,
  nextAvailable: readonly string[],
): GamesFilterSyncResult {
  const shouldHydrate = preferenceLoaded && !state.ready;

  const hydratedState = shouldHydrate
    ? hydrateGamesFilterState(state, nextPersisted, nextAvailable)
    : state;

  const nextState = withAvailableLibraries(hydratedState, nextAvailable);

  return {
    state: nextState,
    didAdjustApplied: hasAppliedLibrariesChanged(state, nextState),
  };
}

export function shouldQueueAvailabilityPersist(
  state: GamesFilterState,
  filterPreferenceLoaded: boolean,
  availabilityPersistSnapshot: string,
): AvailabilityPersistDecision {
  if (!canPersistAvailability(state, filterPreferenceLoaded)) {
    return {
      shouldQueue: false,
      nextSnapshot: availabilityPersistSnapshot,
    };
  }

  const nextSnapshot = createPersistedSnapshot(state);

  if (isKnownPersistedSnapshot(state, nextSnapshot, availabilityPersistSnapshot)) {
    return {
      shouldQueue: false,
      nextSnapshot: availabilityPersistSnapshot,
    };
  }

  return {
    shouldQueue: true,
    nextSnapshot,
  };
}

export async function loadPersistedGamesFilters(): Promise<PersistedGamesFilters | null> {
  try {
    const payload = await getCatalogSetting(GAMES_FILTERS_CATALOG_SETTING_KEY);

    return parsePersistedGamesFilters(payload.value);
  } catch {
    return null;
  }
}

export function hasFilterIndicator(
  searchQuery: string,
  appliedLibraries: readonly string[],
  availableLibraries: readonly string[],
): boolean {
  return (
    searchQuery.trim().length > 0 ||
    hasPartialLibrarySelection(appliedLibraries, availableLibraries)
  );
}

function hasAppliedLibrariesChanged(
  previousState: GamesFilterState,
  nextState: GamesFilterState,
): boolean {
  return !shallowStringArrayEqual(previousState.appliedLibraries, nextState.appliedLibraries);
}

function canPersistAvailability(state: GamesFilterState, filterPreferenceLoaded: boolean): boolean {
  return filterPreferenceLoaded && state.ready;
}

function isKnownPersistedSnapshot(
  state: GamesFilterState,
  nextSnapshot: string,
  availabilityPersistSnapshot: string,
): boolean {
  return (
    nextSnapshot === state.lastPersistedSnapshot || nextSnapshot === availabilityPersistSnapshot
  );
}
