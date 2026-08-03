import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ApplySwapResult, RollbackComponentResult } from '@entities/operation';

import {
  applySwap,
  rollbackComponent,
  publishApplyCompletedNotification,
  publishRollbackCompletedNotification,
} from '@entities/operation';
import { publishCommandErrorNotification, publishErrorNotification } from '@shared/notifications';
import { DesktopCommandError } from '@shared/errors';
import {
  createGameDetailsPageModel,
  type GameDetailsPageModelDeps,
} from './create-game-details-page-model';

vi.mock('@entities/operation', () => ({
  applySwap: vi.fn(),
  rollbackComponent: vi.fn(),
  publishApplyCompletedNotification: vi.fn(),
  publishRollbackCompletedNotification: vi.fn(),
}));

vi.mock('@shared/notifications', () => ({
  publishErrorNotification: vi.fn(),
  publishCommandErrorNotification: vi.fn(),
}));

const ACTIVE_GAME_ID = 'game-1';

function createModel(overrides: Partial<GameDetailsPageModelDeps> = {}) {
  const reloadGameDetails = vi.fn(() => Promise.resolve());
  const model = createGameDetailsPageModel({
    getSelectedGameId: () => ACTIVE_GAME_ID,
    checkIsGameStillSelected: () => true,
    runExclusive: async (task) => task(),
    reloadGameDetails,
    ...overrides,
  });
  return { model, reloadGameDetails };
}

describe('createGameDetailsPageModel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('applies swap, reloads details and notifies on success', async () => {
    vi.mocked(applySwap).mockResolvedValue(createApplySwapResult());
    const { model, reloadGameDetails } = createModel();

    await model.handleSwap({
      componentId: 'component-1',
      artifactId: 'artifact-1',
      isDownloaded: true,
    });

    expect(applySwap).toHaveBeenCalledWith(ACTIVE_GAME_ID, 'component-1', 'artifact-1');
    expect(reloadGameDetails).toHaveBeenCalledTimes(1);
    expect(publishApplyCompletedNotification).toHaveBeenCalledWith(1);
  });

  it('reports the backend file count for an atomic bundle swap', async () => {
    vi.mocked(applySwap).mockResolvedValue(createApplySwapResult(2));
    const { model } = createModel();

    await model.handleSwap({
      componentId: 'component-dxc',
      artifactId: 'artifact-dxc',
      isDownloaded: true,
    });

    expect(publishApplyCompletedNotification).toHaveBeenCalledWith(2);
  });

  it('publishes one combined completion notification when the swap changes the EXE', async () => {
    const executableAction = {
      kind: 'patch' as const,
      executable_path: 'C:/Games/Test/game.exe',
      original_sdk_version: 606,
      from_sdk_version: 606,
      to_sdk_version: 619,
    };
    vi.mocked(applySwap).mockResolvedValue({
      ...createApplySwapResult(2),
      d3d12_executable_action: executableAction,
    });
    const { model } = createModel();

    await model.handleSwap({
      componentId: 'component:d3d12',
      artifactId: 'artifact:d3d12:619',
      isDownloaded: true,
    });

    expect(publishApplyCompletedNotification).toHaveBeenCalledOnce();
    expect(publishApplyCompletedNotification).toHaveBeenCalledWith(2, executableAction);
  });

  it('reports the backend file count for a bundle rollback', async () => {
    vi.mocked(rollbackComponent).mockResolvedValue(createRollbackResult(3));
    const { model } = createModel();

    await model.handleRollback('component-fsr');

    expect(publishRollbackCompletedNotification).toHaveBeenCalledWith(3);
  });

  it('does not publish a separate executable notification after a managed rollback', async () => {
    vi.mocked(rollbackComponent).mockResolvedValue({
      ...createRollbackResult(),
      d3d12_executable_action: {
        kind: 'restore',
        executable_path: 'C:/Games/Test/game.exe',
        original_sdk_version: 606,
        from_sdk_version: 619,
        to_sdk_version: 606,
      },
    });
    const { model } = createModel();

    await model.handleRollback('component:d3d12');

    expect(publishRollbackCompletedNotification).toHaveBeenCalledOnce();
  });

  it('does not notify when runExclusive returns null', async () => {
    vi.mocked(applySwap).mockResolvedValue(createApplySwapResult());
    const { model } = createModel({ runExclusive: () => Promise.resolve(null) });

    await model.handleSwap({
      componentId: 'component-1',
      artifactId: 'artifact-1',
      isDownloaded: true,
    });

    expect(publishApplyCompletedNotification).not.toHaveBeenCalled();
  });

  it('reloads details on single-swap failure so stale candidates clear (error bubbles to runExclusive)', async () => {
    const commandError = DesktopCommandError.fromDto({
      code: 'stale_replacement_source',
    });
    vi.mocked(applySwap).mockRejectedValue(commandError);
    const { model, reloadGameDetails } = createModel();

    await expect(
      model.handleSwap({
        componentId: 'component-1',
        artifactId: 'artifact-1',
        isDownloaded: true,
      }),
    ).rejects.toBe(commandError);
    expect(reloadGameDetails).toHaveBeenCalledTimes(1);
    expect(publishApplyCompletedNotification).not.toHaveBeenCalled();
    // Typed toast is owned by createExclusiveTaskRunner.onError → showError at app shell.
  });

  describe('handleBulkSwap', () => {
    const items = [
      {
        componentId: 'c1',
        artifactId: 'a1',
        isDownloaded: true,
      },
      {
        componentId: 'c2',
        artifactId: 'a2',
        isDownloaded: true,
      },
    ];

    it('swaps every plugin, reloads once and notifies the applied count', async () => {
      vi.mocked(applySwap).mockResolvedValue(createApplySwapResult());
      const { model, reloadGameDetails } = createModel();

      await model.handleBulkSwap(items);

      expect(applySwap).toHaveBeenCalledTimes(2);
      expect(reloadGameDetails).toHaveBeenCalledTimes(1);
      expect(publishApplyCompletedNotification).toHaveBeenCalledWith(2);
      expect(publishErrorNotification).not.toHaveBeenCalled();
    });

    it('sums physical files across successful bundle operations', async () => {
      vi.mocked(applySwap)
        .mockResolvedValueOnce(createApplySwapResult(2))
        .mockResolvedValueOnce(createApplySwapResult(1));
      const { model } = createModel();

      await model.handleBulkSwap(items);

      expect(publishApplyCompletedNotification).toHaveBeenCalledWith(3);
    });

    it('publishes only the aggregate completion notification when a batch patches the EXE', async () => {
      vi.mocked(applySwap)
        .mockResolvedValueOnce({
          ...createApplySwapResult(),
          d3d12_executable_action: {
            kind: 'patch',
            executable_path: 'C:/Games/Test/game.exe',
            original_sdk_version: 606,
            from_sdk_version: 606,
            to_sdk_version: 619,
          },
        })
        .mockResolvedValueOnce(createApplySwapResult());
      const { model } = createModel();

      await model.handleBulkSwap(items);

      expect(publishApplyCompletedNotification).toHaveBeenCalledOnce();
      expect(publishApplyCompletedNotification).toHaveBeenCalledWith(2);
    });

    it('isolates a failed plugin: notifies applied count and surfaces the typed error', async () => {
      const commandError = DesktopCommandError.fromDto({
        code: 'stale_replacement_source',
      });
      vi.mocked(applySwap)
        .mockResolvedValueOnce(createApplySwapResult())
        .mockRejectedValueOnce(commandError);
      const { model, reloadGameDetails } = createModel();

      await model.handleBulkSwap(items);

      expect(reloadGameDetails).toHaveBeenCalledTimes(1);
      expect(publishApplyCompletedNotification).toHaveBeenCalledWith(1);
      // Single failure: typed recovery message only (no generic batch toast).
      expect(publishCommandErrorNotification).toHaveBeenCalledWith(commandError);
      expect(publishErrorNotification).not.toHaveBeenCalled();
    });

    it('reports aggregate batch failure when multiple items fail', async () => {
      const firstError = new Error('first swap failed');
      vi.mocked(applySwap)
        .mockRejectedValueOnce(firstError)
        .mockRejectedValueOnce(new Error('second swap failed'));
      const { model } = createModel();

      await model.handleBulkSwap(items);

      expect(publishCommandErrorNotification).toHaveBeenCalledWith(firstError);
      expect(publishErrorNotification).toHaveBeenCalledTimes(1);
    });

    it('is a no-op for an empty list', async () => {
      const { model, reloadGameDetails } = createModel();

      await model.handleBulkSwap([]);

      expect(applySwap).not.toHaveBeenCalled();
      expect(reloadGameDetails).not.toHaveBeenCalled();
      expect(publishApplyCompletedNotification).not.toHaveBeenCalled();
    });
  });

  describe('handleBulkRollback', () => {
    it('restores every plugin, reloads once and notifies the restored count', async () => {
      vi.mocked(rollbackComponent).mockResolvedValue(createRollbackResult());
      const { model, reloadGameDetails } = createModel();

      await model.handleBulkRollback(['c1', 'c2']);

      expect(rollbackComponent).toHaveBeenCalledTimes(2);
      expect(reloadGameDetails).toHaveBeenCalledTimes(1);
      expect(publishRollbackCompletedNotification).toHaveBeenCalledWith(2);
      expect(publishErrorNotification).not.toHaveBeenCalled();
    });

    it('sums physical files across successful bundle rollbacks', async () => {
      vi.mocked(rollbackComponent)
        .mockResolvedValueOnce(createRollbackResult(3))
        .mockResolvedValueOnce(createRollbackResult(1));
      const { model } = createModel();

      await model.handleBulkRollback(['c1', 'c2']);

      expect(publishRollbackCompletedNotification).toHaveBeenCalledWith(4);
    });

    it('isolates a failed plugin: notifies restored count and surfaces the typed error', async () => {
      const commandError = new Error('rollback failed');
      vi.mocked(rollbackComponent)
        .mockResolvedValueOnce(createRollbackResult())
        .mockRejectedValueOnce(commandError);
      const { model, reloadGameDetails } = createModel();

      await model.handleBulkRollback(['c1', 'c2']);

      expect(reloadGameDetails).toHaveBeenCalledTimes(1);
      expect(publishRollbackCompletedNotification).toHaveBeenCalledWith(1);
      expect(publishCommandErrorNotification).toHaveBeenCalledWith(commandError);
      expect(publishErrorNotification).not.toHaveBeenCalled();
    });

    it('is a no-op for an empty list', async () => {
      const { model, reloadGameDetails } = createModel();

      await model.handleBulkRollback([]);

      expect(rollbackComponent).not.toHaveBeenCalled();
      expect(reloadGameDetails).not.toHaveBeenCalled();
      expect(publishRollbackCompletedNotification).not.toHaveBeenCalled();
    });
  });
});

function createApplySwapResult(updatedFileCount = 1): ApplySwapResult {
  return {
    game_id: ACTIVE_GAME_ID,
    component_id: 'component-1',
    applied_path: '/game/file.dll',
    replacement_path: '/catalog/file.dll',
    updated_file_count: updatedFileCount,
    d3d12_executable_action: null,
  };
}

function createRollbackResult(restoredFileCount = 1): RollbackComponentResult {
  return {
    game_id: ACTIVE_GAME_ID,
    component_id: 'component-1',
    restored_path: '/game/file.dll',
    restored_file_count: restoredFileCount,
    d3d12_executable_action: null,
  };
}
