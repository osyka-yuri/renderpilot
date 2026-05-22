import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ApplySwapResult } from '@entities/operation';

import { applySwap, publishApplyCompletedNotification } from '@entities/operation';
import { createGameDetailsPageModel } from './create-game-details-page-model';

vi.mock('@entities/operation', () => ({
  applySwap: vi.fn(),
  publishApplyCompletedNotification: vi.fn(),
}));

describe('createGameDetailsPageModel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('applies swap, reloads details and notifies on success', async () => {
    const reloadGameDetails = vi.fn(() => Promise.resolve());

    vi.mocked(applySwap).mockResolvedValue(createApplySwapResult());

    const model = createGameDetailsPageModel({
      getSelectedGameId: () => 'game-1',
      checkIsGameStillSelected: () => true,
      runExclusive: async (task) => task(),
      reloadGameDetails,
    });

    await model.handleSwap('component-1', 'artifact-1');

    expect(applySwap).toHaveBeenCalledWith('game-1', 'component-1', 'artifact-1');
    expect(reloadGameDetails).toHaveBeenCalledTimes(1);
    expect(publishApplyCompletedNotification).toHaveBeenCalledWith(1);
  });

  it('does not notify when runExclusive returns null', async () => {
    vi.mocked(applySwap).mockResolvedValue(createApplySwapResult());

    const model = createGameDetailsPageModel({
      getSelectedGameId: () => 'game-1',
      checkIsGameStillSelected: () => true,
      runExclusive: () => Promise.resolve(null),
      reloadGameDetails: vi.fn(() => Promise.resolve()),
    });

    await model.handleSwap('component-1', 'artifact-1');

    expect(publishApplyCompletedNotification).not.toHaveBeenCalled();
  });
});

function createApplySwapResult(): ApplySwapResult {
  return {
    game_id: 'game-1',
    component_id: 'component-1',
    applied_path: '/game/file.dll',
    replacement_path: '/catalog/file.dll',
  };
}
