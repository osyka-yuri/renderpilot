import { describe, expect, it, vi } from 'vitest';
import {
  DEFAULT_GAME_CARDS_CATALOG_PAGE,
  DEFAULT_GAME_CARDS_CATALOG_SORT,
  type GameSummary,
  type GameCardsResult,
  type queryGameCards,
  createGameSummary,
} from '@entities/game';
import {
  buildGameCardsQueryKey,
  createGamesPageQueryScheduler,
  type GamesQueryResultSinks,
  type GamesQuerySnapshot,
} from './games-page-query-scheduler';
import type { AddonCapability } from '@entities/game';

type QueryGameCardsFn = typeof queryGameCards;
type Scheduler = ReturnType<typeof createGamesPageQueryScheduler>;

function createQueryGameCardsMock() {
  return vi.fn<QueryGameCardsFn>();
}

function stubCard(gameId: string): GameSummary {
  return createGameSummary({ game_id: gameId });
}

function makeResult(
  overrides: Partial<GameCardsResult> & Pick<GameCardsResult, 'items'>,
): GameCardsResult {
  return {
    catalogSize: overrides.catalogSize ?? overrides.items.length,
    total: overrides.items.length,
    hiddenCount: 0,
    availableLibraries: [],
    availableLaunchers: [],
    catalogRevision: 1,
    nextOffset: null,
    ...overrides,
  };
}

function createResultSinks() {
  let items: GameSummary[] = [];

  const sinks = {
    setItems: vi.fn((nextItems: GameSummary[]) => {
      items = nextItems;
    }),
    setHiddenCount: vi.fn(),
  } satisfies GamesQueryResultSinks;

  return {
    sinks,
    getItems: () => items,
  };
}

function createReadySnapshot(
  scheduler: Scheduler,
  overrides: Partial<{
    version: number;
    searchQuery: string;
    selectedLibraries: readonly string[];
    selectedAddons: readonly AddonCapability[];
    selectedLaunchers: readonly string[];
  }> = {},
): GamesQuerySnapshot {
  const snapshot = scheduler.createGamesQuerySnapshot(
    overrides.version ?? 1,
    true,
    true,
    overrides.searchQuery ?? '',
    overrides.selectedLibraries ?? [],
    overrides.selectedAddons ?? ([] as readonly AddonCapability[]),
    overrides.selectedLaunchers ?? [],
    false,
    false,
  );

  expect(snapshot).not.toBeNull();

  if (snapshot === null) {
    throw new Error('Snapshot must not be null');
  }

  return snapshot;
}

describe('createGamesPageQueryScheduler', () => {
  describe('buildGameCardsQueryKey', () => {
    it('builds a stable key from search query and selected libraries', () => {
      const queryKey = buildGameCardsQueryKey('abc', ['x', 'y'], [], [], false, false);

      expect(JSON.parse(queryKey)).toEqual({
        searchQuery: 'abc',
        selectedLibraries: ['x', 'y'],
        selectedAddons: [],
        selectedLaunchers: [],
        showHidden: false,
        favoritesOnly: false,
        launcherOrder: [],
        sort: DEFAULT_GAME_CARDS_CATALOG_SORT,
        page: DEFAULT_GAME_CARDS_CATALOG_PAGE,
      });

      expect(queryKey).toBe(buildGameCardsQueryKey('abc', ['x', 'y'], [], [], false, false));
      expect(queryKey).not.toBe(buildGameCardsQueryKey('abc', ['x', 'z'], [], [], false, false));
      expect(queryKey).not.toBe(
        buildGameCardsQueryKey('changed', ['x', 'y'], [], [], false, false),
      );
    });
  });

  describe('createGamesQuerySnapshot', () => {
    it('canonicalizes non-semantic selection order and search casing', () => {
      const scheduler = createGamesPageQueryScheduler({
        queryGameCardsFn: createQueryGameCardsMock(),
      });
      const first = scheduler.createGamesQuerySnapshot(
        1,
        true,
        true,
        '  DOOM ',
        ['z', 'a'],
        ['renodx', 'luma'],
        ['Steam', 'Epic'],
        false,
        false,
      );
      const second = scheduler.createGamesQuerySnapshot(
        1,
        true,
        true,
        'doom',
        ['a', 'z'],
        ['luma', 'renodx'],
        ['Epic', 'Steam'],
        false,
        false,
      );

      expect(first?.requestKey).toBe(second?.requestKey);
    });

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
      const scheduler = createGamesPageQueryScheduler({
        queryGameCardsFn: createQueryGameCardsMock(),
      });

      expect(
        scheduler.createGamesQuerySnapshot(
          1,
          filtersReady,
          preferenceLoaded,
          '',
          [],
          [],
          [],
          false,
          false,
        ),
      ).toBeNull();
    });

    it('normalizes search query and snapshots selected libraries', () => {
      const scheduler = createGamesPageQueryScheduler({
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
      const scheduler = createGamesPageQueryScheduler({
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

      const scheduler = createGamesPageQueryScheduler({ queryGameCardsFn });
      const snapshot = createReadySnapshot(scheduler, {
        searchQuery: '  doom  ',
        selectedLibraries: ['Steam'],
      });
      const { sinks, getItems } = createResultSinks();

      await scheduler.runGamesQuery(snapshot, sinks);

      expect(queryGameCardsFn).toHaveBeenCalledTimes(1);
      expect(queryGameCardsFn).toHaveBeenCalledWith({
        searchQuery: 'doom',
        selectedLibraries: ['Steam'],
        selectedAddons: [],
        selectedLaunchers: [],
        launcherOrder: [],
        showHidden: false,
        favoritesOnly: false,
        sort: DEFAULT_GAME_CARDS_CATALOG_SORT,
        page: DEFAULT_GAME_CARDS_CATALOG_PAGE,
      });

      expect(getItems()).toEqual([stubCard('game-1')]);
    });

    it('does not overwrite newer results when an older query resolves later', async () => {
      const staleResult = Promise.withResolvers<GameCardsResult>();
      const queryGameCardsFn = createQueryGameCardsMock();

      queryGameCardsFn.mockReturnValueOnce(staleResult.promise);
      queryGameCardsFn.mockResolvedValueOnce(
        makeResult({
          items: [stubCard('fresh')],
          availableLibraries: ['LibA'],
        }),
      );

      const scheduler = createGamesPageQueryScheduler({ queryGameCardsFn });
      const { sinks, getItems } = createResultSinks();

      const staleSnapshot = createReadySnapshot(scheduler, {
        searchQuery: 'old',
      });
      const freshSnapshot = createReadySnapshot(scheduler, {
        searchQuery: 'new',
      });

      const staleRun = scheduler.runGamesQuery(staleSnapshot, sinks);
      const freshRun = scheduler.runGamesQuery(freshSnapshot, sinks);

      staleResult.resolve(
        makeResult({
          items: [stubCard('stale')],
          availableLibraries: ['LibB'],
        }),
      );

      await freshRun;
      await staleRun;

      expect(getItems()).toEqual([stubCard('fresh')]);
      expect(sinks.setItems).toHaveBeenCalledTimes(1);
    });

    it('does not start the same active query twice', async () => {
      const result = Promise.withResolvers<GameCardsResult>();
      const queryGameCardsFn = createQueryGameCardsMock();

      queryGameCardsFn.mockReturnValue(result.promise);

      const scheduler = createGamesPageQueryScheduler({ queryGameCardsFn });
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

      const scheduler = createGamesPageQueryScheduler({ queryGameCardsFn });
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

      const scheduler = createGamesPageQueryScheduler({ queryGameCardsFn });
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

    it('logs current request errors and marks the failed query as handled', async () => {
      const error = new Error('Query failed.');
      const consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => undefined);
      const queryGameCardsFn = createQueryGameCardsMock();

      queryGameCardsFn.mockRejectedValue(error);

      const scheduler = createGamesPageQueryScheduler({ queryGameCardsFn });
      const snapshot = createReadySnapshot(scheduler, {
        searchQuery: 'doom',
      });
      const { sinks } = createResultSinks();

      try {
        await scheduler.runGamesQuery(snapshot, sinks);
        await scheduler.runGamesQuery(snapshot, sinks);

        expect(queryGameCardsFn).toHaveBeenCalledTimes(1);
        expect(consoleErrorSpy).toHaveBeenCalledTimes(1);
        expect(consoleErrorSpy).toHaveBeenCalledWith(
          '[RenderPilot diagnostic]',
          {
            source: 'client-boundary',
            operation: 'query_game_cards',
            code: 'unexpected_client_error',
            contractStatus: 'malformed',
            severity: 'error',
          },
          error,
        );
      } finally {
        consoleErrorSpy.mockRestore();
      }
    });

    it('does not log stale request errors', async () => {
      const staleResult = Promise.withResolvers<GameCardsResult>();
      const consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => undefined);
      const queryGameCardsFn = createQueryGameCardsMock();

      queryGameCardsFn.mockReturnValueOnce(staleResult.promise);
      queryGameCardsFn.mockResolvedValueOnce(
        makeResult({
          items: [stubCard('fresh')],
        }),
      );

      const scheduler = createGamesPageQueryScheduler({ queryGameCardsFn });
      const { sinks } = createResultSinks();

      const staleSnapshot = createReadySnapshot(scheduler, {
        searchQuery: 'old',
      });
      const freshSnapshot = createReadySnapshot(scheduler, {
        searchQuery: 'new',
      });

      const staleRun = scheduler.runGamesQuery(staleSnapshot, sinks);
      const freshRun = scheduler.runGamesQuery(freshSnapshot, sinks);

      staleResult.reject(new Error('Stale query failed.'));

      try {
        await freshRun;
        await staleRun;

        expect(consoleErrorSpy).not.toHaveBeenCalled();
      } finally {
        consoleErrorSpy.mockRestore();
      }
    });
  });

  describe('response revision safety', () => {
    it('rejects a response from an older catalog revision', async () => {
      const queryGameCardsFn = createQueryGameCardsMock();
      queryGameCardsFn
        .mockResolvedValueOnce(makeResult({ items: [stubCard('new')], catalogRevision: 2 }))
        .mockResolvedValueOnce(makeResult({ items: [stubCard('old')], catalogRevision: 1 }));
      const scheduler = createGamesPageQueryScheduler({ queryGameCardsFn });
      const { sinks, getItems } = createResultSinks();

      await scheduler.runGamesQuery(
        createReadySnapshot(scheduler, { searchQuery: 'first' }),
        sinks,
      );
      await scheduler.runGamesQuery(
        createReadySnapshot(scheduler, { searchQuery: 'second' }),
        sinks,
      );

      expect(getItems().map((game) => game.game_id)).toEqual(['new']);
    });

    it('rejects a page from another revision and allows an atomic page-zero restart', async () => {
      const queryGameCardsFn = createQueryGameCardsMock();
      queryGameCardsFn.mockImplementation((query) => {
        return Promise.resolve(
          makeResult({
            items: [stubCard(query.page.offset === 0 ? 'restarted' : 'mixed-page')],
            catalogRevision: 2,
          }),
        );
      });
      const scheduler = createGamesPageQueryScheduler({ queryGameCardsFn });
      const base = createReadySnapshot(scheduler);
      const page = scheduler.createPageQuerySnapshot(base, 120, 1);
      const { sinks, getItems } = createResultSinks();
      const onCatalogRevisionMismatch = vi.fn();

      await scheduler.runGamesQuery(page, { ...sinks, onCatalogRevisionMismatch });

      expect(getItems()).toEqual([]);
      expect(onCatalogRevisionMismatch).toHaveBeenCalledOnce();
      expect(onCatalogRevisionMismatch).toHaveBeenCalledWith(2);

      const restart = scheduler.createRevisionRestartSnapshot(base, 2);
      await scheduler.runGamesQuery(restart, sinks);
      expect(getItems().map((game) => game.game_id)).toEqual(['restarted']);
    });
  });

  describe('canRunGamesQuery', () => {
    it('returns false for active query and true again for a different query', async () => {
      const result = Promise.withResolvers<GameCardsResult>();
      const queryGameCardsFn = createQueryGameCardsMock();

      queryGameCardsFn.mockReturnValueOnce(result.promise);

      const scheduler = createGamesPageQueryScheduler({ queryGameCardsFn });
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

      const scheduler = createGamesPageQueryScheduler({ queryGameCardsFn });
      const snapshot = createReadySnapshot(scheduler, {
        searchQuery: 'doom',
      });
      const { sinks } = createResultSinks();

      await scheduler.runGamesQuery(snapshot, sinks);

      expect(scheduler.canRunGamesQuery(snapshot.requestKey)).toBe(false);
    });
  });
});
