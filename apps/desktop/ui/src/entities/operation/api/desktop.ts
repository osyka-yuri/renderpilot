import { invokeDesktop } from '@shared/api';
import { requireNonBlankString } from '@shared/validation';
import type { ApplySwapResult, RollbackComponentResult } from '../model/types';

export async function applySwap(
  gameId: string,
  componentId: string,
  artifactId: string,
): Promise<ApplySwapResult> {
  return invokeDesktop<ApplySwapResult>('apply_swap', {
    gameId: requireNonBlankString(gameId, 'gameId'),
    componentId: requireNonBlankString(componentId, 'componentId'),
    artifactId: requireNonBlankString(artifactId, 'artifactId'),
  });
}

export async function rollbackComponent(
  gameId: string,
  componentId: string,
): Promise<RollbackComponentResult> {
  return invokeDesktop<RollbackComponentResult>('rollback_component', {
    gameId: requireNonBlankString(gameId, 'gameId'),
    componentId: requireNonBlankString(componentId, 'componentId'),
  });
}
