import { describe, expect, it, vi } from 'vitest';
import type { queryGameCards } from '@shared/api/desktop';
import {
  DEFAULT_GAME_CARDS_CATALOG_PAGE,
  DEFAULT_GAME_CARDS_CATALOG_SORT,
} from '@shared/api/game-cards-defaults';
import type { GameCard, GameCardsResult } from '@shared/api/types';
import {
  createGamesScreenQueryScheduler,
  type GamesQueryResultSinks,
  type GamesQuerySnapshot,
} from './games-screen-query-scheduler';

type QueryGameCardsFn = typeof queryGameCards;
type Scheduler = ReturnType<typeof createGamesScreenQueryScheduler>;

type Deferred<T> = {
  promise: Promise<T>;
  resolve(value: T): void;
  reject(error: unknown): void;
};

function createDeferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;

  const promise = new Promise<T>((promiseResolve, promiseReject) => {
    resolve = promiseResolve;
    reject = promiseReject;
  });

  return {
    promise,
    resolve,
    reject,
  };
}

function createQueryGameCardsMock() {
  return vi.fn<QueryGameCardsFn>();
}

function stubCard(gameId: string): GameCard {
  return {
    game_id: gameId,
    title: '',
    launcher: '',
    platform: '',
    runtime: '',
    install_path: '',
    library_tags: [],
    component_count: 0,
    updates_available: false,
    update_count: 0,
    risk_level: '',
    backup_available: false,
    operation_count: 0,
  };
}

function makeResult(
  overrides: Partial<GameCardsResult> & Pick<GameCardsResult, 'items'>,
): GameCardsResult {
  return {
    total: overrides.items.length,
    availableLibraries: [],
    queryFingerprint: 'fp',
    ...overrides,
  };
}

function createResultSinks(initialLibraries: string[] = []) {
  let items: GameCard[] = [];
  let libraries = [...initialLibraries];

  const sinks = {
    setItems: vi.fn((nextItems: GameCard[]) => {
      items = nextItems;
    }),

    getLibraries: vi.fn(() => libraries),

    setLibraries: vi.fn((nextLibraries: string[]) => {
      libraries = [...nextLibraries];
    }),
  } satisfies GamesQueryResultSinks;

  return {
    sinks,
    getItems: () => items,
    getLibraries: () => libraries,
  };
}

function createReadySnapshot(
  scheduler: Scheduler,
  overrides: Partial<{
    version: number;
    searchQuery: string;
    selectedLibraries: readonly string[];
  }> = {},
): GamesQuerySnapshot {
  const snapshot = scheduler.createGamesQuerySnapshot(
    overrides.version ?? 1,
    true,
    true,
    overrides.searchQuery ?? '',
    overrides.selectedLibraries ?? [],
  );

  expect(snapshot).not.toBeNull();

  if (snapshot === null) {
    throw new Error('Snapshot must not be null');
  }

  return snapshot;
}

describe('createGamesScreenQueryScheduler', () => {
  describe('createGamesQuerySnapshot', () => {
    it.each([
      {
        filtersReady: false,
        preferenceLoaded: true,
        caseName: 'filters are not ready',
      },
      {
        filtersReady: true,
        preferenceLoaded: false,
        caseName: 'preferences are not loaded',
      },
      {
        filtersReady: false,
        preferenceLoaded: false,
        caseName: 'filters and preferences are not ready',
      },
    ])('returns null when $caseName', ({ filtersReady, preferenceLoaded }) => {
      const scheduler = createGamesScreenQueryScheduler({
        queryGameCardsFn: createQueryGameCardsMock(),
      });

      expect(
        scheduler.createGamesQuerySnapshot(1, filtersReady, preferenceLoaded, '', []),
      ).toBeNull();
    });

    it('normalizes search query and snapshots selected libraries', () => {
      const scheduler = createGamesScreenQueryScheduler({
        queryGameCardsFn: createQueryGameCardsMock(),
      });

      const selectedLibraries = ['Steam'];

      const snapshot = createReadySnapshot(scheduler, {
        searchQuery: '  cyberpunk  ',
        selectedLibraries,
      });

      selectedLibraries.push('Epic');

      expect(snapshot.searchQuery).toBe('cyberpunk');
      expect(snapshot.selectedLibraries).toEqual(['Steam']);
    });

    it('uses normalized search query when building request key', () => {
      const scheduler = createGamesScreenQueryScheduler({
        queryGameCardsFn: createQueryGameCardsMock(),
      });

      const normalizedSnapshot = createReadySnapshot(scheduler, {
        searchQuery: 'cyberpunk',
        selectedLibraries: ['Steam'],
      });

      const paddedSnapshot = createReadySnapshot(scheduler, {
        searchQuery: '  cyberpunk  ',
        selectedLibraries: ['Steam'],
      });

      expect(paddedSnapshot.requestKey).toBe(normalizedSnapshot.requestKey);
    });
  });

  describe('runGamesQuery', () => {
    it('passes normalized query, selected libraries and catalog defaults to API', async () => {
      const queryGameCardsFn = createQueryGameCardsMock();

      queryGameCardsFn.mockResolvedValueOnce(
        makeResult({
          items: [stubCard('game-1')],
          availableLibraries: ['Steam'],
        }),
      );

      const scheduler = createGamesScreenQueryScheduler({ queryGameCardsFn });
      const snapshot = createReadySnapshot(scheduler, {
        searchQuery: '  doom  ',
        selectedLibraries: ['Steam'],
      });
      const { sinks, getItems, getLibraries } = createResultSinks();

      await scheduler.runGamesQuery(snapshot, sinks);

      expect(queryGameCardsFn).toHaveBeenCalledTimes(1);
      expect(queryGameCardsFn).toHaveBeenCalledWith({
        searchQuery: 'doom',
        selectedLibraries: ['Steam'],
        sort: DEFAULT_GAME_CARDS_CATALOG_SORT,
        page: DEFAULT_GAME_CARDS_CATALOG_PAGE,
      });

      expect(getItems()).toEqual([stubCard('game-1')]);
      expect(getLibraries()).toEqual(['Steam']);
    });

    it('does not overwrite newer results when an older query resolves later', async () => {
      const staleResult = createDeferred<GameCardsResult>();
      const queryGameCardsFn = createQueryGameCardsMock();

      queryGameCardsFn.mockReturnValueOnce(staleResult.promise);
      queryGameCardsFn.mockResolvedValueOnce(
        makeResult({
          items: [stubCard('fresh')],
          availableLibraries: ['LibA'],
        }),
      );

      const scheduler = createGamesScreenQueryScheduler({ queryGameCardsFn });
      const { sinks, getItems, getLibraries } = createResultSinks();

      const staleSnapshot = createReadySnapshot(scheduler, {
        searchQuery: 'old',
      });
      const freshSnapshot = createReadySnapshot(scheduler, {
        searchQuery: 'new',
      });

      const staleRun = scheduler.runGamesQuery(staleSnapshot, sinks);
      const freshRun = scheduler.runGamesQuery(freshSnapshot, sinks);

      await freshRun;

      expect(getItems()).toEqual([stubCard('fresh')]);
      expect(getLibraries()).toEqual(['LibA']);

      staleResult.resolve(
        makeResult({
          items: [stubCard('stale')],
          availableLibraries: ['LibB'],
        }),
      );

      await staleRun;

      expect(getItems()).toEqual([stubCard('fresh')]);
      expect(getLibraries()).toEqual(['LibA']);
      expect(sinks.setItems).toHaveBeenCalledTimes(1);
      expect(sinks.setLibraries).toHaveBeenCalledTimes(1);
    });

    it('does not start the same active query twice', async () => {
      const result = createDeferred<GameCardsResult>();
      const queryGameCardsFn = createQueryGameCardsMock();

      queryGameCardsFn.mockReturnValue(result.promise);

      const scheduler = createGamesScreenQueryScheduler({ queryGameCardsFn });
      const snapshot = createReadySnapshot(scheduler, {
        searchQuery: 'doom',
      });
      const { sinks, getItems } = createResultSinks();

      const firstRun = scheduler.runGamesQuery(snapshot, sinks);
      const duplicateRun = scheduler.runGamesQuery(snapshot, sinks);

      expect(queryGameCardsFn).toHaveBeenCalledTimes(1);

      result.resolve(
        makeResult({
          items: [stubCard('doom')],
        }),
      );

      await Promise.all([firstRun, duplicateRun]);

      expect(queryGameCardsFn).toHaveBeenCalledTimes(1);
      expect(getItems()).toEqual([stubCard('doom')]);
    });

    it('does not start the same settled query twice', async () => {
      const queryGameCardsFn = createQueryGameCardsMock();

      queryGameCardsFn.mockResolvedValue(
        makeResult({
          items: [stubCard('game-1')],
        }),
      );

      const scheduler = createGamesScreenQueryScheduler({ queryGameCardsFn });
      const snapshot = createReadySnapshot(scheduler, {
        version: 1,
        searchQuery: 'doom',
      });
      const { sinks } = createResultSinks();

      await scheduler.runGamesQuery(snapshot, sinks);
      await scheduler.runGamesQuery(snapshot, sinks);

      expect(queryGameCardsFn).toHaveBeenCalledTimes(1);
    });

    it('allows the same query to run again when version changes', async () => {
      const queryGameCardsFn = createQueryGameCardsMock();

      queryGameCardsFn
        .mockResolvedValueOnce(
          makeResult({
            items: [stubCard('game-v1')],
          }),
        )
        .mockResolvedValueOnce(
          makeResult({
            items: [stubCard('game-v2')],
          }),
        );

      const scheduler = createGamesScreenQueryScheduler({ queryGameCardsFn });
      const { sinks, getItems } = createResultSinks();

      const firstSnapshot = createReadySnapshot(scheduler, {
        version: 1,
        searchQuery: 'doom',
      });

      const secondSnapshot = createReadySnapshot(scheduler, {
        version: 2,
        searchQuery: 'doom',
      });

      await scheduler.runGamesQuery(firstSnapshot, sinks);
      await scheduler.runGamesQuery(secondSnapshot, sinks);

      expect(queryGameCardsFn).toHaveBeenCalledTimes(2);
      expect(getItems()).toEqual([stubCard('game-v2')]);
    });

    it('does not update libraries when available libraries are unchanged', async () => {
      const queryGameCardsFn = createQueryGameCardsMock();

      queryGameCardsFn.mockResolvedValueOnce(
        makeResult({
          items: [stubCard('game-1')],
          availableLibraries: ['Steam', 'Epic'],
        }),
      );

      const scheduler = createGamesScreenQueryScheduler({ queryGameCardsFn });
      const snapshot = createReadySnapshot(scheduler);
      const { sinks, getLibraries } = createResultSinks(['Steam', 'Epic']);

      await scheduler.runGamesQuery(snapshot, sinks);

      expect(sinks.setLibraries).not.toHaveBeenCalled();
      expect(getLibraries()).toEqual(['Steam', 'Epic']);
    });

    it('updates libraries when available libraries are changed', async () => {
      const availableLibraries = ['Steam', 'Epic'];
      const queryGameCardsFn = createQueryGameCardsMock();

      queryGameCardsFn.mockResolvedValueOnce(
        makeResult({
          items: [stubCard('game-1')],
          availableLibraries,
        }),
      );

      const scheduler = createGamesScreenQueryScheduler({ queryGameCardsFn });
      const snapshot = createReadySnapshot(scheduler);
      const { sinks, getLibraries } = createResultSinks(['Steam']);

      await scheduler.runGamesQuery(snapshot, sinks);

      expect(sinks.setLibraries).toHaveBeenCalledTimes(1);
      expect(sinks.setLibraries).toHaveBeenCalledWith(['Steam', 'Epic']);
      expect(sinks.setLibraries.mock.calls[0]?.[0]).not.toBe(availableLibraries);
      expect(getLibraries()).toEqual(['Steam', 'Epic']);
    });

    it('logs current request errors and marks the failed query as handled', async () => {
      const error = new Error('Query failed.');
      const consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => undefined);
      const queryGameCardsFn = createQueryGameCardsMock();

      queryGameCardsFn.mockRejectedValue(error);

      const scheduler = createGamesScreenQueryScheduler({ queryGameCardsFn });
      const snapshot = createReadySnapshot(scheduler, {
        searchQuery: 'doom',
      });
      const { sinks } = createResultSinks();

      try {
        await scheduler.runGamesQuery(snapshot, sinks);
        await scheduler.runGamesQuery(snapshot, sinks);

        expect(queryGameCardsFn).toHaveBeenCalledTimes(1);
        expect(consoleErrorSpy).toHaveBeenCalledTimes(1);
        expect(consoleErrorSpy).toHaveBeenCalledWith('Failed to query game cards.', error);
      } finally {
        consoleErrorSpy.mockRestore();
      }
    });

    it('does not log stale request errors', async () => {
      const staleResult = createDeferred<GameCardsResult>();
      const consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => undefined);
      const queryGameCardsFn = createQueryGameCardsMock();

      queryGameCardsFn.mockReturnValueOnce(staleResult.promise);
      queryGameCardsFn.mockResolvedValueOnce(
        makeResult({
          items: [stubCard('fresh')],
        }),
      );

      const scheduler = createGamesScreenQueryScheduler({ queryGameCardsFn });
      const { sinks } = createResultSinks();

      const staleSnapshot = createReadySnapshot(scheduler, {
        searchQuery: 'old',
      });
      const freshSnapshot = createReadySnapshot(scheduler, {
        searchQuery: 'new',
      });

      const staleRun = scheduler.runGamesQuery(staleSnapshot, sinks);
      const freshRun = scheduler.runGamesQuery(freshSnapshot, sinks);

      await freshRun;

      staleResult.reject(new Error('Stale query failed.'));

      try {
        await staleRun;

        expect(consoleErrorSpy).not.toHaveBeenCalled();
      } finally {
        consoleErrorSpy.mockRestore();
      }
    });
  });

  describe('canRunGamesQuery', () => {
    it('returns false for active query and true again for a different query', async () => {
      const result = createDeferred<GameCardsResult>();
      const queryGameCardsFn = createQueryGameCardsMock();

      queryGameCardsFn.mockReturnValueOnce(result.promise);

      const scheduler = createGamesScreenQueryScheduler({ queryGameCardsFn });
      const { sinks } = createResultSinks();

      const activeSnapshot = createReadySnapshot(scheduler, {
        searchQuery: 'active',
      });
      const differentSnapshot = createReadySnapshot(scheduler, {
        searchQuery: 'different',
      });

      const run = scheduler.runGamesQuery(activeSnapshot, sinks);

      expect(scheduler.canRunGamesQuery(activeSnapshot.requestKey)).toBe(false);
      expect(scheduler.canRunGamesQuery(differentSnapshot.requestKey)).toBe(true);

      result.resolve(
        makeResult({
          items: [stubCard('active')],
        }),
      );

      await run;
    });

    it('returns false for already handled query', async () => {
      const queryGameCardsFn = createQueryGameCardsMock();

      queryGameCardsFn.mockResolvedValueOnce(
        makeResult({
          items: [stubCard('game-1')],
        }),
      );

      const scheduler = createGamesScreenQueryScheduler({ queryGameCardsFn });
      const snapshot = createReadySnapshot(scheduler, {
        searchQuery: 'doom',
      });
      const { sinks } = createResultSinks();

      await scheduler.runGamesQuery(snapshot, sinks);

      expect(scheduler.canRunGamesQuery(snapshot.requestKey)).toBe(false);
    });
  });
});
