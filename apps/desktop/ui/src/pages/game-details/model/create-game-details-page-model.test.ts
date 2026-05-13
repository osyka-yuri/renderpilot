import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ApplyOperationResult, SwapPlan } from '@entities/operation';

import {
  applyOperationPlan,
  buildSwapPlan,
  publishApplyCompletedNotification,
} from '@entities/operation';
import { createGameDetailsPageModel } from './create-game-details-page-model';

vi.mock('@entities/operation', () => ({
  buildSwapPlan: vi.fn(),
  applyOperationPlan: vi.fn(),
  publishApplyCompletedNotification: vi.fn(),
}));

describe('createGameDetailsPageModel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows apply success after a completed apply flow', async () => {
    const setCurrentPlan = vi.fn();
    const reloadGameDetails = vi.fn(() => Promise.resolve());

    vi.mocked(applyOperationPlan).mockResolvedValue(createApplyOperationResult(2));

    const model = createGameDetailsPageModel({
      getSelectedGameId: () => 'game-1',
      checkIsGameStillSelected: () => true,
      runExclusive: async (task) => task(),
      setCurrentPlan,
      getCurrentPlan: () => createSwapPlan(),
      showStalePlanError: vi.fn(),
      reloadGameDetails,
    });

    await model.handleApply('op-1');

    expect(applyOperationPlan).toHaveBeenCalledWith('op-1', 'token');
    expect(setCurrentPlan).toHaveBeenCalledWith(null);
    expect(reloadGameDetails).toHaveBeenCalledTimes(1);
    expect(publishApplyCompletedNotification).toHaveBeenCalledWith(2);
  });

  it('does not show apply success when the plan is stale', async () => {
    const showStalePlanError = vi.fn();

    const model = createGameDetailsPageModel({
      getSelectedGameId: () => 'game-1',
      checkIsGameStillSelected: () => true,
      runExclusive: async (task) => task(),
      setCurrentPlan: vi.fn(),
      getCurrentPlan: () => null,
      showStalePlanError,
      reloadGameDetails: vi.fn(() => Promise.resolve()),
    });

    await model.handleApply('op-1');

    expect(showStalePlanError).toHaveBeenCalledTimes(1);
    expect(publishApplyCompletedNotification).not.toHaveBeenCalled();
    expect(applyOperationPlan).not.toHaveBeenCalled();
  });

  it('does not show apply success when runExclusive returns null', async () => {
    vi.mocked(applyOperationPlan).mockResolvedValue(createApplyOperationResult(1));

    const model = createGameDetailsPageModel({
      getSelectedGameId: () => 'game-1',
      checkIsGameStillSelected: () => true,
      runExclusive: () => Promise.resolve(null),
      setCurrentPlan: vi.fn(),
      getCurrentPlan: () => createSwapPlan(),
      showStalePlanError: vi.fn(),
      reloadGameDetails: vi.fn(() => Promise.resolve()),
    });

    await model.handleApply('op-1');

    expect(publishApplyCompletedNotification).not.toHaveBeenCalled();
  });

  it('builds a swap plan for the selected game', async () => {
    const setCurrentPlan = vi.fn();
    const plan = createSwapPlan();

    vi.mocked(buildSwapPlan).mockResolvedValue(plan);

    const model = createGameDetailsPageModel({
      getSelectedGameId: () => 'game-1',
      checkIsGameStillSelected: () => true,
      runExclusive: async (task) => task(),
      setCurrentPlan,
      getCurrentPlan: () => null,
      showStalePlanError: vi.fn(),
      reloadGameDetails: vi.fn(() => Promise.resolve()),
    });

    await model.handleBuildPlan('component-1', 'artifact-1');

    expect(buildSwapPlan).toHaveBeenCalledWith('game-1', 'component-1', 'artifact-1');
    expect(setCurrentPlan).toHaveBeenLastCalledWith(plan);
  });
});

function createSwapPlan(): SwapPlan {
  return {
    operation_id: 'op-1',
    confirmation_token: 'token',
    game_id: 'game-1',
    operation_type: 'swap',
    target_path: '/game/file.dll',
    replacement_path: '/catalog/file.dll',
    original_version: null,
    replacement_version: null,
    original_sha256: null,
    replacement_sha256: null,
    risk_level: 'safe',
    requires_backup: true,
    requires_elevation: false,
    artifact_id: 'artifact-1',
    blockers: [],
    warnings: [],
  };
}

function createApplyOperationResult(itemCount: number): ApplyOperationResult {
  return {
    operation_id: 'op-1',
    game_id: 'game-1',
    status: 'completed',
    completed_at: 1,
    items: Array.from({ length: itemCount }, (_, index) => ({
      backup_id: `backup-${index + 1}`,
      component_id: `component-${index + 1}`,
      applied_path: `/game/file-${index + 1}.dll`,
      replacement_path: `/catalog/file-${index + 1}.dll`,
      backup_path: `/backup/file-${index + 1}.dll`,
    })),
  };
}