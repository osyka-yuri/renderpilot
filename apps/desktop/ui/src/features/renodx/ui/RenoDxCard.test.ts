/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import { defaultHostFacts } from '@entities/addon';

import { createRenoDxStore, type RenoDxStore } from '../model/create-renodx-store.svelte';
import {
  availability,
  DLSS_FIX_PENDING_RECOVERY,
  fakeApi,
  INSTALLED,
  NOT_INSTALLED_SAFE,
} from '../model/renodx-store-test-fixtures';
import RenoDxCardTestHost from './RenoDxCard.test-host.svelte';

describe('RenoDxCard', () => {
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
      (button) => button.textContent.trim() === 'Retry',
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

  it('renders a no-row DLSS-Fix recovery view without mounting the installed panel', () => {
    const retryDlssFixRecovery: RenoDxStore['retryDlssFixRecovery'] = vi.fn(() =>
      Promise.resolve<'ok'>('ok'),
    );
    const store = noRowRecoveryStore(retryDlssFixRecovery);
    component = mount(RenoDxCardTestHost, {
      target,
      props: {
        gameId: 'renodx-game',
        store,
        onOpenRenoDxSettings: vi.fn(),
      },
    });
    flushSync();

    expect(target.textContent).toContain('A previous DLSS-Fix operation needs recovery.');
    expect(target.textContent).not.toContain('ReShade host');

    const retry = [...target.querySelectorAll<HTMLButtonElement>('button')].find(
      (button) => button.textContent === 'Finish recovery',
    );
    retry?.click();
    expect(retryDlssFixRecovery).toHaveBeenCalledWith('renodx-game');
  });

  it('keeps DLSS-Fix recovery ahead of inactive persisted-record cleanup', async () => {
    const persistedRecord = availability({
      ...NOT_INSTALLED_SAFE,
      has_persisted_record: true,
    });
    const retryDlssFixRecovery = vi.fn(() => Promise.resolve(INSTALLED.state));
    const uninstall = vi.fn(() => Promise.resolve(NOT_INSTALLED_SAFE.state));
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(persistedRecord)),
      dlssFixAvailability: vi.fn(() => Promise.resolve(DLSS_FIX_PENDING_RECOVERY)),
      retryDlssFixRecovery,
      uninstall,
    });
    const store = createRenoDxStore({ api });
    await store.load('renodx-game');

    expect(store.hasPersistedRecord).toBe(true);
    expect(store.dlssFix.kind).toBe('recovery_pending');

    component = mount(RenoDxCardTestHost, {
      target,
      props: {
        gameId: 'renodx-game',
        store,
        onOpenRenoDxSettings: vi.fn(),
      },
    });
    flushSync();

    expect(target.textContent).toContain('A previous DLSS-Fix operation needs recovery.');
    const recovery = [...target.querySelectorAll<HTMLButtonElement>('button')].find(
      (button) => button.textContent.trim() === 'Finish recovery',
    );
    expect(recovery).toBeDefined();
    expect(
      [...target.querySelectorAll<HTMLButtonElement>('button')].some(
        (button) => button.textContent.trim() === 'Remove RenoDX',
      ),
    ).toBe(false);

    recovery?.click();
    await vi.waitFor(() => {
      expect(retryDlssFixRecovery).toHaveBeenCalledWith('renodx-game');
    });
    expect(uninstall).not.toHaveBeenCalled();
  });

  it('keeps normal installed DLSS-Fix update and remove actions in the installed panel', () => {
    const updateDlssFix = vi.fn(() => Promise.resolve('ok'));
    const uninstallDlssFix = vi.fn(() => Promise.resolve('ok'));
    const store = {
      ...vulkanInstalledStore(),
      dlssFix: {
        kind: 'component',
        primaryAction: { kind: 'update', labelKey: 'gameDetails.renodx.actionUpdate' },
        canRemove: true,
        descriptionKey: 'gameDetails.renodx.component.dlssFixDesc',
        status: 'available',
      },
      updateDlssFix,
      uninstallDlssFix,
    } as unknown as RenoDxStore;
    component = mount(RenoDxCardTestHost, {
      target,
      props: {
        gameId: 'renodx-game',
        store,
        onOpenRenoDxSettings: vi.fn(),
      },
    });
    flushSync();

    const buttons = [...target.querySelectorAll<HTMLButtonElement>('button')];
    expect(buttons.map((button) => button.textContent.trim())).toContain('Update');
    expect(buttons.map((button) => button.textContent.trim())).toContain('Remove');

    const update = buttons.find((button) => button.textContent.trim() === 'Update');
    const remove = buttons.find((button) => button.textContent.trim() === 'Remove');
    if (!update || !remove) {
      throw new Error('Expected the installed DLSS-Fix update and remove actions.');
    }
    update.click();
    remove.click();

    expect(updateDlssFix).toHaveBeenCalledWith('renodx-game');
    expect(uninstallDlssFix).toHaveBeenCalledWith('renodx-game');
  });

  it('displays compatibility badge alongside installed status when installed', async () => {
    const store = createRenoDxStore({
      api: fakeApi({
        getAvailability: vi.fn(() =>
          Promise.resolve(
            availability({
              ...INSTALLED,
              outcome: {
                kind: 'installable',
                confidence: 'verified',
                generic_profile: null,
                profile_id: null,
                host_kind: 'proxy',
                guidance: [],
                launch: null,
              },
            }),
          ),
        ),
      }),
    });
    await store.load('renodx-game');

    component = mount(RenoDxCardTestHost, {
      target,
      props: {
        gameId: 'renodx-game',
        store,
        onOpenRenoDxSettings: vi.fn(),
      },
    });
    flushSync();

    expect(target.textContent).toContain('Installed');
    expect(target.textContent).toContain('Confirmed');
  });

  it('shows cleanup for an inactive RenoDX record and hides install offers', async () => {
    const persistedRecord = availability({
      state: { status: 'not_installed' },
      has_persisted_record: true,
      outcome: {
        kind: 'installable',
        confidence: 'verified',
        generic_profile: null,
        profile_id: null,
        host_kind: 'proxy',
        guidance: [],
        launch: null,
      },
      manual_install: {
        host_kind: 'proxy',
        expected_addon_name: 'renodx-borderlands2',
        game_arch: 'x86',
      },
    });
    let ownerExists = true;
    const api = fakeApi({
      getAvailability: vi.fn(() =>
        Promise.resolve(
          ownerExists
            ? persistedRecord
            : availability({
                ...persistedRecord,
                has_persisted_record: false,
              }),
        ),
      ),
      uninstall: vi.fn(() => {
        ownerExists = false;
        return Promise.resolve(
          availability({ ...persistedRecord, has_persisted_record: false }).state,
        );
      }),
    });
    const store = createRenoDxStore({ api });
    await store.load('renodx-game');

    component = mount(RenoDxCardTestHost, {
      target,
      props: {
        gameId: 'renodx-game',
        store,
        onOpenRenoDxSettings: vi.fn(),
      },
    });
    flushSync();

    expect(target.textContent).toContain(
      'A previous RenoDX installation is inactive. Remove it before installing again.',
    );
    expect(target.textContent).not.toContain('Install from file');
    expect(
      [...target.querySelectorAll<HTMLButtonElement>('button')].some(
        (button) => button.textContent.trim() === 'Install',
      ),
    ).toBe(false);

    const uninstall = [...target.querySelectorAll<HTMLButtonElement>('button')].find(
      (button) => button.textContent.trim() === 'Remove RenoDX',
    );
    expect(uninstall).toBeDefined();
    uninstall?.click();
    flushSync();

    const confirm = [...document.querySelectorAll<HTMLButtonElement>('button')].find(
      (button) => button.textContent.trim() === 'Remove',
    );
    expect(confirm).toBeDefined();
    confirm?.click();

    await vi.waitFor(() => {
      expect(api.uninstall).toHaveBeenCalledWith('renodx-game');
      expect(store.hasPersistedRecord).toBe(false);
    });
  });

  it('disables inactive-owner removal while the game card is busy', async () => {
    const store = createRenoDxStore({
      api: fakeApi({
        getAvailability: vi.fn(() =>
          Promise.resolve(availability({ ...NOT_INSTALLED_SAFE, has_persisted_record: true })),
        ),
      }),
    });
    await store.load('renodx-game');

    component = mount(RenoDxCardTestHost, {
      target,
      props: {
        gameId: 'renodx-game',
        busy: true,
        store,
        onOpenRenoDxSettings: vi.fn(),
      },
    });
    flushSync();

    const uninstall = [...target.querySelectorAll<HTMLButtonElement>('button')].find(
      (button) => button.textContent.trim() === 'Remove RenoDX',
    );
    expect(uninstall?.disabled).toBe(true);
  });

  it('keeps the regular install view when there is no persisted RenoDX owner', async () => {
    const store = createRenoDxStore({
      api: fakeApi({ getAvailability: vi.fn(() => Promise.resolve(NOT_INSTALLED_SAFE)) }),
    });
    await store.load('renodx-game');

    component = mount(RenoDxCardTestHost, {
      target,
      props: {
        gameId: 'renodx-game',
        store,
        onOpenRenoDxSettings: vi.fn(),
      },
    });
    flushSync();

    expect(store.hasPersistedRecord).toBe(false);
    expect(target.textContent).not.toContain('previous RenoDX installation is inactive');
    expect(
      [...target.querySelectorAll<HTMLButtonElement>('button')].some(
        (button) => button.textContent.trim() === 'Install',
      ),
    ).toBe(true);
  });

  it('displays clean profile badge and unverified confidence for a generic profile', async () => {
    const store = createRenoDxStore({
      api: fakeApi({
        getAvailability: vi.fn(() =>
          Promise.resolve(
            availability({
              state: { status: 'not_installed' },
              outcome: {
                kind: 'installable',
                confidence: 'untested',
                generic_profile: {
                  engine: 'unreal',
                  profile_id: 'ue_extended',
                  message: {
                    id: 'renodx.generic.ue_extended',
                    fallback_text: 'Uses the shared Unreal Engine Extended profile.',
                  },
                },
                profile_id: 'ue_extended',
                host_kind: 'proxy',
                guidance: [],
                launch: null,
              },
              manual_install: null,
            }),
          ),
        ),
      }),
    });
    await store.load('renodx-game');

    component = mount(RenoDxCardTestHost, {
      target,
      props: {
        gameId: 'renodx-game',
        store,
        onOpenRenoDxSettings: vi.fn(),
      },
    });
    flushSync();

    expect(target.textContent).toContain('Compatibility');
    expect(target.textContent).toContain('Unverified');
    expect(target.textContent).toContain('Unreal Engine Extended');
  });

  it('falls back to engine name when generic profile_id is unmapped or unknown', async () => {
    const store = createRenoDxStore({
      api: fakeApi({
        getAvailability: vi.fn(() =>
          Promise.resolve(
            availability({
              state: { status: 'not_installed' },
              outcome: {
                kind: 'installable',
                confidence: 'untested',
                generic_profile: {
                  engine: 'unreal',
                  profile_id: 'ue_future_unknown_v9',
                  message: {
                    id: 'renodx.generic.custom',
                    fallback_text: 'Uses a custom engine profile.',
                  },
                },
                profile_id: 'ue_future_unknown_v9',
                host_kind: 'proxy',
                guidance: [],
                launch: null,
              },
              manual_install: null,
            }),
          ),
        ),
      }),
    });
    await store.load('renodx-game');

    component = mount(RenoDxCardTestHost, {
      target,
      props: {
        gameId: 'renodx-game',
        store,
        onOpenRenoDxSettings: vi.fn(),
      },
    });
    flushSync();

    expect(target.textContent).toContain('Compatibility');
    expect(target.textContent).toContain('Unverified');
    expect(target.textContent).toContain('Unreal Engine');
    expect(target.textContent).not.toContain('ue_future_unknown_v9');
  });

  it('renders disabled install button and attribution when blocked by another addon', async () => {
    const store = createRenoDxStore({
      api: fakeApi({
        getAvailability: vi.fn(() =>
          Promise.resolve(
            availability({
              state: { status: 'not_installed' },
              outcome: {
                kind: 'blocked_by_other_addon',
                other_kind: 'luma',
                unmanaged: false,
              },
              manual_install: null,
            }),
          ),
        ),
      }),
    });
    await store.load('renodx-game');

    component = mount(RenoDxCardTestHost, {
      target,
      props: {
        gameId: 'renodx-game',
        store,
        onOpenRenoDxSettings: vi.fn(),
      },
    });
    flushSync();

    expect(target.textContent).toContain(
      'Luma is installed for this game — uninstall it before installing RenoDX.',
    );
    expect(target.textContent).toContain('RenoDX by clshortfuse.');
    const installButton = [...target.querySelectorAll<HTMLButtonElement>('button')].find(
      (button) => button.textContent.trim() === 'Install',
    );
    expect(installButton).toBeDefined();
    expect(installButton?.disabled).toBe(true);
    expect(installButton?.querySelector('svg')).not.toBeNull();
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
      dlss_fix_evidence_present: false,
      addon_tracked: true,
    },
    freshness: 'current',
    addonDated: null,
    installedAt: null,
    lastCheckedAt: null,
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
    dlssFix: { kind: 'hidden' },
    retryDlssFixRecovery: vi.fn(() => Promise.resolve('success')),
    vulkanUpdateDiagnostics: [],
  } as unknown as RenoDxStore;
}

function noRowRecoveryStore(
  retryDlssFixRecovery: RenoDxStore['retryDlssFixRecovery'],
): RenoDxStore {
  return {
    ...loadErrorStore(vi.fn(() => Promise.resolve())),
    loaded: true,
    loadError: null,
    state: { status: 'not_installed' },
    dlssFix: { kind: 'recovery_pending' },
    retryDlssFixRecovery,
  } as unknown as RenoDxStore;
}
