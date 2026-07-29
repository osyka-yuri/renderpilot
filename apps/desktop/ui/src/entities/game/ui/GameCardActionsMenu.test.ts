/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';

import { normalizeCommandError } from '@shared/api';
import { t } from '@shared/i18n';
import GameCardActionsMenu from './GameCardActionsMenu.svelte';

describe('GameCardActionsMenu catalog removal', () => {
  let target: HTMLDivElement;
  let component: object | undefined;

  beforeEach(() => {
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
    const onRemoveFromCatalog = vi.fn(() => Promise.resolve(true));
    component = mount(GameCardActionsMenu, {
      target,
      props: {
        title: 'Games',
        canRemoveFromCatalog: true,
        open: true,
        onOpenChange: vi.fn(),
        onRemoveFromCatalog,
      },
    });
    flushSync();
    await tick();

    button(t('game.card.menu.removeFromCatalog')).click();
    await vi.waitFor(() => {
      expect(document.body.textContent).toContain(t('game.card.removeConfirm.description'));
    });
    expect(onRemoveFromCatalog).not.toHaveBeenCalled();
    expect(document.querySelector('[data-removal-actions]')).toBeNull();

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
    const onRemoveFromCatalog = vi.fn(() => Promise.reject(new Error('rollback failed')));
    component = mount(GameCardActionsMenu, {
      target,
      props: {
        title: 'Managed Game',
        canRemoveFromCatalog: true,
        open: true,
        onOpenChange: vi.fn(),
        onRemoveFromCatalog,
      },
    });
    flushSync();

    button(t('game.card.menu.removeFromCatalog')).click();
    await vi.waitFor(() => {
      expect(document.body.textContent).toContain(t('game.card.removeConfirm.description'));
    });
    buttons(t('game.card.removeConfirm.action')).pop()?.click();

    await vi.waitFor(() => {
      expect(document.querySelector('[data-removal-error]')?.textContent).toContain(
        'rollback failed',
      );
    });
    expect(document.body.textContent).toContain(t('game.card.removeConfirm.description'));
  });

  it('shows the recovery bundle published for an ambiguous cleanup', async () => {
    const recoveryBundlePath = 'C:/Recovery/renderpilot-bundle';
    const onRemoveFromCatalog = vi.fn(() =>
      Promise.reject(
        normalizeCommandError({
          code: 'managed_cleanup_ambiguous',
          severity: 'error',
          messageKey: 'user_message.managed_cleanup_ambiguous',
          details: 'Cleanup could not be ordered safely.',
          recoveryBundlePath,
          suggestedActions: [],
        }),
      ),
    );
    component = mount(GameCardActionsMenu, {
      target,
      props: {
        title: 'Managed Game',
        canRemoveFromCatalog: true,
        open: true,
        onOpenChange: vi.fn(),
        onRemoveFromCatalog,
      },
    });
    flushSync();

    button(t('game.card.menu.removeFromCatalog')).click();
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
        title: 'Launcher Game',
        canRemoveFromCatalog: false,
        open: true,
        onOpenChange: vi.fn(),
      },
    });
    flushSync();
    await tick();

    expect(
      [...document.body.querySelectorAll('button')].some(
        (candidate) => candidate.textContent.trim() === t('game.card.menu.removeFromCatalog'),
      ),
    ).toBe(false);
  });
});

function button(label: string): HTMLButtonElement {
  const found = [...document.body.querySelectorAll<HTMLButtonElement>('button')].find(
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
