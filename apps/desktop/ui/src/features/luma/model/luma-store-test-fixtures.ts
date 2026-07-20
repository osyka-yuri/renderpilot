import { vi } from 'vitest';

import { defaultHostFacts, type ActionDescriptor } from '@entities/addon';

import type { LumaApi } from '../api/desktop';
import type {
  AvailabilityOutcome,
  AvailabilityReport,
  LumaManagedDependencySummary,
  LumaUpdateReport,
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

export const DEFAULT_HOST_FACTS = defaultHostFacts('nightly');

export function availability(
  report: Pick<AvailabilityReport, 'state' | 'outcome'> & Partial<AvailabilityReport>,
): AvailabilityReport {
  return {
    host_detection: 'absent',
    host_facts: DEFAULT_HOST_FACTS,
    actions: { install: action() },
    min_reshade_version: '6.7.0',
    vcredist_present: null,
    vcredist_installer_url: 'https://aka.ms/vs/17/release/vc_redist.x64.exe',
    install_torn: false,
    ...report,
  };
}

export const INSTALLABLE_OUTCOME: Extract<AvailabilityOutcome, { kind: 'installable' }> = {
  kind: 'installable',
  confidence: 'verified',
  risk: {
    severity: 'info',
    message_key: 'addon.risk.sp_safe',
  },
  launch_args: [],
  profile: { scope: 'game' },
  features: { dlss_fsr: 'unknown', hdr: 'unknown' },
  guidance: [],
  external_requirement: null,
};

export const DGVOODOO_REQUIREMENT: LumaManagedDependencySummary = {
  kind: 'dgvoodoo2',
  version: '2.87.3',
};

export const NOT_INSTALLED_SAFE: AvailabilityReport = availability({
  state: { status: 'not_installed' },
  outcome: INSTALLABLE_OUTCOME,
});

export const INSTALLED: AvailabilityReport = availability({
  state: {
    status: 'installed',
    version: 'Build 515',
    addon_dated: null,
    installed_at: 1_700_000_000_000,
    updated_at: 1_700_000_000_000,
    reshade_channel: 'nightly',
    launch_args: [],
  },
  outcome: NOT_INSTALLED_SAFE.outcome,
  host_detection: 'present',
  host_facts: {
    ...DEFAULT_HOST_FACTS,
    slot: 'dxgi.dll',
    active: true,
    version: '6.7.0',
    addon_support: 'full',
    channel: { selected: 'nightly', detected: 'nightly' },
    update_status: 'current',
  },
  actions: {},
});

export function fakeApi(overrides: Partial<LumaApi> = {}): LumaApi {
  return {
    getAvailability: vi.fn(() => Promise.resolve(NOT_INSTALLED_SAFE)),
    install: vi.fn(() => Promise.resolve(INSTALLED.state)),
    uninstall: vi.fn(() => Promise.resolve(NOT_INSTALLED_SAFE.state)),
    checkUpdate: vi.fn(() =>
      Promise.resolve({
        addon: 'current',
        host: 'current',
        dgvoodoo: null,
        overall: 'current',
      } as LumaUpdateReport),
    ),
    update: vi.fn(() => Promise.resolve(INSTALLED.state)),
    ...overrides,
  };
}
