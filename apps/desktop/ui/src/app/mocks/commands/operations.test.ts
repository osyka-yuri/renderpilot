import type {
  ApplySwapResult,
  RollbackComponentResult,
  RollbackPlan,
  SwapPlan,
} from '@entities/operation';
import { beforeEach, describe, expect, it } from 'vitest';

import { mockInvoker, resetMockDesktopState } from '../desktop';

describe('preview operation commands', () => {
  beforeEach(() => {
    resetMockDesktopState();
  });

  it('dispatches typed swap and rollback plans', async () => {
    const swap = await mockInvoker<SwapPlan>('plan_swap', {
      gameId: 'steam:1091500',
      componentId: 'component:cp2077:dlss',
      artifactId: 'artifact:dlss:3.7.20',
    });

    expect(swap).toEqual(
      expect.objectContaining({
        game_id: 'steam:1091500',
        component_id: 'component:cp2077:dlss',
        artifact_id: 'artifact:dlss:3.7.20',
        d3d12_executable_action: null,
      }),
    );
    expect(typeof swap.confirmation_token).toBe('string');

    await mockInvoker<ApplySwapResult>('apply_swap', {
      gameId: 'steam:1091500',
      componentId: 'component:cp2077:dlss',
      artifactId: 'artifact:dlss:3.7.20',
    });
    const rollback = await mockInvoker<RollbackPlan>('plan_rollback', {
      gameId: 'steam:1091500',
      componentId: 'component:cp2077:dlss',
    });
    expect(rollback.game_id).toBe('steam:1091500');
    expect(rollback.component_id).toBe('component:cp2077:dlss');
    expect(rollback.affected_files).toHaveLength(2);
  });

  it('accepts the optional confirmation token independently from the action', async () => {
    const result = await mockInvoker<ApplySwapResult>('apply_swap', {
      gameId: 'steam:1091500',
      componentId: 'component:cp2077:dlss',
      artifactId: 'artifact:dlss:3.7.20',
      confirmationToken: 'fresh-plan-token',
    });

    expect(result.d3d12_executable_action).toBeNull();
    expect(result.component_id).toBe('component:cp2077:dlss');
  });

  it('rejects rollback without a captured baseline and restores the exact original file', async () => {
    const request = {
      gameId: 'steam:1091500',
      componentId: 'component:cp2077:dlss',
      artifactId: 'artifact:dlss:3.7.20',
    };

    await expect(
      mockInvoker('plan_rollback', {
        gameId: request.gameId,
        componentId: request.componentId,
      }),
    ).rejects.toMatchObject({ dto: { code: 'invalid_argument' } });

    await mockInvoker<ApplySwapResult>('apply_swap', request);
    const rolledBack = await mockInvoker<RollbackComponentResult>('rollback_component', {
      gameId: request.gameId,
      componentId: request.componentId,
    });

    expect(rolledBack.restored_path).toBe('C:/Games/Cyberpunk 2077/bin/x64/nvngx_dlss.dll');
    await expect(
      mockInvoker('rollback_component', {
        gameId: request.gameId,
        componentId: request.componentId,
      }),
    ).rejects.toMatchObject({ dto: { code: 'invalid_argument' } });
  });

  it('enforces and advances the D3D12 confirmation state', async () => {
    const request = {
      gameId: 'steam:1091500',
      componentId: 'component:cp2077:d3d12',
      artifactId: 'artifact:d3d12:1.619.1',
    };
    const plan = await mockInvoker<SwapPlan>('plan_swap', request);

    expect(plan.d3d12_executable_action).toMatchObject({
      kind: 'patch',
      current_sdk_version: 606,
      target_sdk_version: 619,
      requires_confirmation: true,
    });
    expect(plan.d3d12_executable_action).not.toHaveProperty('confirmation_token');

    await expect(mockInvoker('apply_swap', request)).rejects.toMatchObject({
      dto: { code: 'confirmation_token_mismatch' },
    });
    await expect(
      mockInvoker('apply_swap', { ...request, confirmationToken: 'stale-token' }),
    ).rejects.toMatchObject({
      dto: { code: 'confirmation_token_mismatch' },
    });

    const applied = await mockInvoker<ApplySwapResult>('apply_swap', {
      ...request,
      confirmationToken: plan.confirmation_token,
    });
    expect(applied.d3d12_executable_action).toMatchObject({
      kind: 'patch',
      from_sdk_version: 606,
      to_sdk_version: 619,
    });

    const currentPlan = await mockInvoker<SwapPlan>('plan_swap', request);
    expect(currentPlan.d3d12_executable_action).toMatchObject({
      kind: 'none',
      current_sdk_version: 619,
      requires_confirmation: false,
    });

    const rollbackPlan = await mockInvoker<RollbackPlan>('plan_rollback', {
      gameId: request.gameId,
      componentId: request.componentId,
    });
    expect(rollbackPlan.d3d12_executable_action).toMatchObject({
      kind: 'restore',
      current_sdk_version: 619,
      target_sdk_version: 606,
      requires_confirmation: false,
    });
    expect(rollbackPlan.affected_files).toEqual(
      expect.arrayContaining([
        'C:/Games/Cyberpunk 2077/bin/x64/D3D12Core.dll',
        'C:/Games/Cyberpunk 2077/bin/x64/D3D12Core.dll.bak',
        'C:/Games/Cyberpunk 2077/bin/x64/Cyberpunk2077.exe',
        'C:/Games/Cyberpunk 2077/bin/x64/Cyberpunk2077.exe.bak',
      ]),
    );

    const rolledBack = await mockInvoker<RollbackComponentResult>('rollback_component', {
      gameId: request.gameId,
      componentId: request.componentId,
    });
    expect(rolledBack.d3d12_executable_action).toMatchObject({
      kind: 'restore',
      from_sdk_version: 619,
      to_sdk_version: 606,
    });

    const restoredPlan = await mockInvoker<SwapPlan>('plan_swap', request);
    expect(restoredPlan.original_version).toBe('1.606.4');
  });
});
