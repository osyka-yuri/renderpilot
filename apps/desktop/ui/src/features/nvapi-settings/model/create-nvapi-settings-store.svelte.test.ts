import { beforeEach, describe, expect, it, vi } from 'vitest';

import { setLanguageMode } from '@shared/i18n';

import { createNvapiSettingsStore } from './create-nvapi-settings-store.svelte';
import type { SettingStateResponse } from './types';

vi.mock('@shared/errors', () => ({ reportClientError: vi.fn() }));
vi.mock('@shared/notifications', () => ({
  publishPresentedErrorNotification: vi.fn(),
  publishWarningNotification: vi.fn(),
}));

function state(overrides: Partial<SettingStateResponse> = {}): SettingStateResponse {
  return {
    setting_key: 'dlss_sr_render_preset',
    setting_label: 'DLSS preset',
    value_type: 'dword',
    dll_kind: 'sr',
    family: 'sr',
    category: 'DLSS Super Resolution',
    description: null,
    min_driver: null,
    current: { wire: 'default', label: 'Default', dword: 0 },
    predefined: null,
    current_is_explicit: false,
    original: null,
    is_current_predefined: true,
    effective_exe: 'game.exe',
    effective_exe_source: 'auto',
    has_profile_for_exe: true,
    nvapi_available: true,
    catalog_readiness: 'ready',
    available_values: [],
    dll_info: null,
    warnings: [],
    ...overrides,
  };
}

describe('createNvapiSettingsStore catalog state', () => {
  beforeEach(async () => {
    await setLanguageMode('en');
  });

  it('retains an observed unknown DLL version and surfaces its explicit warning', () => {
    const store = createNvapiSettingsStore();
    store.setStates([
      state({
        dll_info: {
          kind: 'sr',
          version: null,
          path: 'C:/Games/Test/nvngx_dlss.dll',
          manifest_label: null,
        },
        warnings: ['dllVersionUnknown'],
      }),
    ]);

    expect(store.dllInfoForFamily('sr')?.version).toBeNull();
    expect(store.familyWarnings('sr')).toEqual([
      'A DLSS DLL was found, but its version is unavailable.',
    ]);
  });

  it('shows catalog-not-ready without inferring DLL absence', () => {
    const store = createNvapiSettingsStore();
    store.setStates([
      state({
        catalog_readiness: 'notReady',
        warnings: ['catalogNotReady'],
      }),
    ]);

    expect(store.dllInfoForFamily('sr')).toBeNull();
    expect(store.familyWarnings('sr')).toEqual([
      'The game catalog is not ready. Rescan the game before changing DLL-dependent NVIDIA settings.',
    ]);
  });

  it('keeps an in-flight same-key write fenced across store sessions', async () => {
    const store = createNvapiSettingsStore();
    const settingKey = 'dlss_sr_render_preset';
    let finishOldWrite!: (updated: SettingStateResponse) => void;
    const oldWrite = store.runWrite(
      settingKey,
      'Change failed',
      () => new Promise((resolve) => (finishOldWrite = resolve)),
    );

    store.beginSession();
    store.setStates([state({ current: { wire: 'external', label: 'External', dword: 2 } })]);
    expect(store.isPending(settingKey)).toBe(false);
    expect(store.busy).toBe(true);

    let blockedWriteCalled = false;
    await expect(
      store.runWrite(settingKey, 'Change failed', () => {
        blockedWriteCalled = true;
        return Promise.resolve(state());
      }),
    ).resolves.toBe(false);
    expect(blockedWriteCalled).toBe(false);

    finishOldWrite(state({ current: { wire: 'stale', label: 'Stale', dword: 1 } }));
    await expect(oldWrite).resolves.toBe(false);
    expect(store.busy).toBe(false);
    expect(store.isPending(settingKey)).toBe(false);

    let finishNewWrite!: (updated: SettingStateResponse) => void;
    const newWrite = store.runWrite(
      settingKey,
      'Change failed',
      () => new Promise((resolve) => (finishNewWrite = resolve)),
    );
    expect(store.isPending(settingKey)).toBe(true);

    finishNewWrite(state({ current: { wire: 'current', label: 'Current', dword: 3 } }));
    await expect(newWrite).resolves.toBe(true);
    expect(store.states[0].current.wire).toBe('current');
    expect(store.isPending(settingKey)).toBe(false);
  });

  it('retains in-flight write tracking when the store is cleared', async () => {
    const store = createNvapiSettingsStore();
    let finishWrite!: (updated: SettingStateResponse) => void;
    const oldWrite = store.runWrite(
      'dlss_sr_render_preset',
      'Change failed',
      () => new Promise((resolve) => (finishWrite = resolve)),
    );

    store.clearAll();
    expect(store.busy).toBe(true);
    await expect(
      store.runWrite('dlss_sr_render_preset', 'Change failed', () => Promise.resolve(state())),
    ).resolves.toBe(false);

    finishWrite(state({ current: { wire: 'stale', label: 'Stale', dword: 1 } }));
    await expect(oldWrite).resolves.toBe(false);
    expect(store.busy).toBe(false);
    expect(store.states).toEqual([]);
  });
});
