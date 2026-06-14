import {
  publishApplyCompletedNotification,
  publishRollbackCompletedNotification,
  rollbackComponent,
} from '@entities/operation';
import { publishErrorNotification } from '@shared/notifications';
import { t } from '@shared/i18n';
import { executeGraphicsSwap } from '@features/swap-graphics-component';
import { clearDownloadProgress } from '@entities/library';

import type { BulkSwapItem } from './streamline-versions';

export type SwapHandler = (
  componentId: string,
  artifactId: string,
  isDownloaded: boolean,
) => Promise<void> | void;

export type RollbackHandler = (componentId: string) => Promise<void> | void;

export type BulkSwapHandler = (items: BulkSwapItem[]) => Promise<void> | void;

export type BulkRollbackHandler = (componentIds: string[]) => Promise<void> | void;

export type GameDetailsPageModelDeps = {
  getSelectedGameId: () => string | null;
  checkIsGameStillSelected: (gameId: string) => boolean;
  runExclusive: <T>(task: () => Promise<T>) => Promise<T | null>;
  reloadGameDetails: () => Promise<void>;
};

export function createGameDetailsPageModel(deps: GameDetailsPageModelDeps) {
  async function runForSelectedGame<T>(task: (gameId: string) => Promise<T>): Promise<T | null> {
    const gameId = deps.getSelectedGameId();

    if (gameId === null) {
      return null;
    }

    return deps.runExclusive(() => task(gameId));
  }

  async function runForSelectedGameWithSignal<T>(
    task: (gameId: string, signal: AbortSignal) => Promise<T>,
  ): Promise<T | null> {
    return runForSelectedGame((gameId) => {
      const controller = new AbortController();
      if (!deps.checkIsGameStillSelected(gameId)) {
        controller.abort();
      }

      return task(gameId, controller.signal);
    });
  }

  async function handleSwap(
    componentId: string,
    artifactId: string,
    isDownloaded: boolean,
  ): Promise<void> {
    clearDownloadProgress([artifactId]);
    const result = await runForSelectedGameWithSignal(async (gameId, signal) => {
      const appliedOperation = await executeGraphicsSwap({
        gameId,
        componentId,
        artifactId,
        isDownloaded,
        signal,
      });

      if (appliedOperation === null) {
        return null;
      }

      await deps.reloadGameDetails();

      return appliedOperation;
    });

    if (result !== null) {
      publishApplyCompletedNotification(1);
    }
  }

  /**
   * Runs an async operation across many items inside a single exclusive session:
   * skips when no game is selected or the list is empty, isolates per-item
   * failures, reloads details once at the end, and reports how many succeeded vs
   * failed (`perItem` returns `true` when the item counts as applied).
   *
   * The shared `AbortSignal` is tripped when the active game changed before the
   * lock was acquired — swaps honor it to skip stale work; rollback ignores it,
   * matching the single-item rollback.
   */
  async function runBatch<T>(
    items: readonly T[],
    perItem: (gameId: string, item: T, signal: AbortSignal) => Promise<boolean>,
  ): Promise<{ succeeded: number; failed: number } | null> {
    if (items.length === 0) {
      return null;
    }

    return runForSelectedGameWithSignal(async (gameId, signal) => {
      let succeeded = 0;
      let failed = 0;

      for (const item of items) {
        try {
          if (await perItem(gameId, item, signal)) {
            succeeded += 1;
          }
        } catch {
          failed += 1;
        }
      }

      await deps.reloadGameDetails();

      return { succeeded, failed };
    });
  }

  /**
   * Applies a set of component swaps in one exclusive run, reusing the
   * per-component download-then-apply path. Serves both the Streamline "bundle
   * swap" and the page-level "update all" button — failures are isolated and the
   * applied count is reported, with any failures surfaced in aggregate.
   */
  async function handleBulkSwap(items: BulkSwapItem[]): Promise<void> {
    clearDownloadProgress(items.map((item) => item.artifactId));
    const outcome = await runBatch(items, async (gameId, item, signal) => {
      const appliedOperation = await executeGraphicsSwap({
        gameId,
        componentId: item.componentId,
        artifactId: item.artifactId,
        isDownloaded: item.isDownloaded,
        signal,
      });
      return appliedOperation !== null;
    });

    if (outcome === null) {
      return;
    }

    if (outcome.succeeded > 0) {
      publishApplyCompletedNotification(outcome.succeeded);
    }

    if (outcome.failed > 0) {
      publishErrorNotification(
        t('notify.swapBatchFailed.title'),
        t('notify.swapBatchFailed.description', { failed: outcome.failed, total: items.length }),
      );
    }
  }

  async function handleRollback(componentId: string): Promise<void> {
    const result = await runForSelectedGame(async (gameId) => {
      const rollbackResult = await rollbackComponent(gameId, componentId);
      await deps.reloadGameDetails();
      return rollbackResult;
    });

    if (result !== null) {
      publishRollbackCompletedNotification(1);
    }
  }

  /**
   * Restores several components to their pre-RenderPilot `.bak` originals in one
   * run — the bulk counterpart to the per-plugin rollback.
   */
  async function handleBulkRollback(componentIds: string[]): Promise<void> {
    const outcome = await runBatch(componentIds, async (gameId, componentId) => {
      await rollbackComponent(gameId, componentId);
      return true;
    });

    if (outcome === null) {
      return;
    }

    if (outcome.succeeded > 0) {
      publishRollbackCompletedNotification(outcome.succeeded);
    }

    if (outcome.failed > 0) {
      publishErrorNotification(
        t('notify.rollbackBatchFailed.title'),
        t('notify.rollbackBatchFailed.description', {
          failed: outcome.failed,
          total: componentIds.length,
        }),
      );
    }
  }

  return {
    handleSwap,
    handleRollback,
    handleBulkSwap,
    handleBulkRollback,
  };
}
