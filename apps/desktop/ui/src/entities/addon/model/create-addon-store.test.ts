import { describe, expect, it, vi } from 'vitest';
import type { MessageKey } from '@shared/i18n';

vi.mock('@shared/notifications', () => ({
  publishErrorNotification: vi.fn(),
}));

vi.mock('@shared/lib', async (importOriginal) => {
  const actual = await importOriginal<unknown>();
  return { ...(actual as Record<string, unknown>), clearDownloadProgress: vi.fn() };
});

import { clearDownloadProgress } from '@shared/lib';
import { publishErrorNotification } from '@shared/notifications';

import { createAddonStore } from './create-addon-store.svelte';
import type { AddonInstallStateBase, FreshnessSource } from './store-helpers';

type TestState = AddonInstallStateBase;
type TestUpdateReport = FreshnessSource;
type TestAvailabilityReport = {
  state: TestState;
  label: string;
};

const NOT_INSTALLED: TestState = { status: 'not_installed' };
const INSTALLED: TestState = {
  status: 'installed',
  addon_dated: null,
  installed_at: 1_700_000_000_000,
  updated_at: 1_700_000_000_000,
  addon_tracked: true,
};

const CURRENT_REPORT: TestUpdateReport = {
  addon: 'current',
  host: 'current',
  overall: 'current',
};

const INSTALLED_AVAILABILITY: TestAvailabilityReport = {
  state: INSTALLED,
  label: 'installed',
};

const NOT_INSTALLED_AVAILABILITY: TestAvailabilityReport = {
  state: NOT_INSTALLED,
  label: 'safe',
};

function fakeApi(overrides: Partial<ReturnType<typeof baseApi>> = {}) {
  return { ...baseApi(), ...overrides };
}

function baseApi() {
  return {
    getAvailability: vi.fn((_gameId: string) => Promise.resolve(NOT_INSTALLED_AVAILABILITY)),
    checkUpdate: vi.fn((_gameId: string) => Promise.resolve(CURRENT_REPORT)),
    install: vi.fn((_gameId: string) => Promise.resolve(INSTALLED)),
    update: vi.fn((_gameId: string) => Promise.resolve(INSTALLED)),
    uninstall: vi.fn((_gameId: string) => Promise.resolve(NOT_INSTALLED)),
  };
}

function createTestStore(api = fakeApi()) {
  let label = 'initial';
  const store = createAddonStore<TestState, TestUpdateReport, TestAvailabilityReport>({
    api,
    messages: { loadFailed: 'gameDetails.renodx.loadFailed' satisfies MessageKey },
    applyLoadReport: (report) => {
      label = report.label;
    },
    applyHostRefresh: (report) => {
      label = `${report.label}-refreshed`;
    },
    buildUpdateReportForInstall: (nextState) =>
      nextState.status === 'installed'
        ? { addon: 'current', host: 'current', overall: 'current' }
        : null,
    buildProbeFailureReport: () => ({ addon: null, host: null, overall: 'unknown' }),
  });
  return { store, getLabel: () => label };
}

describe('createAddonStore', () => {
  it('starts empty before loading', () => {
    const { store } = createTestStore();
    expect(store.loaded).toBe(false);
    expect(store.isInstalled).toBe(false);
  });

  it('load() applies the availability report and probes updates when installed', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED_AVAILABILITY)),
    });
    const { store, getLabel } = createTestStore(api);

    await store.load('game1');

    expect(store.loaded).toBe(true);
    expect(store.isInstalled).toBe(true);
    expect(getLabel()).toBe('installed');
    expect(api.checkUpdate).toHaveBeenCalledWith('game1');
    expect(store.freshness).toBe('current');
  });

  it('discards a stale load when a newer load starts', async () => {
    let releaseSlow: (report: TestAvailabilityReport) => void = () => undefined;
    const slow = new Promise<TestAvailabilityReport>((resolve) => {
      releaseSlow = resolve;
    });
    const api = fakeApi({
      getAvailability: vi.fn((gameId: string) =>
        gameId === 'slow' ? slow : Promise.resolve({ state: INSTALLED, label: 'fast' }),
      ),
    });
    const { store } = createTestStore(api);

    const slowLoad = store.load('slow');
    await store.load('fast');
    expect(store.isInstalled).toBe(true);

    releaseSlow({ state: NOT_INSTALLED, label: 'stale' });
    await slowLoad;

    expect(store.isInstalled).toBe(true);
  });

  it('records a load error without marking loaded', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.reject(new Error('boom'))),
    });
    const { store } = createTestStore(api);

    await store.load('game1');

    expect(store.loadError).not.toBeNull();
    expect(store.loaded).toBe(false);
    expect(publishErrorNotification).toHaveBeenCalled();
  });

  it('runBusyMutation commits install state and notifies exclusivity', async () => {
    const onExclusivityChange = vi.fn();
    const api = fakeApi();
    const store = createAddonStore<TestState, TestUpdateReport, TestAvailabilityReport>({
      api,
      messages: { loadFailed: 'gameDetails.renodx.loadFailed' satisfies MessageKey },
      onExclusivityChange,
      applyLoadReport: () => undefined,
      applyHostRefresh: () => undefined,
      buildUpdateReportForInstall: () => ({
        addon: 'current',
        host: 'current',
        overall: 'current',
      }),
      buildProbeFailureReport: () => ({ addon: null, host: null, overall: 'unknown' }),
    });

    const ok = await store.runBusyMutation('g1', () => api.install('g1'), {
      errorKey: 'gameDetails.renodx.installError' satisfies MessageKey,
      notifyExclusivity: true,
    });

    expect(ok).toBe(true);
    expect(store.isInstalled).toBe(true);
    expect(store.installedAt).toBe(INSTALLED.installed_at);
    expect(store.updatedAt).toBe(INSTALLED.updated_at);
    expect(store.updateAvailable).toBe(false);
    expect(store.freshness).toBe('current');
    expect(onExclusivityChange).toHaveBeenCalledWith('g1');
    expect(clearDownloadProgress).toHaveBeenCalledWith(['g1']);
  });

  it('runBusyMutation no-ops when requireUpdateAvailable and no update pending', async () => {
    const api = fakeApi();
    const { store } = createTestStore(api);

    const ok = await store.runBusyMutation('g1', () => api.update('g1'), {
      errorKey: 'gameDetails.renodx.updateError' satisfies MessageKey,
      requireUpdateAvailable: true,
    });

    expect(ok).toBe(false);
    expect(api.update).not.toHaveBeenCalled();
  });

  it('checkForUpdates records a failed probe as unknown freshness', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED_AVAILABILITY)),
      checkUpdate: vi.fn(() => Promise.reject(new Error('network'))),
    });
    const { store } = createTestStore(api);
    await store.load('g1');

    await store.checkForUpdates('g1');

    expect(store.freshness).toBe('unknown');
    expect(store.updateProbing).toBe(false);
  });

  it('uninstall clears timestamps and drops update report', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED_AVAILABILITY)),
      uninstall: vi.fn(() => Promise.resolve(NOT_INSTALLED)),
    });
    const { store } = createTestStore(api);
    await store.load('g1');
    expect(store.isInstalled).toBe(true);
    expect(store.installedAt).not.toBeNull();

    const ok = await store.runBusyMutation('g1', () => api.uninstall('g1'), {
      errorKey: 'gameDetails.renodx.uninstallError' satisfies MessageKey,
      notifyExclusivity: true,
    });

    expect(ok).toBe(true);
    expect(store.isInstalled).toBe(false);
    expect(store.installedAt).toBeNull();
    expect(store.updatedAt).toBeNull();
    expect(JSON.stringify(store.state)).not.toContain('installed_at');
  });

  it('update preserves backend installed_at and updated_at as returned', async () => {
    const updatedState: TestState = {
      ...INSTALLED,
      installed_at: 1_700_000_000_000,
      updated_at: 1_700_000_999_999,
    };
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED_AVAILABILITY)),
      update: vi.fn(() => Promise.resolve(updatedState)),
    });
    const { store } = createTestStore(api);
    await store.load('g1');

    const ok = await store.runBusyMutation('g1', () => api.update('g1'), {
      errorKey: 'gameDetails.renodx.updateError' satisfies MessageKey,
    });

    expect(ok).toBe(true);
    expect(store.isInstalled).toBe(true);
    expect(store.installedAt).toBe(1_700_000_000_000);
    expect(store.updatedAt).toBe(1_700_000_999_999);
  });
});
