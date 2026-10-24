/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
import type * as SharedApi from '@shared/api';

const invokeDesktop = vi.hoisted(() => vi.fn());
vi.mock('@shared/api', async (importOriginal) => {
  const actual = await importOriginal<typeof SharedApi>();
  return {
    ...actual,
    invokeDesktop,
  };
});

import { DesktopCommandError } from '@shared/errors';
import { t } from '@shared/i18n';
import GameCardActionsMenu from './GameCardActionsMenu.svelte';

function defaultAssessment(gameId = 'game-1') {
  return {
    gameId,
    canRemoveAutomatically: true,
    nvapiAssessment: {
      requiresUserAuthorization: false,
      blockedReason: null,
      claims: [],
    },
  };
}

describe('GameCardActionsMenu catalog removal', () => {
  let target: HTMLDivElement;
  let component: object | undefined;

  beforeEach(() => {
    invokeDesktop.mockReset();
    vi.stubGlobal(
      'ResizeObserver',
      class {
        observe = vi.fn();
        unobserve = vi.fn();
        disconnect = vi.fn();
      },
    );
    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(async () => {
    if (component) {
      await unmount(component);
      component = undefined;
    }
    vi.unstubAllGlobals();
    document.body.replaceChildren();
  });

  it('requires explicit confirmation and calls removal only after confirmation', async () => {
    invokeDesktop.mockResolvedValueOnce(defaultAssessment('game-1'));
    const onRemoveFromCatalog = vi.fn(() => Promise.resolve(true));
    component = mount(GameCardActionsMenu, {
      target,
      props: {
        gameId: 'game-1',
        title: 'Games',
        canRemoveFromCatalog: true,
        open: true,
        onOpenChange: vi.fn(),
        onRemoveFromCatalog,
      },
    });
    flushSync();
    await tick();

    menuAction(t('game.card.menu.removeFromCatalog')).click();
    await vi.waitFor(() => {
      const confirmationDialog = document.querySelector('[role="alertdialog"]');
      expect(confirmationDialog).not.toBeNull();
      expect(confirmationDialog?.textContent).toContain(t('game.card.removeConfirm.description'));
    });
    expect(onRemoveFromCatalog).not.toHaveBeenCalled();

    const confirmationActions = buttons(t('game.card.removeConfirm.action'));
    const confirm = confirmationActions.pop();
    if (!confirm) {
      throw new Error('confirmation action was not rendered');
    }
    confirm.click();
    await vi.waitFor(() => {
      expect(onRemoveFromCatalog).toHaveBeenCalledTimes(1);
    });
  });

  it('keeps a cleanup failure inside the open confirmation dialog', async () => {
    invokeDesktop.mockResolvedValueOnce(defaultAssessment('game-1'));
    const onRemoveFromCatalog = vi.fn(() => Promise.reject(new Error('rollback failed')));
    component = mount(GameCardActionsMenu, {
      target,
      props: {
        gameId: 'game-1',
        title: 'Managed Game',
        canRemoveFromCatalog: true,
        open: true,
        onOpenChange: vi.fn(),
        onRemoveFromCatalog,
      },
    });
    flushSync();
    await tick();

    menuAction(t('game.card.menu.removeFromCatalog')).click();
    await vi.waitFor(() => {
      expect(document.body.textContent).toContain(t('game.card.removeConfirm.description'));
    });
    buttons(t('game.card.removeConfirm.action')).pop()?.click();

    await vi.waitFor(() => {
      expect(document.querySelector('[data-removal-error]')?.textContent).toContain(
        t('error.unexpectedClient'),
      );
    });
    expect(document.body.textContent).not.toContain('rollback failed');
    expect(document.body.textContent).toContain(t('game.card.removeConfirm.description'));
  });

  it('recovers from a removal failure when the user clicks retry assessment', async () => {
    invokeDesktop.mockResolvedValue(defaultAssessment('game-1'));
    const onRemoveFromCatalog = vi
      .fn()
      .mockRejectedValueOnce(new Error('transient write failure'))
      .mockResolvedValueOnce(true);

    component = mount(GameCardActionsMenu, {
      target,
      props: {
        gameId: 'game-1',
        title: 'Managed Game',
        canRemoveFromCatalog: true,
        open: true,
        onOpenChange: vi.fn(),
        onRemoveFromCatalog,
      },
    });
    flushSync();
    await tick();

    menuAction(t('game.card.menu.removeFromCatalog')).click();
    await vi.waitFor(() => {
      expect(document.body.textContent).toContain(t('game.card.removeConfirm.description'));
    });

    const confirmBtn = buttons(t('game.card.removeConfirm.action')).pop();
    if (!confirmBtn) {
      throw new Error('confirm button not found');
    }
    confirmBtn.click();

    await vi.waitFor(() => {
      expect(document.querySelector('[data-removal-error]')).not.toBeNull();
      expect(confirmBtn.disabled).toBe(true);
      expect(document.querySelector('[data-retry-assessment]')).not.toBeNull();
    });

    const retryBtn = document.querySelector('[data-retry-assessment]') as HTMLButtonElement;
    retryBtn.click();

    await vi.waitFor(() => {
      expect(document.querySelector('[data-removal-error]')).toBeNull();
      expect(confirmBtn.disabled).toBe(false);
    });

    confirmBtn.click();
    await vi.waitFor(() => {
      expect(onRemoveFromCatalog).toHaveBeenCalledTimes(2);
    });
  });

  it('shows the recovery bundle published for an ambiguous cleanup', async () => {
    invokeDesktop.mockResolvedValueOnce(defaultAssessment('game-1'));
    const recoveryBundlePath = 'C:/Recovery/renderpilot-bundle';
    const onRemoveFromCatalog = vi.fn(() =>
      Promise.reject(
        DesktopCommandError.fromDto({
          code: 'managed_cleanup_ambiguous',
          recoveryBundlePath,
        }),
      ),
    );
    component = mount(GameCardActionsMenu, {
      target,
      props: {
        gameId: 'game-1',
        title: 'Managed Game',
        canRemoveFromCatalog: true,
        open: true,
        onOpenChange: vi.fn(),
        onRemoveFromCatalog,
      },
    });
    flushSync();
    await tick();

    menuAction(t('game.card.menu.removeFromCatalog')).click();
    await vi.waitFor(() => {
      expect(document.body.textContent).toContain(t('game.card.removeConfirm.description'));
    });
    buttons(t('game.card.removeConfirm.action')).pop()?.click();

    await vi.waitFor(() => {
      expect(document.querySelector('[data-removal-error]')?.textContent).toContain(
        recoveryBundlePath,
      );
    });
  });

  it('does not expose removal for launcher-managed cards', async () => {
    component = mount(GameCardActionsMenu, {
      target,
      props: {
        gameId: 'game-launcher',
        title: 'Launcher Game',
        canRemoveFromCatalog: false,
        open: true,
        onOpenChange: vi.fn(),
      },
    });
    flushSync();
    await tick();

    expect(
      [...document.body.querySelectorAll('button, [role="menuitem"]')].some(
        (candidate) => candidate.textContent.trim() === t('game.card.menu.removeFromCatalog'),
      ),
    ).toBe(false);
  });

  it('shows diverged warning and lists affected settings, passing authorization token on confirm', async () => {
    const onRemoveFromCatalog = vi.fn(() => Promise.resolve(true));
    invokeDesktop.mockResolvedValueOnce({
      gameId: 'game-diverged',
      canRemoveAutomatically: false,
      nvapiAssessment: {
        requiresUserAuthorization: true,
        blockedReason: null,
        claims: [
          {
            targetId: 'Base Profile',
            settingId: 0x10b3292c,
            settingName: 'DLSS Preset',
            status: 'diverged',
            recordedExpected: { present: true, value: 1 },
            recordedOriginal: { present: false, value: null },
            observedLive: { present: true, value: 2 },
            disposition: 'requiresAbandonmentAuthorization',
          },
        ],
      },
    });

    component = mount(GameCardActionsMenu, {
      target,
      props: {
        gameId: 'game-diverged',
        title: 'Cyberpunk 2077',
        canRemoveFromCatalog: true,
        open: true,
        onOpenChange: vi.fn(),
        onRemoveFromCatalog,
      },
    });
    flushSync();
    await tick();

    menuAction(t('game.card.menu.removeFromCatalog')).click();
    await vi.waitFor(() => {
      expect(document.querySelector('[data-diverged-claims-warning]')).not.toBeNull();
      expect(document.querySelector('[data-diverged-settings-list]')?.textContent).toContain(
        'DLSS Preset',
      );
      expect(document.querySelector('[data-diverged-settings-list]')?.textContent).toContain(
        'Base Profile',
      );
      expect(document.querySelector('[data-diverged-settings-list]')?.textContent).toContain('2');
    });

    const confirmBtn = document.querySelector('[data-removal-confirm]') as HTMLButtonElement;
    expect(confirmBtn).not.toBeNull();
    expect(confirmBtn.textContent).toContain(t('game.card.removeConfirm.abandonAction'));

    confirmBtn.click();
    await vi.waitFor(() => {
      expect(onRemoveFromCatalog).toHaveBeenCalledWith({
        gameId: 'game-diverged',
        claims: [
          {
            targetId: 'Base Profile',
            settingId: 0x10b3292c,
            expectedPresent: true,
            expectedValue: 1,
            originalPresent: false,
            originalValue: null,
            observedLivePresent: true,
            observedLiveValue: 2,
          },
        ],
      });
    });
  });

  it('shows blocked alert and disables confirm button when removal is blocked', async () => {
    const onRemoveFromCatalog = vi.fn(() => Promise.resolve(true));
    invokeDesktop.mockResolvedValueOnce({
      gameId: 'game-blocked',
      canRemoveAutomatically: false,
      nvapiAssessment: {
        requiresUserAuthorization: false,
        blockedReason: 'executable witness changed: rebound to other profile',
        claims: [],
      },
    });

    component = mount(GameCardActionsMenu, {
      target,
      props: {
        gameId: 'game-blocked',
        title: 'Blocked Game',
        canRemoveFromCatalog: true,
        open: true,
        onOpenChange: vi.fn(),
        onRemoveFromCatalog,
      },
    });
    flushSync();
    await tick();

    menuAction(t('game.card.menu.removeFromCatalog')).click();
    await vi.waitFor(() => {
      const blockedAlert = document.querySelector('[data-removal-blocked]');
      expect(blockedAlert).not.toBeNull();
      expect(blockedAlert?.textContent).toContain(
        'executable witness changed: rebound to other profile',
      );
    });

    const confirmBtn = document.querySelector('[data-removal-confirm]') as HTMLButtonElement;
    expect(confirmBtn.disabled).toBe(true);
    expect(onRemoveFromCatalog).not.toHaveBeenCalled();
  });

  it('preflight error disables confirm button and displays retry assessment action', async () => {
    let callCount = 0;
    invokeDesktop.mockImplementation((command) => {
      if (command === 'assess_game_removal') {
        callCount++;
        if (callCount === 1) {
          return Promise.reject(new Error('Preflight IPC connection failed'));
        }
        return Promise.resolve(defaultAssessment('game-retry'));
      }
      return Promise.resolve({});
    });

    const onRemoveFromCatalog = vi.fn(() => Promise.resolve(true));
    component = mount(GameCardActionsMenu, {
      target,
      props: {
        gameId: 'game-retry',
        title: 'Retry Game',
        canRemoveFromCatalog: true,
        open: true,
        onOpenChange: vi.fn(),
        onRemoveFromCatalog,
      },
    });
    flushSync();
    await tick();

    menuAction(t('game.card.menu.removeFromCatalog')).click();
    await vi.waitFor(() => {
      expect(document.querySelector('[data-removal-error]')?.textContent).toContain(
        t('error.unexpectedClient'),
      );
    });

    const confirmBtn = document.querySelector('[data-removal-confirm]') as HTMLButtonElement;
    expect(confirmBtn.disabled).toBe(true);
    expect(onRemoveFromCatalog).not.toHaveBeenCalled();

    const retryBtn = document.querySelector('[data-retry-assessment]') as HTMLButtonElement;
    expect(retryBtn).not.toBeNull();
    retryBtn.click();

    await vi.waitFor(() => {
      expect(document.querySelector('[data-removal-error]')).toBeNull();
      expect(confirmBtn.disabled).toBe(false);
    });

    confirmBtn.click();
    await vi.waitFor(() => {
      expect(onRemoveFromCatalog).toHaveBeenCalledTimes(1);
    });
  });

  it('stale authorization triggers re-assessment, displays updated settings and stale warning alert, and requires user re-confirmation with fresh token', async () => {
    const onRemoveFromCatalog = vi.fn();
    onRemoveFromCatalog.mockRejectedValueOnce(
      DesktopCommandError.fromDto({
        code: 'game_removal_cleanup_failed',
        reasonCode: 'stale_authorization',
      }),
    );

    const initialAssessment = {
      gameId: 'game-stale',
      canRemoveAutomatically: false,
      nvapiAssessment: {
        requiresUserAuthorization: true,
        blockedReason: null,
        claims: [
          {
            targetId: 'Base Profile',
            settingId: 0x10b3292c,
            settingName: 'DLSS Preset',
            status: 'diverged',
            recordedExpected: { present: true, value: 1 },
            recordedOriginal: { present: false, value: null },
            observedLive: { present: true, value: 2 },
            disposition: 'requiresAbandonmentAuthorization',
          },
        ],
      },
    };

    const freshAssessment = {
      gameId: 'game-stale',
      canRemoveAutomatically: false,
      nvapiAssessment: {
        requiresUserAuthorization: true,
        blockedReason: null,
        claims: [
          {
            targetId: 'Base Profile',
            settingId: 0x10b3292c,
            settingName: 'DLSS Preset',
            status: 'diverged',
            recordedExpected: { present: true, value: 1 },
            recordedOriginal: { present: false, value: null },
            observedLive: { present: true, value: 3 },
            disposition: 'requiresAbandonmentAuthorization',
          },
        ],
      },
    };

    invokeDesktop.mockResolvedValueOnce(initialAssessment).mockResolvedValueOnce(freshAssessment);

    component = mount(GameCardActionsMenu, {
      target,
      props: {
        gameId: 'game-stale',
        title: 'Stale Game',
        canRemoveFromCatalog: true,
        open: true,
        onOpenChange: vi.fn(),
        onRemoveFromCatalog,
      },
    });
    flushSync();
    await tick();

    menuAction(t('game.card.menu.removeFromCatalog')).click();
    await vi.waitFor(() => {
      expect(document.querySelector('[data-diverged-claims-warning]')).not.toBeNull();
      expect(document.querySelector('[data-diverged-settings-list]')?.textContent).toContain('2');
    });

    const confirmBtn = document.querySelector('[data-removal-confirm]') as HTMLButtonElement;
    confirmBtn.click();

    await vi.waitFor(() => {
      expect(invokeDesktop).toHaveBeenCalledTimes(2);
      expect(document.querySelector('[data-stale-warning]')).not.toBeNull();
      expect(document.querySelector('[data-stale-warning]')?.textContent).toContain(
        t('game.card.removeConfirm.staleWarning'),
      );
      expect(document.querySelector('[data-diverged-settings-list]')?.textContent).toContain('3');
      expect(onRemoveFromCatalog).toHaveBeenCalledTimes(1);
    });

    // User confirms again with the refreshed state
    onRemoveFromCatalog.mockResolvedValueOnce(true);
    confirmBtn.click();

    await vi.waitFor(() => {
      expect(onRemoveFromCatalog).toHaveBeenCalledTimes(2);
      expect(onRemoveFromCatalog).toHaveBeenLastCalledWith({
        gameId: 'game-stale',
        claims: [
          {
            targetId: 'Base Profile',
            settingId: 0x10b3292c,
            expectedPresent: true,
            expectedValue: 1,
            originalPresent: false,
            originalValue: null,
            observedLivePresent: true,
            observedLiveValue: 3,
          },
        ],
      });
    });
  });
});

function menuAction(label: string): HTMLElement {
  const found = [...document.body.querySelectorAll<HTMLElement>('[role="menuitem"]')].find(
    (candidate) => candidate.textContent.trim() === label,
  );
  if (!found) {
    throw new Error(`button not found: ${label}`);
  }
  return found;
}

function buttons(label: string): HTMLButtonElement[] {
  return [...document.body.querySelectorAll<HTMLButtonElement>('button')].filter(
    (candidate) => candidate.textContent.trim() === label,
  );
}
