import type { GameGraphicsComponent } from '@entities/game';

export type ExecutableLockReason = 'd3d12_managed' | 'd3d12_repair_required';

type D3d12StatusCarrier = Pick<GameGraphicsComponent, 'd3d12_executable_status'>;

/**
 * Resolves why executable selection is locked for the current game.
 *
 * The backend-owned `selection_locked` flag remains authoritative. A repair
 * state takes precedence so the selector can present recovery guidance even
 * when another managed component was encountered first.
 */
export function resolveExecutableLockReason(
  components: readonly D3d12StatusCarrier[],
): ExecutableLockReason | null {
  let hasManagedLock = false;

  for (const component of components) {
    const status = component.d3d12_executable_status;
    if (!status?.selection_locked) {
      continue;
    }
    if (status.status === 'repair_required') {
      return 'd3d12_repair_required';
    }
    hasManagedLock = true;
  }

  return hasManagedLock ? 'd3d12_managed' : null;
}
