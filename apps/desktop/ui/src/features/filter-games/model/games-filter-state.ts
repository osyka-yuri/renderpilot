import { shallowStringArrayEqual } from '@shared/text';
import { intersectLibraries, normalizeLibraryValues, normalizeLauncherValues } from '@entities/game';
import { encodePersistedGamesFilters, type PersistedGamesFilters } from './filter-persistence';
import {
  canonicalizeSelection,
  createAvailableSliceUpdate,
  createHydratedSlice,
  applySliceUpdate,
  type FilterSlice,
  type FilterSliceUpdate,
} from './filter-slice';

export type GamesFilterState = {
  ready: boolean;
  isDialogOpen: boolean;
  searchQuery: string;
  appliedLibraries: string[];
  draftLibraries: string[];
  deferSelectAllLibraries: boolean;
  pendingPersistedLibraries: string[] | null;
  availableLibraries: string[];
  appliedLaunchers: string[];
  draftLaunchers: string[];
  deferSelectAllLaunchers: boolean;
  pendingPersistedLaunchers: string[] | null;
  availableLaunchers: string[];
  lastPersistedSnapshot: string;
};

const EMPTY_SEARCH_QUERY = '';
const EMPTY_PERSISTED_SNAPSHOT = '';

export function createInitialGamesFilterState(): GamesFilterState {
  return {
    ready: false,
    isDialogOpen: false,
    searchQuery: EMPTY_SEARCH_QUERY,
    appliedLibraries: [],
    draftLibraries: [],
    deferSelectAllLibraries: false,
    pendingPersistedLibraries: null,
    availableLibraries: [],
    appliedLaunchers: [],
    draftLaunchers: [],
    deferSelectAllLaunchers: false,
    pendingPersistedLaunchers: null,
    availableLaunchers: [],
    lastPersistedSnapshot: EMPTY_PERSISTED_SNAPSHOT,
  };
}

export function hydrateGamesFilterState(
  state: GamesFilterState,
  persisted: PersistedGamesFilters | null,
  availableLibraries: readonly string[],
  availableLaunchers: readonly string[],
): GamesFilterState {
  const availableLibrariesSnapshot = copyStringArray(availableLibraries);
  const availableLaunchersSnapshot = copyStringArray(availableLaunchers);

  const hydratedState: GamesFilterState = {
    ...state,
    ready: true,
    searchQuery: persisted?.searchQuery ?? EMPTY_SEARCH_QUERY,
    availableLibraries: availableLibrariesSnapshot,
    ...createHydratedSliceFilters(
      persisted?.libraries ?? null,
      availableLibrariesSnapshot,
      normalizeLibraryValues,
      LIBRARY_ADAPTER,
    ),
    availableLaunchers: availableLaunchersSnapshot,
    ...createHydratedSliceFilters(
      persisted?.launchers ?? null,
      availableLaunchersSnapshot,
      normalizeLauncherValues,
      LAUNCHER_ADAPTER,
    ),
  };

  return {
    ...hydratedState,
    lastPersistedSnapshot: createPersistedSnapshot(hydratedState),
  };
}

export function withAvailableCatalogFilters(
  state: GamesFilterState,
  availableLibraries: readonly string[],
  availableLaunchers: readonly string[],
): GamesFilterState {
  const availableLibrariesSnapshot = copyStringArray(availableLibraries);
  const availableLaunchersSnapshot = copyStringArray(availableLaunchers);

  let nextState = withAvailableSnapshot(state, availableLibrariesSnapshot, 'availableLibraries');
  nextState = withAvailableSnapshot(nextState, availableLaunchersSnapshot, 'availableLaunchers');

  if (!nextState.ready) {
    return nextState;
  }

  const libraryUpdate = createAvailableSliceFiltersUpdate(
    nextState,
    availableLibrariesSnapshot,
    normalizeLibraryValues,
    LIBRARY_ADAPTER,
  );
  nextState = applySliceUpdate(nextState, libraryUpdate);

  const launcherUpdate = createAvailableSliceFiltersUpdate(
    nextState,
    availableLaunchersSnapshot,
    normalizeLauncherValues,
    LAUNCHER_ADAPTER,
  );
  nextState = applySliceUpdate(nextState, launcherUpdate);

  return nextState;
}

export function withSearchQuery(state: GamesFilterState, searchQuery: string): GamesFilterState {
  if (state.searchQuery === searchQuery) {
    return state;
  }

  return {
    ...state,
    searchQuery,
  };
}

export function openFilterDialog(state: GamesFilterState): GamesFilterState {
  if (
    state.isDialogOpen &&
    shallowStringArrayEqual(state.draftLibraries, state.appliedLibraries) &&
    shallowStringArrayEqual(state.draftLaunchers, state.appliedLaunchers)
  ) {
    return state;
  }

  return {
    ...state,
    draftLibraries: copyStringArray(state.appliedLibraries),
    draftLaunchers: copyStringArray(state.appliedLaunchers),
    isDialogOpen: true,
  };
}

export function cancelFilterDialog(state: GamesFilterState): GamesFilterState {
  if (
    !state.isDialogOpen &&
    shallowStringArrayEqual(state.draftLibraries, state.appliedLibraries) &&
    shallowStringArrayEqual(state.draftLaunchers, state.appliedLaunchers)
  ) {
    return state;
  }

  return {
    ...state,
    draftLibraries: copyStringArray(state.appliedLibraries),
    draftLaunchers: copyStringArray(state.appliedLaunchers),
    isDialogOpen: false,
  };
}

export function setDraftLibraries(
  state: GamesFilterState,
  libraries: readonly string[],
): GamesFilterState {
  const normalized = canonicalizeLibraries(libraries, state.availableLibraries);

  if (shallowStringArrayEqual(state.draftLibraries, normalized)) {
    return state;
  }

  return {
    ...state,
    draftLibraries: normalized,
  };
}

export function setDraftLaunchers(
  state: GamesFilterState,
  launchers: readonly string[],
): GamesFilterState {
  const normalized = canonicalizeLaunchers(launchers, state.availableLaunchers);

  if (shallowStringArrayEqual(state.draftLaunchers, normalized)) {
    return state;
  }

  return {
    ...state,
    draftLaunchers: normalized,
  };
}

export function applyDraftFilters(state: GamesFilterState): GamesFilterState {
  const appliedLibraries = canonicalizeLibraries(state.draftLibraries, state.availableLibraries);
  const appliedLaunchers = canonicalizeLaunchers(state.draftLaunchers, state.availableLaunchers);

  if (
    !state.isDialogOpen &&
    shallowStringArrayEqual(state.appliedLibraries, appliedLibraries) &&
    shallowStringArrayEqual(state.draftLibraries, appliedLibraries) &&
    shallowStringArrayEqual(state.appliedLaunchers, appliedLaunchers) &&
    shallowStringArrayEqual(state.draftLaunchers, appliedLaunchers)
  ) {
    return state;
  }

  return {
    ...state,
    appliedLibraries,
    draftLibraries: copyStringArray(appliedLibraries),
    appliedLaunchers,
    draftLaunchers: copyStringArray(appliedLaunchers),
    isDialogOpen: false,
  };
}

export function createPersistedSnapshot(state: GamesFilterState): string {
  return encodePersistedGamesFilters({
    libraries: state.appliedLibraries,
    launchers: state.appliedLaunchers,
    searchQuery: state.searchQuery,
  });
}

export function commitPersistedSnapshot(
  state: GamesFilterState,
  snapshot: string,
): GamesFilterState {
  if (state.lastPersistedSnapshot === snapshot) {
    return state;
  }

  return {
    ...state,
    lastPersistedSnapshot: snapshot,
  };
}

/**
 * True when `snapshot` still matches current applied filters,
 * so it is safe to mark it as persisted on disk.
 */
export function isPersistedSnapshotStillCurrent(
  state: GamesFilterState,
  snapshot: string,
): boolean {
  return createPersistedSnapshot(state) === snapshot;
}

// ---------------------------------------------------------------------------
// Slice adapters
// ---------------------------------------------------------------------------

type SliceFieldMapping = {
  applied: 'appliedLibraries' | 'appliedLaunchers';
  draft: 'draftLibraries' | 'draftLaunchers';
  deferSelectAll: 'deferSelectAllLibraries' | 'deferSelectAllLaunchers';
  pendingPersisted: 'pendingPersistedLibraries' | 'pendingPersistedLaunchers';
};

function createSliceAdapter(fields: SliceFieldMapping) {
  function fromState(state: GamesFilterState): FilterSlice {
    return {
      applied: state[fields.applied],
      draft: state[fields.draft],
      deferSelectAll: state[fields.deferSelectAll],
      pendingPersisted: state[fields.pendingPersisted],
    };
  }

  function toStateUpdate(slice: FilterSlice | FilterSliceUpdate): Record<string, unknown> {
    const result: Record<string, unknown> = {};

    if ('applied' in slice) result[fields.applied] = slice.applied;
    if ('draft' in slice) result[fields.draft] = slice.draft;
    if ('deferSelectAll' in slice) result[fields.deferSelectAll] = slice.deferSelectAll;
    if ('pendingPersisted' in slice) result[fields.pendingPersisted] = slice.pendingPersisted;

    return result;
  }

  return { fromState, toStateUpdate };
}

const LIBRARY_ADAPTER = createSliceAdapter({
  applied: 'appliedLibraries',
  draft: 'draftLibraries',
  deferSelectAll: 'deferSelectAllLibraries',
  pendingPersisted: 'pendingPersistedLibraries',
});

const LAUNCHER_ADAPTER = createSliceAdapter({
  applied: 'appliedLaunchers',
  draft: 'draftLaunchers',
  deferSelectAll: 'deferSelectAllLaunchers',
  pendingPersisted: 'pendingPersistedLaunchers',
});

// ---------------------------------------------------------------------------
// Canonicalization
// ---------------------------------------------------------------------------

function createCanonicalizeFn(
  normalizeFn: (values: readonly string[]) => string[],
): (selected: readonly string[], available: readonly string[]) => string[] {
  return (selected, available) => canonicalizeSelection(selected, available, normalizeFn, intersectLibraries);
}

const canonicalizeLibraries = createCanonicalizeFn(normalizeLibraryValues);
const canonicalizeLaunchers = createCanonicalizeFn(normalizeLauncherValues);

// ---------------------------------------------------------------------------
// Hydration helpers
// ---------------------------------------------------------------------------

function createHydratedSliceFilters(
  persistedValues: readonly string[] | null,
  availableValues: string[],
  normalizeFn: (values: readonly string[]) => string[],
  adapter: ReturnType<typeof createSliceAdapter>,
): Record<string, unknown> {
  const canonicalizeFn = createCanonicalizeFn(normalizeFn);
  const slice = createHydratedSlice(persistedValues, availableValues, normalizeFn, canonicalizeFn);

  return adapter.toStateUpdate(slice);
}

// ---------------------------------------------------------------------------
// Availability helpers
// ---------------------------------------------------------------------------

function createAvailableSliceFiltersUpdate(
  state: GamesFilterState,
  availableValues: string[],
  normalizeFn: (values: readonly string[]) => string[],
  adapter: ReturnType<typeof createSliceAdapter>,
): Record<string, unknown> | null {
  const canonicalizeFn = createCanonicalizeFn(normalizeFn);
  const update = createAvailableSliceUpdate(
    adapter.fromState(state),
    availableValues,
    canonicalizeFn,
  );

  return update ? adapter.toStateUpdate(update) : null;
}

function withAvailableSnapshot(
  state: GamesFilterState,
  availableValues: string[],
  field: 'availableLibraries' | 'availableLaunchers',
): GamesFilterState {
  if (shallowStringArrayEqual(state[field], availableValues)) {
    return state;
  }

  return { ...state, [field]: availableValues };
}

// ---------------------------------------------------------------------------
// String array copy helpers
// ---------------------------------------------------------------------------

function copyStringArray(values: readonly string[]): string[] {
  return [...values];
}
