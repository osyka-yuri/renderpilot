import { describe, expect, it, vi } from 'vitest';

import type { SwapPlan } from '@entities/operation';
import { t } from '@shared/i18n';
import type { D3d12ExecutableAction } from '@shared/model';

import { prepareBulkD3d12Swaps, prepareD3d12Swap } from './prepare-d3d12-operation';

describe('D3D12 operation preparation', () => {
  it('returns the fresh action and canonical swap token', async () => {
    const deps = planningDeps(swapPlan());

    await expect(prepareD3d12Swap('game', 'component', 'artifact', deps)).resolves.toEqual({
      action: action(),
      confirmationToken: 'fresh-swap-token',
    });
    expect(deps.planSwap).toHaveBeenCalledWith('game', 'component', 'artifact');
  });

  it('plans only D3D12 items in a batch and threads fresh tokens before apply', async () => {
    const deps = planningDeps(swapPlan());
    const nonD3d = {
      componentId: 'dlss',
      artifactId: 'dlss-artifact',
      isDownloaded: false,
    };
    const d3d = {
      componentId: 'd3d12',
      artifactId: 'd3d12-artifact',
      isDownloaded: false,
      d3d12ExecutableAction: action(),
    };

    const prepared = await prepareBulkD3d12Swaps('game', [nonD3d, d3d], deps);

    expect(deps.planSwap).toHaveBeenCalledOnce();
    expect(prepared[0]).toBe(nonD3d);
    expect(prepared[1]?.confirmationToken).toBe('fresh-swap-token');
  });

  it('does not attach the operation-plan token to a silent repatch', async () => {
    const repatchAction = action({
      backup_exists: true,
      current_sdk_version: 619,
      target_sdk_version: 618,
      requires_confirmation: false,
    });
    const deps = planningDeps({
      ...swapPlan(),
      d3d12_executable_action: repatchAction,
    });
    const item = {
      componentId: 'd3d12',
      artifactId: 'd3d12-artifact',
      isDownloaded: true,
      d3d12ExecutableAction: repatchAction,
    };

    const [prepared] = await prepareBulkD3d12Swaps('game', [item], deps);

    expect(prepared.d3d12ExecutableAction).toEqual(repatchAction);
    expect(prepared.confirmationToken).toBeNull();
  });

  it('fails before download when a confirmation-required plan has no token', async () => {
    const plan = swapPlan();
    const deps = planningDeps({
      ...plan,
      confirmation_token: '',
      d3d12_executable_action: action(),
    });

    await expect(prepareD3d12Swap('game', 'component', 'artifact', deps)).rejects.toMatchObject({
      dto: {
        code: 'd3d12_confirmation_unavailable',
      },
    });
  });

  it('rejects blockers and repair states before a download can start', async () => {
    const blocked = planningDeps({ ...swapPlan(), blockers: ['unsafe state'] });
    const blockedResult = prepareD3d12Swap('game', 'component', 'artifact', blocked);
    await expect(blockedResult).rejects.toThrow(t('gameDetails.d3d12.action.blocked'));
    await expect(blockedResult).rejects.toMatchObject({
      dto: {
        code: 'd3d12_plan_blocked',
        debugDetails: 'unsafe state',
      },
    });
    const blankBlockerResult = prepareD3d12Swap(
      'game',
      'component',
      'artifact',
      planningDeps({ ...swapPlan(), blockers: [''] }),
    );
    await expect(blankBlockerResult).rejects.toMatchObject({
      dto: {
        code: 'd3d12_plan_blocked',
      },
    });

    const repair = planningDeps({
      ...swapPlan(),
      d3d12_executable_action: {
        ...action(),
        kind: 'repair_required',
        requires_confirmation: false,
      },
    });
    const repairResult = prepareD3d12Swap('game', 'component', 'artifact', repair);
    await expect(repairResult).rejects.toThrow(t('gameDetails.d3d12.action.repair'));
    await expect(repairResult).rejects.toMatchObject({
      dto: {
        code: 'd3d12_executable_repair_required',
      },
    });
  });
});

function planningDeps(swap: SwapPlan) {
  return {
    planSwap: vi.fn().mockResolvedValue(swap),
  };
}

function swapPlan(): SwapPlan {
  return {
    operation_id: 'operation:swap',
    operation_type: 'replace_component',
    game_id: 'game',
    component_id: 'component',
    artifact_id: 'artifact',
    target_path: 'D3D12Core.dll',
    replacement_path: 'catalog://D3D12Core.dll',
    confirmation_token: 'fresh-swap-token',
    original_version: null,
    replacement_version: '1.619.1',
    original_sha256: null,
    replacement_sha256: 'b'.repeat(64),
    risk_level: 'medium',
    requires_elevation: false,
    blockers: [],
    warnings: [],
    files: [],
    d3d12_executable_action: action(),
  };
}

function action(overrides: Partial<D3d12ExecutableAction> = {}): D3d12ExecutableAction {
  return {
    kind: 'patch',
    executable_path: 'C:/Games/Test/game.exe',
    backup_path: 'C:/Games/Test/game.exe.bak',
    backup_exists: false,
    original_sdk_version: 606,
    current_sdk_version: 606,
    target_sdk_version: 619,
    requires_confirmation: true,
    ...overrides,
  };
}
