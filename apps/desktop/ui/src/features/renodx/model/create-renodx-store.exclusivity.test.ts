import { describe, expect, it, vi } from 'vitest';

vi.mock('@shared/notifications', () => ({
  publishPresentedErrorNotification: vi.fn(),
}));

import { createRenoDxStore } from './create-renodx-store.svelte';
import { fakeApi, INSTALLED, NOT_INSTALLED_SAFE } from './renodx-store-test-fixtures';

describe('createRenoDxStore', () => {
  it('notifies peers and invalidates details after capability-changing mutations', async () => {
    const onExclusivityChange = vi.fn();
    const onGameDetailsInvalidate = vi.fn(() => Promise.resolve());
    const api = fakeApi({ getAvailability: vi.fn(() => Promise.resolve(INSTALLED)) });
    const store = createRenoDxStore({
      api,
      onExclusivityChange,
      onGameDetailsInvalidate,
    });

    const installOk = await store.install('steam:1091500', 'stable', false);
    expect(installOk).toBe('ok');
    expect(onExclusivityChange).toHaveBeenCalledWith('steam:1091500');

    onExclusivityChange.mockClear();
    const fileInstallOk = await store.installFromFile(
      'steam:1091500',
      'C:\\dl\\renodx-x.addon64',
      'nightly',
      false,
    );
    expect(fileInstallOk).toBe('ok');
    expect(onExclusivityChange).toHaveBeenCalledWith('steam:1091500');

    onExclusivityChange.mockClear();
    const uninstallOk = await store.uninstall('steam:1091500');

    expect(uninstallOk).toBe('ok');
    expect(onExclusivityChange).toHaveBeenCalledWith('steam:1091500');
    expect(onGameDetailsInvalidate).toHaveBeenCalledTimes(3);
    expect(onGameDetailsInvalidate).toHaveBeenNthCalledWith(1, 'steam:1091500');
    expect(onGameDetailsInvalidate).toHaveBeenNthCalledWith(2, 'steam:1091500');
    expect(onGameDetailsInvalidate).toHaveBeenNthCalledWith(3, 'steam:1091500');
  });

  it('does not invalidate details after updates or companion mutations', async () => {
    const onGameDetailsInvalidate = vi.fn();
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'available' as const,
          host: 'current' as const,
          dlssFix: null,
          overall: 'available' as const,
        }),
      ),
    });
    const store = createRenoDxStore({ api, onGameDetailsInvalidate });

    await store.load('steam:1091500');
    expect(await store.update('steam:1091500')).toBe('ok');
    expect(await store.installDlssFix('steam:1091500')).toBe('ok');
    expect(await store.uninstallDlssFix('steam:1091500')).toBe('ok');

    expect(onGameDetailsInvalidate).not.toHaveBeenCalled();
  });

  it('does not invalidate details when a capability-changing mutation fails', async () => {
    const onGameDetailsInvalidate = vi.fn();
    const store = createRenoDxStore({
      api: fakeApi({ install: vi.fn(() => Promise.reject(new Error('install failed'))) }),
      onGameDetailsInvalidate,
    });

    expect(await store.install('steam:1091500', 'stable', false)).toBe('failed');
    expect(onGameDetailsInvalidate).not.toHaveBeenCalled();
  });

  it('does not invalidate details when a capability-changing mutation is skipped', async () => {
    const onGameDetailsInvalidate = vi.fn();
    const store = createRenoDxStore({
      api: fakeApi({
        getAvailability: vi.fn(() =>
          Promise.resolve({ ...NOT_INSTALLED_SAFE, reshade_stable_supported: false }),
        ),
      }),
      onGameDetailsInvalidate,
    });

    await store.load('steam:1091500');
    expect(await store.install('steam:1091500', 'stable', false)).toBe('skipped');
    expect(onGameDetailsInvalidate).not.toHaveBeenCalled();
  });

  it('keeps a successful mutation successful when details invalidation fails', async () => {
    const store = createRenoDxStore({
      api: fakeApi(),
      onGameDetailsInvalidate: () => Promise.reject(new Error('refresh failed')),
    });

    expect(await store.install('steam:1091500', 'stable', false)).toBe('ok');
  });
});
