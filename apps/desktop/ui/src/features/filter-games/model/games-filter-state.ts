import { shallowStringArrayEqual } from '@shared/text';
import { intersectLibraries, normalizeLibraryValues } from '@entities/game';
import { encodePersistedGamesFilters, type PersistedGamesFilters } from './filter-persistence';

export type GamesFilterState = {
  ready: boolean;
  isDialogOpen: boolean;
  searchQuery: string;
  appliedLibraries: string[];
  draftLibraries: string[];
  deferSelectAllLibraries: boolean;
  pendingPersistedLibraries: string[] | null;
  availableLibraries: string[];
  lastPersistedSnapshot: string;
};

type LibraryFilterState = Pick<
  GamesFilterState,
  'appliedLibraries' | 'draftLibraries' | 'deferSelectAllLibraries' | 'pendingPersistedLibraries'
>;

type LibraryFilterUpdate = Partial<LibraryFilterState>;

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
    lastPersistedSnapshot: EMPTY_PERSISTED_SNAPSHOT,
  };
}

export function hydrateGamesFilterState(
  state: GamesFilterState,
  persisted: PersistedGamesFilters | null,
  availableLibraries: readonly string[],
): GamesFilterState {
  const availableLibrariesSnapshot = copyLibraries(availableLibraries);

  const hydratedState: GamesFilterState = {
    ...state,
    ready: true,
    searchQuery: persisted?.searchQuery ?? EMPTY_SEARCH_QUERY,
    availableLibraries: availableLibrariesSnapshot,
    ...createHydratedLibraryFilters(persisted, availableLibrariesSnapshot),
  };

  return {
    ...hydratedState,
    lastPersistedSnapshot: createPersistedSnapshot(hydratedState),
  };
}

export function withAvailableLibraries(
  state: GamesFilterState,
  availableLibraries: readonly string[],
): GamesFilterState {
  const availableLibrariesSnapshot = copyLibraries(availableLibraries);
  const stateWithAvailableLibraries = withAvailableLibrariesSnapshot(
    state,
    availableLibrariesSnapshot,
  );

  if (!stateWithAvailableLibraries.ready) {
    return stateWithAvailableLibraries;
  }

  const libraryUpdate = createAvailableLibrariesUpdate(
    stateWithAvailableLibraries,
    availableLibrariesSnapshot,
  );

  return applyLibraryFilterUpdate(stateWithAvailableLibraries, libraryUpdate);
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
    shallowStringArrayEqual(state.draftLibraries, state.appliedLibraries)
  ) {
    return state;
  }

  return {
    ...state,
    draftLibraries: copyLibraries(state.appliedLibraries),
    isDialogOpen: true,
  };
}

export function closeFilterDialog(state: GamesFilterState): GamesFilterState {
  if (!state.isDialogOpen) {
    return state;
  }

  return {
    ...state,
    isDialogOpen: false,
  };
}

export function cancelFilterDialog(state: GamesFilterState): GamesFilterState {
  if (
    !state.isDialogOpen &&
    shallowStringArrayEqual(state.draftLibraries, state.appliedLibraries)
  ) {
    return state;
  }

  return {
    ...state,
    draftLibraries: copyLibraries(state.appliedLibraries),
    isDialogOpen: false,
  };
}

export function toggleDraftLibrary(state: GamesFilterState, library: string): GamesFilterState {
  const normalizedLibrary = normalizeLibraryName(library);

  if (normalizedLibrary === null || !isAvailableLibrary(state, normalizedLibrary)) {
    return state;
  }

  const draftLibraries = state.draftLibraries.includes(normalizedLibrary)
    ? state.draftLibraries.filter((value) => value !== normalizedLibrary)
    : [...state.draftLibraries, normalizedLibrary];
  const normalizedDraftLibraries = canonicalizeSelectedLibraries(
    draftLibraries,
    state.availableLibraries,
  );

  return {
    ...state,
    draftLibraries: normalizedDraftLibraries,
  };
}

export function setDraftLibraries(
  state: GamesFilterState,
  libraries: readonly string[],
): GamesFilterState {
  const normalized = canonicalizeSelectedLibraries(libraries, state.availableLibraries);

  if (shallowStringArrayEqual(state.draftLibraries, normalized)) {
    return state;
  }

  return {
    ...state,
    draftLibraries: normalized,
  };
}

export function applyDraftFilters(state: GamesFilterState): GamesFilterState {
  const appliedLibraries = canonicalizeSelectedLibraries(
    state.draftLibraries,
    state.availableLibraries,
  );

  if (
    !state.isDialogOpen &&
    shallowStringArrayEqual(state.appliedLibraries, appliedLibraries) &&
    shallowStringArrayEqual(state.draftLibraries, appliedLibraries)
  ) {
    return state;
  }

  return {
    ...state,
    appliedLibraries,
    draftLibraries: copyLibraries(appliedLibraries),
    isDialogOpen: false,
  };
}

export function createPersistedSnapshot(state: GamesFilterState): string {
  return encodePersistedGamesFilters({
    libraries: state.appliedLibraries,
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

function createHydratedLibraryFilters(
  persisted: PersistedGamesFilters | null,
  availableLibraries: string[],
): LibraryFilterState {
  if (persisted === null) {
    return createLibraryFiltersWithoutPersistedState(availableLibraries);
  }

  return createLibraryFiltersFromPersistedState(persisted, availableLibraries);
}

function createLibraryFiltersWithoutPersistedState(
  availableLibraries: string[],
): LibraryFilterState {
  if (availableLibraries.length === 0) {
    return createEmptyLibraryFilters({
      deferSelectAllLibraries: true,
      pendingPersistedLibraries: null,
    });
  }

  return createSelectedLibraryFilters(availableLibraries);
}

function createLibraryFiltersFromPersistedState(
  persisted: PersistedGamesFilters,
  availableLibraries: string[],
): LibraryFilterState {
  const persistedLibraries = normalizeLibraryValues(persisted.libraries);

  if (availableLibraries.length === 0) {
    return createEmptyLibraryFilters({
      deferSelectAllLibraries: false,
      pendingPersistedLibraries: persistedLibraries,
    });
  }

  return createSelectedLibraryFilters(
    canonicalizeSelectedLibraries(persistedLibraries, availableLibraries),
  );
}

function createAvailableLibrariesUpdate(
  state: GamesFilterState,
  availableLibraries: string[],
): LibraryFilterUpdate | null {
  if (shouldSelectAllDeferredLibraries(state, availableLibraries)) {
    return createSelectedLibraryFilters(availableLibraries);
  }

  if (shouldApplyPendingPersistedLibraries(state, availableLibraries)) {
    return createSelectedLibraryFilters(
      canonicalizeSelectedLibraries(state.pendingPersistedLibraries, availableLibraries),
    );
  }

  if (availableLibraries.length === 0) {
    return createEmptyAvailableLibrariesUpdate(state);
  }

  return createNormalizedLibraryUpdate(state, availableLibraries);
}

function shouldSelectAllDeferredLibraries(
  state: GamesFilterState,
  availableLibraries: string[],
): boolean {
  return state.deferSelectAllLibraries && availableLibraries.length > 0;
}

function shouldApplyPendingPersistedLibraries(
  state: GamesFilterState,
  availableLibraries: string[],
): state is GamesFilterState & { pendingPersistedLibraries: string[] } {
  return state.pendingPersistedLibraries !== null && availableLibraries.length > 0;
}

function createEmptyAvailableLibrariesUpdate(state: GamesFilterState): LibraryFilterUpdate | null {
  if (state.appliedLibraries.length === 0 && state.draftLibraries.length === 0) {
    return null;
  }

  return {
    appliedLibraries: [],
    draftLibraries: [],
    pendingPersistedLibraries: copyNullableLibraries(state.pendingPersistedLibraries),
  };
}

function createNormalizedLibraryUpdate(
  state: GamesFilterState,
  availableLibraries: string[],
): LibraryFilterUpdate | null {
  const appliedLibraries = canonicalizeSelectedLibraries(
    state.appliedLibraries,
    availableLibraries,
  );
  const draftLibraries = canonicalizeSelectedLibraries(state.draftLibraries, availableLibraries);

  const hasAppliedLibrariesChanged = !shallowStringArrayEqual(
    state.appliedLibraries,
    appliedLibraries,
  );

  const hasDraftLibrariesChanged = !shallowStringArrayEqual(state.draftLibraries, draftLibraries);

  if (!hasAppliedLibrariesChanged && !hasDraftLibrariesChanged) {
    return null;
  }

  const update: LibraryFilterUpdate = {};

  if (hasAppliedLibrariesChanged) {
    update.appliedLibraries = appliedLibraries;
  }

  if (hasDraftLibrariesChanged) {
    update.draftLibraries = draftLibraries;
  }

  return update;
}

function createSelectedLibraryFilters(libraries: readonly string[]): LibraryFilterState {
  const appliedLibraries = copyLibraries(libraries);

  return {
    appliedLibraries,
    draftLibraries: copyLibraries(appliedLibraries),
    deferSelectAllLibraries: false,
    pendingPersistedLibraries: null,
  };
}

function createEmptyLibraryFilters({
  deferSelectAllLibraries,
  pendingPersistedLibraries,
}: {
  deferSelectAllLibraries: boolean;
  pendingPersistedLibraries: readonly string[] | null;
}): LibraryFilterState {
  return {
    appliedLibraries: [],
    draftLibraries: [],
    deferSelectAllLibraries,
    pendingPersistedLibraries: copyNullableLibraries(pendingPersistedLibraries),
  };
}

function applyLibraryFilterUpdate(
  state: GamesFilterState,
  update: LibraryFilterUpdate | null,
): GamesFilterState {
  if (update === null) {
    return state;
  }

  return {
    ...state,
    ...update,
  };
}

function withAvailableLibrariesSnapshot(
  state: GamesFilterState,
  availableLibraries: string[],
): GamesFilterState {
  if (shallowStringArrayEqual(state.availableLibraries, availableLibraries)) {
    return state;
  }

  return {
    ...state,
    availableLibraries,
  };
}

function isAvailableLibrary(state: GamesFilterState, library: string): boolean {
  return state.availableLibraries.includes(library);
}

function normalizeLibraryName(library: string): string | null {
  const normalizedLibrary = library.trim();

  return normalizedLibrary.length === 0 ? null : normalizedLibrary;
}

function canonicalizeSelectedLibraries(
  selectedLibraries: readonly string[],
  availableLibraries: readonly string[],
): string[] {
  const normalizedAvailableLibraries = normalizeLibraryValues(availableLibraries);

  if (normalizedAvailableLibraries.length === 0) {
    return [];
  }

  const selectedLibrarySet = new Set(
    intersectLibraries(selectedLibraries, normalizedAvailableLibraries),
  );

  return normalizedAvailableLibraries.filter((library) => selectedLibrarySet.has(library));
}

function copyLibraries(libraries: readonly string[]): string[] {
  return [...libraries];
}

function copyNullableLibraries(libraries: readonly string[] | null): string[] | null {
  return libraries === null ? null : copyLibraries(libraries);
}
