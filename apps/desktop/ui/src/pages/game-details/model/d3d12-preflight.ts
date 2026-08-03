import type { SwapPlan, SwapPlanBlocker } from '@entities/operation';
import { ClientError } from '@shared/errors';
import type { D3d12ExecutableAction } from '@shared/model';

export const DEVELOPER_MODE_REQUIRED = 'developer_mode_required';
export const DEVELOPER_MODE_CHECK_UNAVAILABLE = 'developer_mode_check_unavailable';

export type DeveloperModePlanBlocker =
  | typeof DEVELOPER_MODE_REQUIRED
  | typeof DEVELOPER_MODE_CHECK_UNAVAILABLE;

export type D3d12PreflightResult<T> =
  | {
      kind: 'ready';
      value: T;
    }
  | {
      kind: 'blocked';
      blockers: SwapPlanBlocker[];
      recovery: DeveloperModePlanBlocker | null;
    };

export type PreparedD3d12Swap = {
  action: D3d12ExecutableAction | null;
  confirmationToken: string | null;
};

/** Every D3D12 swap is replanned so stale presentation state cannot bypass confirmation. */
export function requiresD3d12Preflight(technology: string): boolean {
  return technology === 'd3d12_agility';
}

/** Converts an authoritative backend plan into the UI's minimal preflight result. */
export function evaluateD3d12SwapPlan(plan: SwapPlan): D3d12PreflightResult<PreparedD3d12Swap> {
  const blockers = normalizedBlockers(plan.blockers);
  const action = plan.d3d12_executable_action;
  if (action?.kind === 'repair_required') {
    blockers.push('d3d12_executable_repair_required');
  }

  const blocked = blockedD3d12Preflight(blockers);
  if (blocked) {
    return blocked;
  }

  const confirmationToken = action?.requires_confirmation
    ? firstNonBlank(plan.confirmation_token)
    : null;
  if (action?.requires_confirmation && confirmationToken === null) {
    throw new ClientError('d3d12_confirmation_unavailable', {
      reason: 'missing_confirmation_token',
    });
  }

  return {
    kind: 'ready',
    value: {
      action: action ?? null,
      confirmationToken,
    },
  };
}

/** Builds a blocked result after normalizing and classifying a whole batch. */
export function blockedD3d12Preflight(
  blockers: readonly SwapPlanBlocker[],
): Extract<D3d12PreflightResult<never>, { kind: 'blocked' }> | null {
  const normalized = normalizedBlockers(blockers);
  if (normalized.length === 0) {
    return null;
  }

  return {
    kind: 'blocked',
    blockers: normalized,
    recovery: developerModeRecovery(normalized),
  };
}

/**
 * Developer Mode recovery is safe only when every blocker in the batch can be
 * resolved through that flow. An unavailable check outranks a disabled status.
 */
function developerModeRecovery(
  blockers: readonly SwapPlanBlocker[],
): DeveloperModePlanBlocker | null {
  if (blockers.length === 0 || blockers.some((blocker) => !isDeveloperModeBlocker(blocker))) {
    return null;
  }
  return blockers.includes(DEVELOPER_MODE_CHECK_UNAVAILABLE)
    ? DEVELOPER_MODE_CHECK_UNAVAILABLE
    : DEVELOPER_MODE_REQUIRED;
}

export function blockedSwapPreparationError(blockers: readonly SwapPlanBlocker[]): ClientError {
  const repairRequired = blockers.includes('d3d12_executable_repair_required');
  return new ClientError(
    repairRequired ? 'd3d12_executable_repair_required' : 'd3d12_plan_blocked',
    blockers,
  );
}

function isDeveloperModeBlocker(blocker: SwapPlanBlocker): blocker is DeveloperModePlanBlocker {
  return blocker === DEVELOPER_MODE_REQUIRED || blocker === DEVELOPER_MODE_CHECK_UNAVAILABLE;
}

function firstNonBlank(...values: (string | null | undefined)[]): string | null {
  for (const value of values) {
    if (typeof value === 'string' && value.trim().length > 0) {
      return value.trim();
    }
  }
  return null;
}

function normalizedBlockers(blockers: readonly SwapPlanBlocker[]): SwapPlanBlocker[] {
  return [...new Set(blockers)];
}
