import { invokeDesktop } from '@shared/api';
import { requireNonBlankString } from '@shared/validation';
import type { ApplyOperationResult, RollbackOperationResult, SwapPlan } from '../model/types';

export async function buildSwapPlan(
  gameId: string,
  componentId: string,
  artifactId: string,
): Promise<SwapPlan> {
  return invokeDesktop<SwapPlan>('build_swap_plan', {
    gameId: requireNonBlankString(gameId, 'gameId'),
    componentId: requireNonBlankString(componentId, 'componentId'),
    artifactId: requireNonBlankString(artifactId, 'artifactId'),
  });
}

export async function applyOperationPlan(
  operationId: string,
  confirmationToken: string,
): Promise<ApplyOperationResult> {
  return invokeDesktop<ApplyOperationResult>('apply_operation_plan', {
    operationId: requireNonBlankString(operationId, 'operationId'),
    confirmationToken: requireNonBlankString(confirmationToken, 'confirmationToken'),
  });
}

export async function rollbackOperation(operationId: string): Promise<RollbackOperationResult> {
  return invokeDesktop<RollbackOperationResult>('rollback_operation', {
    operationId: requireNonBlankString(operationId, 'operationId'),
  });
}
