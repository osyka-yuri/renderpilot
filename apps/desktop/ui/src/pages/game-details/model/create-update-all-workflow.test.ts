import { describe, expect, it, vi } from 'vitest';

import type { D3d12ExecutableStatus } from '@entities/game';
import type { D3d12ExecutableAction } from '@shared/model';

import { createUpdateAllWorkflow } from './create-update-all-workflow.svelte';
import type { RunUpdateAllOptions } from './run-update-all';
import {
  d3d12PlanFingerprint,
  type PlannedSwap,
  type PreparedSwap,
  type SwapTarget,
} from './swap-request';

const ACTION: D3d12ExecutableAction = {
  kind: 'patch' as const,
  executable_path: 'C:/Game/game.exe',
  backup_path: 'C:/Game/game.exe.bak',
  backup_exists: false,
  original_sdk_version: 606,
  current_sdk_version: 606,
  target_sdk_version: 619,
  requires_confirmation: true,
};

const COMPONENT_STATUS: D3d12ExecutableStatus = {
  status: 'patched',
  selection_locked: false,
  executable_path: 'C:/Game/game.exe',
  backup_path: 'C:/Game/game.exe.bak',
  backup_exists: false,
  original_sdk_version: 606,
  current_sdk_version: 606,
};

const ITEM = {
  kind: 'd3d12',
  target: {
    componentId: 'd3d12',
    artifactId: 'artifact:619',
    isDownloaded: false,
  },
  planFingerprint: d3d12PlanFingerprint(COMPONENT_STATUS, ACTION),
} satisfies PlannedSwap;

function preparedItem(
  confirmationToken: string,
  planFingerprint = ITEM.planFingerprint,
  action = ACTION,
  target: SwapTarget = ITEM.target,
): PreparedSwap {
  return {
    kind: 'd3d12',
    request: { ...target, confirmationToken },
    d3d12ExecutableAction: action,
    planFingerprint,
  };
}

describe('createUpdateAllWorkflow', () => {
  it('keeps the fresh token on the prepared item until confirmation', async () => {
    const run = vi.fn((_options: RunUpdateAllOptions) => Promise.resolve());
    let installPath = '/game';
    const workflow = createUpdateAllWorkflow({
      getGameId: () => 'game',
      getInstallPath: () => installPath,
      getPlan: () => ({ items: [ITEM], updateCount: 1 }),
      getAddonUpdates: () => [],
      hasUpdates: () => true,
      isBusy: () => false,
      onBulkSwap: vi.fn(),
      onError: vi.fn(),
      prepare: vi.fn(() =>
        Promise.resolve({
          kind: 'ready' as const,
          value: [preparedItem('fresh-token')],
        }),
      ),
      run,
    });

    await workflow.start();
    expect(workflow.preparedBatch).not.toBeNull();
    expect(workflow.confirmationActions).toHaveLength(1);

    await workflow.confirm();
    expect(run).toHaveBeenCalledWith(
      expect.objectContaining({
        items: [expect.objectContaining({ confirmationToken: 'fresh-token' })],
      }),
    );
    expect(workflow.updating).toBe(false);
    expect(workflow.pendingDownloadIds).toEqual([]);
    const runOptions = run.mock.calls[0][0];
    expect(runOptions.isOwnerCurrent()).toBe(true);
    installPath = '/game/moved';
    expect(runOptions.isOwnerCurrent()).toBe(false);
  });

  it('deduplicates identical executable plans without merging distinct backup states', async () => {
    const items: PlannedSwap[] = [
      ITEM,
      {
        ...ITEM,
        target: { ...ITEM.target, componentId: 'd3d12-copy', artifactId: 'artifact:copy' },
        planFingerprint: 'second-plan',
      },
      {
        ...ITEM,
        target: { ...ITEM.target, componentId: 'd3d12-backup', artifactId: 'artifact:backup' },
        planFingerprint: 'third-plan',
      },
    ];
    const backupAlreadyExists = { ...ACTION, backup_exists: true };
    const workflow = createUpdateAllWorkflow({
      getGameId: () => 'game',
      getInstallPath: () => '/game',
      getPlan: () => ({ items, updateCount: items.length }),
      getAddonUpdates: () => [],
      hasUpdates: () => true,
      isBusy: () => false,
      onBulkSwap: vi.fn(),
      onError: vi.fn(),
      prepare: vi.fn(() =>
        Promise.resolve({
          kind: 'ready' as const,
          value: [
            preparedItem('first'),
            preparedItem('second', 'second-plan', ACTION, items[1].target),
            preparedItem('third', 'third-plan', backupAlreadyExists, items[2].target),
          ],
        }),
      ),
      run: vi.fn(() => Promise.resolve()),
    });

    await workflow.start();

    expect(workflow.confirmationActions).toEqual([ACTION, backupAlreadyExists]);
  });

  it('cleans progress and reports execution failures', async () => {
    const onError = vi.fn();
    const workflow = createUpdateAllWorkflow({
      getGameId: () => 'game',
      getInstallPath: () => '/game',
      getPlan: () => ({
        items: [
          {
            kind: 'direct',
            target: {
              componentId: 'dlss',
              artifactId: 'artifact:new',
              isDownloaded: false,
            },
          },
        ],
        updateCount: 1,
      }),
      getAddonUpdates: () => [],
      hasUpdates: () => true,
      isBusy: () => false,
      onBulkSwap: vi.fn(),
      onError,
      prepare: vi.fn((_gameId: string, items: readonly PlannedSwap[]) =>
        Promise.resolve({
          kind: 'ready' as const,
          value: items.map((item): PreparedSwap =>
            item.kind === 'direct'
              ? {
                  kind: 'direct',
                  request: { ...item.target },
                  d3d12ExecutableAction: null,
                }
              : {
                  kind: 'd3d12',
                  request: { ...item.target },
                  d3d12ExecutableAction: null,
                  planFingerprint: item.planFingerprint,
                },
          ),
        }),
      ),
      run: vi.fn(() => Promise.reject(new Error('failed'))),
    });

    await workflow.start();
    await workflow.confirm();

    expect(onError).toHaveBeenCalledOnce();
    expect(workflow.planning).toBe(false);
    expect(workflow.updating).toBe(false);
    expect(workflow.pendingDownloadIds).toEqual([]);
  });

  it('silently settles progress when the installation changes during an add-on update', async () => {
    const mutation = Promise.withResolvers<'skipped'>();
    const started = Promise.withResolvers<undefined>();
    let installPath = '/game';
    const later = { update: vi.fn(() => Promise.resolve('ok' as const)) };
    const store = {
      update: vi.fn(() => {
        started.resolve(undefined);
        return mutation.promise;
      }),
    };
    const onError = vi.fn();
    const workflow = createUpdateAllWorkflow({
      getGameId: () => 'game',
      getInstallPath: () => installPath,
      getPlan: () => ({ items: [ITEM], updateCount: 1 }),
      getAddonUpdates: () => [
        { step: 'luma', store },
        { step: 'optiscaler', store: later },
      ],
      hasUpdates: () => true,
      isBusy: () => false,
      onBulkSwap: vi.fn(),
      onError,
      prepare: vi.fn(() =>
        Promise.resolve({ kind: 'ready' as const, value: [preparedItem('fresh-token')] }),
      ),
    });

    await workflow.start();
    const execution = workflow.confirm();
    await started.promise;

    expect(workflow.updating).toBe(true);
    expect(workflow.pendingDownloadIds).toEqual(['artifact:619']);

    installPath = '/game/moved';
    mutation.resolve('skipped');
    await execution;

    expect(onError).not.toHaveBeenCalled();
    expect(later.update).not.toHaveBeenCalled();
    expect(workflow.updating).toBe(false);
    expect(workflow.pendingDownloadIds).toEqual([]);
  });

  it('reports a real safety scope error while the captured installation remains current', async () => {
    const scopeMismatch = Object.assign(new Error('safety scope mismatch'), {
      code: 'safety_context_scope_mismatch',
    });
    const store = { update: vi.fn(() => Promise.reject(scopeMismatch)) };
    const onError = vi.fn();
    const workflow = createUpdateAllWorkflow({
      getGameId: () => 'game',
      getInstallPath: () => '/game',
      getPlan: () => ({ items: [], updateCount: 1 }),
      getAddonUpdates: () => [{ step: 'luma', store }],
      hasUpdates: () => true,
      isBusy: () => false,
      onBulkSwap: vi.fn(),
      onError,
      prepare: vi.fn(() => Promise.resolve({ kind: 'ready' as const, value: [] })),
    });

    await workflow.start();
    await workflow.confirm();

    expect(onError).toHaveBeenCalledWith(
      expect.objectContaining({
        failures: [{ step: 'luma', error: scopeMismatch }],
      }),
    );
    expect(workflow.updating).toBe(false);
  });

  it('blocks Update all before execution and retries with a fresh preparation', async () => {
    const run = vi.fn(() => Promise.resolve());
    const plannedItems: PlannedSwap[] = [ITEM];
    const prepare = vi
      .fn()
      .mockResolvedValueOnce({
        kind: 'blocked' as const,
        blockers: ['developer_mode_required' as const],
        recovery: 'developer_mode_required' as const,
      })
      .mockResolvedValueOnce({
        kind: 'ready' as const,
        value: [preparedItem('retry-token')],
      });
    const workflow = createUpdateAllWorkflow({
      getGameId: () => 'game',
      getInstallPath: () => '/game',
      getPlan: () => ({
        items: plannedItems,
        updateCount: 1,
      }),
      getAddonUpdates: () => [],
      hasUpdates: () => true,
      isBusy: () => false,
      onBulkSwap: vi.fn(),
      onError: vi.fn(),
      prepare,
      run,
    });

    await workflow.start();
    expect(workflow.developerModeOpen).toBe(true);
    expect(run).not.toHaveBeenCalled();

    await workflow.retryDeveloperMode();
    expect(prepare).toHaveBeenCalledTimes(2);
    expect(prepare).toHaveBeenLastCalledWith('game', [ITEM]);
    expect(workflow.preparedBatch).not.toBeNull();
    expect(run).not.toHaveBeenCalled();

    await workflow.confirm();
    expect(run).toHaveBeenCalledWith(
      expect.objectContaining({
        items: [expect.objectContaining({ confirmationToken: 'retry-token' })],
      }),
    );
    expect(run).toHaveBeenCalledOnce();
    expect(workflow.developerModeOpen).toBe(false);
  });

  it('keeps Developer Mode recovery usable after a retry error', async () => {
    const failure = new Error('temporary planning failure');
    const onError = vi.fn();
    const onPreparationError = vi.fn();
    const prepare = vi
      .fn()
      .mockResolvedValueOnce({
        kind: 'blocked' as const,
        blockers: ['developer_mode_required' as const],
        recovery: 'developer_mode_required' as const,
      })
      .mockRejectedValueOnce(failure);
    const workflow = createUpdateAllWorkflow({
      getGameId: () => 'game',
      getInstallPath: () => '/game',
      getPlan: () => ({ items: [ITEM], updateCount: 1 }),
      getAddonUpdates: () => [],
      hasUpdates: () => true,
      isBusy: () => false,
      onBulkSwap: vi.fn(),
      onError,
      onPreparationError,
      prepare,
      run: vi.fn(() => Promise.resolve()),
    });

    await workflow.start();
    await workflow.retryDeveloperMode();

    expect(onError).not.toHaveBeenCalled();
    expect(onPreparationError).toHaveBeenCalledWith(failure);
    expect(workflow.developerModeOpen).toBe(true);
    expect(workflow.developerModeRetrying).toBe(false);
    expect(workflow.developerModeStillDisabledAfterRetry).toBe(false);
    expect(workflow.planning).toBe(false);
  });

  it('reports a mixed non-recoverable batch without opening Developer Mode recovery', async () => {
    const onError = vi.fn();
    const workflow = createUpdateAllWorkflow({
      getGameId: () => 'game',
      getInstallPath: () => '/game',
      getPlan: () => ({ items: [ITEM], updateCount: 1 }),
      getAddonUpdates: () => [],
      hasUpdates: () => true,
      isBusy: () => false,
      onBulkSwap: vi.fn(),
      onError,
      prepare: vi.fn(() =>
        Promise.resolve({
          kind: 'blocked' as const,
          blockers: [
            'developer_mode_required' as const,
            'd3d12_executable_repair_required' as const,
          ],
          recovery: null,
        }),
      ),
      run: vi.fn(() => Promise.resolve()),
    });

    await workflow.start();

    expect(workflow.developerModeOpen).toBe(false);
    expect(onError).toHaveBeenCalledOnce();
    expect(onError.mock.calls[0]?.[0]).toMatchObject({
      code: 'd3d12_executable_repair_required',
    });
  });

  it('discards a prepared batch when the selected game changes before confirmation', async () => {
    let gameId = 'game-a';
    const run = vi.fn(() => Promise.resolve());
    const workflow = createUpdateAllWorkflow({
      getGameId: () => gameId,
      getInstallPath: () => '/game',
      getPlan: () => ({ items: [ITEM], updateCount: 1 }),
      getAddonUpdates: () => [],
      hasUpdates: () => true,
      isBusy: () => false,
      onBulkSwap: vi.fn(),
      onError: vi.fn(),
      prepare: vi.fn(() =>
        Promise.resolve({
          kind: 'ready' as const,
          value: [preparedItem('game-a-token')],
        }),
      ),
      run,
    });

    await workflow.start();
    expect(workflow.preparedBatch).not.toBeNull();

    gameId = 'game-b';
    await workflow.confirm();

    expect(run).not.toHaveBeenCalled();
    expect(workflow.preparedBatch).toBeNull();
    expect(workflow.confirmationActions).toEqual([]);
    expect(workflow.updating).toBe(false);
  });

  it('discards a prepared batch when its swap plan changes before confirmation', async () => {
    let planItems: PlannedSwap[] = [ITEM];
    const run = vi.fn(() => Promise.resolve());
    const workflow = createUpdateAllWorkflow({
      getGameId: () => 'game',
      getInstallPath: () => '/game',
      getPlan: () => ({ items: planItems, updateCount: 1 }),
      getAddonUpdates: () => [],
      hasUpdates: () => true,
      isBusy: () => false,
      onBulkSwap: vi.fn(),
      onError: vi.fn(),
      prepare: vi.fn(() =>
        Promise.resolve({ kind: 'ready' as const, value: [preparedItem('plan-token')] }),
      ),
      run,
    });

    await workflow.start();
    expect(workflow.preparedBatch).not.toBeNull();
    planItems = [
      {
        ...ITEM,
        target: { ...ITEM.target, artifactId: 'artifact:changed' },
      },
    ];
    await workflow.confirm();

    expect(run).not.toHaveBeenCalled();
    expect(workflow.preparedBatch).toBeNull();
  });

  it.each(['component status', 'candidate EXE action'] as const)(
    'discards a prepared D3D12 batch when the %s changes before confirmation',
    async (changedField) => {
      let status = COMPONENT_STATUS;
      let action = ACTION;
      const run = vi.fn(() => Promise.resolve());
      const workflow = createUpdateAllWorkflow({
        getGameId: () => 'game',
        getInstallPath: () => '/game',
        getPlan: () => ({
          items: [
            {
              ...ITEM,
              planFingerprint: d3d12PlanFingerprint(status, action),
            },
          ],
          updateCount: 1,
        }),
        getAddonUpdates: () => [],
        hasUpdates: () => true,
        isBusy: () => false,
        onBulkSwap: vi.fn(),
        onError: vi.fn(),
        prepare: vi.fn(() =>
          Promise.resolve({
            kind: 'ready' as const,
            value: [preparedItem('prepared-token')],
          }),
        ),
        run,
      });

      await workflow.start();
      expect(workflow.preparedBatch).not.toBeNull();

      if (changedField === 'component status') {
        status = { ...COMPONENT_STATUS, status: 'repair_required' };
      } else {
        action = { ...ACTION, requires_confirmation: false };
      }
      await workflow.confirm();

      expect(run).not.toHaveBeenCalled();
      expect(workflow.preparedBatch).toBeNull();
      expect(workflow.confirmationActions).toEqual([]);
    },
  );

  it('discards an in-flight plan when the selected game changes during preparation', async () => {
    let gameId = 'game-a';
    const preparation = Promise.withResolvers<{ kind: 'ready'; value: PreparedSwap[] }>();
    const prepare = vi.fn(() => preparation.promise);
    const run = vi.fn(() => Promise.resolve());
    const workflow = createUpdateAllWorkflow({
      getGameId: () => gameId,
      getInstallPath: () => '/game',
      getPlan: () => ({ items: [ITEM], updateCount: 1 }),
      getAddonUpdates: () => [],
      hasUpdates: () => true,
      isBusy: () => false,
      onBulkSwap: vi.fn(),
      onError: vi.fn(),
      prepare,
      run,
    });

    const start = workflow.start();
    gameId = 'game-b';
    preparation.resolve({
      kind: 'ready',
      value: [preparedItem('game-a-token')],
    });
    await start;

    expect(run).not.toHaveBeenCalled();
    expect(workflow.preparedBatch).toBeNull();
    expect(workflow.planning).toBe(false);
  });
});
