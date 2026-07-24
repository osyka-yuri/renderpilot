import { describe, expect, it, vi } from 'vitest';

import { createUpdateAllWorkflow } from './create-update-all-workflow.svelte';
import type { BulkSwapItem } from './streamline-versions';

const ITEM = {
  componentId: 'd3d12',
  artifactId: 'artifact:619',
  isDownloaded: false,
  d3d12ExecutableAction: {
    kind: 'patch' as const,
    executable_path: 'C:/Game/game.exe',
    backup_path: 'C:/Game/game.exe.bak',
    backup_exists: false,
    original_sdk_version: 606,
    current_sdk_version: 606,
    target_sdk_version: 619,
    requires_confirmation: true,
  },
};

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
      prepare: vi.fn(() => Promise.resolve([{ ...ITEM, confirmationToken: 'fresh-token' }])),
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
        items: [{ componentId: 'dlss', artifactId: 'artifact:new', isDownloaded: false }],
        updateCount: 1,
      }),
      getAddonUpdates: () => [],
      hasUpdates: () => true,
      isBusy: () => false,
      onBulkSwap: vi.fn(),
      onError,
      prepare: vi.fn((_gameId: string, items: readonly BulkSwapItem[]) =>
        Promise.resolve([...items]),
      ),
      run: vi.fn(() => Promise.reject(new Error('failed'))),
    });

    await workflow.start();

    expect(onError).toHaveBeenCalledOnce();
    expect(workflow.planning).toBe(false);
    expect(workflow.updating).toBe(false);
    expect(workflow.pendingDownloadIds).toEqual([]);
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
      prepare: vi.fn(() => Promise.resolve([{ ...ITEM, confirmationToken: 'game-a-token' }])),
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
    let resolvePreparation!: (items: BulkSwapItem[]) => void;
    const prepare = vi.fn(
      () =>
        new Promise<BulkSwapItem[]>((resolve) => {
          resolvePreparation = resolve;
        }),
    );
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
    resolvePreparation([{ ...ITEM, confirmationToken: 'game-a-token' }]);
    await start;

    expect(run).not.toHaveBeenCalled();
    expect(workflow.confirmationOpen).toBe(false);
    expect(workflow.planning).toBe(false);
  });
});
