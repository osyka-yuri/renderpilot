import { beforeEach, describe, expect, it, vi } from 'vitest';

const invokeDesktop = vi.hoisted(() => vi.fn());

vi.mock('@shared/api', () => ({ invokeDesktop }));

import { getGameFileSafetyAssessment, getSharedVulkanSafetyAssessment } from './desktop';

describe('game file-safety desktop boundary', () => {
  beforeEach(() => {
    invokeDesktop.mockReset();
  });

  it('loads the game assessment independently with the game id', async () => {
    const assessment = {
      game_id: 'steam:1',
      context_token: 'game-token',
      detected_engines: ['EasyAntiCheat'],
      scan_completeness: 'complete',
    };
    invokeDesktop.mockResolvedValueOnce(assessment);

    await expect(getGameFileSafetyAssessment('steam:1')).resolves.toEqual(assessment);
    expect(invokeDesktop).toHaveBeenCalledWith('get_game_file_safety_assessment', {
      gameId: 'steam:1',
    });
  });

  it('loads the shared Vulkan assessment without a cached game-details payload', async () => {
    invokeDesktop.mockResolvedValueOnce({ context_token: 'shared-token' });

    await expect(getSharedVulkanSafetyAssessment()).resolves.toEqual({
      context_token: 'shared-token',
    });
    expect(invokeDesktop).toHaveBeenCalledWith('get_shared_vulkan_safety_assessment');
  });
});
