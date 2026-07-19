import { describe, expect, it, vi } from 'vitest';

import type { ReshadeChannel } from '@entities/addon';
import { CATALOG_SETTING_KEYS } from '@entities/settings';
import type * as SharedLib from '@shared/lib';

import type { RenoDxApi } from '../api/desktop';
import { action } from './renodx-store-test-fixtures';
import type { VulkanLayerManagementReport, VulkanLayerReport } from './types';
import {
  createVulkanLayerSettingsStore,
  VULKAN_LAYER_PROGRESS_ID,
} from './create-vulkan-layer-settings-store.svelte';

vi.mock('@shared/notifications', () => ({
  publishErrorNotification: vi.fn(),
}));

vi.mock('@shared/lib', async (importOriginal) => {
  const actual = await importOriginal<typeof SharedLib>();
  return { ...actual, clearDownloadProgress: vi.fn() };
});

import { clearDownloadProgress } from '@shared/lib';

type SettingsApi = Pick<
  RenoDxApi,
  'vulkanLayerManagementStatus' | 'applyVulkanLayer' | 'removeVulkanLayer'
>;

type SettingsPersistence = NonNullable<Parameters<typeof createVulkanLayerSettingsStore>[1]>;

function installedLayer(overrides: Partial<VulkanLayerReport> = {}): VulkanLayerReport {
  return {
    layer_detection: 'installed',
    layer_facts: {
      manifest_path: 'C:\\Users\\me\\AppData\\Local\\RenderPilot\\vk_layer.json',
      dll_path: 'C:\\Users\\me\\AppData\\Local\\RenderPilot\\ReShade64.dll',
      version: '6.5.1',
      architecture: 'x64',
      loader_visibility: 'normal',
    },
    diagnostic_reasons: [],
    actions: {
      switch_channel: action({
        requires_confirmation: true,
        confirmation_scope: 'all_vulkan_reno_dx_games',
        target_channel: 'nightly',
      }),
      remove: action({
        requires_confirmation: true,
        confirmation_scope: 'all_vulkan_reno_dx_games',
      }),
    },
    ...overrides,
  };
}

function notInstalledLayer(): VulkanLayerReport {
  return {
    layer_detection: 'not_installed',
    layer_facts: {
      manifest_path: null,
      dll_path: null,
      version: null,
      architecture: 'unknown',
      loader_visibility: 'normal',
    },
    diagnostic_reasons: [],
    actions: {
      install: action(),
    },
  };
}

function repairableConflictLayer(): VulkanLayerReport {
  return {
    layer_detection: 'conflict',
    layer_facts: {
      manifest_path: 'C:\\ProgramData\\ReShade\\ReShade64.json',
      dll_path: 'C:\\ProgramData\\ReShade\\ReShade64.dll',
      version: null,
      architecture: 'x64',
      loader_visibility: 'normal',
    },
    diagnostic_reasons: ['registry_missing'],
    actions: {
      resolve_conflict: action({
        requires_confirmation: true,
        confirmation_scope: 'all_vulkan_reno_dx_games',
      }),
    },
  };
}

function conflictWithUpdateAvailableLayer(): VulkanLayerReport {
  return {
    layer_detection: 'conflict',
    layer_facts: {
      manifest_path: 'C:\\ProgramData\\ReShade\\ReShade64.json',
      dll_path: 'C:\\ProgramData\\ReShade\\ReShade64.dll',
      version: '6.4.0',
      architecture: 'x64',
      loader_visibility: 'normal',
    },
    diagnostic_reasons: ['hash_mismatch'],
    actions: {
      update: action(),
    },
  };
}

function managementReport(
  overrides: Partial<VulkanLayerManagementReport> = {},
): VulkanLayerManagementReport {
  return {
    layer: installedLayer(),
    reshade_stable_supported: true,
    recorded_channel: 'stable',
    default_channel: 'stable',
    ...overrides,
  };
}

function fakeApi(overrides: Partial<SettingsApi> = {}): SettingsApi {
  return {
    vulkanLayerManagementStatus: vi.fn<SettingsApi['vulkanLayerManagementStatus']>(() =>
      Promise.resolve(managementReport()),
    ),
    applyVulkanLayer: vi.fn<SettingsApi['applyVulkanLayer']>((channel) =>
      Promise.resolve(managementReport({ recorded_channel: channel })),
    ),
    removeVulkanLayer: vi.fn<SettingsApi['removeVulkanLayer']>(() =>
      Promise.resolve(notInstalledLayer()),
    ),
    ...overrides,
  };
}

function fakeSettings(value: ReshadeChannel | null = null): SettingsPersistence {
  return {
    getCatalogSetting: vi.fn<SettingsPersistence['getCatalogSetting']>(() =>
      Promise.resolve({ value }),
    ),
    setCatalogSetting: vi.fn<SettingsPersistence['setCatalogSetting']>(() =>
      Promise.resolve({ saved: true }),
    ),
  };
}

describe('createVulkanLayerSettingsStore', () => {
  it('loads the settings channel without changing the active Vulkan layer channel', async () => {
    const api = fakeApi();
    const settings = fakeSettings('nightly');
    const store = createVulkanLayerSettingsStore(api, settings);

    await store.load();

    expect(api.vulkanLayerManagementStatus).toHaveBeenCalledTimes(1);
    expect(settings.getCatalogSetting).toHaveBeenCalledWith(
      CATALOG_SETTING_KEYS.RENODX_RESHADE_CHANNEL,
    );
    expect(store.activeChannel).toBe('stable');
    expect(store.selectedChannel).toBe('nightly');
    expect(store.primaryAction).toBe('switch_channel');
  });

  it('keeps an unavailable stored Stable selection and disables its action', async () => {
    const api = fakeApi({
      vulkanLayerManagementStatus: vi.fn<SettingsApi['vulkanLayerManagementStatus']>(() =>
        Promise.resolve(
          managementReport({
            reshade_stable_supported: false,
            recorded_channel: null,
            default_channel: 'nightly',
          }),
        ),
      ),
    });
    const settings = fakeSettings('stable');
    const store = createVulkanLayerSettingsStore(api, settings);

    await store.load();

    expect(store.stableSupported).toBe(false);
    expect(store.selectedChannel).toBe('stable');
    expect(store.primaryAction).toBeNull();
    expect(await store.apply()).toBe(false);
    expect(api.applyVulkanLayer).not.toHaveBeenCalled();
  });

  it('applies the shared Vulkan layer through the dedicated backend command', async () => {
    const api = fakeApi();
    const settings = fakeSettings('nightly');
    const store = createVulkanLayerSettingsStore(api, settings);

    await store.load();
    const ok = await store.apply();

    expect(ok).toBe(true);
    expect(settings.setCatalogSetting).toHaveBeenCalledWith(
      CATALOG_SETTING_KEYS.RENODX_RESHADE_CHANNEL,
      'nightly',
    );
    expect(clearDownloadProgress).toHaveBeenCalledWith([VULKAN_LAYER_PROGRESS_ID]);
    expect(api.applyVulkanLayer).toHaveBeenCalledWith('nightly');
    expect(store.activeChannel).toBe('nightly');
  });

  it('presents a mutable Vulkan layer conflict as a repair action', async () => {
    const api = fakeApi({
      vulkanLayerManagementStatus: vi.fn<SettingsApi['vulkanLayerManagementStatus']>(() =>
        Promise.resolve(
          managementReport({
            layer: repairableConflictLayer(),
            recorded_channel: null,
            default_channel: 'stable',
          }),
        ),
      ),
      applyVulkanLayer: vi.fn<SettingsApi['applyVulkanLayer']>(() =>
        Promise.resolve(managementReport({ recorded_channel: 'stable' })),
      ),
    });
    const settings = fakeSettings('stable');
    const store = createVulkanLayerSettingsStore(api, settings);

    await store.load();

    expect(store.primaryAction).toBe('repair');
    expect(store.primaryActionDescriptor?.enabled).toBe(true);
    expect(store.primaryActionDescriptor?.requires_confirmation).toBe(true);

    const ok = await store.apply();

    expect(ok).toBe(true);
    expect(api.applyVulkanLayer).toHaveBeenCalledWith('stable');
    expect(store.layer?.layer_detection).toBe('installed');
  });

  it('reports the plain detection state before the first load', () => {
    const store = createVulkanLayerSettingsStore(fakeApi(), fakeSettings());

    expect(store.displayState).toBeNull();
  });

  it('reports a conflict with an available update as needing repair', async () => {
    const api = fakeApi({
      vulkanLayerManagementStatus: vi.fn<SettingsApi['vulkanLayerManagementStatus']>(() =>
        Promise.resolve(
          managementReport({
            layer: conflictWithUpdateAvailableLayer(),
            recorded_channel: null,
            default_channel: 'stable',
          }),
        ),
      ),
    });
    const store = createVulkanLayerSettingsStore(api, fakeSettings('stable'));

    await store.load();

    expect(store.displayState).toBe('needs_repair');
  });

  it('reports a conflict without an update action as a plain conflict', async () => {
    const api = fakeApi({
      vulkanLayerManagementStatus: vi.fn<SettingsApi['vulkanLayerManagementStatus']>(() =>
        Promise.resolve(
          managementReport({
            layer: repairableConflictLayer(),
            recorded_channel: null,
            default_channel: 'stable',
          }),
        ),
      ),
    });
    const store = createVulkanLayerSettingsStore(api, fakeSettings('stable'));

    await store.load();

    expect(store.displayState).toBe('conflict');
  });

  it('removes the shared layer and reloads the management report', async () => {
    const afterRemove = managementReport({
      layer: notInstalledLayer(),
      recorded_channel: null,
      default_channel: 'stable',
    });
    const api = fakeApi({
      vulkanLayerManagementStatus: vi
        .fn<SettingsApi['vulkanLayerManagementStatus']>()
        .mockResolvedValueOnce(managementReport())
        .mockResolvedValueOnce(afterRemove),
    });
    const settings = fakeSettings('stable');
    const store = createVulkanLayerSettingsStore(api, settings);

    await store.load();
    const ok = await store.remove();

    expect(ok).toBe(true);
    expect(api.removeVulkanLayer).toHaveBeenCalledTimes(1);
    expect(api.vulkanLayerManagementStatus).toHaveBeenCalledTimes(2);
    expect(store.layer?.layer_detection).toBe('not_installed');
  });
});
