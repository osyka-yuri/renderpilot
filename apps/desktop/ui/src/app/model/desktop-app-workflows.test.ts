import { describe, expect, it, vi, beforeEach } from 'vitest';

import type { getGameDetails } from '@entities/game';
import { createGameDetails } from '@entities/game';
import type { AddGameInspection } from '@features/scan-libraries';

import type { OpenDesktopGameDeps } from './desktop-app-workflows';

const scanMocks = vi.hoisted(() => ({
  scanAutoLibrariesWithErrorRecovery: vi.fn(() => Promise.resolve({ kind: 'ok', errors: [] })),
  refreshRemoteManifests: vi.fn(() => Promise.resolve()),
  refreshCatalogCapabilities: vi.fn(() => Promise.resolve({ refreshed: true })),
  publishAutomaticLibraryScanFailedNotification: vi.fn(),
  publishAddGameWarnings: vi.fn(),
  publishPartialLibraryScanWarning: vi.fn(),
  addGame: vi.fn(),
}));

vi.mock('@features/scan-libraries', () => ({
  scanAutoLibrariesWithErrorRecovery: scanMocks.scanAutoLibrariesWithErrorRecovery,
  refreshRemoteManifests: scanMocks.refreshRemoteManifests,
  refreshCatalogCapabilities: scanMocks.refreshCatalogCapabilities,
  publishAutomaticLibraryScanFailedNotification:
    scanMocks.publishAutomaticLibraryScanFailedNotification,
  publishAddGameWarnings: scanMocks.publishAddGameWarnings,
  publishPartialLibraryScanWarning: scanMocks.publishPartialLibraryScanWarning,
  addGame: scanMocks.addGame,
}));

import {
  loadAndPresentGameDetails,
  openDesktopGame,
  queueBackgroundCoverSync,
  reloadSelectedGame,
  removeGameAndRefreshCards,
  rollbackRootCorrectionComponents,
  runCatalogRefreshWithCoverSync,
  runUserCatalogRefresh,
  submitAddGameAndRefreshCards,
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
            can_remove_from_catalog: true,
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

  it('openDesktopGame preloads the page before starting the exclusive details request', async () => {
    const calls: string[] = [];
    const preloadPage = vi.fn(() => {
      calls.push('preload');
    });
    const runExclusiveCall = vi.fn();
    const runExclusive: OpenDesktopGameDeps['runExclusive'] = async <T>(task: () => Promise<T>) => {
      runExclusiveCall();
      calls.push('exclusive');
      return await task();
    };
    const loadGameDetails = vi.fn(() => {
      calls.push('details');
      return Promise.resolve(undefined);
    });

    await openDesktopGame('  raw-id  ', 'operations', {
      preloadPage,
      runExclusive,
      loadGameDetails,
      normalizeGameId: (gameId) => gameId.trim(),
    });

    expect(preloadPage).toHaveBeenCalledTimes(1);
    expect(runExclusiveCall).toHaveBeenCalledTimes(1);
    expect(loadGameDetails).toHaveBeenCalledWith('raw-id', 'operations');
    expect(calls).toEqual(['preload', 'exclusive', 'details']);
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

  describe('submitAddGameAndRefreshCards', () => {
    beforeEach(() => {
      vi.clearAllMocks();
    });

    function inspection(): AddGameInspection {
      return {
        selectedRoot: 'D:/Games/The Last of Us Part I',
        inspectionFingerprint: 'inspection:test',
        catalogGeneration: 11,
        boundary: {
          kind: 'single_install',
          completeness: 'complete',
          candidateRoots: ['D:/Games/The Last of Us Part I'],
          evidence: ['root_executable'],
        },
        recommendation: {
          root: 'D:/Games',
          source: 'existing_catalog',
          confidence: 'suggested',
          completeness: 'complete',
          evidence: [],
        },
        relationship: {
          kind: 'inside_existing' as const,
          gameIds: ['game:oversized-root'],
          provenInstallRoots: [],
        },
        executables: [],
        requiresExplicitExecutable: false,
        rootCorrection: {
          gameId: 'game:existing',
          status: 'ready',
          cleanupActions: [],
          blockers: [],
        },
        decision: {
          kind: 'review',
          defaultOption: {
            rootChoice: 'selected',
            catalogAction: 'correct_existing_root',
          },
          options: [
            {
              rootChoice: 'selected',
              catalogAction: 'correct_existing_root',
            },
          ],
        },
        warnings: [],
      };
    }

    function addDeps() {
      const coverQueue = vi.fn();
      return {
        runExclusive: <T>(task: () => Promise<T>): Promise<T | null> => task(),
        refreshGameCards: vi.fn(() => Promise.resolve()),
        coverSyncQueue: {
          queue: coverQueue,
          setAutoFetching: vi.fn(),
          autoFetchingIds: new Set<string>(),
        } as never,
        coverQueue,
        syncMissingCoversAfterCardsLoad: vi.fn(() => Promise.resolve()),
      };
    }

    it('submits one explicit correction and refreshes the catalog', async () => {
      scanMocks.addGame.mockResolvedValue({
        gameId: 'game:oversized-root',
        effectiveRoot: 'D:/Games/The Last of Us Part I',
        disposition: 'root_corrected',
        rootAuthority: 'user_confirmed',
        detectedLibraryCount: 1,
        consolidatedGameIds: [],
        recoveryBundlePath: null,
        warnings: [],
      });
      const deps = addDeps();

      const result = await submitAddGameAndRefreshCards(
        inspection(),
        {
          rootChoice: 'selected',
          allowRootCorrection: true,
          chosenExecutable: null,
        },
        deps,
      );

      expect(result?.gameId).toBe('game:oversized-root');
      expect(scanMocks.addGame).toHaveBeenCalledWith(
        expect.objectContaining({
          selectedRoot: 'D:/Games/The Last of Us Part I',
          rootChoice: 'selected',
          allowRootCorrection: true,
        }),
      );
      expect(deps.refreshGameCards).toHaveBeenCalledOnce();
      expect(scanMocks.publishAddGameWarnings).toHaveBeenCalledWith(result);
      expect(deps.coverQueue).toHaveBeenCalledOnce();
    });

    it('does not publish completion effects when the exclusive runner reports an error', async () => {
      const deps = {
        ...addDeps(),
        runExclusive: <T>(_task: () => Promise<T>): Promise<T | null> => Promise.resolve(null),
      };

      const result = await submitAddGameAndRefreshCards(
        inspection(),
        {
          rootChoice: 'selected',
          allowRootCorrection: true,
          chosenExecutable: null,
        },
        deps,
      );

      expect(result).toBeNull();
      expect(scanMocks.publishAddGameWarnings).not.toHaveBeenCalled();
      expect(deps.coverQueue).not.toHaveBeenCalled();
    });
  });

  describe('rollbackRootCorrectionComponents', () => {
    it('rolls back the approved components sequentially and refreshes once', async () => {
      const calls: string[] = [];
      const rollbackComponent = vi.fn((_gameId: string, componentId: string) => {
        calls.push(`rollback:${componentId}`);
        return Promise.resolve();
      });
      const refreshGameCards = vi.fn(() => {
        calls.push('refresh');
        return Promise.resolve();
      });

      await rollbackRootCorrectionComponents(
        'game:oversized-root',
        ['component:a', 'component:b'],
        { rollbackComponent, refreshGameCards },
      );

      expect(calls).toEqual(['rollback:component:a', 'rollback:component:b', 'refresh']);
    });

    it('stops after a rollback failure, refreshes visible state, and preserves the exact error', async () => {
      const rollbackFailure = new Error('baseline verification failed');
      const rollbackComponent = vi.fn((_gameId: string, componentId: string) => {
        if (componentId === 'component:b') {
          return Promise.reject(rollbackFailure);
        }
        return Promise.resolve();
      });
      const refreshGameCards = vi.fn(() => Promise.resolve());

      await expect(
        rollbackRootCorrectionComponents(
          'game:oversized-root',
          ['component:a', 'component:b', 'component:c'],
          { rollbackComponent, refreshGameCards },
        ),
      ).rejects.toBe(rollbackFailure);

      expect(rollbackComponent).toHaveBeenCalledTimes(2);
      expect(refreshGameCards).toHaveBeenCalledOnce();
    });

    it('does not let a refresh failure hide the rollback failure', async () => {
      const rollbackFailure = new Error('rollback failed');
      const rollbackComponent = vi.fn(() => Promise.reject(rollbackFailure));
      const refreshGameCards = vi.fn(() => Promise.reject(new Error('refresh failed')));

      await expect(
        rollbackRootCorrectionComponents('game:oversized-root', ['component:a'], {
          rollbackComponent,
          refreshGameCards,
        }),
      ).rejects.toBe(rollbackFailure);
    });
  });

  describe('removeGameAndRefreshCards', () => {
    it('removes the card and refreshes the visible catalog under one exclusive task', async () => {
      const calls: string[] = [];
      const removeGame = vi.fn((gameId: string) => {
        calls.push(`remove:${gameId}`);
        return Promise.resolve({ gameId });
      });
      const refreshGameCards = vi.fn(() => {
        calls.push('refresh');
        return Promise.resolve();
      });

      const removed = await removeGameAndRefreshCards('game:oversized-root', {
        runExclusive: async (task) => {
          calls.push('exclusive');
          return await task();
        },
        refreshGameCards,
        removeGame,
      });

      expect(removed).toBe(true);
      expect(calls).toEqual(['exclusive', 'remove:game:oversized-root', 'refresh']);
    });

    it('does not refresh when the exclusive task cannot start', async () => {
      const refreshGameCards = vi.fn(() => Promise.resolve());
      const removeGame = vi.fn((gameId: string) => Promise.resolve({ gameId }));

      const removed = await removeGameAndRefreshCards('game:missing', {
        runExclusive: () => Promise.resolve(null),
        refreshGameCards,
        removeGame,
      });

      expect(removed).toBe(false);
      expect(removeGame).not.toHaveBeenCalled();
      expect(refreshGameCards).not.toHaveBeenCalled();
    });
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
