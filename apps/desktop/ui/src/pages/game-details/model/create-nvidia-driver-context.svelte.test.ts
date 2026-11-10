import { beforeEach, describe, expect, it, vi } from 'vitest';

import { setLanguageMode } from '@shared/i18n';
import type * as NvidiaSettingsApi from '@features/nvapi-settings';
import {
  publishPresentedErrorNotification,
  publishWarningNotification,
} from '@shared/notifications';
import { reportClientError } from '@shared/errors';
import {
  listNvapiSettingStates,
  setNvapiSettingValue,
  type SettingStateResponse,
} from '@features/nvapi-settings';

import { createNvidiaDriverContext } from './create-nvidia-driver-context.svelte';

vi.mock('@features/nvapi-settings', async (importOriginal) => {
  const actual = await importOriginal<typeof NvidiaSettingsApi>();
  return {
    ...actual,
    listNvapiSettingStates: vi.fn(),
    revertNvapiSetting: vi.fn(),
    setNvapiSettingValue: vi.fn(),
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

const GAME = 'manual:test-game';
const OTHER_GAME = 'manual:other-game';
const KEY = 'dlss_sr_render_preset';

function setting(path: string, wire = 'default'): SettingStateResponse {
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
    effective_exe: path,
    effective_exe_source: 'auto',
    has_profile_for_exe: true,
    nvapi_available: true,
    catalog_readiness: 'ready',
    available_values: [],
    dll_info: null,
    warnings: [],
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

describe('createNvidiaDriverContext live state', () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    await setLanguageMode('en');
  });

  it('refreshes the external value after a failed write and reports the failure once', async () => {
    vi.mocked(listNvapiSettingStates)
      .mockResolvedValueOnce([setting('C:/Games/Test/game.exe')])
      .mockResolvedValueOnce([setting('C:/Games/Test/game.exe', 'external-reset')]);
    vi.mocked(setNvapiSettingValue).mockRejectedValueOnce(new Error('stored setting changed'));

    const onChanged = vi.fn();
    const context = createNvidiaDriverContext(onChanged);
    await context.reload(GAME);

    await expect(context.setValue(GAME, KEY, 'quality')).resolves.toBeUndefined();

    expect(context.settingsForFamily('sr')[0].current.wire).toBe('external-reset');
    expect(publishPresentedErrorNotification).toHaveBeenCalledOnce();
    expect(publishPresentedErrorNotification).toHaveBeenCalledWith(
      'Failed to apply settings',
      expect.any(Error),
    );
    expect(publishWarningNotification).not.toHaveBeenCalled();
    expect(reportClientError).toHaveBeenCalledWith('nvapi_settings_action', expect.any(Error));
    expect(onChanged).toHaveBeenCalledOnce();
    expect(onChanged).toHaveBeenCalledWith(GAME);
  });

  it('ignores an old executable write after the same game is reloaded', async () => {
    const write = deferred<SettingStateResponse>();
    vi.mocked(listNvapiSettingStates)
      .mockResolvedValueOnce([setting('C:/Games/Test/old.exe')])
      .mockResolvedValueOnce([setting('C:/Games/Test/new.exe')])
      .mockResolvedValueOnce([setting('C:/Games/Test/new.exe')]);
    vi.mocked(setNvapiSettingValue).mockReturnValueOnce(write.promise);

    const onChanged = vi.fn();
    const context = createNvidiaDriverContext(onChanged);
    await context.reload(GAME);

    const oldAction = context.setValue(GAME, KEY, 'quality');
    expect(context.isPending(KEY)).toBe(true);

    await context.reload(GAME);
    expect(context.effectiveExe).toBe('C:/Games/Test/new.exe');
    expect(context.isPending(KEY)).toBe(false);

    write.resolve(setting('C:/Games/Test/old.exe', 'quality'));
    await oldAction;

    expect(context.settingsForFamily('sr')[0].effective_exe).toBe('C:/Games/Test/new.exe');
    expect(context.settingsForFamily('sr')[0].current.wire).toBe('default');
    expect(context.isPending(KEY)).toBe(false);
    expect(onChanged).toHaveBeenCalledOnce();
    expect(onChanged).toHaveBeenCalledWith(GAME);
    expect(publishPresentedErrorNotification).not.toHaveBeenCalled();
  });

  it('keeps the newest reply when overlapping reads finish out of order', async () => {
    const olderRead = deferred<SettingStateResponse[]>();
    vi.mocked(listNvapiSettingStates)
      .mockReturnValueOnce(olderRead.promise)
      .mockResolvedValueOnce([setting('C:/Games/Test/current.exe', 'latest')]);

    const context = createNvidiaDriverContext();
    const oldReload = context.reload(GAME);
    await context.reload(GAME);

    olderRead.resolve([setting('C:/Games/Test/stale.exe', 'stale')]);
    await oldReload;

    expect(context.settingsForFamily('sr')[0].effective_exe).toBe('C:/Games/Test/current.exe');
    expect(context.settingsForFamily('sr')[0].current.wire).toBe('latest');
  });

  it('does not let a previous game write patch or report into the next game', async () => {
    const write = deferred<SettingStateResponse>();
    vi.mocked(listNvapiSettingStates)
      .mockResolvedValueOnce([setting('C:/Games/One/game.exe')])
      .mockResolvedValueOnce([setting('C:/Games/Two/game.exe')]);
    vi.mocked(setNvapiSettingValue).mockReturnValueOnce(write.promise);

    const onChanged = vi.fn();
    const context = createNvidiaDriverContext(onChanged);
    await context.reload(GAME);
    const oldAction = context.setValue(GAME, KEY, 'quality');

    await context.reload(OTHER_GAME);
    write.reject(new Error('old game write failed'));
    await oldAction;

    expect(context.settingsForFamily('sr')[0].effective_exe).toBe('C:/Games/Two/game.exe');
    expect(context.isPending(KEY)).toBe(false);
    expect(onChanged).not.toHaveBeenCalled();
    expect(publishPresentedErrorNotification).not.toHaveBeenCalled();
    expect(reportClientError).not.toHaveBeenCalled();
  });

  it('keeps a committed setting success if its profile refresh callback fails', async () => {
    vi.mocked(listNvapiSettingStates).mockResolvedValueOnce([setting('C:/Games/Test/game.exe')]);
    vi.mocked(setNvapiSettingValue).mockResolvedValueOnce(
      setting('C:/Games/Test/game.exe', 'quality'),
    );
    const onChanged = vi.fn().mockRejectedValueOnce(new Error('profile status unavailable'));
    const context = createNvidiaDriverContext(onChanged);
    await context.reload(GAME);

    await expect(context.setValue(GAME, KEY, 'quality')).resolves.toBeUndefined();

    expect(context.settingsForFamily('sr')[0].current.wire).toBe('quality');
    expect(reportClientError).toHaveBeenCalledWith('nvapi_profile_refresh', expect.any(Error));
    expect(publishWarningNotification).toHaveBeenCalledOnce();
    expect(publishPresentedErrorNotification).not.toHaveBeenCalled();
  });

  it('ignores an old profile refresh callback failure after switching games', async () => {
    vi.mocked(listNvapiSettingStates)
      .mockResolvedValueOnce([setting('C:/Games/One/game.exe')])
      .mockResolvedValueOnce([setting('C:/Games/Two/game.exe')]);
    vi.mocked(setNvapiSettingValue).mockResolvedValueOnce(
      setting('C:/Games/One/game.exe', 'quality'),
    );

    let rejectRefresh!: (error: unknown) => void;
    const onChanged = vi.fn(() => {
      return new Promise<void>((_resolve, reject) => {
        rejectRefresh = reject;
      });
    });
    const context = createNvidiaDriverContext(onChanged);
    await context.reload(GAME);

    const action = context.setValue(GAME, KEY, 'quality');
    await vi.waitFor(() => {
      expect(onChanged).toHaveBeenCalledOnce();
    });
    await context.reload(OTHER_GAME);
    rejectRefresh(new Error('old profile refresh failed'));
    await action;

    expect(context.settingsForFamily('sr')[0].effective_exe).toBe('C:/Games/Two/game.exe');
    expect(reportClientError).not.toHaveBeenCalledWith('nvapi_profile_refresh', expect.any(Error));
    expect(publishWarningNotification).not.toHaveBeenCalled();
  });
});
