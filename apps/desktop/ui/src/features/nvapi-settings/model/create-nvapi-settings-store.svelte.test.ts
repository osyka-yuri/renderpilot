import { beforeEach, describe, expect, it } from 'vitest';

import { setLanguageMode } from '@shared/i18n';

import { createNvapiSettingsStore } from './create-nvapi-settings-store.svelte';
import type { SettingStateResponse } from './types';

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
    baseline: null,
    is_current_predefined: true,
    is_modified_outside_renderpilot: false,
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
});
