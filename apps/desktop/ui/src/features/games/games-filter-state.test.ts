import { describe, expect, it } from 'vitest';
import {
  applyDraftFilters,
  cancelFilterPopover,
  closeFilterPopover,
  commitPersistedSnapshot,
  createInitialGamesFilterState,
  createPersistedSnapshot,
  hydrateGamesFilterState,
  isPersistedSnapshotStillCurrent,
  openFilterPopover,
  toggleDraftLibrary,
  withAvailableLibraries,
  withSearchQuery,
} from '@features/games/games-filter-state';
import type { GamesFilterState } from '@features/games/games-filter-state';
import type { PersistedGamesFilters } from '@features/games/games-screen-filters';

const LIBRARY_ALPHA = 'LibraryAlpha';
const LIBRARY_BETA = 'LibraryBeta';
const LIBRARY_GAMMA = 'LibraryGamma';
const MISSING_LIBRARY = 'Missing';

const EMPTY_AVAILABLE_LIBRARIES = [] as const;
const DEFAULT_AVAILABLE_LIBRARIES = [LIBRARY_ALPHA, LIBRARY_BETA] as const;
const EXTENDED_AVAILABLE_LIBRARIES = [LIBRARY_ALPHA, LIBRARY_GAMMA, LIBRARY_BETA] as const;

describe('games-filter-state', () => {
  describe('createInitialGamesFilterState', () => {
    it('creates a not-ready empty state', () => {
      expect(createInitialGamesFilterState()).toEqual({
        ready: false,
        isPopoverOpen: false,
        searchQuery: '',
        appliedLibraries: [],
        draftLibraries: [],
        deferSelectAllLibraries: false,
        pendingPersistedLibraries: null,
        availableLibraries: [],
        lastPersistedSnapshot: '',
      });
    });
  });

  describe('hydrateGamesFilterState', () => {
    it('hydrates persisted search query and available library selection', () => {
      const persisted = createPersistedFilters({
        libraries: [LIBRARY_ALPHA, LIBRARY_GAMMA, MISSING_LIBRARY],
        searchQuery: 'witcher',
      });

      const state = createHydratedState({
        persisted,
        availableLibraries: EXTENDED_AVAILABLE_LIBRARIES,
      });

      expect(state.ready).toBe(true);
      expect(state.searchQuery).toBe('witcher');
      expect(state.availableLibraries).toEqual(EXTENDED_AVAILABLE_LIBRARIES);
      expectSelectedLibraries(state, [LIBRARY_ALPHA, LIBRARY_GAMMA]);
      expectNoPendingLibrarySelection(state);
      expectLastPersistedSnapshotToMatchState(state);
    });

    it('selects all available libraries when there is no persisted selection', () => {
      const state = createHydratedState({
        persisted: null,
        availableLibraries: DEFAULT_AVAILABLE_LIBRARIES,
      });

      expectSelectedLibraries(state, DEFAULT_AVAILABLE_LIBRARIES);
      expectNoPendingLibrarySelection(state);
      expectLastPersistedSnapshotToMatchState(state);
    });

    it('defers select-all when there is no persisted selection and catalog is empty', () => {
      const state = createHydratedState({
        persisted: null,
        availableLibraries: EMPTY_AVAILABLE_LIBRARIES,
      });

      expectSelectedLibraries(state, []);
      expect(state.deferSelectAllLibraries).toBe(true);
      expect(state.pendingPersistedLibraries).toBeNull();
      expectLastPersistedSnapshotToMatchState(state);
    });

    it('keeps persisted libraries pending while catalog is empty', () => {
      const persisted = createPersistedFilters({
        libraries: [LIBRARY_ALPHA],
        searchQuery: 'war',
      });

      const state = createHydratedState({
        persisted,
        availableLibraries: EMPTY_AVAILABLE_LIBRARIES,
      });

      expect(state.searchQuery).toBe('war');
      expectSelectedLibraries(state, []);
      expect(state.deferSelectAllLibraries).toBe(false);
      expect(state.pendingPersistedLibraries).toEqual([LIBRARY_ALPHA]);
      expectLastPersistedSnapshotToMatchState(state);
    });

    it('uses defensive copies for available libraries', () => {
      const availableLibraries = [LIBRARY_ALPHA, LIBRARY_BETA];

      const state = createHydratedState({
        availableLibraries,
      });

      availableLibraries.push(LIBRARY_GAMMA);

      expect(state.availableLibraries).toEqual(DEFAULT_AVAILABLE_LIBRARIES);
      expectSelectedLibraries(state, DEFAULT_AVAILABLE_LIBRARIES);
    });

    it('uses defensive copies for pending persisted libraries', () => {
      const persistedLibraries = [LIBRARY_ALPHA];
      const persisted = createPersistedFilters({
        libraries: persistedLibraries,
      });

      const state = createHydratedState({
        persisted,
        availableLibraries: EMPTY_AVAILABLE_LIBRARIES,
      });

      persistedLibraries.push(LIBRARY_BETA);

      expect(state.pendingPersistedLibraries).toEqual([LIBRARY_ALPHA]);
    });
  });

  describe('withAvailableLibraries', () => {
    it('updates available libraries before state is ready without applying filters', () => {
      const initial = createInitialGamesFilterState();

      const state = withAvailableLibraries(initial, DEFAULT_AVAILABLE_LIBRARIES);

      expect(state.ready).toBe(false);
      expect(state.availableLibraries).toEqual(DEFAULT_AVAILABLE_LIBRARIES);
      expectSelectedLibraries(state, []);
      expectNoPendingLibrarySelection(state);
    });

    it('uses defensive copies when available libraries are updated', () => {
      const availableLibraries = [LIBRARY_ALPHA, LIBRARY_BETA];

      const state = withAvailableLibraries(createInitialGamesFilterState(), availableLibraries);

      availableLibraries.push(LIBRARY_GAMMA);

      expect(state.availableLibraries).toEqual(DEFAULT_AVAILABLE_LIBRARIES);
    });

    it('applies deferred select-all once catalog becomes available', () => {
      const hydrated = createHydratedState({
        persisted: null,
        availableLibraries: EMPTY_AVAILABLE_LIBRARIES,
      });

      const state = withAvailableLibraries(hydrated, DEFAULT_AVAILABLE_LIBRARIES);

      expectSelectedLibraries(state, DEFAULT_AVAILABLE_LIBRARIES);
      expectNoPendingLibrarySelection(state);
    });

    it('keeps deferred select-all while catalog remains empty', () => {
      const hydrated = createHydratedState({
        persisted: null,
        availableLibraries: EMPTY_AVAILABLE_LIBRARIES,
      });

      const state = withAvailableLibraries(hydrated, EMPTY_AVAILABLE_LIBRARIES);

      expect(state).toBe(hydrated);
      expectSelectedLibraries(state, []);
      expect(state.deferSelectAllLibraries).toBe(true);
      expect(state.pendingPersistedLibraries).toBeNull();
    });

    it('applies pending persisted selection once catalog becomes available', () => {
      const persisted = createPersistedFilters({
        libraries: [LIBRARY_ALPHA, MISSING_LIBRARY],
      });

      const hydrated = createHydratedState({
        persisted,
        availableLibraries: EMPTY_AVAILABLE_LIBRARIES,
      });

      const state = withAvailableLibraries(hydrated, DEFAULT_AVAILABLE_LIBRARIES);

      expectSelectedLibraries(state, [LIBRARY_ALPHA]);
      expectNoPendingLibrarySelection(state);
    });

    it('keeps pending persisted selection while catalog remains empty', () => {
      const persisted = createPersistedFilters({
        libraries: [LIBRARY_ALPHA, MISSING_LIBRARY],
      });

      const hydrated = createHydratedState({
        persisted,
        availableLibraries: EMPTY_AVAILABLE_LIBRARIES,
      });

      const state = withAvailableLibraries(hydrated, EMPTY_AVAILABLE_LIBRARIES);

      expect(state).toBe(hydrated);
      expectSelectedLibraries(state, []);
      expect(state.deferSelectAllLibraries).toBe(false);
      expect(state.pendingPersistedLibraries).toEqual([LIBRARY_ALPHA, MISSING_LIBRARY]);
    });

    it('normalizes both applied and draft libraries when available catalog shrinks', () => {
      const hydrated = createHydratedState({
        availableLibraries: [LIBRARY_ALPHA, LIBRARY_BETA, LIBRARY_GAMMA],
      });

      const opened = openFilterPopover(hydrated);
      const draftChanged = toggleDraftLibrary(opened, LIBRARY_ALPHA);

      const state = withAvailableLibraries(draftChanged, [LIBRARY_BETA, LIBRARY_GAMMA]);

      expect(state.availableLibraries).toEqual([LIBRARY_BETA, LIBRARY_GAMMA]);
      expectSelectedLibraries(state, [LIBRARY_BETA, LIBRARY_GAMMA]);
    });

    it('normalizes only changed library lists when catalog changes', () => {
      const hydrated = createHydratedState({
        availableLibraries: [LIBRARY_ALPHA, LIBRARY_BETA, LIBRARY_GAMMA],
      });

      const opened = openFilterPopover(hydrated);
      const draftChanged = toggleDraftLibrary(opened, LIBRARY_GAMMA);

      const state = withAvailableLibraries(draftChanged, [LIBRARY_ALPHA, LIBRARY_BETA]);

      expect(state.appliedLibraries).toEqual([LIBRARY_ALPHA, LIBRARY_BETA]);
      expect(state.draftLibraries).toEqual([LIBRARY_ALPHA, LIBRARY_BETA]);
      expect(state.availableLibraries).toEqual([LIBRARY_ALPHA, LIBRARY_BETA]);
    });

    it('clears applied and draft libraries when catalog becomes empty', () => {
      const hydrated = createHydratedState();

      const state = withAvailableLibraries(hydrated, EMPTY_AVAILABLE_LIBRARIES);

      expect(state.availableLibraries).toEqual([]);
      expectSelectedLibraries(state, []);
      expect(state.pendingPersistedLibraries).toBeNull();
    });

    it('returns the same state when available libraries are unchanged and no normalization is needed', () => {
      const hydrated = createHydratedState();

      const state = withAvailableLibraries(hydrated, DEFAULT_AVAILABLE_LIBRARIES);

      expect(state).toBe(hydrated);
    });
  });

  describe('popover lifecycle', () => {
    it('opens popover and resets dirty draft to applied libraries', () => {
      const hydrated = createHydratedState();
      const dirtyDraft: GamesFilterState = {
        ...hydrated,
        draftLibraries: [LIBRARY_ALPHA],
      };

      const state = openFilterPopover(dirtyDraft);

      expect(state.isPopoverOpen).toBe(true);
      expect(state.appliedLibraries).toEqual(DEFAULT_AVAILABLE_LIBRARIES);
      expect(state.draftLibraries).toEqual(DEFAULT_AVAILABLE_LIBRARIES);
    });

    it('closes popover without changing draft libraries', () => {
      const opened = openFilterPopover(createHydratedState());
      const draftChanged = toggleDraftLibrary(opened, LIBRARY_BETA);

      const state = closeFilterPopover(draftChanged);

      expect(state.isPopoverOpen).toBe(false);
      expect(state.appliedLibraries).toEqual(DEFAULT_AVAILABLE_LIBRARIES);
      expect(state.draftLibraries).toEqual([LIBRARY_ALPHA]);
    });

    it('returns the same state when closing an already closed popover', () => {
      const hydrated = createHydratedState();

      const state = closeFilterPopover(hydrated);

      expect(state).toBe(hydrated);
    });

    it('cancels draft changes and closes popover', () => {
      const opened = openFilterPopover(createHydratedState());
      const draftChanged = toggleDraftLibrary(opened, LIBRARY_BETA);

      const state = cancelFilterPopover(draftChanged);

      expect(state.isPopoverOpen).toBe(false);
      expectSelectedLibraries(state, DEFAULT_AVAILABLE_LIBRARIES);
    });

    it('returns the same state when canceling a closed popover with clean draft', () => {
      const hydrated = createHydratedState();

      const state = cancelFilterPopover(hydrated);

      expect(state).toBe(hydrated);
    });
  });

  describe('draft library selection', () => {
    it('ignores draft toggle for unknown libraries', () => {
      const opened = openFilterPopover(createHydratedState());

      const state = toggleDraftLibrary(opened, LIBRARY_GAMMA);

      expect(state).toBe(opened);
      expect(state.draftLibraries).toEqual(DEFAULT_AVAILABLE_LIBRARIES);
    });

    it('ignores draft toggle for blank library names', () => {
      const opened = openFilterPopover(createHydratedState());

      const state = toggleDraftLibrary(opened, '   ');

      expect(state).toBe(opened);
      expect(state.draftLibraries).toEqual(DEFAULT_AVAILABLE_LIBRARIES);
    });

    it('normalizes library name before toggling draft selection', () => {
      const opened = openFilterPopover(createHydratedState());

      const state = toggleDraftLibrary(opened, `  ${LIBRARY_BETA}  `);

      expect(state.draftLibraries).toEqual([LIBRARY_ALPHA]);
    });

    it('toggles available draft libraries', () => {
      const opened = openFilterPopover(createHydratedState());

      const unchecked = toggleDraftLibrary(opened, LIBRARY_BETA);
      const checkedAgain = toggleDraftLibrary(unchecked, LIBRARY_BETA);

      expect(unchecked.draftLibraries).toEqual([LIBRARY_ALPHA]);
      expect(checkedAgain.draftLibraries).toEqual(DEFAULT_AVAILABLE_LIBRARIES);
    });

    it('applies draft selection and closes popover', () => {
      const opened = openFilterPopover(createHydratedState());
      const draftChanged = toggleDraftLibrary(opened, LIBRARY_BETA);

      const state = applyDraftFilters(draftChanged);

      expect(state.isPopoverOpen).toBe(false);
      expectSelectedLibraries(state, [LIBRARY_ALPHA]);
    });

    it('applies only draft libraries that are still available', () => {
      const hydrated = createHydratedState({
        availableLibraries: [LIBRARY_ALPHA, LIBRARY_BETA, LIBRARY_GAMMA],
      });

      const stateWithStaleDraft: GamesFilterState = {
        ...openFilterPopover(hydrated),
        draftLibraries: [LIBRARY_ALPHA, MISSING_LIBRARY],
      };

      const state = applyDraftFilters(stateWithStaleDraft);

      expect(state.isPopoverOpen).toBe(false);
      expectSelectedLibraries(state, [LIBRARY_ALPHA]);
    });

    it('returns the same state when applying clean draft while popover is already closed', () => {
      const hydrated = createHydratedState();

      const state = applyDraftFilters(hydrated);

      expect(state).toBe(hydrated);
    });
  });

  describe('search query', () => {
    it('updates search query', () => {
      const hydrated = createHydratedState();

      const state = withSearchQuery(hydrated, 'new query');

      expect(state.searchQuery).toBe('new query');
      expect(state.appliedLibraries).toEqual(hydrated.appliedLibraries);
      expect(state.draftLibraries).toEqual(hydrated.draftLibraries);
    });

    it('returns the same state when search query is unchanged', () => {
      const hydrated = createHydratedState({
        persisted: createPersistedFilters({
          searchQuery: 'war',
        }),
      });

      const state = withSearchQuery(hydrated, 'war');

      expect(state).toBe(hydrated);
    });

    it('preserves filters across settings roundtrip without phantom reset', () => {
      const persisted = createPersistedFilters({
        libraries: [LIBRARY_ALPHA],
        searchQuery: 'war',
      });

      const firstMount = createHydratedState({
        persisted,
        availableLibraries: DEFAULT_AVAILABLE_LIBRARIES,
      });

      const activeState = withSearchQuery(firstMount, 'war');

      // Simulates unmount/mount when user opens settings and goes back.
      const secondMount = createHydratedState({
        persisted,
        availableLibraries: DEFAULT_AVAILABLE_LIBRARIES,
      });

      expect(secondMount.appliedLibraries).toEqual(activeState.appliedLibraries);
      expect(secondMount.draftLibraries).toEqual(activeState.draftLibraries);
      expect(secondMount.searchQuery).toBe(activeState.searchQuery);
    });
  });

  describe('persisted snapshots', () => {
    it('creates persisted snapshot from applied libraries and search query', () => {
      const searched = withSearchQuery(createHydratedState(), 'witcher');

      const snapshot = createPersistedSnapshot(searched);

      expect(isPersistedSnapshotStillCurrent(searched, snapshot)).toBe(true);
    });

    it('detects stale in-flight snapshot when search query changes before save finishes', () => {
      const hydrated = createHydratedState();
      const inFlightSnapshot = createPersistedSnapshot(hydrated);

      const changed = withSearchQuery(hydrated, 'new query');

      expect(isPersistedSnapshotStillCurrent(hydrated, inFlightSnapshot)).toBe(true);
      expect(isPersistedSnapshotStillCurrent(changed, inFlightSnapshot)).toBe(false);
    });

    it('detects stale in-flight snapshot when applied libraries change before save finishes', () => {
      const hydrated = createHydratedState();
      const inFlightSnapshot = createPersistedSnapshot(hydrated);

      const changed = applyDraftFilters(
        toggleDraftLibrary(openFilterPopover(hydrated), LIBRARY_BETA),
      );

      expect(isPersistedSnapshotStillCurrent(hydrated, inFlightSnapshot)).toBe(true);
      expect(isPersistedSnapshotStillCurrent(changed, inFlightSnapshot)).toBe(false);
    });

    it('commits snapshot after caller verifies it is still current', () => {
      const changed = withSearchQuery(createHydratedState(), 'new query');
      const currentSnapshot = createPersistedSnapshot(changed);

      const state = commitSnapshotIfCurrent(changed, currentSnapshot);

      expect(state.lastPersistedSnapshot).toBe(currentSnapshot);
      expect(isPersistedSnapshotStillCurrent(state, state.lastPersistedSnapshot)).toBe(true);
    });

    it('does not update last persisted snapshot when caller rejects stale in-flight snapshot', () => {
      const hydrated = createHydratedState();
      const inFlightSnapshot = createPersistedSnapshot(withSearchQuery(hydrated, 'in-flight'));
      const changed = withSearchQuery(hydrated, 'new query');

      const state = commitSnapshotIfCurrent(changed, inFlightSnapshot);

      expect(state).toBe(changed);
      expect(state.lastPersistedSnapshot).toBe(hydrated.lastPersistedSnapshot);
      expect(state.lastPersistedSnapshot).not.toBe(inFlightSnapshot);
    });

    it('returns the same state when committing the already stored snapshot', () => {
      const hydrated = createHydratedState();

      const state = commitPersistedSnapshot(hydrated, hydrated.lastPersistedSnapshot);

      expect(state).toBe(hydrated);
    });
  });
});

type HydrateOptions = {
  persisted?: PersistedGamesFilters | null;
  availableLibraries?: readonly string[];
};

function createHydratedState({
  persisted = null,
  availableLibraries = DEFAULT_AVAILABLE_LIBRARIES,
}: HydrateOptions = {}): GamesFilterState {
  return hydrateGamesFilterState(createInitialGamesFilterState(), persisted, availableLibraries);
}

function createPersistedFilters(
  overrides: Partial<PersistedGamesFilters> = {},
): PersistedGamesFilters {
  return {
    libraries: [LIBRARY_ALPHA],
    searchQuery: '',
    ...overrides,
  };
}

function commitSnapshotIfCurrent(state: GamesFilterState, snapshot: string): GamesFilterState {
  return isPersistedSnapshotStillCurrent(state, snapshot)
    ? commitPersistedSnapshot(state, snapshot)
    : state;
}

function expectSelectedLibraries(
  state: Pick<GamesFilterState, 'appliedLibraries' | 'draftLibraries'>,
  libraries: readonly string[],
): void {
  expect(state.appliedLibraries).toEqual(libraries);
  expect(state.draftLibraries).toEqual(libraries);
}

function expectNoPendingLibrarySelection(
  state: Pick<GamesFilterState, 'deferSelectAllLibraries' | 'pendingPersistedLibraries'>,
): void {
  expect(state.deferSelectAllLibraries).toBe(false);
  expect(state.pendingPersistedLibraries).toBeNull();
}

function expectLastPersistedSnapshotToMatchState(state: GamesFilterState): void {
  expect(state.lastPersistedSnapshot).toBe(createPersistedSnapshot(state));
  expect(isPersistedSnapshotStillCurrent(state, state.lastPersistedSnapshot)).toBe(true);
}
