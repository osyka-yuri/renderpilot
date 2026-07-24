import { planSwap, type SwapPlan } from '@entities/operation';
import { DesktopCommandError } from '@shared/api';
import type { MessageKey } from '@shared/i18n';
import type { D3d12ExecutableAction } from '@shared/model';

import type { BulkSwapItem } from './streamline-versions';

type PlanningDeps = {
  planSwap: typeof planSwap;
};

const defaultDeps: PlanningDeps = { planSwap };

export type PreparedD3d12Action = {
  action: D3d12ExecutableAction | null;
  confirmationToken: string | null;
};

/** Builds a fresh authoritative swap plan before any artifact download. */
export async function prepareD3d12Swap(
  gameId: string,
  componentId: string,
  artifactId: string,
  deps: PlanningDeps = defaultDeps,
): Promise<PreparedD3d12Action> {
  return preparedAction(await deps.planSwap(gameId, componentId, artifactId));
}

/**
 * Replans every D3D12 item in a batch before downloads begin. The returned
 * action carries a canonical token only when confirmation is actually required.
 */
export async function prepareBulkD3d12Swaps(
  gameId: string,
  items: readonly BulkSwapItem[],
  deps: PlanningDeps = defaultDeps,
): Promise<BulkSwapItem[]> {
  return Promise.all(
    items.map(async (item) => {
      if (!item.d3d12ExecutableAction) {
        return item;
      }
      const prepared = await prepareD3d12Swap(gameId, item.componentId, item.artifactId, deps);
      return {
        ...item,
        d3d12ExecutableAction: prepared.action,
        confirmationToken: prepared.confirmationToken,
      };
    }),
  );
}

function preparedAction(plan: SwapPlan): PreparedD3d12Action {
  if (plan.blockers.length > 0) {
    const blocker = plan.blockers[0]?.trim();
    const debugDetails = blocker && blocker.length > 0 ? blocker : 'D3D12 swap plan is blocked';
    throw d3d12PreparationError(
      'd3d12_plan_blocked',
      'gameDetails.d3d12.action.blocked',
      'This D3D12 version cannot be applied in the current state.',
      debugDetails,
    );
  }
  const action = plan.d3d12_executable_action;
  if (action?.kind === 'repair_required') {
    throw d3d12PreparationError(
      'd3d12_executable_repair_required',
      'gameDetails.d3d12.action.repair',
      'The EXE must be repaired before this D3D12 version can be applied.',
    );
  }
  const confirmationToken = action?.requires_confirmation
    ? firstNonBlank(plan.confirmation_token)
    : null;
  if (action?.requires_confirmation && confirmationToken === null) {
    throw d3d12PreparationError(
      'd3d12_confirmation_unavailable',
      'gameDetails.d3d12.action.blocked',
      'This D3D12 version cannot be applied in the current state.',
      'The authoritative swap plan did not contain a confirmation token.',
    );
  }
  return {
    action: action ?? null,
    confirmationToken,
  };
}

function d3d12PreparationError(
  code: string,
  messageKey: MessageKey,
  fallback: string,
  debugDetails?: string,
): DesktopCommandError {
  return new DesktopCommandError({
    code,
    severity: 'error',
    messageKey,
    details: fallback,
    suggestedActions: [],
    debugDetails,
  });
}

function firstNonBlank(...values: (string | null | undefined)[]): string | null {
  for (const value of values) {
    if (typeof value === 'string' && value.trim().length > 0) {
      return value.trim();
    }
  }
  return null;
}
