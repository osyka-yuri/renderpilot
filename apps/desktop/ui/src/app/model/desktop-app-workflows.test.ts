import { describe, expect, it, vi } from 'vitest';

import type { GameDetails, GameSummary, getGameDetails, queryGameCards } from '@entities/game';

import type { OpenDesktopGameDeps } from './desktop-app-workflows';

import {
  loadAndPresentGameDetails,
  openDesktopGame,
  refreshDesktopCatalog,
  reloadSelectedGame,
} from './desktop-app-workflows';

describe('desktop-app-workflows', () => {
  it('refreshDesktopCatalog loads cards and updates catalog state', async () => {
    const setGames = vi.fn();
    const incrementCatalogVersion = vi.fn();
    const clearSelectionIfSelectedGameMissing = vi.fn();

    const queryGameCardsMock = vi.fn<typeof queryGameCards>(() =>
      Promise.resolve({
        items: [{ id: 'game-1', title: 'Test Game' } as unknown as GameSummary],
        total: 1,
        hiddenCount: 0,
        availableLibraries: [],
        availableLaunchers: [],
        queryFingerprint: 'fp-1',
      }),
    );

    await refreshDesktopCatalog({
      queryGameCards: queryGameCardsMock,
      setGames,
      incrementCatalogVersion,
      clearSelectionIfSelectedGameMissing,
    });

    expect(setGames).toHaveBeenCalledWith([{ id: 'game-1', title: 'Test Game' }]);
    expect(incrementCatalogVersion).toHaveBeenCalledTimes(1);
    expect(clearSelectionIfSelectedGameMissing).toHaveBeenCalledTimes(1);
  });

  it('loadAndPresentGameDetails ignores stale requests', async () => {
    const presentGameDetails = vi.fn();

    const getGameDetailsMock = vi.fn<typeof getGameDetails>(() =>
      Promise.resolve({
        game: { identity: { id: 'game-1', title: 'Test Game' } },
        components: [],
        candidate_groups: [],
        operations: [],
      } as unknown as GameDetails),
    );

    await loadAndPresentGameDetails('game-1', 'details', {
      getGameDetails: getGameDetailsMock,
      beginDetailsRequest: () => 'request-1',
      isDetailsRequestActive: () => false,
      presentGameDetails,
    });

    expect(presentGameDetails).not.toHaveBeenCalled();
  });

  it('openDesktopGame normalizes ids and runs the loader exclusively', async () => {
    const runExclusive = vi.fn(
      async (task: () => Promise<unknown>) => await task(),
    ) as unknown as OpenDesktopGameDeps['runExclusive'];
    const loadGameDetails = vi.fn(() => Promise.resolve(undefined));

    await openDesktopGame('  raw-id  ', 'operations', {
      runExclusive,
      loadGameDetails,
      normalizeGameId: (gameId) => gameId.trim(),
    });

    expect(runExclusive).toHaveBeenCalledTimes(1);
    expect(loadGameDetails).toHaveBeenCalledWith('raw-id', 'operations');
  });

  it('reloadSelectedGame skips when there is no selection', async () => {
    const loadGameDetails = vi.fn(() => Promise.resolve(undefined));

    await reloadSelectedGame('details', {
      selectedGameId: null,
      loadGameDetails,
    });

    expect(loadGameDetails).not.toHaveBeenCalled();
  });
});
