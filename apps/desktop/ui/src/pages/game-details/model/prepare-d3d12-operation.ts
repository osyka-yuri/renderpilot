import { planSwap, type SwapPlanBlocker } from '@entities/operation';

import type { PlannedSwap, PreparedSwap } from './swap-request';
import {
  blockedD3d12Preflight,
  evaluateD3d12SwapPlan,
  type D3d12PreflightResult,
  type PreparedD3d12Swap,
} from './d3d12-preflight';

type PlanningDeps = {
  planSwap: typeof planSwap;
};

const defaultDeps: PlanningDeps = { planSwap };

/** Builds a fresh authoritative swap plan before any artifact download. */
export async function prepareD3d12Swap(
  gameId: string,
  componentId: string,
  artifactId: string,
  deps: PlanningDeps = defaultDeps,
): Promise<D3d12PreflightResult<PreparedD3d12Swap>> {
  return evaluateD3d12SwapPlan(await deps.planSwap(gameId, componentId, artifactId));
}

/**
 * Replans every marked D3D12 item in a batch before downloads begin. Each
 * returned item carries a fresh token only when confirmation is required.
 */
export async function prepareBulkD3d12Swaps(
  gameId: string,
  items: readonly PlannedSwap[],
  deps: PlanningDeps = defaultDeps,
): Promise<D3d12PreflightResult<PreparedSwap[]>> {
  const preparedItems: PreparedSwap[] = [];
  const blockers: SwapPlanBlocker[] = [];

  // All items target the same game and the backend serializes them on the same
  // mutation lock. Planning in order avoids queued work outliving a rejected
  // batch and provides no less effective concurrency.
  for (const item of items) {
    if (item.kind === 'direct') {
      preparedItems.push({
        request: { ...item.target },
        d3d12ExecutableAction: null,
      });
      continue;
    }
    const preparation = await prepareD3d12Swap(
      gameId,
      item.target.componentId,
      item.target.artifactId,
      deps,
    );
    if (preparation.kind === 'blocked') {
      blockers.push(...preparation.blockers);
      continue;
    }
    preparedItems.push({
      request: {
        ...item.target,
        confirmationToken: preparation.value.confirmationToken,
      },
      d3d12ExecutableAction: preparation.value.action,
    });
  }

  const blocked = blockedD3d12Preflight(blockers);
  if (blocked) {
    return blocked;
  }

  return { kind: 'ready', value: preparedItems };
}
