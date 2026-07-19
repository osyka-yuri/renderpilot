/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import type { RenoDxStore } from '../model/create-renodx-store.svelte';
import RenoDxCard from './RenoDxCard.svelte';

describe('RenoDxCard availability failure', () => {
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

    component = mount(RenoDxCard, {
      target,
      props: {
        gameId: 'renodx-game',
        store,
        onOpenRenoDxSettings: vi.fn(),
      },
    });
    flushSync();

    expect(target.textContent).toContain('Could not check');

    const retry = [...target.querySelectorAll<HTMLButtonElement>('button')].find(
      (button) => button.textContent === 'Retry',
    );
    retry?.click();

    expect(retryStore).toHaveBeenCalledWith('renodx-game');
  });
});

function loadErrorStore(retry: RenoDxStore['retry']): RenoDxStore {
  return {
    busy: false,
    retry,
    loading: false,
    loaded: false,
    loadError: 'RenoDX availability failed',
    isInstalled: false,
    isBlockedByOtherAddon: false,
    isExternal: false,
    isNativeHdr: false,
    isBlacklisted: false,
    isUnsupported: false,
    isIncompatible: false,
    isInstallable: false,
    blacklistMessage: null,
    outcome: null,
    manualInstall: null,
    state: null,
    otherAddonUnmanaged: false,
    otherAddonKind: null,
  } as unknown as RenoDxStore;
}
