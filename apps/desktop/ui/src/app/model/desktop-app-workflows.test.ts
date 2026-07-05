import { describe, expect, it, vi } from 'vitest';

import type { getGameDetails, queryGameCards } from '@entities/game';
import { createGameDetails, createGameSummary } from '@entities/game';

import type { OpenDesktopGameDeps } from './desktop-app-workflows';

const scanMocks = vi.hoisted(() => ({
  scanAutoLibrariesWithErrorRecovery: vi.fn(() => Promise.resolve({ kind: 'ok', errors: [] })),
  publishAutomaticLibraryScanFailedNotification: vi.fn(),
  publishPartialLibraryScanWarning: vi.fn(),
}));

vi.mock('@features/scan-libraries', () => ({
  scanAutoLibrariesWithErrorRecovery: scanMocks.scanAutoLibrariesWithErrorRecovery,
  publishAutomaticLibraryScanFailedNotification:
    scanMocks.publishAutomaticLibraryScanFailedNotification,
  publishPartialLibraryScanWarning: scanMocks.publishPartialLibraryScanWarning,
  selectManualScanFolder: vi.fn(),
  scanManualFolder: vi.fn(),
}));

import {
  loadAndPresentGameDetails,
  openDesktopGame,
  refreshDesktopCatalog,
  reloadSelectedGame,
  runCatalogRefreshWithCoverSync,
  scanAutoLibrariesAndRefreshCards,
} from './desktop-app-workflows';

describe('desktop-app-workflows', () => {
  it('refreshDesktopCatalog loads cards and updates catalog state', async () => {
    const setGames = vi.fn();
    const incrementCatalogVersion = vi.fn();
    const clearSelectionIfSelectedGameMissing = vi.fn();

    const cards = [createGameSummary({ game_id: 'game-1', title: 'Test Game' })];
    const queryGameCardsMock = vi.fn<typeof queryGameCards>(() =>
      Promise.resolve({
        items: cards,
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

    expect(setGames).toHaveBeenCalledWith(cards);
    expect(incrementCatalogVersion).toHaveBeenCalledTimes(1);
    expect(clearSelectionIfSelectedGameMissing).toHaveBeenCalledTimes(1);
  });

  it('loadAndPresentGameDetails ignores stale requests', async () => {
    const presentGameDetails = vi.fn();

    const getGameDetailsMock = vi.fn<typeof getGameDetails>(() =>
      Promise.resolve(
        createGameDetails({
          game: {
            identity: { id: 'game-1', title: 'Test Game', launcher: 'Manual' },
            platform: 'Windows',
            runtime: 'NativeWindows',
            install_path: '/test',
            executable_candidates: [],
          },
        }),
      ),
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

  it('runCatalogRefreshWithCoverSync refreshes and queues cover sync on success', async () => {
    const refreshGameCards = vi.fn(() => Promise.resolve());
    const syncMissingCoversAfterCardsLoad = vi.fn(() => Promise.resolve());
    const queue = vi.fn((fn: () => Promise<void>) => {
      void fn();
    });

    await runCatalogRefreshWithCoverSync(() => Promise.resolve(true), {
      runExclusive: (task) => task(),
      refreshGameCards,
      coverSyncQueue: {
        queue,
        setAutoFetching: vi.fn(),
        autoFetchingIds: new Set<string>(),
      } as never,
      syncMissingCoversAfterCardsLoad,
    });

    expect(refreshGameCards).toHaveBeenCalledTimes(1);
    expect(queue).toHaveBeenCalledTimes(1);
    expect(syncMissingCoversAfterCardsLoad).toHaveBeenCalledTimes(1);
  });

  it('runCatalogRefreshWithCoverSync skips refresh when prepare cancels', async () => {
    const refreshGameCards = vi.fn(() => Promise.resolve());
    const queue = vi.fn();

    await runCatalogRefreshWithCoverSync(() => Promise.resolve(false), {
      runExclusive: (task) => task(),
      refreshGameCards,
      coverSyncQueue: {
        queue,
        setAutoFetching: vi.fn(),
        autoFetchingIds: new Set<string>(),
      } as never,
      syncMissingCoversAfterCardsLoad: vi.fn(),
    });

    expect(refreshGameCards).not.toHaveBeenCalled();
    expect(queue).not.toHaveBeenCalled();
  });

  it('scanAutoLibrariesAndRefreshCards runs the scan before refreshing cards', async () => {
    scanMocks.scanAutoLibrariesWithErrorRecovery.mockResolvedValue({
      kind: 'ok',
      errors: [],
    });

    await scanAutoLibrariesAndRefreshCards({
      runExclusive: (task) => task(),
      refreshGameCards: vi.fn(() => Promise.resolve()),
      coverSyncQueue: {
        queue: vi.fn(),
        setAutoFetching: vi.fn(),
        autoFetchingIds: new Set<string>(),
      } as never,
      syncMissingCoversAfterCardsLoad: vi.fn(),
    });

    expect(scanMocks.scanAutoLibrariesWithErrorRecovery).toHaveBeenCalledTimes(1);
  });
});
