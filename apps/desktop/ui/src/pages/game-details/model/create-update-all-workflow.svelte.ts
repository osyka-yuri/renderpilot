import { isD3d12ExecutableMutationAction, type D3d12ExecutableMutationAction } from '@shared/model';

import type { BulkSwapHandler } from './create-game-details-page-model';
import { createD3d12PreflightFlow } from './create-d3d12-preflight-flow.svelte';
import { prepareBulkD3d12Swaps } from './prepare-d3d12-operation';
import { runUpdateAll, type RunUpdateAllOptions } from './run-update-all';
import type { PlannedSwap, PreparedSwap } from './swap-request';
import type { UpdateAllPlan } from './update-all-to-latest';

type AddonUpdates = RunUpdateAllOptions['addonUpdates'];

type PreparedUpdateAllBatch = {
  gameId: string;
  items: PreparedSwap[];
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
  let confirmationOpen = $state(false);
  let preparedBatch = $state<PreparedUpdateAllBatch | null>(null);
  let confirmationActions = $state<D3d12ExecutableMutationAction[]>([]);
  let pendingDownloadIds = $state<string[]>([]);
  const preflight = createD3d12PreflightFlow<
    { gameId: string; items: PlannedSwap[] },
    PreparedSwap[]
  >({
    prepare: (pending) => prepare(pending.gameId, pending.items),
    isCurrent: (pending) => deps.getGameId() === pending.gameId,
    onReady: (pending, items) => acceptPreparedBatch({ gameId: pending.gameId, items }),
    onError: (error) => {
      deps.onError(error);
    },
  });

  async function start(): Promise<void> {
    const gameId = deps.getGameId();
    const plan = deps.getPlan();
    if (!gameId || updating || preflight.planning || deps.isBusy() || !deps.hasUpdates()) {
      return;
    }

    await preflight.start({ gameId, items: [...plan.items] });
  }

  async function retryDeveloperMode(): Promise<void> {
    const pending = preflight.pendingRecovery;
    if (!pending) {
      return;
    }
    if (deps.getGameId() !== pending.gameId || updating || preflight.planning || deps.isBusy()) {
      preflight.cancel();
      return;
    }

    await preflight.retry();
  }

  async function acceptPreparedBatch(batch: PreparedUpdateAllBatch): Promise<void> {
    const actions = batch.items
      .map((item) => item.d3d12ExecutableAction)
      .filter(
        (action): action is D3d12ExecutableMutationAction =>
          action?.requires_confirmation === true && isD3d12ExecutableMutationAction(action),
      );
    if (actions.length === 0) {
      await execute(batch);
      return;
    }
    preparedBatch = batch;
    confirmationActions = actions;
    confirmationOpen = true;
  }

  async function execute(batch: PreparedUpdateAllBatch | null): Promise<void> {
    confirmationOpen = false;
    preparedBatch = null;
    confirmationActions = [];
    if (!batch?.gameId || deps.getGameId() !== batch.gameId) {
      return;
    }

    const capturedItems = batch.items.map(({ request }) => ({ ...request }));
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

  function cancelDeveloperMode(): void {
    invalidatePending();
  }

  function invalidatePending(): void {
    preflight.cancel();
    setConfirmationOpen(false);
  }

  function destroy(): void {
    invalidatePending();
  }

  return {
    get updating() {
      return updating;
    },
    get planning() {
      return preflight.planning;
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
    get developerModeOpen() {
      return preflight.developerModeOpen;
    },
    get developerModeBlocker() {
      return preflight.developerModeBlocker;
    },
    get developerModeRetrying() {
      return preflight.developerModeRetrying;
    },
    get developerModeStillDisabledAfterRetry() {
      return preflight.developerModeStillDisabledAfterRetry;
    },
    start,
    confirm: () => execute(preparedBatch),
    retryDeveloperMode,
    cancelDeveloperMode,
    setConfirmationOpen,
    invalidatePending,
    destroy,
  };
}
