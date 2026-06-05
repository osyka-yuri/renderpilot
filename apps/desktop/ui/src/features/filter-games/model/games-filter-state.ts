import { shallowStringArrayEqual } from '@shared/text';
import {
  intersectLibraries,
  normalizeLibraryValues,
  normalizeLauncherValues,
} from '@entities/game';
import { encodePersistedGamesFilters, type PersistedGamesFilters } from './filter-persistence';
import { canonicalizeLauncherOrder, buildInitialLauncherOrder } from './launcher-order';
import {
  canonicalizeSelection,
  createAvailableSliceUpdate,
  createHydratedSlice,
  type FilterSlice,
  type FilterSliceUpdate,
} from './filter-slice';

export type GamesFilterState = {
  ready: boolean;
  isDialogOpen: boolean;
  searchQuery: string;

  appliedShowHidden: boolean;
  draftShowHidden: boolean;

  appliedFavoritesOnly: boolean;
  draftFavoritesOnly: boolean;

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

  appliedLauncherOrder: string[];
  draftLauncherOrder: string[];

  lastPersistedSnapshot: string;
};

const EMPTY_SEARCH_QUERY = '';
const EMPTY_PERSISTED_SNAPSHOT = '';

type Normalizer = (values: readonly string[]) => string[];
type Canonicalizer = (selected: readonly string[], available: readonly string[]) => string[];

type DraftFilterFields = Pick<
  GamesFilterState,
  | 'draftLibraries'
  | 'draftLaunchers'
  | 'draftLauncherOrder'
  | 'draftShowHidden'
  | 'draftFavoritesOnly'
>;

type AppliedFilterFields = Pick<
  GamesFilterState,
  | 'appliedLibraries'
  | 'appliedLaunchers'
  | 'appliedLauncherOrder'
  | 'appliedShowHidden'
  | 'appliedFavoritesOnly'
>;

export function createInitialGamesFilterState(): GamesFilterState {
  return {
    ready: false,
    isDialogOpen: false,

    searchQuery: EMPTY_SEARCH_QUERY,

    appliedShowHidden: false,
    draftShowHidden: false,

    appliedFavoritesOnly: false,
    draftFavoritesOnly: false,

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

    appliedLauncherOrder: [],
    draftLauncherOrder: [],

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

  const initialLauncherOrder = buildInitialLauncherOrder(
    persisted?.launcherOrder ?? null,
    availableLaunchersSnapshot,
  );

  const hydratedState: GamesFilterState = {
    ...state,
    ready: true,
    // Force-close any dialog that may have been open before hydration ran,
    // so the user never sees stale draft values on first render.
    isDialogOpen: false,

    searchQuery: persisted?.searchQuery ?? EMPTY_SEARCH_QUERY,

    appliedShowHidden: persisted?.showHidden ?? false,
    draftShowHidden: persisted?.showHidden ?? false,

    appliedFavoritesOnly: persisted?.favoritesOnly ?? false,
    draftFavoritesOnly: persisted?.favoritesOnly ?? false,

    availableLibraries: availableLibrariesSnapshot,
    ...createHydratedSliceFilters(
      persisted?.libraries ?? null,
      availableLibrariesSnapshot,
      normalizeLibraryValues,
      canonicalizeLibraries,
      LIBRARY_ADAPTER,
    ),

    availableLaunchers: availableLaunchersSnapshot,
    ...createHydratedSliceFilters(
      persisted?.launchers ?? null,
      availableLaunchersSnapshot,
      normalizeLauncherValues,
      canonicalizeLaunchers,
      LAUNCHER_ADAPTER,
    ),

    appliedLauncherOrder: initialLauncherOrder,
    draftLauncherOrder: copyStringArray(initialLauncherOrder),
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

  let nextState = withAvailableSnapshots(
    state,
    availableLibrariesSnapshot,
    availableLaunchersSnapshot,
  );

  if (!nextState.ready) {
    return nextState;
  }

  nextState = applyStateUpdate(
    nextState,
    createAvailableSliceFiltersUpdate(
      nextState,
      availableLibrariesSnapshot,
      canonicalizeLibraries,
      LIBRARY_ADAPTER,
    ),
  );

  nextState = applyStateUpdate(
    nextState,
    createAvailableSliceFiltersUpdate(
      nextState,
      availableLaunchersSnapshot,
      canonicalizeLaunchers,
      LAUNCHER_ADAPTER,
    ),
  );

  return reconcileLauncherOrderWithAvailability(nextState, availableLaunchersSnapshot);
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
  // Важно: повторный open не должен сбрасывать уже измененный draft.
  if (state.isDialogOpen) {
    return state;
  }

  return {
    ...state,
    ...createDraftsFromApplied(state),
    isDialogOpen: true,
  };
}

export function cancelFilterDialog(state: GamesFilterState): GamesFilterState {
  if (!state.isDialogOpen && areDraftsEqualToApplied(state)) {
    return state;
  }

  return {
    ...state,
    ...createDraftsFromApplied(state),
    isDialogOpen: false,
  };
}

export function setDraftLibraries(
  state: GamesFilterState,
  libraries: readonly string[],
): GamesFilterState {
  const draftLibraries = canonicalizeLibraries(libraries, state.availableLibraries);

  if (shallowStringArrayEqual(state.draftLibraries, draftLibraries)) {
    return state;
  }

  return {
    ...state,
    draftLibraries,
  };
}

export function setDraftLaunchers(
  state: GamesFilterState,
  launchers: readonly string[],
): GamesFilterState {
  const draftLaunchers = canonicalizeLaunchers(launchers, state.availableLaunchers);

  if (shallowStringArrayEqual(state.draftLaunchers, draftLaunchers)) {
    return state;
  }

  return {
    ...state,
    draftLaunchers,
  };
}

export function setDraftLauncherOrder(
  state: GamesFilterState,
  order: readonly string[],
): GamesFilterState {
  const draftLauncherOrder = canonicalizeLauncherOrder(order, state.availableLaunchers);

  if (shallowStringArrayEqual(state.draftLauncherOrder, draftLauncherOrder)) {
    return state;
  }

  return {
    ...state,
    draftLauncherOrder,
  };
}

export function setDraftShowHidden(state: GamesFilterState, value: boolean): GamesFilterState {
  if (state.draftShowHidden === value) {
    return state;
  }
  return { ...state, draftShowHidden: value };
}

export function setDraftFavoritesOnly(state: GamesFilterState, value: boolean): GamesFilterState {
  if (state.draftFavoritesOnly === value) {
    return state;
  }
  return { ...state, draftFavoritesOnly: value };
}

/**
 * Atomically toggles `appliedFavoritesOnly` and syncs the draft.
 * Use this for toolbar quick-toggles that bypass the dialog draft flow.
 */
export function toggleAppliedFavoritesOnly(state: GamesFilterState): GamesFilterState {
  const next = !state.appliedFavoritesOnly;
  return { ...state, appliedFavoritesOnly: next, draftFavoritesOnly: next };
}

/**
 * Atomically toggles `appliedShowHidden` and syncs the draft.
 * Use this for toolbar quick-toggles that bypass the dialog draft flow.
 */
export function toggleAppliedShowHidden(state: GamesFilterState): GamesFilterState {
  const next = !state.appliedShowHidden;
  return { ...state, appliedShowHidden: next, draftShowHidden: next };
}

export function applyDraftFilters(state: GamesFilterState): GamesFilterState {
  const appliedFilters = createAppliedFiltersFromDrafts(state);

  if (!state.isDialogOpen && areAppliedFiltersEqual(state, appliedFilters)) {
    return state;
  }

  return {
    ...state,
    ...appliedFilters,

    draftLibraries: copyStringArray(appliedFilters.appliedLibraries),
    draftLaunchers: copyStringArray(appliedFilters.appliedLaunchers),
    draftLauncherOrder: copyStringArray(appliedFilters.appliedLauncherOrder),
    draftShowHidden: appliedFilters.appliedShowHidden,
    draftFavoritesOnly: appliedFilters.appliedFavoritesOnly,

    isDialogOpen: false,
  };
}

export function createPersistedSnapshot(state: GamesFilterState): string {
  return encodePersistedGamesFilters({
    libraries: state.appliedLibraries,
    launchers: state.appliedLaunchers,
    launcherOrder: state.appliedLauncherOrder,
    searchQuery: state.searchQuery,
    showHidden: state.appliedShowHidden,
    favoritesOnly: state.appliedFavoritesOnly,
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
// Draft / applied helpers
// ---------------------------------------------------------------------------

function createDraftsFromApplied(state: GamesFilterState): DraftFilterFields {
  return {
    draftLibraries: copyStringArray(state.appliedLibraries),
    draftLaunchers: copyStringArray(state.appliedLaunchers),
    draftLauncherOrder: copyStringArray(state.appliedLauncherOrder),
    draftShowHidden: state.appliedShowHidden,
    draftFavoritesOnly: state.appliedFavoritesOnly,
  };
}

function createAppliedFiltersFromDrafts(state: GamesFilterState): AppliedFilterFields {
  return {
    appliedLibraries: canonicalizeLibraries(state.draftLibraries, state.availableLibraries),
    appliedLaunchers: canonicalizeLaunchers(state.draftLaunchers, state.availableLaunchers),
    appliedLauncherOrder: canonicalizeLauncherOrder(
      state.draftLauncherOrder,
      state.availableLaunchers,
    ),
    appliedShowHidden: state.draftShowHidden,
    appliedFavoritesOnly: state.draftFavoritesOnly,
  };
}

function areDraftsEqualToApplied(state: GamesFilterState): boolean {
  return (
    shallowStringArrayEqual(state.draftLibraries, state.appliedLibraries) &&
    shallowStringArrayEqual(state.draftLaunchers, state.appliedLaunchers) &&
    shallowStringArrayEqual(state.draftLauncherOrder, state.appliedLauncherOrder) &&
    state.draftShowHidden === state.appliedShowHidden &&
    state.draftFavoritesOnly === state.appliedFavoritesOnly
  );
}

function areAppliedFiltersEqual(
  state: GamesFilterState,
  appliedFilters: AppliedFilterFields,
): boolean {
  return (
    shallowStringArrayEqual(state.appliedLibraries, appliedFilters.appliedLibraries) &&
    shallowStringArrayEqual(state.appliedLaunchers, appliedFilters.appliedLaunchers) &&
    shallowStringArrayEqual(state.appliedLauncherOrder, appliedFilters.appliedLauncherOrder) &&
    state.appliedShowHidden === appliedFilters.appliedShowHidden &&
    state.appliedFavoritesOnly === appliedFilters.appliedFavoritesOnly
  );
}

// ---------------------------------------------------------------------------
// Availability helpers
// ---------------------------------------------------------------------------

function withAvailableSnapshots(
  state: GamesFilterState,
  availableLibraries: string[],
  availableLaunchers: string[],
): GamesFilterState {
  const librariesChanged = !shallowStringArrayEqual(state.availableLibraries, availableLibraries);
  const launchersChanged = !shallowStringArrayEqual(state.availableLaunchers, availableLaunchers);

  if (!librariesChanged && !launchersChanged) {
    return state;
  }

  return {
    ...state,
    availableLibraries: librariesChanged ? availableLibraries : state.availableLibraries,
    availableLaunchers: launchersChanged ? availableLaunchers : state.availableLaunchers,
  };
}

function reconcileLauncherOrderWithAvailability(
  state: GamesFilterState,
  availableLaunchers: readonly string[],
): GamesFilterState {
  const appliedLauncherOrder = canonicalizeLauncherOrder(
    state.appliedLauncherOrder,
    availableLaunchers,
  );

  const draftLauncherOrder = state.isDialogOpen
    ? canonicalizeLauncherOrder(state.draftLauncherOrder, availableLaunchers)
    : copyStringArray(appliedLauncherOrder);

  const appliedChanged = !shallowStringArrayEqual(state.appliedLauncherOrder, appliedLauncherOrder);

  const draftChanged = !shallowStringArrayEqual(state.draftLauncherOrder, draftLauncherOrder);

  if (!appliedChanged && !draftChanged) {
    return state;
  }

  return {
    ...state,
    appliedLauncherOrder: appliedChanged ? appliedLauncherOrder : state.appliedLauncherOrder,
    draftLauncherOrder: draftChanged ? draftLauncherOrder : state.draftLauncherOrder,
  };
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

type SliceStateField = SliceFieldMapping[keyof SliceFieldMapping];

type SliceStateUpdate = Partial<Pick<GamesFilterState, SliceStateField>>;

type SliceAdapter = ReturnType<typeof createSliceAdapter>;

function createSliceAdapter(fields: SliceFieldMapping) {
  function fromState(state: GamesFilterState): FilterSlice {
    return {
      applied: state[fields.applied],
      draft: state[fields.draft],
      deferSelectAll: state[fields.deferSelectAll],
      pendingPersisted: state[fields.pendingPersisted],
    };
  }

  function toStateUpdate(slice: FilterSlice | FilterSliceUpdate): SliceStateUpdate {
    const update: SliceStateUpdate = {};

    if (slice.applied !== undefined) {
      update[fields.applied] = slice.applied;
    }

    if (slice.draft !== undefined) {
      update[fields.draft] = slice.draft;
    }

    if (slice.deferSelectAll !== undefined) {
      update[fields.deferSelectAll] = slice.deferSelectAll;
    }

    if (slice.pendingPersisted !== undefined) {
      update[fields.pendingPersisted] = slice.pendingPersisted;
    }

    return update;
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
// Slice hydration / availability
// ---------------------------------------------------------------------------

function createHydratedSliceFilters(
  persistedValues: readonly string[] | null,
  availableValues: readonly string[],
  normalizeFn: Normalizer,
  canonicalizeFn: Canonicalizer,
  adapter: SliceAdapter,
): SliceStateUpdate {
  const slice = createHydratedSlice(persistedValues, availableValues, normalizeFn, canonicalizeFn);

  return adapter.toStateUpdate(slice);
}

function createAvailableSliceFiltersUpdate(
  state: GamesFilterState,
  availableValues: readonly string[],
  canonicalizeFn: Canonicalizer,
  adapter: SliceAdapter,
): SliceStateUpdate | null {
  const update = createAvailableSliceUpdate(
    adapter.fromState(state),
    availableValues,
    canonicalizeFn,
  );

  return update ? adapter.toStateUpdate(update) : null;
}

function applyStateUpdate(
  state: GamesFilterState,
  update: SliceStateUpdate | null,
): GamesFilterState {
  if (!update || Object.keys(update).length === 0) {
    return state;
  }

  return {
    ...state,
    ...update,
  };
}

// ---------------------------------------------------------------------------
// Canonicalization
// ---------------------------------------------------------------------------

function createCanonicalizeFn(normalizeFn: Normalizer): Canonicalizer {
  return (selected, available) =>
    canonicalizeSelection(selected, available, normalizeFn, intersectLibraries);
}

const canonicalizeLibraries = createCanonicalizeFn(normalizeLibraryValues);
const canonicalizeLaunchers = createCanonicalizeFn(normalizeLauncherValues);

// ---------------------------------------------------------------------------
// String array helpers
// ---------------------------------------------------------------------------

function copyStringArray(values: readonly string[]): string[] {
  return [...values];
}
