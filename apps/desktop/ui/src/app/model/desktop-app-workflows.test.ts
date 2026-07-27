import { describe, expect, it, vi, beforeEach } from 'vitest';

import type { getGameDetails } from '@entities/game';
import { createGameDetails } from '@entities/game';

import type { OpenDesktopGameDeps } from './desktop-app-workflows';

const scanMocks = vi.hoisted(() => ({
  scanAutoLibrariesWithErrorRecovery: vi.fn(() => Promise.resolve({ kind: 'ok', errors: [] })),
  refreshRemoteManifests: vi.fn(() => Promise.resolve()),
  refreshCatalogCapabilities: vi.fn(() => Promise.resolve({ refreshed: true })),
  publishAutomaticLibraryScanFailedNotification: vi.fn(),
  publishPartialLibraryScanWarning: vi.fn(),
}));

vi.mock('@features/scan-libraries', () => ({
  scanAutoLibrariesWithErrorRecovery: scanMocks.scanAutoLibrariesWithErrorRecovery,
  refreshRemoteManifests: scanMocks.refreshRemoteManifests,
  refreshCatalogCapabilities: scanMocks.refreshCatalogCapabilities,
  publishAutomaticLibraryScanFailedNotification:
    scanMocks.publishAutomaticLibraryScanFailedNotification,
  publishPartialLibraryScanWarning: scanMocks.publishPartialLibraryScanWarning,
  selectManualScanFolder: vi.fn(),
  scanManualFolder: vi.fn(),
}));

import {
  loadAndPresentGameDetails,
  openDesktopGame,
  queueBackgroundCoverSync,
  reloadSelectedGame,
  runCatalogRefreshWithCoverSync,
  runUserCatalogRefresh,
} from './desktop-app-workflows';

describe('desktop-app-workflows', () => {
  it('queues non-blocking cover hydration for the current card snapshot', () => {
    const syncMissingCoversAfterCardsLoad = vi.fn(() => Promise.resolve());
    const queue = vi.fn();

    queueBackgroundCoverSync({
      coverSyncQueue: {
        queue,
        setAutoFetching: vi.fn(),
        autoFetchingIds: new Set<string>(),
      } as never,
      syncMissingCoversAfterCardsLoad,
    });

    expect(queue).toHaveBeenCalledWith(syncMissingCoversAfterCardsLoad, expect.any(Function));
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
    const runExclusiveCall = vi.fn();
    const runExclusive: OpenDesktopGameDeps['runExclusive'] = async <T>(task: () => Promise<T>) => {
      runExclusiveCall();
      return await task();
    };
    const loadGameDetails = vi.fn(() => Promise.resolve(undefined));

    await openDesktopGame('  raw-id  ', 'operations', {
      runExclusive,
      loadGameDetails,
      normalizeGameId: (gameId) => gameId.trim(),
    });

    expect(runExclusiveCall).toHaveBeenCalledTimes(1);
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

  describe('runUserCatalogRefresh', () => {
    beforeEach(() => {
      vi.clearAllMocks();
      scanMocks.scanAutoLibrariesWithErrorRecovery.mockResolvedValue({
        kind: 'ok',
        errors: [],
      });
      scanMocks.refreshRemoteManifests.mockResolvedValue(undefined);
      scanMocks.refreshCatalogCapabilities.mockResolvedValue({ refreshed: true });
    });

    function coverDeps() {
      return {
        runExclusive: <T>(task: () => Promise<T>) => task(),
        refreshGameCards: vi.fn(() => Promise.resolve()),
        coverSyncQueue: {
          queue: vi.fn((fn: () => Promise<void>) => {
            void fn();
          }),
          setAutoFetching: vi.fn(),
          autoFetchingIds: new Set<string>(),
        } as never,
        syncMissingCoversAfterCardsLoad: vi.fn(() => Promise.resolve()),
      };
    }

    it('force-refreshes remote manifests before scanning', async () => {
      const forceManifests = vi.fn(() => Promise.resolve());
      const deps = coverDeps();

      await runUserCatalogRefresh({
        ...deps,
        refreshRemoteManifests: forceManifests,
      });

      expect(forceManifests).toHaveBeenCalledTimes(1);
      expect(scanMocks.scanAutoLibrariesWithErrorRecovery).toHaveBeenCalledTimes(1);
      expect(scanMocks.refreshCatalogCapabilities).toHaveBeenCalledTimes(1);
      expect(deps.refreshGameCards).toHaveBeenCalledTimes(1);
      expect(forceManifests.mock.invocationCallOrder[0]).toBeLessThan(
        scanMocks.scanAutoLibrariesWithErrorRecovery.mock.invocationCallOrder[0],
      );
    });

    it('continues with scan when remote manifest force fails', async () => {
      const forceManifests = vi.fn(() => Promise.reject(new Error('cdn down')));
      const deps = coverDeps();

      await runUserCatalogRefresh({
        ...deps,
        refreshRemoteManifests: forceManifests,
      });

      expect(scanMocks.scanAutoLibrariesWithErrorRecovery).toHaveBeenCalledTimes(1);
      expect(deps.refreshGameCards).toHaveBeenCalledTimes(1);
    });
  });
});
