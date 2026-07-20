/**
 * Preview/mock stubs for Luma and RenoDX IPC used by game-details cards.
 * Read paths return stable "not installed / unsupported" reports so cards load.
 * Write paths reject with an explicit message — preview does not simulate installs.
 *
 * Shapes are untyped plain objects so the `app` layer stays free of feature-slice
 * imports (FSD). Runtime stores still accept these as wire JSON.
 */

import { defaultHostFacts } from '@entities/addon';

const MOCK_WRITE_ERROR = 'Mock preview does not simulate add-on install, update, or uninstall.';

export function mockUnsupportedLumaAvailability(): unknown {
  return {
    state: { status: 'not_installed' },
    host_detection: 'absent',
    host_facts: defaultHostFacts('nightly'),
    actions: {},
    min_reshade_version: '6.0.0',
    vcredist_present: null,
    vcredist_installer_url: 'https://aka.ms/vs/17/release/vc_redist.x64.exe',
    install_torn: false,
    outcome: { kind: 'unsupported' },
  };
}

export function mockLumaUpdateReport(): unknown {
  return {
    addon: null,
    host: null,
    dgvoodoo: null,
    overall: 'current',
  };
}

const MOCK_VULKAN_LAYER = {
  layer_detection: 'not_installed',
  layer_facts: {
    manifest_path: null,
    dll_path: null,
    version: null,
    architecture: 'unknown',
    loader_visibility: 'normal',
  },
  diagnostic_reasons: [] as string[],
  actions: {},
};

export function mockUnsupportedRenoDxAvailability(): unknown {
  return {
    state: { status: 'not_installed' },
    host_detection: 'absent',
    host_facts: defaultHostFacts('stable'),
    actions: {},
    reshade_stable_supported: true,
    renodx_addon: null,
    outcome: { kind: 'unsupported' },
    manual_install: null,
    vulkan_layer: MOCK_VULKAN_LAYER,
  };
}

export function mockRenoDxUpdateReport(): unknown {
  return {
    addon: null,
    host: null,
    dlssFix: null,
    overall: 'current',
  };
}

export function mockVulkanLayerStatus(): unknown {
  return MOCK_VULKAN_LAYER;
}

export function mockVulkanLayerManagementStatus(): unknown {
  return {
    layer: MOCK_VULKAN_LAYER,
    reshade_stable_supported: true,
    recorded_channel: null,
    default_channel: 'stable',
  };
}

export function mockAddonWriteUnsupported(): never {
  throw new Error(MOCK_WRITE_ERROR);
}
