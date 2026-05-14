import { describe, expect, it, vi } from 'vitest';
import type { GameDetails } from '@entities/game';
import type { SwapPlan } from '@entities/operation';
import * as notificationsModule from '@shared/notifications';
import * as themeModule from '@shared/theme';
import { createDesktopAppModel } from './create-desktop-app-model.svelte';
import * as appNotificationsModule from './notifications';

function createStubPlan(operationId: string): SwapPlan {
  return {
    operation_id: operationId,
    confirmation_token: 'token',
    game_id: 'game-1',
    operation_type: 'swap',
    target_path: '/a',
    replacement_path: '/b',
    original_version: null,
    replacement_version: null,
    original_sha256: null,
    replacement_sha256: null,
    risk_level: 'safe',
    requires_backup: false,
    requires_elevation: false,
    artifact_id: 'art-1',
    blockers: [],
    warnings: [],
  };
}

describe('createDesktopAppModel', () => {
  it('initializes with default screen "games"', () => {
    const model = createDesktopAppModel();
    expect(model.screen).toBe('games');
  });

  it('initializes with no selected game', () => {
    const model = createDesktopAppModel();
    expect(model.selectedGameId).toBeNull();
    expect(model.currentDetails).toBeNull();
    expect(model.currentPlan).toBeNull();
  });

  it('clears the active status notification', () => {
    const clearStatusNotificationSpy = vi
      .spyOn(notificationsModule, 'clearStatusNotification')
      .mockImplementation(() => undefined);

    const model = createDesktopAppModel();

    model.clearError();

    expect(clearStatusNotificationSpy).toHaveBeenCalledTimes(1);

    clearStatusNotificationSpy.mockRestore();
  });

  it('getCurrentPlan returns null when operation_id mismatches', () => {
    const model = createDesktopAppModel();
    model.setCurrentPlan(createStubPlan('op-1'));
    expect(model.getCurrentPlan('op-2')).toBeNull();
  });

  it('getCurrentPlan returns plan when operation_id matches', () => {
    const model = createDesktopAppModel();
    const plan = createStubPlan('op-1');
    model.setCurrentPlan(plan);
    expect(model.getCurrentPlan('op-1')).toBe(plan);
  });

  it('runExclusive returns null when busy', async () => {
    const model = createDesktopAppModel();
    let releaseFirst: (value: string) => void = () => undefined;
    const firstPromise = new Promise<string>((resolve) => {
      releaseFirst = resolve;
    });

    void model.runExclusive(() => firstPromise);
    expect(model.busy).toBe(true);

    const result = await model.runExclusive(() => Promise.resolve('skipped'));
    expect(result).toBeNull();

    releaseFirst('done');
  });

  it('clearSelection resets selected game', () => {
    const model = createDesktopAppModel();
    model.setCurrentPlan(createStubPlan('op-1'));

    model.clearSelection();
    expect(model.selectedGameId).toBeNull();
    expect(model.currentDetails).toBeNull();
    expect(model.currentPlan).toBeNull();
  });

  it('changeThemeMode updates themeMode', () => {
    const model = createDesktopAppModel();
    expect(model.themeMode).toBeDefined();
    model.changeThemeMode('dark');
    expect(model.themeMode).toBe('dark');
    model.changeThemeMode('light');
    expect(model.themeMode).toBe('light');
  });

  it('handleNavigate switches to settings', () => {
    const model = createDesktopAppModel();
    model.handleNavigate('settings');
    expect(model.screen).toBe('settings');
  });

  it('handleNavigate opens workspace screen when selection exists', () => {
    const model = createDesktopAppModel();
    model.presentGameDetails(createStubDetails('game-1'), 'details');
    model.handleNavigate('operations');
    expect(model.screen).toBe('operations');
  });

  it('handleNavigate clears selection when workspace screen requested without selection', () => {
    const model = createDesktopAppModel();
    model.handleNavigate('details');
    expect(model.screen).toBe('games');
    expect(model.selectedGameId).toBeNull();
  });

  it('handleNavigate falls back to games for unknown screen', () => {
    const model = createDesktopAppModel();
    model.handleNavigate('unknown' as never);
    expect(model.screen).toBe('games');
  });

  it('presentGameDetails publishes the missing stable id notification when canonical id is null', () => {
    const publishMissingStableGameDetailsNotificationSpy = vi
      .spyOn(appNotificationsModule, 'publishMissingStableGameDetailsNotification')
      .mockReturnValue('desktop-status');

    const model = createDesktopAppModel();
    model.presentGameDetails(createStubDetails(''), 'details');

    expect(publishMissingStableGameDetailsNotificationSpy).toHaveBeenCalledTimes(1);
    expect(model.screen).toBe('games');

    publishMissingStableGameDetailsNotificationSpy.mockRestore();
  });

  it('showStalePlanError publishes the stale plan notification', () => {
    const publishStalePlanNotificationSpy = vi
      .spyOn(appNotificationsModule, 'publishStalePlanNotification')
      .mockReturnValue('desktop-status');

    const model = createDesktopAppModel();
    model.showStalePlanError();

    expect(publishStalePlanNotificationSpy).toHaveBeenCalledTimes(1);

    publishStalePlanNotificationSpy.mockRestore();
  });

  it('showError respects warning severity for command warnings', () => {
    const publishCommandErrorNotificationSpy = vi
      .spyOn(notificationsModule, 'publishCommandErrorNotification')
      .mockReturnValue('desktop-status');

    const warning = {
      code: 'catalog_partial_scan',
      severity: 'warning' as const,
      messageKey: 'warnings.catalog_partial_scan',
      details: 'Some folders could not be scanned.',
      suggestedActions: [],
    };

    const model = createDesktopAppModel();
    model.showError(warning);

    expect(publishCommandErrorNotificationSpy).toHaveBeenCalledWith(warning);

    publishCommandErrorNotificationSpy.mockRestore();
  });

  it('changeThemeMode rolls back when persistThemeMode throws', () => {
    const spy = vi.spyOn(themeModule, 'persistThemeMode').mockImplementation(() => {
      throw new Error('disk error');
    });
    const publishCommandErrorNotificationSpy = vi
      .spyOn(notificationsModule, 'publishCommandErrorNotification')
      .mockReturnValue('desktop-status');

    const model = createDesktopAppModel();
    const previousMode = model.themeMode;
    model.changeThemeMode('dark');

    const latestCall =
      publishCommandErrorNotificationSpy.mock.calls[
        publishCommandErrorNotificationSpy.mock.calls.length - 1
      ];
    const [error] = latestCall;

    expect(model.themeMode).toBe(previousMode);
    expect(error).toBeInstanceOf(Error);
    expect((error as Error).message).toContain('disk error');

    spy.mockRestore();
    publishCommandErrorNotificationSpy.mockRestore();
  });

  it('runExclusive shows error and returns null when task throws', async () => {
    const publishCommandErrorNotificationSpy = vi
      .spyOn(notificationsModule, 'publishCommandErrorNotification')
      .mockReturnValue('desktop-status');
    const model = createDesktopAppModel();
    const result = await model.runExclusive(async () => {
      await Promise.resolve();
      throw new Error('task failed');
    });

    const latestCall =
      publishCommandErrorNotificationSpy.mock.calls[
        publishCommandErrorNotificationSpy.mock.calls.length - 1
      ];
    const [error] = latestCall;

    expect(result).toBeNull();
    expect(error).toBeInstanceOf(Error);
    expect((error as Error).message).toContain('task failed');
    expect(model.busy).toBe(false);

    publishCommandErrorNotificationSpy.mockRestore();
  });
});

function createStubDetails(gameId: string): GameDetails {
  return {
    game: {
      identity: { id: gameId, title: 'Test Game' },
    },
    components: [],
    candidate_groups: [],
    operations: [],
  } as unknown as GameDetails;
}
