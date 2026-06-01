import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ApplySwapResult, RollbackComponentResult } from '@entities/operation';

import {
  applySwap,
  rollbackComponent,
  publishApplyCompletedNotification,
  publishRollbackCompletedNotification,
} from '@entities/operation';
import { publishErrorNotification } from '@shared/notifications';
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

    await model.handleSwap('component-1', 'artifact-1');

    expect(applySwap).toHaveBeenCalledWith(ACTIVE_GAME_ID, 'component-1', 'artifact-1');
    expect(reloadGameDetails).toHaveBeenCalledTimes(1);
    expect(publishApplyCompletedNotification).toHaveBeenCalledWith(1);
  });

  it('does not notify when runExclusive returns null', async () => {
    vi.mocked(applySwap).mockResolvedValue(createApplySwapResult());
    const { model } = createModel({ runExclusive: () => Promise.resolve(null) });

    await model.handleSwap('component-1', 'artifact-1');

    expect(publishApplyCompletedNotification).not.toHaveBeenCalled();
  });

  describe('handleBulkSwap', () => {
    const items = [
      { componentId: 'c1', artifactId: 'a1', entryId: null },
      { componentId: 'c2', artifactId: 'a2', entryId: null },
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

    it('isolates a failed plugin: notifies the applied count and reports the failure', async () => {
      vi.mocked(applySwap)
        .mockResolvedValueOnce(createApplySwapResult())
        .mockRejectedValueOnce(new Error('swap failed'));
      const { model, reloadGameDetails } = createModel();

      await model.handleBulkSwap(items);

      expect(reloadGameDetails).toHaveBeenCalledTimes(1);
      expect(publishApplyCompletedNotification).toHaveBeenCalledWith(1);
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

    it('isolates a failed plugin: notifies the restored count and reports the failure', async () => {
      vi.mocked(rollbackComponent)
        .mockResolvedValueOnce(createRollbackResult())
        .mockRejectedValueOnce(new Error('rollback failed'));
      const { model, reloadGameDetails } = createModel();

      await model.handleBulkRollback(['c1', 'c2']);

      expect(reloadGameDetails).toHaveBeenCalledTimes(1);
      expect(publishRollbackCompletedNotification).toHaveBeenCalledWith(1);
      expect(publishErrorNotification).toHaveBeenCalledTimes(1);
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

function createApplySwapResult(): ApplySwapResult {
  return {
    game_id: ACTIVE_GAME_ID,
    component_id: 'component-1',
    applied_path: '/game/file.dll',
    replacement_path: '/catalog/file.dll',
  };
}

function createRollbackResult(): RollbackComponentResult {
  return {
    game_id: ACTIVE_GAME_ID,
    component_id: 'component-1',
    restored_path: '/game/file.dll',
  };
}
