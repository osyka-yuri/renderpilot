import { describe, expect, it, vi } from 'vitest';

vi.mock('@shared/notifications', () => ({
  publishErrorNotification: vi.fn(),
}));

import { createRenoDxStore } from './create-renodx-store.svelte';
import type { RenoDxUpdateReport } from './types';
import { fakeApi, INSTALLED, INSTALLED_WITH_DLSS_FIX } from './renodx-store-test-fixtures';

describe('createRenoDxStore', () => {
  it('installDlssFix() resolves false and leaves dlssFixInstalled untouched when the backend fails', async () => {
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
      dlssFixAvailability: vi.fn(() => Promise.resolve(true)),
      installDlssFix: vi.fn(() => Promise.reject(new Error('boom'))),
    });
    const store = createRenoDxStore({ api });
    await store.load('steam:1091500');
    expect(store.dlssFixAvailable).toBe(true);

    const ok = await store.installDlssFix('steam:1091500');

    expect(ok).toBe('failed');
    expect(store.busy).toBe(false);
    expect(store.dlssFixInstalled).toBe(false);
  });

  it('uninstallDlssFix() resolves false and leaves dlssFixInstalled untouched when the backend fails', async () => {
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
      uninstallDlssFix: vi.fn(() => Promise.reject(new Error('boom'))),
    });
    const store = createRenoDxStore({ api });
    await store.load('steam:1091500');
    expect(store.dlssFixInstalled).toBe(true);

    const ok = await store.uninstallDlssFix('steam:1091500');

    expect(ok).toBe('failed');
    expect(store.busy).toBe(false);
    expect(store.dlssFixInstalled).toBe(true);
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
      dlssFixAvailability: vi.fn(() => Promise.resolve(true)),
    });
    const store = createRenoDxStore({ api });

    await store.load('steam:1091500');

    expect(store.isInstalled).toBe(true);
    expect(store.dlssFixInstalled).toBe(false);
    expect(store.dlssFixAvailable).toBe(true);
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
      dlssFixAvailability: vi.fn(() => Promise.resolve(true)),
      installDlssFix: vi.fn(() => Promise.resolve(INSTALLED_WITH_DLSS_FIX)),
    });
    const store = createRenoDxStore({ api });

    await store.load('steam:1091500');
    expect(store.dlssFixAvailable).toBe(true);

    const ok = await store.installDlssFix('steam:1091500');

    expect(ok).toBe('ok');
    expect(api.installDlssFix).toHaveBeenCalledWith('steam:1091500');
    // After install, the backend reports a DlssFix tracked source, so the state
    // carries `dlss_fix_installed` and the companion reads as installed; it is no
    // longer "available" to install (the stale flag must not linger).
    expect(store.dlssFixInstalled).toBe(true);
    expect(store.dlssFixAvailable).toBe(false);
  });
});
