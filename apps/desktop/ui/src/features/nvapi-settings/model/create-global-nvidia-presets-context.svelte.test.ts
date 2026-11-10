import { beforeEach, describe, expect, it, vi } from 'vitest';

import {
  publishPresentedErrorNotification,
  publishWarningNotification,
} from '@shared/notifications';
import { reportClientError } from '@shared/errors';
import { setLanguageMode } from '@shared/i18n';
import type * as DesktopApi from '../api/desktop';
import { listGlobalNvapiSettingStates, setGlobalNvapiSettingValue } from '../api/desktop';
import type { SettingStateResponse } from './types';
import { createGlobalNvidiaPresetsContext } from './create-global-nvidia-presets-context.svelte';

vi.mock('../api/desktop', async (importOriginal) => {
  const actual = await importOriginal<typeof DesktopApi>();
  return {
    ...actual,
    listGlobalNvapiSettingStates: vi.fn(),
    setGlobalNvapiSettingValue: vi.fn(),
    revertGlobalNvapiSetting: vi.fn(),
  };
});
vi.mock('@shared/errors', () => ({ reportClientError: vi.fn() }));
vi.mock('@shared/notifications', () => ({
  publishPresentedErrorNotification: vi.fn(),
  publishWarningNotification: vi.fn(),
}));
vi.mock('@shared/error-presentation', () => ({
  formatPresentedError: (error: unknown) =>
    error instanceof Error ? error.message : String(error),
}));

const KEY = 'dlss_sr_render_preset';

function setting(wire: string): SettingStateResponse {
  return {
    setting_key: KEY,
    setting_label: 'DLSS preset',
    value_type: 'dword',
    dll_kind: 'sr',
    family: 'sr',
    category: 'DLSS Super Resolution',
    description: null,
    min_driver: null,
    current: { wire, label: wire, dword: wire === 'default' ? 0 : 1 },
    current_is_explicit: wire !== 'default',
    predefined: null,
    original: null,
    is_current_predefined: wire === 'default',
    effective_exe: null,
    effective_exe_source: null,
    has_profile_for_exe: true,
    nvapi_available: true,
    catalog_readiness: 'ready',
    available_values: [],
    dll_info: null,
    warnings: [],
  };
}

describe('createGlobalNvidiaPresetsContext live state', () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    await setLanguageMode('en');
  });

  it('re-reads external changes when explicitly refreshed', async () => {
    vi.mocked(listGlobalNvapiSettingStates)
      .mockResolvedValueOnce([setting('default')])
      .mockResolvedValueOnce([setting('external-reset')]);
    const context = createGlobalNvidiaPresetsContext();

    await context.load();
    await context.refresh();

    expect(context.settingsForFamily('sr')[0].current.wire).toBe('external-reset');
    expect(listGlobalNvapiSettingStates).toHaveBeenCalledTimes(2);
  });

  it('shows the actual state after a failed write without repeating the action error', async () => {
    vi.mocked(listGlobalNvapiSettingStates)
      .mockResolvedValueOnce([setting('default')])
      .mockResolvedValueOnce([setting('reset-by-driver')]);
    vi.mocked(setGlobalNvapiSettingValue).mockRejectedValueOnce(
      new Error('global setting changed'),
    );
    const context = createGlobalNvidiaPresetsContext();
    await context.load();

    await expect(context.setValue(KEY, 'quality')).resolves.toBeUndefined();

    expect(context.settingsForFamily('sr')[0].current.wire).toBe('reset-by-driver');
    expect(publishPresentedErrorNotification).toHaveBeenCalledOnce();
    expect(publishWarningNotification).not.toHaveBeenCalled();
    expect(reportClientError).toHaveBeenCalledWith('nvapi_settings_action', expect.any(Error));
  });

  it('reports a refresh failure separately from the primary write error', async () => {
    vi.mocked(listGlobalNvapiSettingStates)
      .mockResolvedValueOnce([setting('default')])
      .mockRejectedValueOnce(new Error('NVAPI is currently unavailable'));
    vi.mocked(setGlobalNvapiSettingValue).mockRejectedValueOnce(
      new Error('setting changed before write'),
    );
    const context = createGlobalNvidiaPresetsContext();
    await context.load();

    await context.setValue(KEY, 'quality');

    expect(publishPresentedErrorNotification).toHaveBeenCalledOnce();
    expect(publishWarningNotification).toHaveBeenCalledOnce();
    expect(context.loadError).toBe('NVAPI is currently unavailable');
    expect(reportClientError).toHaveBeenCalledWith('nvapi_settings_action', expect.any(Error));
    expect(reportClientError).toHaveBeenCalledWith('global_nvapi_settings_load', expect.any(Error));
  });

  it('ignores a focus refresh while settings are being written', async () => {
    vi.mocked(listGlobalNvapiSettingStates).mockResolvedValueOnce([setting('default')]);
    const context = createGlobalNvidiaPresetsContext();
    await context.load();

    let finishWrite!: (updated: SettingStateResponse) => void;
    vi.mocked(setGlobalNvapiSettingValue).mockReturnValueOnce(
      new Promise((resolve) => {
        finishWrite = resolve;
      }),
    );

    const write = context.setValue(KEY, 'quality');
    await context.refresh();
    expect(listGlobalNvapiSettingStates).toHaveBeenCalledOnce();

    finishWrite(setting('quality'));
    await write;
    expect(context.settingsForFamily('sr')[0].current.wire).toBe('quality');
  });
});
