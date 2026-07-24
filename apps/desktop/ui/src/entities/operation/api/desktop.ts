import { invokeDesktop } from '@shared/api';
import { requireNonBlankString } from '@shared/validation';
import type {
  ApplySwapResult,
  RollbackComponentResult,
  RollbackPlan,
  SwapPlan,
} from '../model/types';

export async function applySwap(
  gameId: string,
  componentId: string,
  artifactId: string,
  confirmationToken?: string | null,
): Promise<ApplySwapResult> {
  return invokeDesktop<ApplySwapResult>('apply_swap', {
    gameId: requireNonBlankString(gameId, 'gameId'),
    componentId: requireNonBlankString(componentId, 'componentId'),
    artifactId: requireNonBlankString(artifactId, 'artifactId'),
    confirmationToken: confirmationToken ?? null,
  });
}

export async function planSwap(
  gameId: string,
  componentId: string,
  artifactId: string,
): Promise<SwapPlan> {
  return invokeDesktop<SwapPlan>('plan_swap', {
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

export async function planRollback(gameId: string, componentId: string): Promise<RollbackPlan> {
  return invokeDesktop<RollbackPlan>('plan_rollback', {
    gameId: requireNonBlankString(gameId, 'gameId'),
    componentId: requireNonBlankString(componentId, 'componentId'),
  });
}
