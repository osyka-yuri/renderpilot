import { describe, expect, it, vi } from 'vitest';
import {
  applyDraftFilters,
  cancelFilterPopover,
  createInitialGamesFilterState,
  hydrateGamesFilterState,
  openFilterPopover,
  toggleDraftLibrary,
  type GamesFilterState,
} from '@features/games/games-filter-state';
import type { PersistedGamesFilters } from '@features/games/games-screen-filters';
import {
  buildGameCardsQueryKey,
  hasFilterIndicator,
  isCoverOperationBusy,
  pruneCoverMenuState,
  shouldCloseOpenMenu,
  shouldQueueAvailabilityPersist,
  syncGamesFilterState,
  withManualCoverBusy,
} from '@features/games/games-screen-controller';

const LIBRARY_ALPHA = 'LibraryAlpha';
const LIBRARY_BETA = 'LibraryBeta';
const UNKNOWN_LIBRARY = 'Unknown';

const GAME_1 = 'game-1';
const GAME_2 = 'game-2';
const GAME_3 = 'game-3';

const AVAILABLE_LIBRARIES = [LIBRARY_ALPHA, LIBRARY_BETA] as const;
const PREVIOUS_AVAILABILITY_SNAPSHOT = 'previous-availability-snapshot';

type ManualCoverBusyParams = Parameters<typeof withManualCoverBusy>[0];

describe('games-screen-controller', () => {
  describe('syncGamesFilterState', () => {
    it('hydrates persisted filters and removes unavailable libraries', () => {
      const persisted: PersistedGamesFilters = {
        searchQuery: 'witcher',
        libraries: [LIBRARY_ALPHA, UNKNOWN_LIBRARY],
      };

      const result = syncGamesFilterState(
        createInitialGamesFilterState(),
        true,
        persisted,
        AVAILABLE_LIBRARIES,
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
        },
        AVAILABLE_LIBRARIES,
      );

      const ignoredPersisted: PersistedGamesFilters = {
        searchQuery: 'ignored-query',
        libraries: [LIBRARY_BETA],
      };

      const result = syncGamesFilterState(readyState, true, ignoredPersisted, AVAILABLE_LIBRARIES);

      expect(result.didAdjustApplied).toBe(false);
      expect(result.state.searchQuery).toBe('ready-query');
      expect(result.state.appliedLibraries).toEqual([LIBRARY_ALPHA]);
    });

    it('adjusts applied libraries when available libraries change', () => {
      const readyState = createReadyFilterState(AVAILABLE_LIBRARIES);

      const result = syncGamesFilterState(readyState, true, null, [LIBRARY_ALPHA]);

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

  describe('buildGameCardsQueryKey', () => {
    it('builds a stable key from search query and selected libraries', () => {
      const queryKey = buildGameCardsQueryKey('abc', ['x', 'y']);

      expect(JSON.parse(queryKey)).toEqual({
        searchQuery: 'abc',
        selectedLibraries: ['x', 'y'],
      });

      expect(queryKey).toBe(buildGameCardsQueryKey('abc', ['x', 'y']));
      expect(queryKey).not.toBe(buildGameCardsQueryKey('abc', ['x', 'z']));
      expect(queryKey).not.toBe(buildGameCardsQueryKey('changed', ['x', 'y']));
    });
  });

  describe('withManualCoverBusy', () => {
    it('runs successful command in the expected order', async () => {
      const calls: string[] = [];
      const onCoverError = vi.fn();

      await withManualCoverBusy(
        createManualCoverBusyParams({
          setManualCoverBusyFor: (gameId) => {
            calls.push(`busy:${gameId ?? 'none'}`);
          },
          task: () => {
            calls.push('task');

            return Promise.resolve();
          },
          onClearError: () => {
            calls.push('clear-error');
          },
          onReloadCards: () => {
            calls.push('reload');

            return Promise.resolve();
          },
          onCoverError,
          focusMenuTrigger: (gameId) => {
            calls.push(`focus:${gameId}`);
          },
        }),
      );

      expect(calls).toEqual([
        'busy:game-1',
        'task',
        'clear-error',
        'reload',
        'busy:none',
        'focus:game-1',
      ]);
      expect(onCoverError).not.toHaveBeenCalled();
    });

    it('skips command when another manual action is already running', async () => {
      const task = vi.fn(() => Promise.resolve());
      const setManualCoverBusyFor = vi.fn();
      const onClearError = vi.fn();
      const onReloadCards = vi.fn(() => Promise.resolve());
      const onCoverError = vi.fn();
      const focusMenuTrigger = vi.fn();

      await withManualCoverBusy(
        createManualCoverBusyParams({
          manualCoverBusyFor: GAME_2,
          setManualCoverBusyFor,
          task,
          onClearError,
          onReloadCards,
          onCoverError,
          focusMenuTrigger,
        }),
      );

      expect(task).not.toHaveBeenCalled();
      expect(setManualCoverBusyFor).not.toHaveBeenCalled();
      expect(onClearError).not.toHaveBeenCalled();
      expect(onReloadCards).not.toHaveBeenCalled();
      expect(onCoverError).not.toHaveBeenCalled();
      expect(focusMenuTrigger).not.toHaveBeenCalled();
    });

    it('reports task error and still restores busy state and focus', async () => {
      const calls: string[] = [];
      const error = new Error('failed');

      const onClearError = vi.fn(() => {
        calls.push('clear-error');
      });
      const onReloadCards = vi.fn(() => {
        calls.push('reload');

        return Promise.resolve();
      });
      const onCoverError = vi.fn((message: string) => {
        calls.push(`error:${message}`);
      });

      await withManualCoverBusy(
        createManualCoverBusyParams({
          setManualCoverBusyFor: (gameId) => {
            calls.push(`busy:${gameId ?? 'none'}`);
          },
          task: () => {
            calls.push('task');

            return Promise.reject(error);
          },
          onClearError,
          onReloadCards,
          onCoverError,
          focusMenuTrigger: (gameId) => {
            calls.push(`focus:${gameId}`);
          },
        }),
      );

      expect(calls).toEqual(['busy:game-1', 'task', 'error:failed', 'busy:none', 'focus:game-1']);
      expect(onClearError).not.toHaveBeenCalled();
      expect(onReloadCards).not.toHaveBeenCalled();
      expect(onCoverError).toHaveBeenCalledWith('failed');
    });

    it('reports reload error and still restores busy state and focus', async () => {
      const calls: string[] = [];
      const reloadError = new Error('reload failed');

      const onClearError = vi.fn(() => {
        calls.push('clear-error');
      });
      const onCoverError = vi.fn((message: string) => {
        calls.push(`error:${message}`);
      });

      await withManualCoverBusy(
        createManualCoverBusyParams({
          setManualCoverBusyFor: (gameId) => {
            calls.push(`busy:${gameId ?? 'none'}`);
          },
          task: () => {
            calls.push('task');

            return Promise.resolve();
          },
          onClearError,
          onReloadCards: () => {
            calls.push('reload');

            return Promise.reject(reloadError);
          },
          onCoverError,
          focusMenuTrigger: (gameId) => {
            calls.push(`focus:${gameId}`);
          },
        }),
      );

      expect(calls).toEqual([
        'busy:game-1',
        'task',
        'clear-error',
        'reload',
        'error:reload failed',
        'busy:none',
        'focus:game-1',
      ]);
      expect(onClearError).toHaveBeenCalledTimes(1);
      expect(onCoverError).toHaveBeenCalledWith('reload failed');
    });
  });

  describe('isCoverOperationBusy', () => {
    const cases: [
      caseName: string,
      gameId: string,
      manualCoverBusyFor: string | null,
      coversAutoFetchingIds: ReadonlySet<string>,
      expected: boolean,
    ][] = [
      ['manual operation targets the game', GAME_1, GAME_1, new Set(), true],
      ['auto operation targets the game', GAME_1, null, new Set([GAME_1]), true],
      ['operations target other games', GAME_1, GAME_2, new Set([GAME_3]), false],
    ];

    it.each(cases)(
      '%s',
      (_caseName, gameId, manualCoverBusyFor, coversAutoFetchingIds, expected) => {
        expect(isCoverOperationBusy(gameId, manualCoverBusyFor, coversAutoFetchingIds)).toBe(
          expected,
        );
      },
    );
  });

  describe('shouldCloseOpenMenu', () => {
    const cases: [
      caseName: string,
      menuOpenFor: string | null,
      manualCoverBusyFor: string | null,
      coversAutoFetchingIds: ReadonlySet<string>,
      expected: boolean,
    ][] = [
      ['menu is already closed', null, null, new Set([GAME_1]), false],
      ['any manual cover operation is running', GAME_1, GAME_2, new Set(), true],
      ['auto operation targets the open menu game', GAME_1, null, new Set([GAME_1]), true],
      ['auto operation targets another game', GAME_1, null, new Set([GAME_2]), false],
    ];

    it.each(cases)(
      '%s',
      (_caseName, menuOpenFor, manualCoverBusyFor, coversAutoFetchingIds, expected) => {
        expect(shouldCloseOpenMenu(menuOpenFor, manualCoverBusyFor, coversAutoFetchingIds)).toBe(
          expected,
        );
      },
    );
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
        expect(hasFilterIndicator(searchQuery, appliedLibraries, availableLibraries)).toBe(
          expected,
        );
      },
    );
  });

  describe('pruneCoverMenuState', () => {
    it('prunes stale refs and clears stale open menu id', () => {
      const refs = {
        [GAME_1]: { id: 1 },
        [GAME_2]: { id: 2 },
      };

      const result = pruneCoverMenuState(refs, GAME_2, [GAME_1]);

      expect(result.refs).toEqual({
        [GAME_1]: refs[GAME_1],
      });
      expect(result.menuOpenFor).toBeNull();
    });

    it('keeps refs object identity when nothing was pruned', () => {
      const refs = {
        [GAME_1]: { id: 1 },
        [GAME_2]: { id: 2 },
      };

      const result = pruneCoverMenuState(refs, GAME_1, [GAME_1, GAME_2]);

      expect(result.refs).toBe(refs);
      expect(result.menuOpenFor).toBe(GAME_1);
    });

    it('clears open menu id even when refs do not need pruning', () => {
      const refs = {
        [GAME_1]: { id: 1 },
      };

      const result = pruneCoverMenuState(refs, GAME_2, [GAME_1]);

      expect(result.refs).toBe(refs);
      expect(result.menuOpenFor).toBeNull();
    });
  });

  describe('filter popover state smoke', () => {
    it('cancels draft changes and applies draft changes as expected', () => {
      const hydrated = createReadyFilterState(AVAILABLE_LIBRARIES);

      const opened = openFilterPopover(hydrated);
      const changedDraft = toggleDraftLibrary(opened, LIBRARY_BETA);
      const canceled = cancelFilterPopover(changedDraft);

      expect(canceled.appliedLibraries).toEqual([LIBRARY_ALPHA, LIBRARY_BETA]);
      expect(canceled.draftLibraries).toEqual([LIBRARY_ALPHA, LIBRARY_BETA]);

      const reopened = openFilterPopover(canceled);
      const changedAgain = toggleDraftLibrary(reopened, LIBRARY_BETA);
      const applied = applyDraftFilters(changedAgain);

      expect(applied.appliedLibraries).toEqual([LIBRARY_ALPHA]);
      expect(applied.draftLibraries).toEqual([LIBRARY_ALPHA]);
    });
  });
});

function createReadyFilterState(
  availableLibraries: readonly string[] = AVAILABLE_LIBRARIES,
): GamesFilterState {
  return hydrateGamesFilterState(createInitialGamesFilterState(), null, availableLibraries);
}

function createReadyStateWithChangedSelection(): GamesFilterState {
  const readyState = createReadyFilterState([LIBRARY_ALPHA]);

  return {
    ...readyState,
    appliedLibraries: [],
    draftLibraries: [],
  };
}

function createManualCoverBusyParams(
  overrides: Partial<ManualCoverBusyParams> = {},
): ManualCoverBusyParams {
  return {
    gameId: GAME_1,
    manualCoverBusyFor: null,
    setManualCoverBusyFor: vi.fn(),
    task: vi.fn(() => Promise.resolve()),
    onClearError: vi.fn(),
    onReloadCards: vi.fn(() => Promise.resolve()),
    onCoverError: vi.fn(),
    describeError: (error) => (error instanceof Error ? error.message : 'unknown'),
    focusMenuTrigger: vi.fn(),
    ...overrides,
  };
}
