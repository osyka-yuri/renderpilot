import type { D3d12ExecutableStatus } from '@entities/game';
import type { D3d12ExecutableAction, D3d12ExecutableMutationAction } from '@shared/model';

export function d3d12PlanFingerprint(
  status: D3d12ExecutableStatus | null,
  action: D3d12ExecutableAction | null,
): string {
  // Serialize complete DTOs so changes to any current or future safety field
  // invalidate the prepared plan.
  return JSON.stringify([status, action]);
}

/** Complete request for one component replacement. */
export type SwapRequest = {
  componentId: string;
  artifactId: string;
  isDownloaded: boolean;
  confirmationToken?: string | null;
  gameContextToken?: string;
};

/** Immutable component/artifact selection captured before preflight. */
export type SwapTarget = Readonly<Omit<SwapRequest, 'confirmationToken'>>;

/** Prepared D3D12 confirmation data bound to the exact selected game files. */
export type PreparedSwapPresentation = Readonly<{
  action: D3d12ExecutableMutationAction;
  owner: Readonly<{
    gameId: string;
    installPath: string;
    componentId: string;
    artifactId: string;
    planFingerprint: string;
  }>;
}>;

/**
 * A captured swap classified by whether it needs an authoritative D3D12 plan.
 * The discriminant keeps planning metadata out of the execution request.
 */
export type PlannedSwap =
  | Readonly<{
      kind: 'direct';
      target: SwapTarget;
      planFingerprint?: never;
    }>
  | Readonly<{
      kind: 'd3d12';
      target: SwapTarget;
      planFingerprint: string;
    }>;

/** Execution-ready request plus fresh presentation data for confirmation UI. */
export type PreparedSwap =
  | Readonly<{
      kind: 'direct';
      request: SwapRequest;
      d3d12ExecutableAction: null;
      planFingerprint?: never;
    }>
  | Readonly<{
      kind: 'd3d12';
      request: SwapRequest;
      d3d12ExecutableAction: D3d12ExecutableAction | null;
      planFingerprint: string;
    }>;
