import type { OptiScalerAvailability, OptiScalerOperationResult } from './types';

export function buildOptiScalerAvailability(
  overrides: Partial<OptiScalerAvailability> = {},
): OptiScalerAvailability {
  const gameId = overrides.game_id ?? 'test:optiscaler';
  return {
    game_id: gameId,
    launcher: 'Manual',
    install: {
      installed: false,
      release: null,
    },
    eligibility: {
      available: true,
      block_code: null,
    },
    selected_release: 'stable',
    relocation: null,
    proxy_conflict: null,
    compatibility: {
      status: 'untested',
      declared_inputs: [],
      launch: null,
      guidance: [],
    },
    prerequisite: {
      state: 'none',
    },
    modules: [],
    lifecycle: {
      update_available: false,
      repair_required: false,
      drifted: false,
      unmanaged: false,
      maintenance_available: true,
      maintenance_block_code: null,
    },
    ...overrides,
  };
}

export function buildOptiScalerOperationResult(
  overrides: Partial<OptiScalerOperationResult> = {},
): OptiScalerOperationResult {
  return {
    installed: false,
    release: null,
    changed_count: 0,
    preserved_count: 0,
    config_conflicts: [],
    ...overrides,
  };
}
