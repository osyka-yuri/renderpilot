/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import type { LumaStore } from '../model/create-luma-store.svelte';
import LumaCard from './LumaCard.svelte';

describe('LumaCard availability failure', () => {
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

    component = mount(LumaCard, {
      target,
      props: { gameId: 'luma-game', launcher: 'Steam', store },
    });
    flushSync();

    expect(target.textContent).toContain('Could not check');

    const retry = [...target.querySelectorAll<HTMLButtonElement>('button')].find(
      (button) => button.textContent === 'Retry',
    );
    retry?.click();

    expect(retryStore).toHaveBeenCalledWith('luma-game');
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
