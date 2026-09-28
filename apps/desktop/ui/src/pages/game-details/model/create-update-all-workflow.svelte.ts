import {
  isD3d12ExecutableMutationAction,
  uniqueD3d12ExecutableMutationActions,
  type D3d12ExecutableMutationAction,
} from '@shared/model';

import type { BulkSwapHandler } from './create-game-details-page-model';
import { createD3d12PreflightFlow } from './create-d3d12-preflight-flow.svelte';
import { prepareBulkD3d12Swaps } from './prepare-d3d12-operation';
import {
  runUpdateAll,
  UpdateAllOwnerChangedError,
  type RunUpdateAllOptions,
  type UpdateAllOwner,
} from './run-update-all';
import type { PlannedSwap, PreparedSwap } from './swap-request';
import type { UpdateAllPlan } from './update-all-to-latest';

type AddonUpdates = RunUpdateAllOptions['addonUpdates'];

export type PreparedUpdateAllBatch = {
  gameId: string;
  installPath: string;
  items: PreparedSwap[];
  addonUpdates: AddonUpdates;
};

type PendingUpdateAllPlan = Omit<PreparedUpdateAllBatch, 'items'> & {
  items: PlannedSwap[];
};

export type UpdateAllWorkflowDeps = {
  getGameId: () => string | null;
  getInstallPath: () => string | null;
  getPlan: () => UpdateAllPlan;
  getAddonUpdates: () => AddonUpdates;
  hasUpdates: () => boolean;
  isBusy: () => boolean;
  onBulkSwap: BulkSwapHandler;
  onPreparationError?: (error: unknown) => void;
  onError: (error: unknown) => void;
  prepare?: typeof prepareBulkD3d12Swaps;
  run?: typeof runUpdateAll;
};

/** Testable owner of update-all planning, confirmation, progress, and cleanup. */
export function createUpdateAllWorkflow(deps: UpdateAllWorkflowDeps) {
  const prepare = deps.prepare ?? prepareBulkD3d12Swaps;
  const run = deps.run ?? runUpdateAll;

  let updating = $state(false);
  let ownerInvalidationRevision = 0;
  // This is an immutable snapshot containing live store references. Deep
  // proxies would change store identity and make the prepared owner appear
  // stale to the page confirmation coordinator.
  let preparedBatch = $state.raw<PreparedUpdateAllBatch | null>(null);
  let pendingDownloadIds = $state<string[]>([]);
  const preflight = createD3d12PreflightFlow<PendingUpdateAllPlan, PreparedSwap[]>({
    prepare: (pending) => prepare(pending.gameId, pending.items),
    isCurrent: (pending) => isCurrentBatch(pending),
    onReady: (pending, items) => {
      preparedBatch = { ...pending, items };
    },
    onError: (error) => {
      (deps.onPreparationError ?? deps.onError)(error);
    },
  });

  async function start(): Promise<void> {
    const gameId = deps.getGameId();
    const installPath = deps.getInstallPath();
    const plan = deps.getPlan();
    const addonUpdates = [...deps.getAddonUpdates()];
    if (
      !gameId ||
      !installPath ||
      updating ||
      preflight.planning ||
      deps.isBusy() ||
      !deps.hasUpdates()
    ) {
      return;
    }

    await preflight.start({ gameId, installPath, items: [...plan.items], addonUpdates });
  }

  async function retryDeveloperMode(): Promise<void> {
    const pending = preflight.pendingRecovery;
    if (!pending) {
      return;
    }
    if (!isCurrentBatch(pending) || updating || preflight.planning || deps.isBusy()) {
      preflight.cancel();
      return;
    }

    await preflight.retry();
  }

  function getExecutableActions(
    batch: PreparedUpdateAllBatch | null,
  ): D3d12ExecutableMutationAction[] {
    if (!batch) {
      return [];
    }
    const actions: D3d12ExecutableMutationAction[] = [];
    for (const { d3d12ExecutableAction: action } of batch.items) {
      if (isD3d12ExecutableMutationAction(action)) {
        actions.push(action);
      }
    }
    return uniqueD3d12ExecutableMutationActions(actions);
  }

  async function execute(batch: PreparedUpdateAllBatch | null): Promise<void> {
    preparedBatch = null;
    if (!batch || !isCurrentBatch(batch)) {
      return;
    }

    const capturedItems = batch.items.map(({ request }) => ({ ...request }));
    const owner: UpdateAllOwner = Object.freeze({
      gameId: batch.gameId,
      installPath: batch.installPath,
    });
    const executionRevision = ownerInvalidationRevision;
    updating = true;
    pendingDownloadIds = capturedItems
      .filter((item) => !item.isDownloaded)
      .map((item) => item.artifactId);

    try {
      await run({
        items: capturedItems,
        owner,
        isOwnerCurrent: () =>
          ownerInvalidationRevision === executionRevision &&
          deps.getGameId() === owner.gameId &&
          deps.getInstallPath() === owner.installPath,
        addonUpdates: batch.addonUpdates,
        onBulkSwap: deps.onBulkSwap,
      });
    } catch (error) {
      if (!(error instanceof UpdateAllOwnerChangedError)) {
        deps.onError(error);
      }
    } finally {
      updating = false;
      pendingDownloadIds = [];
    }
  }

  function isCurrentBatch(batch: PendingUpdateAllPlan | PreparedUpdateAllBatch): boolean {
    if (
      deps.getGameId() !== batch.gameId ||
      deps.getInstallPath() !== batch.installPath ||
      !samePlanItems(deps.getPlan().items, batch.items) ||
      !sameAddonUpdates(deps.getAddonUpdates(), batch.addonUpdates)
    ) {
      return false;
    }
    return true;
  }

  function samePlanItems(
    left: readonly PlannedSwap[],
    right: readonly (PlannedSwap | PreparedSwap)[],
  ): boolean {
    return (
      left.length === right.length &&
      left.every((item, index) => {
        const nextItem = right[index];
        const target = 'target' in nextItem ? nextItem.target : nextItem.request;
        return (
          item.kind === nextItem.kind &&
          (item.kind !== 'd3d12' ||
            (nextItem.kind === 'd3d12' && item.planFingerprint === nextItem.planFingerprint)) &&
          item.target.componentId === target.componentId &&
          item.target.artifactId === target.artifactId &&
          item.target.isDownloaded === target.isDownloaded
        );
      })
    );
  }

  function sameAddonUpdates(left: AddonUpdates, right: AddonUpdates): boolean {
    return (
      left.length === right.length &&
      left.every((item, index) => {
        const next = right[index];
        return item.step === next.step && item.store === next.store;
      })
    );
  }

  function cancelDeveloperMode(): void {
    invalidatePending();
  }

  function invalidatePending(): void {
    ownerInvalidationRevision += 1;
    preflight.cancel();
    preparedBatch = null;
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
    get confirmationActions() {
      return getExecutableActions(preparedBatch);
    },
    get preparedBatch() {
      return preparedBatch;
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
    isCurrentPreparedBatch: (batch: PreparedUpdateAllBatch) => isCurrentBatch(batch),
    retryDeveloperMode,
    cancelDeveloperMode,
    invalidatePending,
    destroy,
  };
}
