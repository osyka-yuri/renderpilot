import {
  publishApplyCompletedNotification,
  publishRollbackCompletedNotification,
  rollbackComponent,
} from '@entities/operation';
import { publishCommandErrorNotification, publishErrorNotification } from '@shared/notifications';
import { t, type MessageKey } from '@shared/i18n';
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

type BatchOutcome = {
  succeeded: number;
  failed: number;
  hasFirstError: boolean;
  firstError: unknown;
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

  async function handleSwap(
    componentId: string,
    artifactId: string,
    isDownloaded: boolean,
  ): Promise<void> {
    clearDownloadProgress([artifactId]);
    const result = await runForSelectedGameWithSignal(async (gameId, signal) => {
      try {
        return await executeGraphicsSwap({
          gameId,
          componentId,
          artifactId,
          isDownloaded,
          signal,
        });
      } finally {
        // Refresh even on failure so invalidated stale sources leave the candidate list.
        await deps.reloadGameDetails();
      }
    });

    if (result !== null) {
      publishApplyCompletedNotification(1);
    }
  }

  /** Exclusive multi-item run: isolate failures, reload once, keep first error. */
  async function runBatch<T>(
    items: readonly T[],
    perItem: (gameId: string, item: T, signal: AbortSignal) => Promise<boolean>,
  ): Promise<BatchOutcome | null> {
    if (items.length === 0) {
      return null;
    }

    return runForSelectedGameWithSignal(async (gameId, signal) => {
      let succeeded = 0;
      let failed = 0;
      let hasFirstError = false;
      let firstError: unknown;

      for (const item of items) {
        try {
          if (await perItem(gameId, item, signal)) {
            succeeded += 1;
          }
        } catch (error) {
          failed += 1;
          if (!hasFirstError) {
            hasFirstError = true;
            firstError = error;
          }
        }
      }

      await deps.reloadGameDetails();

      return { succeeded, failed, hasFirstError, firstError };
    });
  }

  /**
   * One failure → typed command error only; several → typed first error plus
   * aggregate count toast.
   */
  function publishBatchFailures(
    failed: number,
    total: number,
    hasFirstError: boolean,
    firstError: unknown,
    aggregate: BatchFailureMessages,
  ): void {
    if (failed <= 0) {
      return;
    }

    if (hasFirstError) {
      publishCommandErrorNotification(firstError);
    }

    if (failed > 1 || !hasFirstError) {
      publishErrorNotification(
        t(aggregate.titleKey),
        t(aggregate.descriptionKey, { failed, total }),
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

    publishBatchFailures(outcome.failed, items.length, outcome.hasFirstError, outcome.firstError, {
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
      publishRollbackCompletedNotification(1);
    }
  }

  /** Bulk restore to pre-RenderPilot `.bak` originals. */
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

    publishBatchFailures(
      outcome.failed,
      componentIds.length,
      outcome.hasFirstError,
      outcome.firstError,
      {
        titleKey: 'notify.rollbackBatchFailed.title',
        descriptionKey: 'notify.rollbackBatchFailed.description',
      },
    );
  }

  return {
    handleSwap,
    handleRollback,
    handleBulkSwap,
    handleBulkRollback,
  };
}
