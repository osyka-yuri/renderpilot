import { describe, expect, it, vi } from 'vitest';

vi.mock('@shared/notifications', () => ({
  publishErrorNotification: vi.fn(),
}));

import { createRenoDxStore } from './create-renodx-store.svelte';
import type { AvailabilityReport } from './types';
import {
  action,
  availability,
  DEFAULT_HOST_FACTS,
  fakeApi,
  INSTALLED,
  NOT_INSTALLED_SAFE,
  PRESENT_HOST_FACTS,
  VULKAN_EXTERNAL_READ_ONLY,
} from './renodx-store-test-fixtures';

describe('createRenoDxStore', () => {
  it('keeps private proof fields out of availability DTO fixtures', () => {
    const serialized = JSON.stringify([
      NOT_INSTALLED_SAFE,
      INSTALLED,
      {
        ...NOT_INSTALLED_SAFE,
        vulkan_layer: VULKAN_EXTERNAL_READ_ONLY,
      },
    ]);
    const forbidden = [
      ['reshade_', 'mana', 'ged_by_us'].join(''),
      ['mana', 'ged_by_us'].join(''),
      ['mana', 'ged'].join(''),
      ['unmana', 'ged'].join(''),
      ['fore', 'ign'].join(''),
      ['own', 'ed'].join(''),
      ['owner', 'ship'].join(''),
      ['mark', 'er'].join(''),
      ['mark', 'er_version'].join(''),
      ['sour', 'ce'].join(''),
      ['dig', 'est'].join(''),
      ['sha', '256'].join(''),
      ['valid', 'ator'].join(''),
      ['backup', '_path'].join(''),
      ['rollback', '_manifest'].join(''),
      ['created', '_by'].join(''),
      ['installed', '_by'].join(''),
      ['tracked', '_source'].join(''),
      ['proven', 'ance'].join(''),
    ];

    for (const key of forbidden) {
      expect(serialized).not.toContain(key);
    }
  });

  it('starts empty before loading', () => {
    const store = createRenoDxStore({ api: fakeApi() });
    expect(store.loaded).toBe(false);
    expect(store.isInstalled).toBe(false);
    expect(store.isInstallable).toBe(false);
  });

  it('load() reflects an installable, safe game', async () => {
    const store = createRenoDxStore({ api: fakeApi() });
    await store.load('steam:1091500');

    expect(store.loaded).toBe(true);
    expect(store.isInstallable).toBe(true);
    expect(store.requiresConfirmation).toBe(false);
    expect(store.risk?.severity).toBe('info');
  });

  it('preserves the complete generic catalogue profile', async () => {
    const genericProfile = {
      engine: 'unreal' as const,
      message: {
        id: 'renodx.generic.universal',
        fallback_text: 'Uses the shared Unreal Engine profile.',
      },
    };
    const report = availability({
      state: { status: 'not_installed' },
      outcome: {
        kind: 'installable',
        confidence: 'verified',
        risk: {
          severity: 'info',
          message_key: 'addon.risk.sp_safe',
        },
        generic_profile: genericProfile,
        host_kind: 'proxy',
      },
      manual_install: null,
    });
    const store = createRenoDxStore({
      api: fakeApi({ getAvailability: vi.fn(() => Promise.resolve(report)) }),
    });

    await store.load('steam:1091500');

    expect(store.genericProfile).toEqual(genericProfile);
  });

  it('does not silently remap an unavailable selected Stable channel', async () => {
    const withoutStable: AvailabilityReport = {
      ...NOT_INSTALLED_SAFE,
      reshade_stable_supported: false,
      host_facts: {
        ...DEFAULT_HOST_FACTS,
        channel: {
          selected: 'stable',
          detected: null,
        },
      },
    };
    const store = createRenoDxStore({
      api: fakeApi({ getAvailability: vi.fn(() => Promise.resolve(withoutStable)) }),
    });

    await store.load('steam:1091500');

    expect(store.reshadeStableSupported).toBe(false);
    expect(store.selectedReshadeChannel).toBe('stable');
    expect(await store.install('steam:1091500', 'stable', false)).toBe('skipped');
  });

  it('applies the availability snapshot consistently on load', async () => {
    const report: AvailabilityReport = {
      ...INSTALLED,
      host_detection: 'present',
      host_facts: {
        ...PRESENT_HOST_FACTS,
        channel: {
          selected: 'nightly',
          detected: 'nightly',
        },
      },
      actions: {
        use_existing: action(),
        switch_channel: action({ target_channel: 'stable' }),
      },
      reshade_stable_supported: false,
      renodx_addon: {
        present_on_disk: true,
        expected_path: 'C:\\Games\\Game\\renodx.addon64',
        discovered_path: 'C:\\Games\\Game\\renodx.addon64',
        enabled_by_config: true,
        load_mode: 'auto_search',
      },
    };
    const store = createRenoDxStore({
      api: fakeApi({ getAvailability: vi.fn(() => Promise.resolve(report)) }),
    });

    await store.load('steam:1091500');

    expect(store.hostDetection).toBe('present');
    expect(store.hostFacts).toEqual(report.host_facts);
    expect(store.hostActions).toEqual(report.actions);
    expect(store.reshadeChannel).toBe('nightly');
    expect(store.reshadeStableSupported).toBe(false);
    expect(store.renodxAddon).toEqual(report.renodx_addon);
    expect(store.selectedReshadeChannel).toBe('nightly');
  });

  it('uses the backend selected channel rather than the detected channel', async () => {
    const detectedNightlyDx: AvailabilityReport = {
      ...NOT_INSTALLED_SAFE,
      host_facts: {
        ...DEFAULT_HOST_FACTS,
        channel: { selected: 'stable', detected: 'nightly' },
      },
    };
    const store = createRenoDxStore({
      api: fakeApi({ getAvailability: vi.fn(() => Promise.resolve(detectedNightlyDx)) }),
    });

    await store.load('steam:1091500');

    expect(store.selectedReshadeChannel).toBe('stable');
  });

  it('uses the backend selected channel when no host channel is detected', async () => {
    const effectiveNightlyDx: AvailabilityReport = {
      ...NOT_INSTALLED_SAFE,
      host_facts: {
        ...DEFAULT_HOST_FACTS,
        channel: { selected: 'nightly', detected: null },
      },
    };
    const store = createRenoDxStore({
      api: fakeApi({ getAvailability: vi.fn(() => Promise.resolve(effectiveNightlyDx)) }),
    });

    await store.load('steam:1091500');

    expect(store.selectedReshadeChannel).toBe('nightly');
  });

  it('flags a warn-risk game as requiring confirmation', async () => {
    const warn: AvailabilityReport = availability({
      state: { status: 'not_installed' },
      outcome: {
        kind: 'installable',
        confidence: 'untested',
        risk: {
          severity: 'warn',
          message_key: 'addon.risk.anticheat_detected',
        },
        generic_profile: null,
        host_kind: 'proxy',
      },
      manual_install: null,
    });
    const store = createRenoDxStore({
      api: fakeApi({ getAvailability: vi.fn(() => Promise.resolve(warn)) }),
    });
    await store.load('steam:42');

    expect(store.requiresConfirmation).toBe(true);
  });
});
