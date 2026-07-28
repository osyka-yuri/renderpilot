/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import { defaultHostFacts } from '@entities/addon';

import type { RenoDxStore } from '../model/create-renodx-store.svelte';
import RenoDxCardTestHost from './RenoDxCard.test-host.svelte';

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

    component = mount(RenoDxCardTestHost, {
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

  it('prefetches settings on pointer and keyboard intent before opening them', () => {
    const onPreloadRenoDxSettings = vi.fn();
    const onOpenRenoDxSettings = vi.fn();

    component = mount(RenoDxCardTestHost, {
      target,
      props: {
        gameId: 'renodx-game',
        store: vulkanInstalledStore(),
        onOpenRenoDxSettings,
        onPreloadRenoDxSettings,
      },
    });
    flushSync();

    const settings = target.querySelector<HTMLButtonElement>(
      'button[aria-label="Open RenoDX settings"]',
    );
    expect(settings).not.toBeNull();

    settings?.dispatchEvent(new Event('pointerenter'));
    settings?.focus();
    settings?.click();

    expect(onPreloadRenoDxSettings).toHaveBeenCalledTimes(2);
    expect(onOpenRenoDxSettings).toHaveBeenCalledTimes(1);
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

function vulkanInstalledStore(): RenoDxStore {
  return {
    ...loadErrorStore(vi.fn(() => Promise.resolve())),
    loaded: true,
    loadError: null,
    isInstalled: true,
    state: {
      status: 'installed',
      host_kind: 'vulkan',
      version: null,
      addon_dated: null,
      installed_at: 0,
      updated_at: 0,
      dlss_fix_installed: false,
      addon_tracked: true,
    },
    freshness: 'current',
    addonDated: null,
    installedAt: null,
    lastCheckedAt: null,
    requiresConfirmation: false,
    hostDetection: 'absent',
    hostFacts: defaultHostFacts('stable'),
    hostActions: {},
    hostUpdate: null,
    addonUpdate: null,
    updateAvailable: false,
    checkForUpdates: vi.fn(() => Promise.resolve()),
    update: vi.fn(() => Promise.resolve('success')),
    uninstall: vi.fn(() => Promise.resolve('success')),
    renodxAddon: null,
    addonTracked: true,
    reshadeChannel: null,
    selectedReshadeChannel: 'stable',
    reshadeStableSupported: true,
    dlssFixInstalled: false,
    dlssFixAvailable: false,
    dlssFixUpdate: null,
    vulkanUpdateDiagnostics: [],
  } as unknown as RenoDxStore;
}
