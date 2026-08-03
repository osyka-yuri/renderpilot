import { describe, expect, it, vi } from 'vitest';

vi.mock('@shared/notifications', () => ({
  publishPresentedErrorNotification: vi.fn(),
}));

import { createLumaStore } from './create-luma-store.svelte';
import { fakeApi, INSTALLED } from './luma-store-test-fixtures';

describe('createLumaStore', () => {
  it('notifies when install or uninstall changes the peer exclusivity block', async () => {
    const onExclusivityChange = vi.fn();
    const api = fakeApi({ getAvailability: vi.fn(() => Promise.resolve(INSTALLED)) });
    const store = createLumaStore({ api, onExclusivityChange });

    const installOk = await store.install('steam:403640', false);
    expect(installOk).toBe('ok');
    expect(onExclusivityChange).toHaveBeenCalledWith('steam:403640');

    onExclusivityChange.mockClear();
    const uninstallOk = await store.uninstall('steam:403640');

    expect(uninstallOk).toBe('ok');
    expect(onExclusivityChange).toHaveBeenCalledWith('steam:403640');
  });

  it('does not notify peer exclusivity when uninstall fails', async () => {
    const onExclusivityChange = vi.fn();
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      uninstall: vi.fn(() => Promise.reject(new Error('boom'))),
    });
    const store = createLumaStore({ api, onExclusivityChange });
    await store.load('steam:403640');

    const ok = await store.uninstall('steam:403640');

    expect(ok).toBe('failed');
    expect(onExclusivityChange).not.toHaveBeenCalled();
  });
});
