import { getCatalogSetting } from '@shared/api/desktop';
import {
  GAMES_FILTERS_CATALOG_SETTING_KEY,
  hasPartialLibrarySelection,
  parsePersistedGamesFilters,
  shallowStringArrayEqual,
  type PersistedGamesFilters,
} from '@features/games/games-screen-filters';
import {
  createPersistedSnapshot,
  hydrateGamesFilterState,
  type GamesFilterState,
  withAvailableLibraries,
} from '@features/games/games-filter-state';

export type GamesFilterSyncResult = {
  state: GamesFilterState;
  didAdjustApplied: boolean;
};

export type AvailabilityPersistDecision = {
  shouldQueue: boolean;
  nextSnapshot: string;
};

export type CoverMenuRefs<T> = Record<string, T | undefined>;

export type PrunedCoverMenuState<T> = {
  refs: CoverMenuRefs<T>;
  menuOpenFor: string | null;
};

export type ManualCoverBusyParams = {
  gameId: string;
  manualCoverBusyFor: string | null;
  setManualCoverBusyFor: (gameId: string | null) => void;
  task: () => Promise<unknown>;
  onClearError: () => void;
  onReloadCards: () => Promise<void>;
  onCoverError: (message: string) => void;
  describeError: (error: unknown) => string;
  focusMenuTrigger: (gameId: string) => void;
};

export { buildGameCardsQueryKey } from '@features/games/games-screen-filters';

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

export async function withManualCoverBusy({
  gameId,
  manualCoverBusyFor,
  setManualCoverBusyFor,
  task,
  onClearError,
  onReloadCards,
  onCoverError,
  describeError,
  focusMenuTrigger,
}: ManualCoverBusyParams): Promise<void> {
  if (manualCoverBusyFor !== null) {
    return;
  }

  setManualCoverBusyFor(gameId);

  try {
    await task();

    onClearError();
    await onReloadCards();
  } catch (error: unknown) {
    onCoverError(describeError(error));
  } finally {
    setManualCoverBusyFor(null);
    focusMenuTrigger(gameId);
  }
}

export function isCoverOperationBusy(
  gameId: string,
  manualCoverBusyFor: string | null,
  coversAutoFetchingIds: ReadonlySet<string>,
): boolean {
  return manualCoverBusyFor === gameId || coversAutoFetchingIds.has(gameId);
}

export function shouldCloseOpenMenu(
  menuOpenFor: string | null,
  manualCoverBusyFor: string | null,
  coversAutoFetchingIds: ReadonlySet<string>,
): boolean {
  if (menuOpenFor === null) {
    return false;
  }

  const hasManualCoverOperation = manualCoverBusyFor !== null;
  const hasAutoCoverOperationForOpenMenu = coversAutoFetchingIds.has(menuOpenFor);

  return hasManualCoverOperation || hasAutoCoverOperationForOpenMenu;
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

export function pruneCoverMenuState<T>(
  refs: CoverMenuRefs<T>,
  menuOpenFor: string | null,
  activeGameIds: readonly string[],
): PrunedCoverMenuState<T> {
  const activeGameIdsSet = new Set(activeGameIds);

  let didPruneRefs = false;
  const nextRefs: CoverMenuRefs<T> = {};

  for (const [gameId, menuRef] of Object.entries(refs)) {
    if (activeGameIdsSet.has(gameId)) {
      nextRefs[gameId] = menuRef;
      continue;
    }

    didPruneRefs = true;
  }

  const nextMenuOpenFor = isActiveGameId(menuOpenFor, activeGameIdsSet) ? menuOpenFor : null;

  return {
    refs: didPruneRefs ? nextRefs : refs,
    menuOpenFor: nextMenuOpenFor,
  };
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

function isActiveGameId(
  gameId: string | null,
  activeGameIds: ReadonlySet<string>,
): gameId is string {
  return gameId !== null && activeGameIds.has(gameId);
}
