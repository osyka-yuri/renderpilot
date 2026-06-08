import {
  intersectLibraries,
  normalizeLibraryValues,
  normalizeLauncherValues,
} from '@entities/game';

import {
  canonicalizeSelection,
  createAvailableSliceUpdate,
  createHydratedSlice,
  type FilterSlice,
  type FilterSliceUpdate,
} from './filter-slice';
import type { GamesFilterState } from './games-filter-state';

type Normalizer = (values: readonly string[]) => string[];
type Canonicalizer = (selected: readonly string[], available: readonly string[]) => string[];

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

export type SliceStateUpdate = Partial<Pick<GamesFilterState, SliceStateField>>;

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

export const LIBRARY_ADAPTER = createSliceAdapter({
  applied: 'appliedLibraries',
  draft: 'draftLibraries',
  deferSelectAll: 'deferSelectAllLibraries',
  pendingPersisted: 'pendingPersistedLibraries',
});

export const LAUNCHER_ADAPTER = createSliceAdapter({
  applied: 'appliedLaunchers',
  draft: 'draftLaunchers',
  deferSelectAll: 'deferSelectAllLaunchers',
  pendingPersisted: 'pendingPersistedLaunchers',
});

// ---------------------------------------------------------------------------
// Canonicalization
// ---------------------------------------------------------------------------

function createCanonicalizeFn(normalizeFn: Normalizer): Canonicalizer {
  return (selected, available) =>
    canonicalizeSelection(selected, available, normalizeFn, intersectLibraries);
}

export const canonicalizeLibraries = createCanonicalizeFn(normalizeLibraryValues);
export const canonicalizeLaunchers = createCanonicalizeFn(normalizeLauncherValues);

// ---------------------------------------------------------------------------
// Slice hydration / availability
// ---------------------------------------------------------------------------

export function createHydratedSliceFilters(
  persistedValues: readonly string[] | null,
  availableValues: readonly string[],
  normalizeFn: Normalizer,
  canonicalizeFn: Canonicalizer,
  adapter: SliceAdapter,
): SliceStateUpdate {
  const slice = createHydratedSlice(persistedValues, availableValues, normalizeFn, canonicalizeFn);

  return adapter.toStateUpdate(slice);
}

export function createAvailableSliceFiltersUpdate(
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

export function applyStateUpdate(
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
