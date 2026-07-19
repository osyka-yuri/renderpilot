import { describe, expect, it, vi } from 'vitest';

vi.mock('@shared/notifications', () => ({
  publishErrorNotification: vi.fn(),
}));

import { createRenoDxStore } from './create-renodx-store.svelte';
import { fakeApi, INSTALLED } from './renodx-store-test-fixtures';

describe('createRenoDxStore', () => {
  it('notifies when install, file install, or uninstall changes the peer exclusivity block', async () => {
    const onExclusivityChange = vi.fn();
    const api = fakeApi({ getAvailability: vi.fn(() => Promise.resolve(INSTALLED)) });
    const store = createRenoDxStore({ api, onExclusivityChange });

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
  });
});
