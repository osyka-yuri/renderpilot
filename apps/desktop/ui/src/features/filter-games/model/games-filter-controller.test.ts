import { describe, expect, it } from 'vitest';
import {
  applyDraftFilters,
  cancelFilterDialog,
  createInitialGamesFilterState,
  hydrateGamesFilterState,
  openFilterDialog,
  setDraftLibraries,
  type GamesFilterState,
} from './games-filter-state';
import type { PersistedGamesFilters } from './filter-persistence';
import {
  hasFilterIndicator,
  shouldQueueAvailabilityPersist,
  syncGamesFilterState,
} from './games-filter-controller';

const LIBRARY_ALPHA = 'LibraryAlpha';
const LIBRARY_BETA = 'LibraryBeta';
const UNKNOWN_LIBRARY = 'Unknown';

const AVAILABLE_LIBRARIES = [LIBRARY_ALPHA, LIBRARY_BETA] as const;
const PREVIOUS_AVAILABILITY_SNAPSHOT = 'previous-availability-snapshot';

describe('games-filter-controller', () => {
  describe('syncGamesFilterState', () => {
    it('hydrates persisted filters and removes unavailable libraries', () => {
      const persisted: PersistedGamesFilters = {
        searchQuery: 'witcher',
        libraries: [LIBRARY_ALPHA, UNKNOWN_LIBRARY],
        launchers: [],
        launcherOrder: [],
        showHidden: false,
        favoritesOnly: false,
      };

      const result = syncGamesFilterState(
        createInitialGamesFilterState(),
        true,
        persisted,
        AVAILABLE_LIBRARIES,
        [],
      );

      expect(result.didAdjustApplied).toBe(true);
      expect(result.state.ready).toBe(true);
      expect(result.state.searchQuery).toBe('witcher');
      expect(result.state.appliedLibraries).toEqual([LIBRARY_ALPHA]);
    });

    it('does not hydrate again when state is already ready', () => {
      const readyState = hydrateGamesFilterState(
        createInitialGamesFilterState(),
        {
          searchQuery: 'ready-query',
          libraries: [LIBRARY_ALPHA],
          launchers: [],
          launcherOrder: [],
          showHidden: false,
          favoritesOnly: false,
        },
        AVAILABLE_LIBRARIES,
        [],
      );

      const ignoredPersisted: PersistedGamesFilters = {
        searchQuery: 'witcher',
        libraries: ['LibraryAlpha'],
        launchers: [],
        launcherOrder: [],
        showHidden: false,
        favoritesOnly: false,
      };

      const result = syncGamesFilterState(
        readyState,
        true,
        ignoredPersisted,
        AVAILABLE_LIBRARIES,
        [],
      );

      expect(result.didAdjustApplied).toBe(false);
      expect(result.state.searchQuery).toBe('ready-query');
      expect(result.state.appliedLibraries).toEqual([LIBRARY_ALPHA]);
    });

    it('adjusts applied libraries when available libraries change', () => {
      const readyState = createReadyFilterState(AVAILABLE_LIBRARIES);

      const result = syncGamesFilterState(readyState, true, null, [LIBRARY_ALPHA], []);

      expect(result.didAdjustApplied).toBe(true);
      expect(result.state.appliedLibraries).toEqual([LIBRARY_ALPHA]);
    });
  });

  describe('shouldQueueAvailabilityPersist', () => {
    it('does not queue when preferences are not loaded', () => {
      const decision = shouldQueueAvailabilityPersist(
        createReadyFilterState([LIBRARY_ALPHA]),
        false,
        PREVIOUS_AVAILABILITY_SNAPSHOT,
      );

      expect(decision).toEqual({
        shouldQueue: false,
        nextSnapshot: PREVIOUS_AVAILABILITY_SNAPSHOT,
      });
    });

    it('does not queue when state is not ready', () => {
      const decision = shouldQueueAvailabilityPersist(
        createInitialGamesFilterState(),
        true,
        PREVIOUS_AVAILABILITY_SNAPSHOT,
      );

      expect(decision).toEqual({
        shouldQueue: false,
        nextSnapshot: PREVIOUS_AVAILABILITY_SNAPSHOT,
      });
    });

    it('does not queue unchanged ready snapshot', () => {
      const decision = shouldQueueAvailabilityPersist(
        createReadyFilterState([LIBRARY_ALPHA]),
        true,
        PREVIOUS_AVAILABILITY_SNAPSHOT,
      );

      expect(decision).toEqual({
        shouldQueue: false,
        nextSnapshot: PREVIOUS_AVAILABILITY_SNAPSHOT,
      });
    });

    it('queues changed ready snapshot', () => {
      const changedState = createReadyStateWithChangedSelection();

      const decision = shouldQueueAvailabilityPersist(changedState, true, '');

      expect(decision.shouldQueue).toBe(true);
      expect(decision.nextSnapshot).not.toBe('');
    });

    it('does not queue already queued snapshot again', () => {
      const changedState = createReadyStateWithChangedSelection();

      const firstDecision = shouldQueueAvailabilityPersist(changedState, true, '');
      const secondDecision = shouldQueueAvailabilityPersist(
        changedState,
        true,
        firstDecision.nextSnapshot,
      );

      expect(firstDecision.shouldQueue).toBe(true);
      expect(secondDecision).toEqual({
        shouldQueue: false,
        nextSnapshot: firstDecision.nextSnapshot,
      });
    });
  });

  describe('hasFilterIndicator', () => {
    const cases: [
      caseName: string,
      searchQuery: string,
      appliedLibraries: readonly string[],
      availableLibraries: readonly string[],
      expected: boolean,
    ][] = [
      [
        'no search and all libraries selected',
        '',
        [LIBRARY_ALPHA, LIBRARY_BETA],
        [LIBRARY_ALPHA, LIBRARY_BETA],
        false,
      ],
      ['non-empty search query', ' witcher ', [LIBRARY_ALPHA], [LIBRARY_ALPHA], true],
      ['partial library selection', '', [LIBRARY_ALPHA], [LIBRARY_ALPHA, LIBRARY_BETA], true],
    ];

    it.each(cases)(
      '%s',
      (_caseName, searchQuery, appliedLibraries, availableLibraries, expected) => {
        expect(hasFilterIndicator(searchQuery, appliedLibraries, availableLibraries, [], [])).toBe(
          expected,
        );
      },
    );
  });

  describe('filter dialog state smoke', () => {
    it('cancels draft changes and applies draft changes as expected', () => {
      const hydrated = createReadyFilterState(AVAILABLE_LIBRARIES);

      const opened = openFilterDialog(hydrated);
      const changedDraft = setDraftLibraries(opened, [LIBRARY_ALPHA]);
      const canceled = cancelFilterDialog(changedDraft);

      expect(canceled.appliedLibraries).toEqual([LIBRARY_ALPHA, LIBRARY_BETA]);
      expect(canceled.draftLibraries).toEqual([LIBRARY_ALPHA, LIBRARY_BETA]);

      const reopened = openFilterDialog(canceled);
      const changedAgain = setDraftLibraries(reopened, [LIBRARY_ALPHA]);
      const applied = applyDraftFilters(changedAgain);

      expect(applied.appliedLibraries).toEqual([LIBRARY_ALPHA]);
      expect(applied.draftLibraries).toEqual([LIBRARY_ALPHA]);
    });
  });
});

function createReadyFilterState(
  availableLibraries: readonly string[] = AVAILABLE_LIBRARIES,
): GamesFilterState {
  return hydrateGamesFilterState(createInitialGamesFilterState(), null, availableLibraries, []);
}

function createReadyStateWithChangedSelection(): GamesFilterState {
  const readyState = createReadyFilterState([LIBRARY_ALPHA]);

  return {
    ...readyState,
    appliedLibraries: [],
    draftLibraries: [],
    appliedLaunchers: [],
    draftLaunchers: [],
  };
}
