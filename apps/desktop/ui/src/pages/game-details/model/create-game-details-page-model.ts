import {
  applyOperationPlan,
  buildSwapPlan,
  publishApplyCompletedNotification,
} from '@entities/operation';
import type { SwapPlan } from '@entities/operation';

export type GameDetailsPageModelDeps = {
  getSelectedGameId: () => string | null;
  checkIsGameStillSelected: (gameId: string) => boolean;
  runExclusive: <T>(task: () => Promise<T>) => Promise<T | null>;
  setCurrentPlan: (plan: SwapPlan | null) => void;
  getCurrentPlan: (operationId: string) => SwapPlan | null;
  showStalePlanError: () => void;
  reloadGameDetails: () => Promise<void>;
};

export function createGameDetailsPageModel(deps: GameDetailsPageModelDeps) {
  async function handleBuildPlan(componentId: string, artifactId: string): Promise<void> {
    const gameId = deps.getSelectedGameId();

    if (gameId === null) {
      return;
    }

    await deps.runExclusive(async () => {
      if (deps.checkIsGameStillSelected(gameId)) {
        deps.setCurrentPlan(null);
      }

      const plan = await buildSwapPlan(gameId, componentId, artifactId);

      if (deps.checkIsGameStillSelected(gameId)) {
        deps.setCurrentPlan(plan);
      }
    });
  }

  async function handleApply(operationId: string): Promise<void> {
    const plan = deps.getCurrentPlan(operationId);

    if (plan === null) {
      deps.showStalePlanError();
      return;
    }

    const result = await deps.runExclusive(async () => {
      const appliedOperation = await applyOperationPlan(operationId, plan.confirmation_token);

      deps.setCurrentPlan(null);

      await deps.reloadGameDetails();

      return appliedOperation;
    });

    if (result !== null) {
      publishApplyCompletedNotification(result.items.length);
    }
  }

  return { handleBuildPlan, handleApply };
}
