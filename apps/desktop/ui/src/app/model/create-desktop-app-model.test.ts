import { describe, expect, it, vi } from 'vitest';
import type { GameDetails } from '@entities/game';
import { createGameDetails } from '@entities/game';
import type { SwapPlan } from '@entities/operation';
import * as notificationsModule from '@shared/notifications';
import { createDesktopAppModel } from './create-desktop-app-model.svelte';
import * as appNotificationsModule from './notifications';

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

  it('exposes the full initialization elevation snapshot', () => {
    const model = createDesktopAppModel(() => ({
      isElevated: false,
      elevationSupported: true,
      elevationUserDeclined: true,
      elevationAttempted: true,
    }));

    expect(model.isElevated).toBe(false);
    expect(model.elevationSupported).toBe(true);
    expect(model.elevationUserDeclined).toBe(true);
    expect(model.elevationAttempted).toBe(true);
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

  it('routes plan state through the workspace submodel', () => {
    const model = createDesktopAppModel();
    const plan = swapPlan({
      game_id: 'game-1',
      artifact_id: 'art-1',
      component_id: 'comp-1',
      target_path: '/a',
      replacement_path: '/b',
    });

    model.workspace.setCurrentPlan(plan);
    expect(model.workspace.getCurrentPlan('game-2')).toBeNull();
    expect(model.workspace.getCurrentPlan('game-1')).toBe(plan);
    expect(model.currentPlan).toBe(plan);
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

  it('runExclusive can route a scoped workflow error to an inline handler', async () => {
    const model = createDesktopAppModel();
    const error = new Error('folder rejected');
    const onError = vi.fn();

    const result = await model.runExclusive(() => Promise.reject(error), { onError });

    expect(result).toBeNull();
    expect(onError).toHaveBeenCalledWith(error);
  });

  it('clearSelection resets selected game and plan via workspace', () => {
    const model = createDesktopAppModel();
    model.workspace.setCurrentPlan(
      swapPlan({
        game_id: 'game-1',
        artifact_id: 'art-1',
        component_id: 'comp-1',
        target_path: '/a',
        replacement_path: '/b',
      }),
    );

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

  it('changeLanguageMode exposes the pending mode and settles on the applied mode', async () => {
    const model = createDesktopAppModel();
    const previous = model.languageMode;
    const next = previous === 'en' ? 'ru' : 'en';

    const transition = model.changeLanguageMode(next);
    expect(model.languageMode).toBe(next);

    await transition;
    expect(model.languageMode).toBe(next);
    expect(model.languageBusy).toBe(false);
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

  it('showError respects warning severity for command warnings', () => {
    const publishCommandErrorNotificationSpy = vi
      .spyOn(notificationsModule, 'publishCommandErrorNotification')
      .mockReturnValue('desktop-status');

    const warning = {
      kind: 'command',
      severity: 'warning',
      message: 'soft failure',
    };

    const model = createDesktopAppModel();
    model.showError(warning);

    expect(publishCommandErrorNotificationSpy).toHaveBeenCalledWith(warning);
    publishCommandErrorNotificationSpy.mockRestore();
  });

  it('showError reports task-failed errors', () => {
    const publishCommandErrorNotificationSpy = vi
      .spyOn(notificationsModule, 'publishCommandErrorNotification')
      .mockReturnValue('desktop-status');

    const error = new Error('task failed');
    const model = createDesktopAppModel();
    model.showError(error);

    expect(publishCommandErrorNotificationSpy).toHaveBeenCalledWith(error);
    expect(model.busy).toBe(false);

    publishCommandErrorNotificationSpy.mockRestore();
  });
});

function swapPlan(overrides: Partial<SwapPlan> = {}): SwapPlan {
  return {
    operation_id: 'operation:test',
    confirmation_token: 'token:test',
    game_id: 'game-1',
    component_id: 'component-1',
    operation_type: 'replace_component',
    artifact_id: 'artifact-1',
    target_path: '/target',
    replacement_path: '/replacement',
    original_version: null,
    replacement_version: null,
    original_sha256: null,
    replacement_sha256: null,
    risk_level: 'low',
    requires_elevation: false,
    blockers: [],
    warnings: [],
    files: [],
    d3d12_executable_action: null,
    ...overrides,
  };
}

function createStubDetails(gameId: string): GameDetails {
  return createGameDetails({
    game: {
      identity: { id: gameId, title: 'Test Game', launcher: 'Manual' },
      platform: 'Windows',
      runtime: 'NativeWindows',
      install_path: '/test',
      can_remove_from_catalog: true,
    },
  });
}
