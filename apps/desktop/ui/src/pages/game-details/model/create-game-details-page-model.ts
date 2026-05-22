import {
  publishApplyCompletedNotification,
  publishRollbackCompletedNotification,
  rollbackComponent,
} from '@entities/operation';
import { executeGraphicsSwap } from '@features/swap-graphics-component';

export type SwapHandler = (
  componentId: string,
  artifactId: string,
  entryId?: string | null,
) => Promise<void> | void;

export type RollbackHandler = (componentId: string) => Promise<void> | void;

export type GameDetailsPageModelDeps = {
  getSelectedGameId: () => string | null;
  checkIsGameStillSelected: (gameId: string) => boolean;
  runExclusive: <T>(task: () => Promise<T>) => Promise<T | null>;
  reloadGameDetails: () => Promise<void>;
};

export function createGameDetailsPageModel(deps: GameDetailsPageModelDeps) {
  async function handleSwap(
    componentId: string,
    artifactId: string,
    entryId?: string | null,
  ): Promise<void> {
    const gameId = deps.getSelectedGameId();

    if (gameId === null) {
      return;
    }

    const result = await deps.runExclusive(async () => {
      const controller = new AbortController();
      if (!deps.checkIsGameStillSelected(gameId)) {
        controller.abort();
      }
      const appliedOperation = await executeGraphicsSwap({
        gameId,
        componentId,
        artifactId,
        entryId,
        signal: controller.signal,
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

  async function handleRollback(componentId: string): Promise<void> {
    const gameId = deps.getSelectedGameId();

    if (gameId === null) {
      return;
    }

    const result = await deps.runExclusive(async () => {
      const rollbackResult = await rollbackComponent(gameId, componentId);
      await deps.reloadGameDetails();
      return rollbackResult;
    });

    if (result !== null) {
      publishRollbackCompletedNotification(1);
    }
  }

  return { handleSwap, handleRollback };
}
