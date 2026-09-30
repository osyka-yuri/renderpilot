import { describe, expect, it, vi } from 'vitest';

vi.mock('@shared/notifications', () => ({
  publishPresentedErrorNotification: vi.fn(),
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
    expect(store.hasPersistedRecord).toBe(false);
  });

  it('projects persisted-record presence and clears it when the record disappears or the store deactivates', async () => {
    const persistedRecord = availability({
      ...NOT_INSTALLED_SAFE,
      has_persisted_record: true,
    });
    const getAvailability = vi
      .fn()
      .mockResolvedValueOnce(persistedRecord)
      .mockResolvedValueOnce(NOT_INSTALLED_SAFE)
      .mockResolvedValueOnce(persistedRecord);
    const store = createRenoDxStore({ api: fakeApi({ getAvailability }) });

    await store.load('steam:1091500');
    expect(store.isInstalled).toBe(false);
    expect(store.hasPersistedRecord).toBe(true);

    await store.load('steam:1091500');
    expect(store.hasPersistedRecord).toBe(false);

    await store.load('steam:1091500');
    expect(store.hasPersistedRecord).toBe(true);
    store.deactivate();
    expect(store.hasPersistedRecord).toBe(false);
  });

  it('load() reflects an installable, safe game', async () => {
    const store = createRenoDxStore({ api: fakeApi() });
    await store.load('steam:1091500');

    expect(store.loaded).toBe(true);
    expect(store.isInstallable).toBe(true);
  });

  it('preserves and clears the torn-install observation with availability state', async () => {
    const report = { ...NOT_INSTALLED_SAFE, install_torn: true };
    const store = createRenoDxStore({
      api: fakeApi({ getAvailability: vi.fn(() => Promise.resolve(report)) }),
    });

    await store.load('steam:1091500');
    expect(store.installTorn).toBe(true);

    store.deactivate();
    expect(store.installTorn).toBe(false);
  });

  it('preserves the complete generic catalogue profile', async () => {
    const genericProfile = {
      engine: 'unreal' as const,
      message: {
        id: 'renodx.generic.universal',
        fallback_text: 'Uses the shared Unreal Engine profile.',
      },
      profile_id: 'ue_extended',
    };
    const report = availability({
      state: { status: 'not_installed' },
      outcome: {
        kind: 'installable',
        confidence: 'verified',
        generic_profile: genericProfile,
        profile_id: 'ue_extended',
        host_kind: 'proxy',
        guidance: [],
        launch: null,
      },
      manual_install: null,
    });
    const store = createRenoDxStore({
      api: fakeApi({ getAvailability: vi.fn(() => Promise.resolve(report)) }),
    });

    await store.load('steam:1091500');

    expect(store.genericProfile).toEqual(genericProfile);
  });

  it('retains resolved guidance and launch arguments for the same installed game', async () => {
    const guidance = [
      {
        id: 'renodx.test.ini',
        kind: 'engine_ini' as const,
        message_id: 'renodx.test.ini',
        fallback_text: 'Add the reviewed setting.',
        code: 'r.HDR.EnableHDROutput=1',
        settings: [],
        url: null,
      },
    ];
    const first = availability({
      ...INSTALLED,
      outcome: {
        kind: 'installable',
        confidence: 'verified',
        generic_profile: null,
        profile_id: 'ue_extended',
        host_kind: 'proxy',
        guidance,
        launch: { arguments: ['-force-d3d11'], requirement: 'recommended' },
      },
    });
    const drifted = availability({
      ...INSTALLED,
      outcome: { kind: 'unsupported' },
      manual_install: null,
    });
    const getAvailability = vi.fn().mockResolvedValueOnce(first).mockResolvedValueOnce(drifted);
    const store = createRenoDxStore({ api: fakeApi({ getAvailability }) });

    await store.load('steam:2358720');
    await store.load('steam:2358720');

    expect(store.outcome?.kind).toBe('unsupported');
    expect(store.guidance).toEqual(guidance);
    expect(store.profileId).toBe('ue_extended');
    expect(store.launch).toEqual({
      arguments: ['-force-d3d11'],
      requirement: 'recommended',
    });
  });

  it('drops retained guidance, launch arguments, and profile_id when switching to a different game', async () => {
    const guidance = [
      {
        id: 'renodx.test.ini',
        kind: 'engine_ini' as const,
        message_id: 'renodx.test.ini',
        fallback_text: 'Add the reviewed setting.',
        code: 'r.HDR.EnableHDROutput=1',
        settings: [],
        url: null,
      },
    ];
    const gameAInstalled = availability({
      ...INSTALLED,
      outcome: {
        kind: 'installable',
        confidence: 'verified',
        generic_profile: null,
        profile_id: 'ue_extended',
        host_kind: 'proxy',
        guidance,
        launch: { arguments: ['-force-d3d11'], requirement: 'recommended' },
      },
    });
    const gameBUnsupported = availability({
      ...INSTALLED,
      outcome: { kind: 'unsupported' },
      manual_install: null,
    });
    const getAvailability = vi
      .fn()
      .mockResolvedValueOnce(gameAInstalled)
      .mockResolvedValueOnce(gameBUnsupported);
    const store = createRenoDxStore({ api: fakeApi({ getAvailability }) });

    await store.load('steam:game-a');
    expect(store.guidance).toEqual(guidance);
    expect(store.profileId).toBe('ue_extended');
    expect(store.launch).toEqual({
      arguments: ['-force-d3d11'],
      requirement: 'recommended',
    });

    await store.load('steam:game-b');
    expect(store.guidance).toEqual([]);
    expect(store.profileId).toBeNull();
    expect(store.launch).toBeNull();
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
    expect(await store.install('steam:1091500', 'stable')).toBe('skipped');
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

  it('deactivate() clears shared and RenoDX-specific state', async () => {
    const report: AvailabilityReport = {
      ...INSTALLED,
      host_detection: 'present',
      host_facts: PRESENT_HOST_FACTS,
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

    store.deactivate();

    expect(store.loaded).toBe(false);
    expect(store.isInstalled).toBe(false);
    expect(store.isInstallable).toBe(false);
    expect(store.hostDetection).toBe('absent');
    expect(store.hostFacts).toEqual(DEFAULT_HOST_FACTS);
    expect(store.renodxAddon).toBeNull();
    expect(store.reshadeStableSupported).toBe(true);
    expect(store.selectedReshadeChannel).toBe('stable');
    expect(store.vulkanLayer).toBeNull();
    expect(store.dlssFix).toEqual({ kind: 'hidden' });
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

  it('loads an installable game without a duplicate risk confirmation flow', async () => {
    const warn: AvailabilityReport = availability({
      state: { status: 'not_installed' },
      outcome: {
        kind: 'installable',
        confidence: 'untested',
        generic_profile: null,
        profile_id: null,
        host_kind: 'proxy',
        guidance: [],
        launch: null,
      },
      manual_install: null,
    });
    const store = createRenoDxStore({
      api: fakeApi({ getAvailability: vi.fn(() => Promise.resolve(warn)) }),
    });
    await store.load('steam:42');

    expect(store.isInstallable).toBe(true);
    expect(store.confidence).toBe('untested');
    expect(store.safetyContextError).toBeNull();
  });

  it('exposes confidence for both installable and external outcomes', async () => {
    const installableReport: AvailabilityReport = availability({
      state: { status: 'not_installed' },
      outcome: {
        kind: 'installable',
        confidence: 'verified',
        generic_profile: null,
        profile_id: null,
        host_kind: 'proxy',
        guidance: [],
        launch: null,
      },
      manual_install: null,
    });
    const externalReport: AvailabilityReport = availability({
      state: { status: 'not_installed' },
      outcome: {
        kind: 'external',
        url: 'https://nexusmods.com/example',
        message: { id: 'test', fallback_text: 'Test' },
        file_install: {
          confidence: 'experimental',
          host_kind: 'proxy',
          generic_profile: null,
          profile_id: null,
          guidance: [],
          launch: null,
        },
      },
      manual_install: null,
    });

    const store = createRenoDxStore({
      api: fakeApi({
        getAvailability: vi
          .fn()
          .mockResolvedValueOnce(installableReport)
          .mockResolvedValueOnce(externalReport),
      }),
    });

    await store.load('steam:installable');
    expect(store.confidence).toBe('verified');

    await store.load('steam:external');
    expect(store.confidence).toBe('experimental');
  });

  it('preserves catalog confidence when installed', async () => {
    const installedWithConfidence: AvailabilityReport = availability({
      ...INSTALLED,
      outcome: {
        kind: 'installable',
        confidence: 'verified',
        generic_profile: null,
        profile_id: null,
        host_kind: 'proxy',
        guidance: [],
        launch: null,
      },
    });

    const store = createRenoDxStore({
      api: fakeApi({
        getAvailability: vi.fn(() => Promise.resolve(installedWithConfidence)),
      }),
    });

    await store.load('steam:installed');
    expect(store.isInstalled).toBe(true);
    expect(store.confidence).toBe('verified');
  });
});
