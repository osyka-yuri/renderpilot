import type { D3d12ExecutableAction } from '@shared/model';

/** Complete request for one component replacement. */
export type SwapRequest = {
  componentId: string;
  artifactId: string;
  isDownloaded: boolean;
  confirmationToken?: string | null;
};

/** Immutable component/artifact selection captured before preflight. */
export type SwapTarget = Readonly<Omit<SwapRequest, 'confirmationToken'>>;

/**
 * A captured swap classified by whether it needs an authoritative D3D12 plan.
 * The discriminant keeps planning metadata out of the execution request.
 */
export type PlannedSwap =
  | Readonly<{
      kind: 'direct';
      target: SwapTarget;
    }>
  | Readonly<{
      kind: 'd3d12';
      target: SwapTarget;
    }>;

/** Execution-ready request plus fresh presentation data for confirmation UI. */
export type PreparedSwap = Readonly<{
  request: SwapRequest;
  d3d12ExecutableAction: D3d12ExecutableAction | null;
}>;
