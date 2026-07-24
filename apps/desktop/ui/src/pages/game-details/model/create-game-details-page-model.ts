import {
  publishApplyCompletedNotification,
  publishRollbackCompletedNotification,
  rollbackComponent,
} from '@entities/operation';
import { publishCommandErrorNotification, publishErrorNotification } from '@shared/notifications';
import { t, type MessageKey } from '@shared/i18n';
import { executeGraphicsSwap } from '@features/swap-graphics-component';
import { clearDownloadProgress } from '@shared/lib';

import type { BulkSwapItem } from './streamline-versions';
import type { SwapRequest } from './swap-request';

export type SwapHandler = (request: SwapRequest) => Promise<void> | void;

export type RollbackHandler = (componentId: string) => Promise<void> | void;

export type BulkSwapHandler = (items: BulkSwapItem[]) => Promise<void> | void;

export type BulkRollbackHandler = (componentIds: string[]) => Promise<void> | void;

export type GameDetailsPageModelDeps = {
  getSelectedGameId: () => string | null;
  checkIsGameStillSelected: (gameId: string) => boolean;
  runExclusive: <T>(task: () => Promise<T>) => Promise<T | null>;
  reloadGameDetails: () => Promise<void>;
};

type BatchOutcome = {
  successfulItems: number;
  completedFileCount: number;
  failedItems: number;
  firstFailure: { error: unknown } | null;
};

type BatchFailureMessages = {
  titleKey: MessageKey;
  descriptionKey: MessageKey;
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

  async function handleSwap(request: SwapRequest): Promise<void> {
    clearDownloadProgress([request.artifactId]);
    const result = await runForSelectedGameWithSignal(async (gameId, signal) => {
      try {
        return await executeGraphicsSwap({
          gameId,
          componentId: request.componentId,
          artifactId: request.artifactId,
          isDownloaded: request.isDownloaded,
          confirmationToken: request.confirmationToken,
          signal,
        });
      } finally {
        // Refresh even on failure so invalidated stale sources leave the candidate list.
        await deps.reloadGameDetails();
      }
    });

    if (result !== null) {
      if (result.d3d12_executable_action) {
        publishApplyCompletedNotification(
          result.updated_file_count,
          result.d3d12_executable_action,
        );
      } else {
        publishApplyCompletedNotification(result.updated_file_count);
      }
    }
  }

  /** Exclusive multi-item run: isolate failures, reload once, keep first error. */
  async function runBatch<T>(
    items: readonly T[],
    perItem: (gameId: string, item: T, signal: AbortSignal) => Promise<number | null>,
  ): Promise<BatchOutcome | null> {
    if (items.length === 0) {
      return null;
    }

    return runForSelectedGameWithSignal(async (gameId, signal) => {
      let successfulItems = 0;
      let completedFileCount = 0;
      let failedItems = 0;
      let firstFailure: BatchOutcome['firstFailure'] = null;

      for (const item of items) {
        try {
          const completedFiles = await perItem(gameId, item, signal);
          if (completedFiles !== null) {
            successfulItems += 1;
            completedFileCount += completedFiles;
          }
        } catch (error) {
          failedItems += 1;
          firstFailure ??= { error };
        }
      }

      await deps.reloadGameDetails();

      return { successfulItems, completedFileCount, failedItems, firstFailure };
    });
  }

  /**
   * One failure → typed command error only; several → typed first error plus
   * aggregate count toast.
   */
  function publishBatchFailures(
    outcome: BatchOutcome,
    total: number,
    aggregate: BatchFailureMessages,
  ): void {
    if (outcome.failedItems <= 0) {
      return;
    }

    if (outcome.firstFailure !== null) {
      publishCommandErrorNotification(outcome.firstFailure.error);
    }

    if (outcome.failedItems > 1 || outcome.firstFailure === null) {
      publishErrorNotification(
        t(aggregate.titleKey),
        t(aggregate.descriptionKey, { failed: outcome.failedItems, total }),
      );
    }
  }

  /** Bulk download-then-apply (Streamline bundle swap / update-all). */
  async function handleBulkSwap(items: BulkSwapItem[]): Promise<void> {
    clearDownloadProgress(items.map((item) => item.artifactId));
    const outcome = await runBatch(items, async (gameId, item, signal) => {
      const appliedOperation = await executeGraphicsSwap({
        gameId,
        componentId: item.componentId,
        artifactId: item.artifactId,
        isDownloaded: item.isDownloaded,
        confirmationToken: item.confirmationToken,
        signal,
      });
      return appliedOperation?.updated_file_count ?? null;
    });

    if (outcome === null) {
      return;
    }

    if (outcome.successfulItems > 0) {
      publishApplyCompletedNotification(outcome.completedFileCount);
    }

    publishBatchFailures(outcome, items.length, {
      titleKey: 'notify.swapBatchFailed.title',
      descriptionKey: 'notify.swapBatchFailed.description',
    });
  }

  async function handleRollback(componentId: string): Promise<void> {
    const result = await runForSelectedGame(async (gameId) => {
      const rollbackResult = await rollbackComponent(gameId, componentId);
      await deps.reloadGameDetails();
      return rollbackResult;
    });

    if (result !== null) {
      publishRollbackCompletedNotification(result.restored_file_count);
    }
  }

  /** Bulk restore to pre-RenderPilot `.bak` originals. */
  async function handleBulkRollback(componentIds: string[]): Promise<void> {
    const outcome = await runBatch(componentIds, async (gameId, componentId) => {
      const rollbackResult = await rollbackComponent(gameId, componentId);
      return rollbackResult.restored_file_count;
    });

    if (outcome === null) {
      return;
    }

    if (outcome.successfulItems > 0) {
      publishRollbackCompletedNotification(outcome.completedFileCount);
    }

    publishBatchFailures(outcome, componentIds.length, {
      titleKey: 'notify.rollbackBatchFailed.title',
      descriptionKey: 'notify.rollbackBatchFailed.description',
    });
  }

  return {
    handleSwap,
    handleRollback,
    handleBulkSwap,
    handleBulkRollback,
  };
}
