/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import { createLumaStore, type LumaStore } from '../model/create-luma-store.svelte';
import {
  availability,
  fakeApi,
  INSTALLED,
  INSTALLABLE_OUTCOME,
} from '../model/luma-store-test-fixtures';
import LumaCardTestHost from './LumaCard.test-host.svelte';

describe('LumaCard', () => {
  let target: HTMLDivElement;
  let component: object | undefined;

  beforeEach(() => {
    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(async () => {
    if (component) {
      await unmount(component);
      component = undefined;
    }
    target.remove();
  });

  it('uses the shared one-line availability failure and retries through its store', () => {
    const retryStore = vi.fn(() => Promise.resolve());
    const store = loadErrorStore(retryStore);

    component = mount(LumaCardTestHost, {
      target,
      props: { gameId: 'luma-game', launcher: 'Steam', store },
    });
    flushSync();

    expect(target.textContent).toContain('Could not check');

    const retry = [...target.querySelectorAll<HTMLButtonElement>('button')].find(
      (button) => button.textContent.trim() === 'Retry',
    );
    retry?.click();

    expect(retryStore).toHaveBeenCalledWith('luma-game');
  });

  it('keeps Luma launch arguments in the shared copyable launcher-aware callout', async () => {
    const store = createLumaStore({
      api: fakeApi({
        getAvailability: vi.fn(() =>
          Promise.resolve(
            availability({
              state: { status: 'not_installed' },
              outcome: { ...INSTALLABLE_OUTCOME, launch_args: ['-dx11'] },
            }),
          ),
        ),
      }),
    });
    await store.load('luma-game');

    component = mount(LumaCardTestHost, {
      target,
      props: { gameId: 'luma-game', launcher: 'Steam', store },
    });
    flushSync();

    expect(target.textContent).toContain('This add-on requires DirectX 11');
    expect(target.textContent).toContain('-dx11');
    expect(target.textContent).toContain('If you start the game through Steam');
    expect(target.querySelector('button[aria-label="Copy arguments"]')).not.toBeNull();
  });

  it('explains and disables uninstall when persisted OptiScaler requires Luma', async () => {
    const store = createLumaStore({
      api: fakeApi({
        getAvailability: vi.fn(() =>
          Promise.resolve({ ...INSTALLED, uninstall_blocked_by: 'optiscaler' as const }),
        ),
      }),
    });
    await store.load('luma-game');

    component = mount(LumaCardTestHost, {
      target,
      props: { gameId: 'luma-game', launcher: 'Steam', store },
    });
    flushSync();

    expect(target.textContent).toContain(
      'OptiScaler requires Luma for this game. Uninstall OptiScaler first.',
    );
    const uninstall = [...target.querySelectorAll<HTMLButtonElement>('button')].find(
      (button) => button.textContent.trim() === 'Remove Luma',
    );
    expect(uninstall?.disabled).toBe(true);
  });

  it('displays compatibility badge alongside installed status when installed', async () => {
    const store = createLumaStore({
      api: fakeApi({
        getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      }),
    });
    await store.load('luma-game');

    component = mount(LumaCardTestHost, {
      target,
      props: { gameId: 'luma-game', launcher: 'Steam', store },
    });
    flushSync();

    expect(target.textContent).toContain('Installed');
    expect(target.textContent).toContain('Confirmed');
  });

  it('renders disabled install button and attribution when blocked by another addon', async () => {
    const store = createLumaStore({
      api: fakeApi({
        getAvailability: vi.fn(() =>
          Promise.resolve(
            availability({
              state: { status: 'not_installed' },
              outcome: {
                kind: 'blocked_by_other_addon',
                other_kind: 'renodx',
                unmanaged: false,
              },
            }),
          ),
        ),
      }),
    });
    await store.load('luma-game');

    component = mount(LumaCardTestHost, {
      target,
      props: { gameId: 'luma-game', launcher: 'Steam', store },
    });
    flushSync();

    expect(target.textContent).toContain(
      'RenoDX is installed for this game — uninstall it before installing Luma.',
    );
    expect(target.textContent).toContain('Luma Framework by Filoppi.');
    const installButton = [...target.querySelectorAll<HTMLButtonElement>('button')].find(
      (button) => button.textContent.trim() === 'Install',
    );
    expect(installButton).toBeDefined();
    expect(installButton?.disabled).toBe(true);
    expect(installButton?.querySelector('svg')).not.toBeNull();
  });
});

function loadErrorStore(retry: LumaStore['retry']): LumaStore {
  return {
    busy: false,
    retry,
    loading: false,
    loaded: false,
    loadError: 'Luma availability failed',
    isInstalled: false,
    isBlockedByOtherAddon: false,
    isUnmanagedPresent: false,
    isBlacklisted: false,
    isUnsupported: false,
    isIncompatible: false,
    isInstallable: false,
    blacklistMessage: null,
    outcome: null,
  } as unknown as LumaStore;
}
