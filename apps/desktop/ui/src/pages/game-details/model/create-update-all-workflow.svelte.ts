import { isD3d12ExecutableMutationAction, type D3d12ExecutableMutationAction } from '@shared/model';

import type { BulkSwapHandler } from './create-game-details-page-model';
import { prepareBulkD3d12Swaps } from './prepare-d3d12-operation';
import { runUpdateAll, type RunUpdateAllOptions } from './run-update-all';
import type { BulkSwapItem } from './streamline-versions';
import type { UpdateAllPlan } from './update-all-to-latest';

type AddonUpdates = RunUpdateAllOptions['addonUpdates'];

type PreparedUpdateAllBatch = {
  gameId: string;
  items: BulkSwapItem[];
};

export type UpdateAllWorkflowDeps = {
  getGameId: () => string | null;
  getPlan: () => UpdateAllPlan;
  getAddonUpdates: () => AddonUpdates;
  hasUpdates: () => boolean;
  isBusy: () => boolean;
  onBulkSwap: BulkSwapHandler;
  onError: (error: unknown) => void;
  prepare?: typeof prepareBulkD3d12Swaps;
  run?: typeof runUpdateAll;
};

/** Testable owner of update-all planning, confirmation, progress, and cleanup. */
export function createUpdateAllWorkflow(deps: UpdateAllWorkflowDeps) {
  const prepare = deps.prepare ?? prepareBulkD3d12Swaps;
  const run = deps.run ?? runUpdateAll;

  let updating = $state(false);
  let planning = $state(false);
  let confirmationOpen = $state(false);
  let preparedBatch = $state<PreparedUpdateAllBatch | null>(null);
  let confirmationActions = $state<D3d12ExecutableMutationAction[]>([]);
  let pendingDownloadIds = $state<string[]>([]);

  async function start(): Promise<void> {
    const gameId = deps.getGameId();
    const plan = deps.getPlan();
    if (!gameId || updating || planning || deps.isBusy() || !deps.hasUpdates()) {
      return;
    }

    planning = true;
    try {
      const items = await prepare(gameId, plan.items);
      if (deps.getGameId() !== gameId) {
        return;
      }
      const actions = items
        .map((item) => item.d3d12ExecutableAction)
        .filter(
          (action): action is D3d12ExecutableMutationAction =>
            action?.requires_confirmation === true && isD3d12ExecutableMutationAction(action),
        );
      if (actions.length === 0) {
        await execute({ gameId, items });
        return;
      }
      preparedBatch = { gameId, items };
      confirmationActions = actions;
      confirmationOpen = true;
    } catch (error) {
      deps.onError(error);
    } finally {
      planning = false;
    }
  }

  async function execute(batch: PreparedUpdateAllBatch | null): Promise<void> {
    confirmationOpen = false;
    preparedBatch = null;
    confirmationActions = [];
    if (!batch?.gameId || deps.getGameId() !== batch.gameId) {
      return;
    }

    const capturedItems = [...batch.items];
    updating = true;
    pendingDownloadIds = capturedItems
      .filter((item) => !item.isDownloaded)
      .map((item) => item.artifactId);

    try {
      await run({
        items: capturedItems,
        gameId: batch.gameId,
        addonUpdates: deps.getAddonUpdates(),
        onBulkSwap: deps.onBulkSwap,
      });
    } catch (error) {
      deps.onError(error);
    } finally {
      updating = false;
      pendingDownloadIds = [];
    }
  }

  function setConfirmationOpen(open: boolean): void {
    confirmationOpen = open;
    if (!open) {
      preparedBatch = null;
      confirmationActions = [];
    }
  }

  return {
    get updating() {
      return updating;
    },
    get planning() {
      return planning;
    },
    get confirmationOpen() {
      return confirmationOpen;
    },
    get confirmationActions() {
      return confirmationActions;
    },
    get pendingDownloadIds() {
      return pendingDownloadIds;
    },
    start,
    confirm: () => execute(preparedBatch),
    setConfirmationOpen,
  };
}
