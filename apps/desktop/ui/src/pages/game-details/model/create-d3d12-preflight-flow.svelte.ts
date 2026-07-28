import {
  blockedSwapPreparationError,
  DEVELOPER_MODE_REQUIRED,
  type D3d12PreflightResult,
  type DeveloperModePlanBlocker,
} from './d3d12-preflight';

type RecoverySnapshot<TPending> = {
  pending: TPending;
  blocker: DeveloperModePlanBlocker;
};

type D3d12PreflightFlowDeps<TPending, TReady> = {
  prepare: (pending: TPending) => Promise<D3d12PreflightResult<TReady>>;
  isCurrent: (pending: TPending) => boolean;
  onReady: (pending: TPending, ready: TReady) => void | Promise<void>;
  onError: (error: unknown, pending: TPending) => void;
  onCancel?: (pending: TPending | null) => void;
};

/**
 * Coordinates the one recoverable D3D12 prerequisite shared by single and
 * batched swaps. Business-specific ready handling stays with each caller.
 */
export function createD3d12PreflightFlow<TPending, TReady>(
  deps: D3d12PreflightFlowDeps<TPending, TReady>,
) {
  let generation = 0;
  let planning = $state(false);
  let retrying = $state(false);
  let stillDisabledAfterRetry = $state(false);
  let recovery = $state<RecoverySnapshot<TPending> | null>(null);

  async function start(pending: TPending): Promise<void> {
    if (planning) {
      return;
    }
    clearRecovery();
    await run(pending, false);
  }

  async function retry(): Promise<void> {
    const pending = recovery?.pending;
    if (!pending || planning || retrying) {
      return;
    }
    retrying = true;
    await run(pending, true);
  }

  async function run(pending: TPending, isRetry: boolean): Promise<void> {
    const currentGeneration = ++generation;
    planning = true;
    try {
      const result = await deps.prepare(pending);
      if (currentGeneration !== generation) {
        return;
      }
      if (!deps.isCurrent(pending)) {
        clearRecovery();
        return;
      }

      if (result.kind === 'blocked') {
        if (result.recovery) {
          recovery = { pending, blocker: result.recovery };
          stillDisabledAfterRetry = isRetry && result.recovery === DEVELOPER_MODE_REQUIRED;
          return;
        }
        clearRecovery();
        throw blockedSwapPreparationError(result.blockers);
      }

      clearRecovery();
      await deps.onReady(pending, result.value);
    } catch (error) {
      if (currentGeneration !== generation) {
        return;
      }
      if (!deps.isCurrent(pending)) {
        clearRecovery();
        return;
      }
      deps.onError(error, pending);
    } finally {
      if (currentGeneration === generation) {
        planning = false;
        retrying = false;
      }
    }
  }

  function cancel(): void {
    const pending = recovery?.pending ?? null;
    generation++;
    planning = false;
    clearRecovery();
    deps.onCancel?.(pending);
  }

  function clearRecovery(): void {
    recovery = null;
    retrying = false;
    stillDisabledAfterRetry = false;
  }

  return {
    get planning() {
      return planning;
    },
    get developerModeOpen() {
      return recovery !== null;
    },
    get developerModeBlocker() {
      return recovery?.blocker ?? null;
    },
    get developerModeRetrying() {
      return retrying;
    },
    get developerModeStillDisabledAfterRetry() {
      return stillDisabledAfterRetry;
    },
    get pendingRecovery() {
      return recovery?.pending ?? null;
    },
    start,
    retry,
    cancel,
  };
}
