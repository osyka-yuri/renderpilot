import { describe, expect, it, vi } from 'vitest';

vi.mock('@shared/notifications', () => ({
  publishPresentedErrorNotification: vi.fn(),
}));

import { createRenoDxStore } from './create-renodx-store.svelte';
import { publishPresentedErrorNotification } from '@shared/notifications';
import type { DlssFixAvailability, RenoDxUpdateReport } from './types';
import {
  DLSS_FIX_INSTALLABLE,
  DLSS_FIX_MANAGED,
  DLSS_FIX_NEEDS_REPAIR,
  DLSS_FIX_PENDING_RECOVERY,
  DLSS_FIX_UNAVAILABLE,
  fakeApi,
  INSTALLED,
  INSTALLED_WITH_DLSS_FIX,
  NOT_INSTALLED_SAFE,
} from './renodx-store-test-fixtures';

describe('createRenoDxStore', () => {
  it('skips unavailable DLSS-Fix actions before requesting safety tokens', async () => {
    const requireSafetyTokens = vi.fn(() => Promise.resolve({ gameContextToken: 'unused' }));
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      dlssFixAvailability: vi.fn(() => Promise.resolve(DLSS_FIX_UNAVAILABLE)),
    });
    const store = createRenoDxStore({ api, requireSafetyTokens });
    await store.load('steam:1091500');

    await expect(store.installDlssFix('steam:1091500')).resolves.toBe('skipped');
    await expect(store.updateDlssFix('steam:1091500')).resolves.toBe('skipped');

    expect(requireSafetyTokens).not.toHaveBeenCalled();
    expect(api.installDlssFix).not.toHaveBeenCalled();
    expect(api.updateDlssFix).not.toHaveBeenCalled();
  });

  it('does not capture safety tokens for a DLSS-Fix action while another mutation is running', async () => {
    const pendingUpdate = Promise.withResolvers<typeof INSTALLED_WITH_DLSS_FIX>();
    const requireSafetyTokens = vi.fn(() => Promise.resolve({ gameContextToken: 'game-token' }));
    const api = fakeApi({
      getAvailability: vi.fn(() =>
        Promise.resolve({ ...INSTALLED, state: INSTALLED_WITH_DLSS_FIX }),
      ),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'current',
          host: 'current',
          dlssFix: 'unknown_needs_validation',
          overall: 'current',
        } as RenoDxUpdateReport),
      ),
      dlssFixAvailability: vi.fn(() => Promise.resolve(DLSS_FIX_NEEDS_REPAIR)),
      updateDlssFix: vi.fn(() => pendingUpdate.promise),
    });
    const store = createRenoDxStore({ api, requireSafetyTokens });
    await store.load('steam:1091500');

    const firstUpdate = store.updateDlssFix('steam:1091500');
    await vi.waitFor(() => {
      expect(api.updateDlssFix).toHaveBeenCalledOnce();
    });
    await expect(store.updateDlssFix('steam:1091500')).resolves.toBe('skipped');

    expect(requireSafetyTokens).toHaveBeenCalledOnce();

    pendingUpdate.resolve(INSTALLED_WITH_DLSS_FIX);
    await expect(firstUpdate).resolves.toBe('ok');
  });

  it('claims the DLSS-Fix mutation slot synchronously without a safety gate', async () => {
    const pendingUpdate = Promise.withResolvers<typeof INSTALLED_WITH_DLSS_FIX>();
    const api = fakeApi({
      getAvailability: vi.fn(() =>
        Promise.resolve({ ...INSTALLED, state: INSTALLED_WITH_DLSS_FIX }),
      ),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'current',
          host: 'current',
          dlssFix: 'unknown_needs_validation',
          overall: 'current',
        } as RenoDxUpdateReport),
      ),
      dlssFixAvailability: vi.fn(() => Promise.resolve(DLSS_FIX_NEEDS_REPAIR)),
      updateDlssFix: vi.fn(() => pendingUpdate.promise),
    });
    const store = createRenoDxStore({ api });
    await store.load('steam:1091500');

    const update = store.updateDlssFix('steam:1091500');

    expect(store.busy).toBe(true);
    expect(api.updateDlssFix).toHaveBeenCalledOnce();
    await expect(store.uninstall('steam:1091500')).resolves.toBe('skipped');

    pendingUpdate.resolve(INSTALLED_WITH_DLSS_FIX);
    await expect(update).resolves.toBe('ok');
  });

  it('reports safety context failures without starting the DLSS-Fix mutation', async () => {
    vi.mocked(publishPresentedErrorNotification).mockClear();
    const failure = Object.assign(new Error('safety context is stale'), {
      code: 'safety_context_stale',
    });
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      dlssFixAvailability: vi.fn(() => Promise.resolve(DLSS_FIX_INSTALLABLE)),
    });
    const store = createRenoDxStore({
      api,
      requireSafetyTokens: vi.fn(() => Promise.reject(failure)),
    });
    await store.load('steam:1091500');

    await expect(store.installDlssFix('steam:1091500')).resolves.toBe('failed');

    expect(api.installDlssFix).not.toHaveBeenCalled();
    expect(publishPresentedErrorNotification).toHaveBeenCalledOnce();
  });

  it('installDlssFix() preserves the install presentation when the backend fails', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'current',
          host: 'current',
          dlssFix: null,
          overall: 'current',
        } as RenoDxUpdateReport),
      ),
      dlssFixAvailability: vi.fn(() => Promise.resolve(DLSS_FIX_INSTALLABLE)),
      installDlssFix: vi.fn(() => Promise.reject(new Error('boom'))),
    });
    const store = createRenoDxStore({ api });
    await store.load('steam:1091500');
    expect(store.dlssFix).toMatchObject({
      kind: 'component',
      primaryAction: { kind: 'install' },
    });

    const ok = await store.installDlssFix('steam:1091500');

    expect(ok).toBe('failed');
    expect(store.busy).toBe(false);
    expect(store.dlssFix).toMatchObject({
      kind: 'component',
      primaryAction: { kind: 'install' },
    });
  });

  it('uninstallDlssFix() preserves the managed presentation when the backend fails', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() =>
        Promise.resolve({ ...INSTALLED, state: INSTALLED_WITH_DLSS_FIX }),
      ),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'current',
          host: 'current',
          dlssFix: 'current',
          overall: 'current',
        } as RenoDxUpdateReport),
      ),
      dlssFixAvailability: vi.fn(() => Promise.resolve(DLSS_FIX_MANAGED)),
      uninstallDlssFix: vi.fn(() => Promise.reject(new Error('boom'))),
    });
    const store = createRenoDxStore({ api });
    await store.load('steam:1091500');
    expect(store.dlssFix).toMatchObject({ kind: 'component', canRemove: true });

    const ok = await store.uninstallDlssFix('steam:1091500');

    expect(ok).toBe('failed');
    expect(store.busy).toBe(false);
    expect(store.dlssFix).toMatchObject({ kind: 'component', canRemove: true });
  });

  it('reports DLSS-Fix availability for an installed game without one', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'current',
          host: 'current',
          dlssFix: null,
          overall: 'current',
        } as RenoDxUpdateReport),
      ),
      dlssFixAvailability: vi
        .fn()
        .mockResolvedValueOnce(DLSS_FIX_INSTALLABLE)
        .mockResolvedValueOnce(DLSS_FIX_MANAGED),
    });
    const store = createRenoDxStore({ api });

    await store.load('steam:1091500');

    expect(store.isInstalled).toBe(true);
    expect(store.dlssFix).toMatchObject({
      kind: 'component',
      primaryAction: { kind: 'install' },
    });
  });

  it('probes DLSS-Fix availability after successful load, retry, and update checks when RenoDX is not installed', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(NOT_INSTALLED_SAFE)),
      dlssFixAvailability: vi.fn(() => Promise.resolve(DLSS_FIX_UNAVAILABLE)),
    });
    const store = createRenoDxStore({ api });

    await store.load('steam:1091500');
    await store.retry('steam:1091500');
    await store.checkForUpdates('steam:1091500');

    expect(api.dlssFixAvailability).toHaveBeenCalledTimes(3);
    expect(api.dlssFixAvailability).toHaveBeenNthCalledWith(1, 'steam:1091500');
    expect(api.dlssFixAvailability).toHaveBeenNthCalledWith(2, 'steam:1091500');
    expect(api.dlssFixAvailability).toHaveBeenNthCalledWith(3, 'steam:1091500');
    expect(store.state).toEqual({ status: 'not_installed' });
    expect(store.dlssFix).toEqual({ kind: 'hidden' });
  });

  it('preserves DLSS-Fix availability while checking for updates', async () => {
    const checkDeferred = Promise.withResolvers<RenoDxUpdateReport>();
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi
        .fn()
        .mockResolvedValueOnce({
          addon: 'current',
          host: 'current',
          dlssFix: null,
          overall: 'current',
        })
        .mockImplementationOnce(() => checkDeferred.promise),
      dlssFixAvailability: vi
        .fn()
        .mockResolvedValueOnce(DLSS_FIX_INSTALLABLE)
        .mockResolvedValueOnce(DLSS_FIX_MANAGED),
    });
    const store = createRenoDxStore({ api });

    await store.load('steam:1091500');
    expect(store.dlssFix).toMatchObject({
      kind: 'component',
      primaryAction: { kind: 'install' },
    });

    const checkPromise = store.checkForUpdates('steam:1091500');
    // Preserve the last known availability while the refresh is in flight.
    expect(store.dlssFix).toMatchObject({
      kind: 'component',
      primaryAction: { kind: 'install' },
    });

    checkDeferred.resolve({
      addon: 'current',
      host: 'current',
      dlssFix: null,
      overall: 'current',
    });
    await checkPromise;

    expect(api.dlssFixAvailability).toHaveBeenCalledTimes(2);
    expect(store.dlssFix).toMatchObject({
      kind: 'component',
      primaryAction: null,
      canRemove: true,
    });
  });

  it('makes DLSS-Fix available immediately without waiting for background update check', async () => {
    let loadSettled = false;
    const checkDeferred = Promise.withResolvers<RenoDxUpdateReport>();
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() => checkDeferred.promise),
      dlssFixAvailability: vi.fn(() => Promise.resolve(DLSS_FIX_INSTALLABLE)),
    });
    const store = createRenoDxStore({ api });

    const loadPromise = store.load('steam:1091500').then(() => {
      loadSettled = true;
    });

    await vi.waitFor(() => {
      expect(store.dlssFix).toMatchObject({
        kind: 'component',
        primaryAction: { kind: 'install' },
      });
    });

    expect(loadSettled).toBe(false);

    checkDeferred.resolve({
      addon: 'current',
      host: 'current',
      dlssFix: null,
      overall: 'current',
    });
    await loadPromise;
    expect(loadSettled).toBe(true);
  });

  it('does not apply a stale DLSS-Fix probe after a newer core request', async () => {
    const staleProbe = Promise.withResolvers<DlssFixAvailability>();
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(NOT_INSTALLED_SAFE)),
      dlssFixAvailability: vi
        .fn()
        .mockReturnValueOnce(staleProbe.promise)
        .mockResolvedValueOnce(DLSS_FIX_UNAVAILABLE),
    });
    const store = createRenoDxStore({ api });

    const firstLoad = store.load('steam:first');
    await vi.waitFor(() => {
      expect(api.dlssFixAvailability).toHaveBeenCalledWith('steam:first');
    });

    await store.load('steam:second');
    staleProbe.resolve(DLSS_FIX_PENDING_RECOVERY);
    await firstLoad;

    expect(store.state).toEqual({ status: 'not_installed' });
    expect(store.dlssFix).toEqual({ kind: 'hidden' });
  });

  it('installDlssFix clears availability once the companion is tracked', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'current',
          host: 'current',
          dlssFix: null,
          overall: 'current',
        } as RenoDxUpdateReport),
      ),
      dlssFixAvailability: vi
        .fn()
        .mockResolvedValueOnce(DLSS_FIX_INSTALLABLE)
        .mockResolvedValueOnce(DLSS_FIX_MANAGED),
      installDlssFix: vi.fn(() => Promise.resolve(INSTALLED_WITH_DLSS_FIX)),
    });
    const store = createRenoDxStore({ api });

    await store.load('steam:1091500');
    expect(store.dlssFix).toMatchObject({
      kind: 'component',
      primaryAction: { kind: 'install' },
    });

    const ok = await store.installDlssFix('steam:1091500');

    expect(ok).toBe('ok');
    expect(api.installDlssFix).toHaveBeenCalledWith('steam:1091500');
    // After install, the backend reports a DlssFix tracked source, so the state
    // carries DLSS-Fix evidence and the refreshed action projection retains
    // independent removal capability. The update capability is not presented
    // until the dedicated update probe reports an available verdict.
    expect(store.dlssFix).toMatchObject({
      kind: 'component',
      primaryAction: null,
      canRemove: true,
    });
  });

  it('updateDlssFix uses the dedicated route for a repairable partial projection', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() =>
        Promise.resolve({ ...INSTALLED, state: INSTALLED_WITH_DLSS_FIX }),
      ),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'current',
          host: 'current',
          dlssFix: 'unknown_needs_validation',
          overall: 'current',
        } as RenoDxUpdateReport),
      ),
      dlssFixAvailability: vi.fn(() => Promise.resolve(DLSS_FIX_NEEDS_REPAIR)),
      updateDlssFix: vi.fn(() => Promise.resolve(INSTALLED_WITH_DLSS_FIX)),
    });
    const store = createRenoDxStore({ api });

    await store.load('steam:1091500');
    expect(store.dlssFix).toMatchObject({
      kind: 'component',
      primaryAction: { kind: 'repair' },
      canRemove: true,
    });

    const result = await store.updateDlssFix('steam:1091500');

    expect(result).toBe('ok');
    expect(api.updateDlssFix).toHaveBeenCalledWith('steam:1091500');
    expect(store.busy).toBe(false);
  });

  it('retries a no-row DLSS-Fix recovery and refreshes both availability projections', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(NOT_INSTALLED_SAFE)),
      dlssFixAvailability: vi
        .fn()
        .mockResolvedValueOnce(DLSS_FIX_PENDING_RECOVERY)
        .mockResolvedValueOnce(DLSS_FIX_UNAVAILABLE),
      retryDlssFixRecovery: vi.fn(() => Promise.resolve(NOT_INSTALLED_SAFE.state)),
    });
    const store = createRenoDxStore({ api });

    await store.load('steam:1091500');
    expect(store.state).toEqual({ status: 'not_installed' });
    expect(store.dlssFix).toEqual({ kind: 'recovery_pending' });

    const result = await store.retryDlssFixRecovery('steam:1091500');

    expect(result).toBe('ok');
    expect(api.retryDlssFixRecovery).toHaveBeenCalledWith('steam:1091500');
    expect(api.getAvailability).toHaveBeenCalledTimes(2);
    expect(api.dlssFixAvailability).toHaveBeenCalledTimes(2);
    expect(store.state).toEqual({ status: 'not_installed' });
    expect(store.dlssFix).toEqual({ kind: 'hidden' });
    expect(store.busy).toBe(false);
  });
});
