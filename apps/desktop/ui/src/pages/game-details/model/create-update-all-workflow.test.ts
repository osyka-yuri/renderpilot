import { describe, expect, it, vi } from 'vitest';

import { createUpdateAllWorkflow } from './create-update-all-workflow.svelte';
import type { PlannedSwap, PreparedSwap } from './swap-request';

const ACTION = {
  kind: 'patch' as const,
  executable_path: 'C:/Game/game.exe',
  backup_path: 'C:/Game/game.exe.bak',
  backup_exists: false,
  original_sdk_version: 606,
  current_sdk_version: 606,
  target_sdk_version: 619,
  requires_confirmation: true,
};

const ITEM = {
  kind: 'd3d12',
  target: {
    componentId: 'd3d12',
    artifactId: 'artifact:619',
    isDownloaded: false,
  },
} satisfies PlannedSwap;

function preparedItem(confirmationToken: string): PreparedSwap {
  return {
    request: { ...ITEM.target, confirmationToken },
    d3d12ExecutableAction: ACTION,
  };
}

describe('createUpdateAllWorkflow', () => {
  it('keeps the fresh token on the prepared item until confirmation', async () => {
    const run = vi.fn(() => Promise.resolve());
    const workflow = createUpdateAllWorkflow({
      getGameId: () => 'game',
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
    expect(workflow.confirmationOpen).toBe(true);
    expect(workflow.confirmationActions).toHaveLength(1);

    await workflow.confirm();
    expect(run).toHaveBeenCalledWith(
      expect.objectContaining({
        items: [expect.objectContaining({ confirmationToken: 'fresh-token' })],
      }),
    );
    expect(workflow.updating).toBe(false);
    expect(workflow.pendingDownloadIds).toEqual([]);
  });

  it('cleans progress and reports execution failures', async () => {
    const onError = vi.fn();
    const workflow = createUpdateAllWorkflow({
      getGameId: () => 'game',
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
          value: items.map(
            (item): PreparedSwap => ({
              request: { ...item.target },
              d3d12ExecutableAction: null,
            }),
          ),
        }),
      ),
      run: vi.fn(() => Promise.reject(new Error('failed'))),
    });

    await workflow.start();

    expect(onError).toHaveBeenCalledOnce();
    expect(workflow.planning).toBe(false);
    expect(workflow.updating).toBe(false);
    expect(workflow.pendingDownloadIds).toEqual([]);
  });

  it('blocks Update all before execution and retries with a fresh preparation', async () => {
    const run = vi.fn(() => Promise.resolve());
    let plannedItems: PlannedSwap[] = [ITEM];
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

    plannedItems = [
      {
        ...ITEM,
        target: { ...ITEM.target, artifactId: 'artifact:changed' },
      },
    ];
    await workflow.retryDeveloperMode();
    expect(prepare).toHaveBeenCalledTimes(2);
    expect(prepare).toHaveBeenLastCalledWith('game', [ITEM]);
    expect(workflow.confirmationOpen).toBe(true);
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
      getPlan: () => ({ items: [ITEM], updateCount: 1 }),
      getAddonUpdates: () => [],
      hasUpdates: () => true,
      isBusy: () => false,
      onBulkSwap: vi.fn(),
      onError,
      prepare,
      run: vi.fn(() => Promise.resolve()),
    });

    await workflow.start();
    await workflow.retryDeveloperMode();

    expect(onError).toHaveBeenCalledWith(failure);
    expect(workflow.developerModeOpen).toBe(true);
    expect(workflow.developerModeRetrying).toBe(false);
    expect(workflow.developerModeStillDisabledAfterRetry).toBe(false);
    expect(workflow.planning).toBe(false);
  });

  it('reports a mixed non-recoverable batch without opening Developer Mode recovery', async () => {
    const onError = vi.fn();
    const workflow = createUpdateAllWorkflow({
      getGameId: () => 'game',
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
    expect(workflow.confirmationOpen).toBe(true);

    gameId = 'game-b';
    await workflow.confirm();

    expect(run).not.toHaveBeenCalled();
    expect(workflow.confirmationOpen).toBe(false);
    expect(workflow.confirmationActions).toEqual([]);
    expect(workflow.updating).toBe(false);
  });

  it('discards an in-flight plan when the selected game changes during preparation', async () => {
    let gameId = 'game-a';
    const preparation = Promise.withResolvers<{ kind: 'ready'; value: PreparedSwap[] }>();
    const prepare = vi.fn(() => preparation.promise);
    const run = vi.fn(() => Promise.resolve());
    const workflow = createUpdateAllWorkflow({
      getGameId: () => gameId,
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
    expect(workflow.confirmationOpen).toBe(false);
    expect(workflow.planning).toBe(false);
  });
});
