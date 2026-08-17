import { describe, expect, it } from 'vitest';

import type { SettingStateResponse } from '@features/nvapi-settings';

import { displayDllLabel, isDllDependentCatalogBlocked } from './DlssComponentCard.model';

function state(overrides: Partial<SettingStateResponse>): SettingStateResponse {
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

describe('DlssComponentCard catalog presentation', () => {
  it('labels an observed unknown version without rendering a null version', () => {
    expect(
      displayDllLabel(
        { kind: 'sr', version: null, path: 'C:/Games/Test/nvngx_dlss.dll', manifest_label: null },
        'DLSS version unavailable',
      ),
    ).toBe('DLSS version unavailable');
  });

  it('blocks only DLL-dependent rows while a game catalog is not ready', () => {
    expect(isDllDependentCatalogBlocked(state({ catalog_readiness: 'notReady' }))).toBe(true);
    expect(
      isDllDependentCatalogBlocked(
        state({ catalog_readiness: 'notReady', dll_kind: null, family: 'sr' }),
      ),
    ).toBe(false);
  });
});
