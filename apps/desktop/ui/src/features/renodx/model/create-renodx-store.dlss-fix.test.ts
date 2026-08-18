import { describe, expect, it, vi } from 'vitest';

vi.mock('@shared/notifications', () => ({
  publishPresentedErrorNotification: vi.fn(),
}));

import { createRenoDxStore } from './create-renodx-store.svelte';
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
    // carries DLSS-Fix evidence and the refreshed action projection exposes a
    // single update action plus independent removal capability.
    expect(store.dlssFix).toMatchObject({
      kind: 'component',
      primaryAction: { kind: 'update' },
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
