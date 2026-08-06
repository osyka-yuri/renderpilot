import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { MessageKey } from '@shared/i18n';

vi.mock('@shared/notifications', () => ({
  publishPresentedErrorNotification: vi.fn(),
}));

vi.mock('@shared/lib', async (importOriginal) => {
  const actual = await importOriginal<unknown>();
  return { ...(actual as Record<string, unknown>), clearDownloadProgress: vi.fn() };
});

import { clearDownloadProgress } from '@shared/lib';
import { publishPresentedErrorNotification } from '@shared/notifications';

import { isMutationSuccess } from './busy-mutation';
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
    checkUpdate: vi.fn((_gameId: string, _kind: 'user' | 'passive') =>
      Promise.resolve(CURRENT_REPORT),
    ),
    install: vi.fn((_gameId: string) => Promise.resolve(INSTALLED)),
    update: vi.fn((_gameId: string) => Promise.resolve(INSTALLED)),
    uninstall: vi.fn((_gameId: string) => Promise.resolve(NOT_INSTALLED)),
  };
}

function createTestStore(
  api = fakeApi(),
  resetToolState: (gameId: string | null) => void = vi.fn(),
) {
  let label = 'initial';
  const store = createAddonStore<TestState, TestUpdateReport, TestAvailabilityReport>({
    api,
    messages: { loadFailed: 'addon.availability.loadFailed' satisfies MessageKey },
    applyLoadReport: (report) => {
      label = report.label;
    },
    applyHostRefresh: (report) => {
      label = `${report.label}-refreshed`;
    },
    resetToolState,
    buildUpdateReportForInstall: (nextState) =>
      nextState.status === 'installed'
        ? { addon: 'current', host: 'current', overall: 'current' }
        : null,
    buildProbeFailureReport: () => ({ addon: null, host: null, overall: 'unknown' }),
  });
  return { store, getLabel: () => label, resetToolState };
}

describe('createAddonStore', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('treats only a committed mutation as confirm-dialog success', () => {
    expect(isMutationSuccess('ok')).toBe(true);
    expect(isMutationSuccess('skipped')).toBe(false);
    expect(isMutationSuccess('failed')).toBe(false);
  });

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
    expect(api.checkUpdate).toHaveBeenCalledWith('game1', 'passive');
    expect(store.freshness).toBe('current');
  });

  it('discards a stale load when a newer load starts', async () => {
    const slow = Promise.withResolvers<TestAvailabilityReport>();
    const api = fakeApi({
      getAvailability: vi.fn((gameId: string) =>
        gameId === 'slow' ? slow.promise : Promise.resolve({ state: INSTALLED, label: 'fast' }),
      ),
    });
    const { store } = createTestStore(api);

    const slowLoad = store.load('slow');
    await store.load('fast');
    expect(store.isInstalled).toBe(true);

    slow.resolve({ state: NOT_INSTALLED, label: 'stale' });
    await slowLoad;

    expect(store.isInstalled).toBe(true);
  });

  it('deactivate() discards an in-flight load without probing or publishing an error', async () => {
    const pending = Promise.withResolvers<TestAvailabilityReport>();
    const api = fakeApi({
      getAvailability: vi.fn(() => pending.promise),
    });
    const { store, resetToolState } = createTestStore(api);

    const load = store.load('game1');
    expect(store.loading).toBe(true);

    store.deactivate();
    expect(store.loading).toBe(false);
    expect(store.loaded).toBe(false);
    expect(store.busy).toBe(false);
    expect(resetToolState).toHaveBeenLastCalledWith(null);

    pending.reject(new Error('late failure'));
    await load;

    expect(store.loadError).toBeNull();
    expect(api.checkUpdate).not.toHaveBeenCalled();
    expect(publishPresentedErrorNotification).not.toHaveBeenCalled();
  });

  it('normalizes navigation game ids before resetting tool state and issuing I/O', async () => {
    const api = fakeApi();
    const { store, resetToolState } = createTestStore(api);

    await store.load('  game1  ');

    expect(resetToolState).toHaveBeenCalledWith('game1');
    expect(api.getAvailability).toHaveBeenCalledWith('game1');
  });

  it('deactivate() prevents a late successful load from restoring stale state', async () => {
    const pending = Promise.withResolvers<TestAvailabilityReport>();
    const api = fakeApi({
      getAvailability: vi.fn(() => pending.promise),
    });
    const { store } = createTestStore(api);

    const load = store.load('game1');
    store.deactivate();
    pending.resolve(INSTALLED_AVAILABILITY);
    await load;

    expect(store.loaded).toBe(false);
    expect(store.isInstalled).toBe(false);
    expect(api.checkUpdate).not.toHaveBeenCalled();
  });

  it('clears install chrome while loading a different game', async () => {
    const g2Load = Promise.withResolvers<TestAvailabilityReport>();
    const api = fakeApi({
      getAvailability: vi.fn((gameId: string) =>
        gameId === 'g2' ? g2Load.promise : Promise.resolve(INSTALLED_AVAILABILITY),
      ),
    });
    const { store } = createTestStore(api);

    await store.load('g1');
    expect(store.isInstalled).toBe(true);
    expect(store.loaded).toBe(true);

    const pending = store.load('g2');
    await vi.waitFor(() => {
      expect(store.loading).toBe(true);
      expect(store.loaded).toBe(false);
      expect(store.isInstalled).toBe(false);
    });

    g2Load.resolve(NOT_INSTALLED_AVAILABILITY);
    await pending;

    expect(store.loading).toBe(false);
    expect(store.loaded).toBe(true);
    expect(store.isInstalled).toBe(false);
  });

  it('records an availability load error without marking loaded and shows a toast', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.reject(new Error('boom'))),
    });
    const { store } = createTestStore(api);

    await store.load('game1');

    expect(store.loadError).not.toBeNull();
    expect(store.loaded).toBe(false);
    expect(publishPresentedErrorNotification).toHaveBeenCalledWith(
      'Could not check',
      expect.any(Error),
    );
  });

  it('does not probe updates after a failed reload of a retained installed state', async () => {
    const api = fakeApi({
      getAvailability: vi
        .fn()
        .mockResolvedValueOnce(INSTALLED_AVAILABILITY)
        .mockRejectedValueOnce(new Error('offline')),
    });
    const { store } = createTestStore(api);
    await store.load('game1');
    api.checkUpdate.mockClear();

    await store.retry('game1');

    expect(store.isInstalled).toBe(true);
    expect(store.loadError).toBe('Something unexpected went wrong. Try the action again.');
    expect(api.checkUpdate).not.toHaveBeenCalled();
  });

  it('keeps a failed availability state visible while an explicit retry is in progress', async () => {
    const retryResponse = Promise.withResolvers<TestAvailabilityReport>();
    const api = fakeApi({
      getAvailability: vi
        .fn()
        .mockRejectedValueOnce(new Error('boom'))
        .mockReturnValueOnce(retryResponse.promise),
    });
    const { store } = createTestStore(api);

    await store.load('game1');
    const retry = store.retry('game1');

    expect(store.loading).toBe(true);
    expect(store.loadError).toBe('Something unexpected went wrong. Try the action again.');

    retryResponse.resolve(NOT_INSTALLED_AVAILABILITY);
    await retry;

    expect(store.loaded).toBe(true);
    expect(store.loadError).toBeNull();
  });

  it('runBusyMutation commits install state and notifies exclusivity', async () => {
    const onExclusivityChange = vi.fn();
    const api = fakeApi();
    const store = createAddonStore<TestState, TestUpdateReport, TestAvailabilityReport>({
      api,
      messages: { loadFailed: 'addon.availability.loadFailed' satisfies MessageKey },
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

    expect(ok).toBe('ok');
    expect(store.isInstalled).toBe(true);
    expect(store.installedAt).toBe(INSTALLED.installed_at);
    expect(store.updatedAt).toBe(INSTALLED.updated_at);
    expect(store.updateAvailable).toBe(false);
    expect(store.freshness).toBe('current');
    expect(onExclusivityChange).toHaveBeenCalledWith('g1');
    expect(clearDownloadProgress).toHaveBeenCalledWith(['g1']);
  });

  it('keeps a committed mutation successful when post-commit work fails', async () => {
    const afterCommit = vi.fn(() => Promise.reject(new Error('refresh failed')));
    const onExclusivityChange = vi.fn(() => {
      throw new Error('peer refresh failed');
    });
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const api = fakeApi();
    const store = createAddonStore<TestState, TestUpdateReport, TestAvailabilityReport>({
      api,
      messages: { loadFailed: 'addon.availability.loadFailed' satisfies MessageKey },
      onExclusivityChange,
      applyLoadReport: () => undefined,
      applyHostRefresh: () => undefined,
      buildUpdateReportForInstall: () => CURRENT_REPORT,
      buildProbeFailureReport: () => ({ addon: null, host: null, overall: 'unknown' }),
    });

    const ok = await store.runBusyMutation('g1', () => api.install('g1'), {
      errorKey: 'gameDetails.renodx.installError' satisfies MessageKey,
      afterCommit,
      notifyExclusivity: true,
    });

    expect(ok).toBe('ok');
    expect(store.isInstalled).toBe(true);
    expect(afterCommit).toHaveBeenCalledTimes(1);
    expect(onExclusivityChange).toHaveBeenCalledWith('g1');
    expect(publishPresentedErrorNotification).not.toHaveBeenCalled();
    expect(warn).toHaveBeenCalledTimes(2);
    warn.mockRestore();
  });

  it('runs refresh, tool hook, and peer notification in deterministic order', async () => {
    const events: string[] = [];
    const api = fakeApi({
      getAvailability: vi.fn(() => {
        events.push('refresh');
        return Promise.resolve(INSTALLED_AVAILABILITY);
      }),
    });
    const store = createAddonStore<TestState, TestUpdateReport, TestAvailabilityReport>({
      api,
      messages: { loadFailed: 'addon.availability.loadFailed' satisfies MessageKey },
      onExclusivityChange: () => events.push('peers'),
      applyLoadReport: () => undefined,
      applyHostRefresh: () => events.push('apply-refresh'),
      buildUpdateReportForInstall: () => CURRENT_REPORT,
      buildProbeFailureReport: () => ({ addon: null, host: null, overall: 'unknown' }),
    });

    const ok = await store.runBusyMutation('g1', () => api.install('g1'), {
      errorKey: 'gameDetails.renodx.installError' satisfies MessageKey,
      afterCommit: () => {
        events.push('tool-hook');
      },
      notifyExclusivity: true,
    });

    expect(ok).toBe('ok');
    expect(events).toEqual(['refresh', 'apply-refresh', 'tool-hook', 'peers']);
  });

  it('keeps busy until the complete post-commit sequence settles', async () => {
    const hook = Promise.withResolvers<undefined>();
    const afterCommit = vi.fn(() => hook.promise);
    const { store } = createTestStore();

    const mutation = store.runBusyMutation('g1', () => Promise.resolve(INSTALLED), {
      errorKey: 'gameDetails.renodx.installError' satisfies MessageKey,
      afterCommit,
    });
    await vi.waitFor(() => {
      expect(afterCommit).toHaveBeenCalledTimes(1);
    });
    expect(store.busy).toBe(true);

    hook.resolve(undefined);
    await mutation;

    expect(store.busy).toBe(false);
  });

  it('invalidates a load that was already in flight when a mutation starts', async () => {
    const staleLoad = Promise.withResolvers<TestAvailabilityReport>();
    const api = fakeApi({
      getAvailability: vi
        .fn()
        .mockReturnValueOnce(staleLoad.promise)
        .mockResolvedValueOnce(INSTALLED_AVAILABILITY),
    });
    const { store, getLabel } = createTestStore(api);

    const load = store.load('g1');
    const ok = await store.runBusyMutation('g1', () => Promise.resolve(INSTALLED), {
      errorKey: 'gameDetails.renodx.installError' satisfies MessageKey,
    });
    staleLoad.resolve({ state: NOT_INSTALLED, label: 'stale' });
    await load;

    expect(ok).toBe('ok');
    expect(store.isInstalled).toBe(true);
    expect(store.loading).toBe(false);
    expect(getLabel()).toBe('installed-refreshed');
  });

  it('discards a mutation that finishes after navigating to another game', async () => {
    const pendingInstall = Promise.withResolvers<TestState>();
    const onExclusivityChange = vi.fn();
    const api = fakeApi({
      getAvailability: vi.fn((gameId: string) =>
        Promise.resolve(
          gameId === 'g2' ? NOT_INSTALLED_AVAILABILITY : { state: NOT_INSTALLED, label: 'g1-load' },
        ),
      ),
    });
    const store = createAddonStore<TestState, TestUpdateReport, TestAvailabilityReport>({
      api,
      messages: { loadFailed: 'addon.availability.loadFailed' satisfies MessageKey },
      onExclusivityChange,
      applyLoadReport: () => undefined,
      applyHostRefresh: () => undefined,
      buildUpdateReportForInstall: () => CURRENT_REPORT,
      buildProbeFailureReport: () => ({ addon: null, host: null, overall: 'unknown' }),
    });

    const mutation = store.runBusyMutation('g1', () => pendingInstall.promise, {
      errorKey: 'gameDetails.renodx.installError' satisfies MessageKey,
      notifyExclusivity: true,
    });
    await vi.waitFor(() => {
      expect(store.busy).toBe(true);
    });

    await store.load('g2');
    expect(store.busy).toBe(false);
    expect(store.isInstalled).toBe(false);

    pendingInstall.resolve(INSTALLED);
    const ok = await mutation;

    // Backend committed; paint discarded for g1 -- still `ok` for bulk callers.
    expect(ok).toBe('ok');
    expect(store.isInstalled).toBe(false);
    expect(store.busy).toBe(false);
    // Backend already committed: exclusivity peers still refresh for g1.
    expect(onExclusivityChange).toHaveBeenCalledWith('g1');
  });

  it('deactivation discards late mutation UI work but preserves the commit signal', async () => {
    const pendingInstall = Promise.withResolvers<TestState>();
    const afterCommit = vi.fn();
    const onMutationSideEffect = vi.fn();
    const onExclusivityChange = vi.fn();
    const api = fakeApi();
    const store = createAddonStore<TestState, TestUpdateReport, TestAvailabilityReport>({
      api,
      messages: { loadFailed: 'addon.availability.loadFailed' satisfies MessageKey },
      onExclusivityChange,
      onMutationSideEffect,
      applyLoadReport: () => undefined,
      applyHostRefresh: () => undefined,
      buildUpdateReportForInstall: () => CURRENT_REPORT,
      buildProbeFailureReport: () => ({ addon: null, host: null, overall: 'unknown' }),
    });

    const mutation = store.runBusyMutation('g1', () => pendingInstall.promise, {
      errorKey: 'gameDetails.renodx.installError' satisfies MessageKey,
      notifyExclusivity: true,
      probeUpdates: true,
      afterCommit,
    });
    store.deactivate();
    pendingInstall.resolve(INSTALLED);

    await expect(mutation).resolves.toBe('ok');
    expect(store.loaded).toBe(false);
    expect(store.isInstalled).toBe(false);
    expect(api.getAvailability).not.toHaveBeenCalled();
    expect(api.checkUpdate).not.toHaveBeenCalled();
    expect(afterCommit).not.toHaveBeenCalled();
    expect(onMutationSideEffect).not.toHaveBeenCalled();
    expect(onExclusivityChange).toHaveBeenCalledWith('g1');
  });

  it('treats a late mutation failure after deactivation as discarded', async () => {
    const pendingInstall = Promise.withResolvers<TestState>();
    const onExclusivityChange = vi.fn();
    const api = fakeApi();
    const store = createAddonStore<TestState, TestUpdateReport, TestAvailabilityReport>({
      api,
      messages: { loadFailed: 'addon.availability.loadFailed' satisfies MessageKey },
      onExclusivityChange,
      applyLoadReport: () => undefined,
      applyHostRefresh: () => undefined,
      buildUpdateReportForInstall: () => CURRENT_REPORT,
      buildProbeFailureReport: () => ({ addon: null, host: null, overall: 'unknown' }),
    });

    const mutation = store.runBusyMutation('g1', () => pendingInstall.promise, {
      errorKey: 'gameDetails.renodx.installError' satisfies MessageKey,
      notifyExclusivity: true,
    });
    store.deactivate();
    pendingInstall.reject(new Error('late failure'));

    await expect(mutation).resolves.toBe('skipped');
    expect(publishPresentedErrorNotification).not.toHaveBeenCalled();
    expect(onExclusivityChange).not.toHaveBeenCalled();
  });

  it('deactivation during post-commit refresh blocks later probes and hooks but preserves the commit signal', async () => {
    const pendingHostRefresh = Promise.withResolvers<TestAvailabilityReport>();
    const afterCommit = vi.fn();
    const onMutationSideEffect = vi.fn();
    const onExclusivityChange = vi.fn();
    const api = fakeApi({
      getAvailability: vi.fn(() => pendingHostRefresh.promise),
    });
    const store = createAddonStore<TestState, TestUpdateReport, TestAvailabilityReport>({
      api,
      messages: { loadFailed: 'addon.availability.loadFailed' satisfies MessageKey },
      onExclusivityChange,
      onMutationSideEffect,
      applyLoadReport: () => undefined,
      applyHostRefresh: () => undefined,
      buildUpdateReportForInstall: () => CURRENT_REPORT,
      buildProbeFailureReport: () => ({ addon: null, host: null, overall: 'unknown' }),
    });

    const mutation = store.runBusyMutation('g1', () => Promise.resolve(INSTALLED), {
      errorKey: 'gameDetails.renodx.installError' satisfies MessageKey,
      notifyExclusivity: true,
      probeUpdates: true,
      afterCommit,
    });
    await vi.waitFor(() => {
      expect(api.getAvailability).toHaveBeenCalledWith('g1');
    });

    store.deactivate();
    pendingHostRefresh.resolve(INSTALLED_AVAILABILITY);
    await expect(mutation).resolves.toBe('ok');

    expect(store.loaded).toBe(false);
    expect(store.isInstalled).toBe(false);
    expect(api.checkUpdate).not.toHaveBeenCalled();
    expect(afterCommit).not.toHaveBeenCalled();
    expect(onMutationSideEffect).not.toHaveBeenCalled();
    expect(onExclusivityChange).toHaveBeenCalledWith('g1');
  });

  it('probeUpdates marks freshness checking before host refresh resolves', async () => {
    const pendingRefresh = Promise.withResolvers<TestAvailabilityReport>();
    let refreshCalls = 0;
    const api = fakeApi({
      getAvailability: vi.fn(() => {
        refreshCalls += 1;
        // First call is post-commit host refresh; second would be unrelated.
        if (refreshCalls === 1) {
          return pendingRefresh.promise;
        }
        return Promise.resolve(INSTALLED_AVAILABILITY);
      }),
      checkUpdate: vi.fn(() => Promise.resolve(CURRENT_REPORT)),
    });
    const store = createAddonStore<TestState, TestUpdateReport, TestAvailabilityReport>({
      api,
      messages: { loadFailed: 'addon.availability.loadFailed' satisfies MessageKey },
      applyLoadReport: () => undefined,
      applyHostRefresh: () => undefined,
      buildUpdateReportForInstall: () => ({
        addon: 'current',
        host: 'current',
        overall: 'current',
      }),
      buildProbeFailureReport: () => ({ addon: null, host: null, overall: 'unknown' }),
    });

    const mutation = store.runBusyMutation('g1', () => Promise.resolve(INSTALLED), {
      errorKey: 'gameDetails.renodx.installError' satisfies MessageKey,
      probeUpdates: true,
    });
    await vi.waitFor(() => {
      expect(store.updateProbing).toBe(true);
      expect(store.freshness).toBe('checking');
    });
    pendingRefresh.resolve(INSTALLED_AVAILABILITY);
    await mutation;

    expect(store.freshness).toBe('current');
    expect(store.updateProbing).toBe(false);
    expect(api.checkUpdate).toHaveBeenCalledWith('g1', 'passive');
  });

  it('postMutationProbe passive probes by default without per-call probeUpdates', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED_AVAILABILITY)),
      checkUpdate: vi.fn(() => Promise.resolve(CURRENT_REPORT)),
    });
    const store = createAddonStore<TestState, TestUpdateReport, TestAvailabilityReport>({
      api,
      messages: { loadFailed: 'addon.availability.loadFailed' satisfies MessageKey },
      applyLoadReport: () => undefined,
      applyHostRefresh: () => undefined,
      postMutationProbe: 'passive',
      buildUpdateReportForInstall: () => ({
        addon: 'current',
        host: 'current',
        overall: 'current',
      }),
      buildProbeFailureReport: () => ({ addon: null, host: null, overall: 'unknown' }),
    });

    await store.runBusyMutation('g1', () => Promise.resolve(INSTALLED), {
      errorKey: 'gameDetails.luma.installError' satisfies MessageKey,
    });

    expect(api.checkUpdate).toHaveBeenCalledWith('g1', 'passive');
  });

  it('postMutationProbe never skips re-probe unless probeUpdates overrides', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED_AVAILABILITY)),
      checkUpdate: vi.fn(() => Promise.resolve(CURRENT_REPORT)),
    });
    const store = createAddonStore<TestState, TestUpdateReport, TestAvailabilityReport>({
      api,
      messages: { loadFailed: 'addon.availability.loadFailed' satisfies MessageKey },
      applyLoadReport: () => undefined,
      applyHostRefresh: () => undefined,
      postMutationProbe: 'never',
      buildUpdateReportForInstall: () => ({
        addon: 'current',
        host: 'current',
        overall: 'current',
      }),
      buildProbeFailureReport: () => ({ addon: null, host: null, overall: 'unknown' }),
    });

    await store.runBusyMutation('g1', () => Promise.resolve(INSTALLED), {
      errorKey: 'gameDetails.renodx.installError' satisfies MessageKey,
    });
    expect(api.checkUpdate).not.toHaveBeenCalled();

    vi.mocked(api.checkUpdate).mockClear();
    await store.runBusyMutation('g1', () => Promise.resolve(INSTALLED), {
      errorKey: 'gameDetails.renodx.installError' satisfies MessageKey,
      probeUpdates: true,
    });
    expect(api.checkUpdate).toHaveBeenCalledWith('g1', 'passive');
  });

  it('onMutationSideEffect runs after afterCommit on successful mutation', async () => {
    const afterCommit = vi.fn();
    const onMutationSideEffect = vi.fn();
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED_AVAILABILITY)),
    });
    const store = createAddonStore<TestState, TestUpdateReport, TestAvailabilityReport>({
      api,
      messages: { loadFailed: 'addon.availability.loadFailed' satisfies MessageKey },
      applyLoadReport: () => undefined,
      applyHostRefresh: () => undefined,
      onMutationSideEffect,
      buildUpdateReportForInstall: () => ({
        addon: 'current',
        host: 'current',
        overall: 'current',
      }),
      buildProbeFailureReport: () => ({ addon: null, host: null, overall: 'unknown' }),
    });

    await store.runBusyMutation('g1', () => Promise.resolve(INSTALLED), {
      errorKey: 'gameDetails.luma.installError' satisfies MessageKey,
      afterCommit,
    });

    expect(afterCommit).toHaveBeenCalledTimes(1);
    expect(onMutationSideEffect).toHaveBeenCalledTimes(1);
    expect(onMutationSideEffect).toHaveBeenCalledWith('g1', expect.any(Number));
    const afterOrders = afterCommit.mock.invocationCallOrder;
    const sideOrders = onMutationSideEffect.mock.invocationCallOrder;
    expect(afterOrders).toHaveLength(1);
    expect(sideOrders).toHaveLength(1);
    expect(sideOrders[0]).toBeGreaterThan(afterOrders[0]);
  });

  it('checkForUpdates identifies explicit user probes', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED_AVAILABILITY)),
      checkUpdate: vi.fn(() => Promise.resolve(CURRENT_REPORT)),
    });
    const store = createAddonStore<TestState, TestUpdateReport, TestAvailabilityReport>({
      api,
      messages: { loadFailed: 'addon.availability.loadFailed' satisfies MessageKey },
      applyLoadReport: () => undefined,
      applyHostRefresh: () => undefined,
      buildUpdateReportForInstall: () => ({
        addon: 'current',
        host: 'current',
        overall: 'current',
      }),
      buildProbeFailureReport: () => ({ addon: null, host: null, overall: 'unknown' }),
    });
    await store.load('g1');
    // load already probed passively
    vi.mocked(api.checkUpdate).mockClear();

    await store.checkForUpdates('g1');

    expect(api.checkUpdate).toHaveBeenCalledWith('g1', 'user');
  });

  it('load identifies passive probes', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED_AVAILABILITY)),
      checkUpdate: vi.fn(() => Promise.resolve(CURRENT_REPORT)),
    });
    const { store } = createTestStore(api);
    await store.load('g1');
    expect(api.checkUpdate).toHaveBeenCalledWith('g1', 'passive');
  });

  it('discards mutation errors after the request was superseded by load', async () => {
    const pendingInstall = Promise.withResolvers<TestState>();
    const { store } = createTestStore();

    const mutation = store.runBusyMutation('g1', () => pendingInstall.promise, {
      errorKey: 'gameDetails.renodx.installError' satisfies MessageKey,
    });
    await store.load('g2');
    pendingInstall.reject(new Error('install failed late'));
    const ok = await mutation;

    expect(ok).toBe('skipped');
    expect(publishPresentedErrorNotification).not.toHaveBeenCalled();
    expect(store.busy).toBe(false);
  });

  it('runBusyMutation no-ops when requireUpdateAvailable and no update pending', async () => {
    const api = fakeApi();
    const { store } = createTestStore(api);

    const ok = await store.runBusyMutation('g1', () => api.update('g1'), {
      errorKey: 'gameDetails.renodx.updateError' satisfies MessageKey,
      requireUpdateAvailable: true,
    });

    expect(ok).toBe('skipped');
    expect(api.update).not.toHaveBeenCalled();
  });

  it('continues to notify when a mutation fails', async () => {
    const api = fakeApi({
      install: vi.fn(() => Promise.reject(new Error('install failed'))),
    });
    const { store } = createTestStore(api);

    const ok = await store.runBusyMutation('g1', () => api.install('g1'), {
      errorKey: 'gameDetails.renodx.installError' satisfies MessageKey,
    });

    expect(ok).toBe('failed');
    expect(publishPresentedErrorNotification).toHaveBeenCalledTimes(1);
    expect(publishPresentedErrorNotification).toHaveBeenCalledWith(
      'RenoDX installation failed',
      expect.any(Error),
    );
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

    expect(ok).toBe('ok');
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

    expect(ok).toBe('ok');
    expect(store.isInstalled).toBe(true);
    expect(store.installedAt).toBe(1_700_000_000_000);
    expect(store.updatedAt).toBe(1_700_000_999_999);
  });
});
