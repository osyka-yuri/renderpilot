import { describe, expect, it, vi } from 'vitest';
import type { GameDetails } from '@entities/game';
import type { SwapPlan } from '@entities/operation';
import * as themeModule from '@shared/theme';
import { createDesktopAppModel } from './create-desktop-app-model.svelte';

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

  it('toggles advanced mode', () => {
    const model = createDesktopAppModel();
    expect(model.advancedMode).toBe(false);
    model.toggleAdvancedMode();
    expect(model.advancedMode).toBe(true);
    model.toggleAdvancedMode();
    expect(model.advancedMode).toBe(false);
  });

  it('clears error message', () => {
    const model = createDesktopAppModel();
    model.setErrorMessage('Something went wrong');
    expect(model.errorMessage).toBe('Something went wrong');
    model.clearError();
    expect(model.errorMessage).toBe('');
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

  it('handleNavigate switches to settings with correct backTarget', () => {
    const model = createDesktopAppModel();
    model.handleNavigate('settings');
    expect(model.screen).toBe('settings');
    expect(model.backTarget).toBe('games');
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

  it('handleBack from settings returns to backTarget', () => {
    const model = createDesktopAppModel();
    model.presentGameDetails(createStubDetails('game-1'), 'details');
    model.handleNavigate('settings');
    expect(model.screen).toBe('settings');
    model.handleBack();
    expect(model.screen).toBe('details');
  });

  it('handleBack from operations goes to details when selection exists', () => {
    const model = createDesktopAppModel();
    model.presentGameDetails(createStubDetails('game-1'), 'operations');
    expect(model.screen).toBe('operations');
    model.handleBack();
    expect(model.screen).toBe('details');
  });

  it('handleBack from operations goes to games when no selection', () => {
    const model = createDesktopAppModel();
    model.presentGameDetails(createStubDetails('game-1'), 'operations');
    model.clearSelection();
    expect(model.screen).toBe('games');
  });

  it('presentGameDetails sets error when canonical id is null', () => {
    const model = createDesktopAppModel();
    model.presentGameDetails(createStubDetails(''), 'details');
    expect(model.errorMessage).toBe('Catalog returned game details without a stable identifier.');
    expect(model.screen).toBe('games');
  });

  it('clearSelection resets backTarget when it was a workspace screen', () => {
    const model = createDesktopAppModel();
    model.presentGameDetails(createStubDetails('game-1'), 'details');
    model.handleNavigate('settings');
    expect(model.backTarget).toBe('details');

    model.clearSelection();
    expect(model.backTarget).toBe('games');
    expect(model.screen).toBe('settings');
  });

  it('changeThemeMode rolls back when persistThemeMode throws', () => {
    const spy = vi.spyOn(themeModule, 'persistThemeMode').mockImplementation(() => {
      throw new Error('disk error');
    });

    const model = createDesktopAppModel();
    const previousMode = model.themeMode;
    model.changeThemeMode('dark');
    expect(model.themeMode).toBe(previousMode);
    expect(model.errorMessage).toContain('disk error');

    spy.mockRestore();
  });

  it('runExclusive shows error and returns null when task throws', async () => {
    const model = createDesktopAppModel();
    const result = await model.runExclusive(async () => {
      await Promise.resolve();
      throw new Error('task failed');
    });
    expect(result).toBeNull();
    expect(model.errorMessage).toContain('task failed');
    expect(model.busy).toBe(false);
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
