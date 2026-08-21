import { vi } from 'vitest';

import {
  defaultHostFacts,
  type ActionDescriptor,
  type HostFacts,
  type ReshadeChannel,
} from '@entities/addon';

import type { RenoDxApi } from '../api/desktop';
import type {
  AvailabilityReport,
  DlssFixAvailability,
  HostKind,
  RenoDxInstallState,
  RenoDxUpdateReport,
  VulkanLayerManagementReport,
  VulkanLayerReport,
} from './types';

/** Test builder for an enabled action descriptor. */
export function action(overrides: Partial<ActionDescriptor> = {}): ActionDescriptor {
  return {
    enabled: true,
    requires_confirmation: false,
    confirmation_scope: null,
    disabled_reason: null,
    target_channel: null,
    ...overrides,
  };
}

export const DEFAULT_HOST_FACTS = defaultHostFacts('stable');

export const PRESENT_HOST_FACTS: HostFacts = {
  slot: 'dxgi.dll',
  active: true,
  path: 'C:\\Games\\Game\\dxgi.dll',
  version: '6.5.1',
  addon_support: 'full',
  channel: {
    selected: 'stable',
    detected: 'stable',
  },
  update_status: 'current',
  is_custom_build: false,
};

export const VULKAN_NOT_INSTALLED: VulkanLayerReport = {
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

export const VULKAN_INSTALLED: VulkanLayerReport = {
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
    update: action({ requires_confirmation: true, confirmation_scope: 'all_vulkan_reno_dx_games' }),
    switch_channel: action({
      requires_confirmation: true,
      confirmation_scope: 'all_vulkan_reno_dx_games',
      target_channel: 'nightly',
    }),
    remove: action({ requires_confirmation: true, confirmation_scope: 'all_vulkan_reno_dx_games' }),
  },
};

export const VULKAN_EXTERNAL_READ_ONLY: VulkanLayerReport = {
  layer_detection: 'external_read_only',
  layer_facts: {
    manifest_path: 'C:\\ReShade\\ReShade64.json',
    dll_path: 'C:\\ReShade\\ReShade64.dll',
    version: '6.5.1',
    architecture: 'x64',
    loader_visibility: 'normal',
  },
  diagnostic_reasons: ['external_layer_detected'],
  actions: {},
};

export function availability(
  report: Pick<AvailabilityReport, 'state' | 'outcome' | 'manual_install'> &
    Partial<AvailabilityReport>,
): AvailabilityReport {
  return {
    host_detection: 'absent',
    host_facts: DEFAULT_HOST_FACTS,
    actions: {
      install: action(),
    },
    reshade_stable_supported: true,
    renodx_addon: null,
    vulkan_layer: VULKAN_NOT_INSTALLED,
    ...report,
  };
}

export const NOT_INSTALLED_SAFE: AvailabilityReport = availability({
  state: { status: 'not_installed' },
  outcome: {
    kind: 'installable',
    confidence: 'verified',
    generic_profile: null,
    host_kind: 'proxy',
  },
  manual_install: null,
});

export const INSTALLED: AvailabilityReport = availability({
  state: {
    status: 'installed',
    host_kind: 'proxy',
    version: 'snapshot-2026.06',
    addon_dated: 'Thu, 18 Jun 2026 12:00:00 GMT',
    installed_at: 1_700_000_000_000,
    updated_at: 1_700_000_500_000,
    dlss_fix_evidence_present: false,
    addon_tracked: true,
  },
  host_detection: 'present',
  host_facts: PRESENT_HOST_FACTS,
  actions: {
    use_existing: action(),
    switch_channel: action({ target_channel: 'nightly' }),
  },
  outcome: { kind: 'unsupported' },
  manual_install: null,
});

export function installedWithChannel(
  channel: ReshadeChannel,
  hostKind: HostKind = 'proxy',
): AvailabilityReport {
  const other = channel === 'stable' ? 'nightly' : 'stable';
  return {
    ...INSTALLED,
    state: {
      ...(INSTALLED.state as Extract<RenoDxInstallState, { status: 'installed' }>),
      host_kind: hostKind,
    },
    host_facts: {
      ...PRESENT_HOST_FACTS,
      channel: {
        selected: channel,
        detected: channel,
      },
    },
    actions: {
      use_existing: action(),
      switch_channel: action({ target_channel: other }),
    },
  };
}

/** Install state with a DLSS-Fix companion tracked (mirrors the backend's
 *  `install_dlss_fix` response, which records a DlssFix tracked source). */
export const INSTALLED_WITH_DLSS_FIX: RenoDxInstallState = {
  ...(INSTALLED.state as Extract<RenoDxInstallState, { status: 'installed' }>),
  dlss_fix_evidence_present: true,
};

export const DLSS_FIX_INSTALLABLE: DlssFixAvailability = {
  kind: 'binding',
  state: 'none',
  actions: ['install'],
};

export const DLSS_FIX_UNAVAILABLE: DlssFixAvailability = {
  kind: 'binding',
  state: 'none',
  actions: [],
};

export const DLSS_FIX_MANAGED: DlssFixAvailability = {
  kind: 'binding',
  state: 'bound',
  actions: ['update', 'remove'],
};

export const DLSS_FIX_NEEDS_REPAIR: DlssFixAvailability = {
  kind: 'binding',
  state: 'source_only',
  actions: ['repair', 'remove'],
};

export const DLSS_FIX_PENDING_RECOVERY: DlssFixAvailability = {
  kind: 'recovery_pending',
  actions: ['retry_recovery'],
};

export const DLSS_FIX_VALIDATION_REQUIRED: DlssFixAvailability = {
  kind: 'binding',
  state: 'invalid',
  actions: ['validation_required'],
};

export function fakeApi(overrides: Partial<RenoDxApi> = {}): RenoDxApi {
  return {
    getAvailability: vi.fn(() => Promise.resolve(NOT_INSTALLED_SAFE)),
    install: vi.fn(() => Promise.resolve(INSTALLED.state)),
    installFromFile: vi.fn(() => Promise.resolve(INSTALLED.state)),
    uninstall: vi.fn(() => Promise.resolve(NOT_INSTALLED_SAFE.state)),
    checkUpdate: vi.fn(() =>
      Promise.resolve({
        addon: 'current',
        host: 'current',
        dlssFix: null,
        overall: 'current',
      } as RenoDxUpdateReport),
    ),
    update: vi.fn(() => Promise.resolve(INSTALLED.state)),
    switchChannel: vi.fn(() => Promise.resolve(INSTALLED.state)),
    installDlssFix: vi.fn(() => Promise.resolve(INSTALLED_WITH_DLSS_FIX)),
    updateDlssFix: vi.fn(() => Promise.resolve(INSTALLED_WITH_DLSS_FIX)),
    retryDlssFixRecovery: vi.fn(() => Promise.resolve(INSTALLED_WITH_DLSS_FIX)),
    uninstallDlssFix: vi.fn(() => Promise.resolve(INSTALLED.state)),
    dlssFixAvailability: vi.fn(() => Promise.resolve(DLSS_FIX_UNAVAILABLE)),
    vulkanLayerStatus: vi.fn(() => Promise.resolve(VULKAN_NOT_INSTALLED)),
    vulkanLayerManagementStatus: vi.fn(() =>
      Promise.resolve({
        layer: VULKAN_NOT_INSTALLED,
        reshade_stable_supported: true,
        recorded_channel: null,
        default_channel: 'stable',
      } satisfies VulkanLayerManagementReport),
    ),
    applyVulkanLayer: vi.fn(() =>
      Promise.resolve({
        layer: VULKAN_INSTALLED,
        reshade_stable_supported: true,
        recorded_channel: 'stable',
        default_channel: 'stable',
      } satisfies VulkanLayerManagementReport),
    ),
    removeVulkanLayer: vi.fn(() => Promise.resolve(VULKAN_NOT_INSTALLED)),
    ...overrides,
  };
}
