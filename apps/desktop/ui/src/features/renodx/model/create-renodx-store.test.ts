import { describe, expect, it, vi } from 'vitest';

import type * as EntitiesLibrary from '@entities/library';

vi.mock('@shared/notifications', () => ({
  publishErrorNotification: vi.fn(),
}));

vi.mock('@entities/library', async (importOriginal) => {
  const actual = await importOriginal<typeof EntitiesLibrary>();
  return { ...actual, clearDownloadProgress: vi.fn() };
});

import { clearDownloadProgress } from '@entities/library';
import { publishErrorNotification } from '@shared/notifications';
import type { RenoDxApi } from '../api/desktop';
import { createRenoDxStore, deriveFreshness } from './create-renodx-store.svelte';
import type {
  ActionDescriptor,
  AvailabilityReport,
  HostFacts,
  HostKind,
  RenoDxInstallState,
  RenoDxUpdateReport,
  ReshadeChannel,
  VulkanLayerManagementReport,
  VulkanLayerReport,
} from './types';

function action(overrides: Partial<ActionDescriptor> = {}): ActionDescriptor {
  return {
    enabled: true,
    requires_confirmation: false,
    confirmation_scope: null,
    disabled_reason: null,
    target_channel: null,
    ...overrides,
  };
}

const DEFAULT_HOST_FACTS: HostFacts = {
  slot: null,
  active: false,
  path: null,
  version: null,
  addon_support: 'unknown',
  channel: {
    selected: 'stable',
    effective: 'stable',
    detected: null,
  },
  update_status: 'unknown_needs_validation',
  is_custom_build: false,
};

const PRESENT_HOST_FACTS: HostFacts = {
  slot: 'dxgi.dll',
  active: true,
  path: 'C:\\Games\\Game\\dxgi.dll',
  version: '6.5.1',
  addon_support: 'full',
  channel: {
    selected: 'stable',
    effective: 'stable',
    detected: 'stable',
  },
  update_status: 'current',
  is_custom_build: false,
};

const VULKAN_NOT_INSTALLED: VulkanLayerReport = {
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

const VULKAN_INSTALLED: VulkanLayerReport = {
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

const VULKAN_EXTERNAL_READ_ONLY: VulkanLayerReport = {
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

function availability(
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

const NOT_INSTALLED_SAFE: AvailabilityReport = availability({
  state: { status: 'not_installed' },
  outcome: {
    kind: 'installable',
    confidence: 'verified',
    risk: {
      severity: 'info',
      anticheat_engine: 'none',
      online: 'singleplayer',
      message_key: 'renodx.risk.sp_safe',
      confidence: 'high',
      reference_url: null,
      detected_locally: false,
    },
    notes_keys: [],
    host_kind: 'proxy',
  },
  manual_install: null,
});

const INSTALLED: AvailabilityReport = availability({
  state: {
    status: 'installed',
    host_kind: 'proxy',
    version: 'snapshot-2026.06',
    addon_dated: 'Wed, 18 Jun 2026 12:00:00 GMT',
    installed_at: 1_700_000_000_000,
    updated_at: 1_700_000_500_000,
    dlss_fix_installed: false,
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

function installedWithChannel(
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
        effective: channel,
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
const INSTALLED_WITH_DLSS_FIX: RenoDxInstallState = {
  ...(INSTALLED.state as Extract<RenoDxInstallState, { status: 'installed' }>),
  dlss_fix_installed: true,
};

function fakeApi(overrides: Partial<RenoDxApi> = {}): RenoDxApi {
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
    uninstallDlssFix: vi.fn(() => Promise.resolve(INSTALLED.state)),
    dlssFixAvailability: vi.fn(() => Promise.resolve(false)),
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
    const store = createRenoDxStore(fakeApi());
    expect(store.loaded).toBe(false);
    expect(store.isInstalled).toBe(false);
    expect(store.isInstallable).toBe(false);
  });

  it('load() reflects an installable, safe game', async () => {
    const store = createRenoDxStore(fakeApi());
    await store.load('steam:1091500');

    expect(store.loaded).toBe(true);
    expect(store.isInstallable).toBe(true);
    expect(store.requiresConfirmation).toBe(false);
    expect(store.isBlocked).toBe(false);
    expect(store.risk?.severity).toBe('info');
  });

  it('falls back the selected ReShade channel when stable is unsupported', async () => {
    const withoutStable: AvailabilityReport = {
      ...NOT_INSTALLED_SAFE,
      reshade_stable_supported: false,
      host_facts: {
        ...DEFAULT_HOST_FACTS,
        channel: {
          selected: 'stable',
          effective: 'nightly',
          detected: null,
        },
      },
    };
    const store = createRenoDxStore(
      fakeApi({ getAvailability: vi.fn(() => Promise.resolve(withoutStable)) }),
    );

    await store.load('steam:1091500');

    expect(store.reshadeStableSupported).toBe(false);
    expect(store.selectedReshadeChannel).toBe('nightly');
  });

  it('applies the availability snapshot consistently on load', async () => {
    const report: AvailabilityReport = {
      ...INSTALLED,
      host_detection: 'present',
      host_facts: {
        ...PRESENT_HOST_FACTS,
        channel: {
          selected: 'nightly',
          effective: 'nightly',
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
    const store = createRenoDxStore(
      fakeApi({ getAvailability: vi.fn(() => Promise.resolve(report)) }),
    );

    await store.load('steam:1091500');

    expect(store.hostDetection).toBe('present');
    expect(store.hostFacts).toEqual(report.host_facts);
    expect(store.hostActions).toEqual(report.actions);
    expect(store.reshadeChannel).toBe('nightly');
    expect(store.reshadeStableSupported).toBe(false);
    expect(store.renodxAddon).toEqual(report.renodx_addon);
    expect(store.selectedReshadeChannel).toBe('nightly');
  });

  it('uses the backend channel facts as the per-game card selection', async () => {
    const detectedNightlyDx: AvailabilityReport = {
      ...NOT_INSTALLED_SAFE,
      host_facts: {
        ...DEFAULT_HOST_FACTS,
        channel: { selected: 'stable', effective: 'stable', detected: 'nightly' },
      },
    };
    const store = createRenoDxStore(
      fakeApi({ getAvailability: vi.fn(() => Promise.resolve(detectedNightlyDx)) }),
    );

    await store.load('steam:1091500');

    expect(store.selectedReshadeChannel).toBe('nightly');
  });

  it('falls back to the backend effective channel when no host channel is detected', async () => {
    const effectiveNightlyDx: AvailabilityReport = {
      ...NOT_INSTALLED_SAFE,
      host_facts: {
        ...DEFAULT_HOST_FACTS,
        channel: { selected: 'stable', effective: 'nightly', detected: null },
      },
    };
    const store = createRenoDxStore(
      fakeApi({ getAvailability: vi.fn(() => Promise.resolve(effectiveNightlyDx)) }),
    );

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
          anticheat_engine: 'eac',
          online: 'pvp',
          message_key: 'renodx.risk.anticheat_detected',
          confidence: 'high',
          reference_url: null,
          detected_locally: true,
        },
        notes_keys: [],
        host_kind: 'proxy',
      },
      manual_install: null,
    });
    const store = createRenoDxStore(
      fakeApi({ getAvailability: vi.fn(() => Promise.resolve(warn)) }),
    );
    await store.load('steam:42');

    expect(store.requiresConfirmation).toBe(true);
    expect(store.isBlocked).toBe(false);
  });

  it('install() passes the confirmation flag and refreshes state', async () => {
    // The refresh after install must observe the new installed state.
    let installed = false;
    const api = fakeApi({
      install: vi.fn(() => {
        installed = true;
        return Promise.resolve(INSTALLED.state);
      }),
      getAvailability: vi.fn(() => Promise.resolve(installed ? INSTALLED : NOT_INSTALLED_SAFE)),
    });
    const store = createRenoDxStore(api);

    await store.install('steam:1091500', 'stable', true);

    expect(api.install).toHaveBeenCalledWith('steam:1091500', 'stable', true);
    expect(store.isInstalled).toBe(true);
    expect(store.hostActions.use_existing?.enabled).toBe(true);
    expect(store.busy).toBe(false);
  });

  it('refreshes the Vulkan layer report after a transparent Vulkan install', async () => {
    const VULKAN: AvailabilityReport = availability({
      state: { status: 'not_installed' },
      outcome: {
        kind: 'installable',
        confidence: 'untested',
        risk: {
          severity: 'info',
          anticheat_engine: 'none',
          online: 'singleplayer',
          message_key: 'renodx.risk.sp_safe',
          confidence: 'high',
          reference_url: null,
          detected_locally: false,
        },
        notes_keys: [],
        host_kind: 'vulkan',
      },
      manual_install: null,
    });
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(VULKAN)),
      vulkanLayerStatus: vi.fn(() => Promise.resolve(VULKAN_INSTALLED)),
    });
    const store = createRenoDxStore(api);
    await store.load('steam:1091500');

    // Vulkan layer is installed transparently — no consent needed.
    await store.install('steam:1091500', 'nightly', false);

    expect(api.install).toHaveBeenCalledWith('steam:1091500', 'nightly', false);
    expect(api.vulkanLayerStatus).toHaveBeenCalled();
    // The layer report comes from the backend, not from optimistic inference.
    expect(store.vulkanLayer?.layer_detection).toBe('installed');
  });

  it('reuses an existing Vulkan layer without consent', async () => {
    const VULKAN_LAYER_PRESENT: AvailabilityReport = {
      ...NOT_INSTALLED_SAFE,
      outcome: {
        ...NOT_INSTALLED_SAFE.outcome,
        host_kind: 'vulkan',
      } as AvailabilityReport['outcome'],
      vulkan_layer: VULKAN_EXTERNAL_READ_ONLY,
    };
    const store = createRenoDxStore(
      fakeApi({ getAvailability: vi.fn(() => Promise.resolve(VULKAN_LAYER_PRESENT)) }),
    );
    await store.load('steam:1091500');
    expect(store.vulkanLayer?.actions.update).toBeUndefined();
    expect(store.vulkanLayer?.actions.switch_channel).toBeUndefined();
    expect(store.vulkanLayer?.actions.remove).toBeUndefined();
  });

  it('keeps Vulkan action permissions backend-authored', async () => {
    const VULKAN_LAYER_PRESENT: AvailabilityReport = {
      ...NOT_INSTALLED_SAFE,
      outcome: {
        ...NOT_INSTALLED_SAFE.outcome,
        host_kind: 'vulkan',
      } as AvailabilityReport['outcome'],
      vulkan_layer: VULKAN_INSTALLED,
    };
    const store = createRenoDxStore(
      fakeApi({ getAvailability: vi.fn(() => Promise.resolve(VULKAN_LAYER_PRESENT)) }),
    );

    await store.load('steam:1091500');

    expect(store.vulkanLayer?.layer_detection).toBe('installed');
    expect(store.vulkanLayer?.actions.update?.requires_confirmation).toBe(true);
    expect(store.vulkanLayer?.actions.update?.confirmation_scope).toBe('all_vulkan_reno_dx_games');
  });

  it('surfaces an available update for an installed game', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'available',
          host: 'current',
          dlssFix: null,
          overall: 'available',
        } as RenoDxUpdateReport),
      ),
    });
    const store = createRenoDxStore(api);

    await store.load('steam:1091500');

    expect(store.isInstalled).toBe(true);
    expect(store.updateStatus).toBe('available');
    expect(store.updateAvailable).toBe(true);
  });

  it('surfaces a channel mismatch as an available update for an installed game', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'current',
          host: 'channel_mismatch',
          dlssFix: null,
          overall: 'channel_mismatch',
        } as RenoDxUpdateReport),
      ),
    });
    const store = createRenoDxStore(api);

    await store.load('steam:1091500');

    expect(store.isInstalled).toBe(true);
    expect(store.updateStatus).toBe('channel_mismatch');
    expect(store.updateAvailable).toBe(true);
  });

  it('update() applies the update and refreshes, clearing the flag', async () => {
    let updated = false;
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: updated ? 'current' : 'available',
          host: 'current',
          dlssFix: null,
          overall: updated ? 'current' : 'available',
        } as RenoDxUpdateReport),
      ),
      update: vi.fn(() => {
        updated = true;
        return Promise.resolve(INSTALLED.state);
      }),
    });
    const store = createRenoDxStore(api);

    await store.load('steam:1091500');
    expect(store.updateAvailable).toBe(true);

    const ok = await store.update('steam:1091500');

    expect(ok).toBe(true);
    expect(api.update).toHaveBeenCalledWith('steam:1091500');
    expect(store.updateAvailable).toBe(false);
  });

  it('does not surface a channel switch action as an update-like state', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'current',
          host: 'current',
          dlssFix: null,
          overall: 'current',
        } as RenoDxUpdateReport),
      ),
    });
    const store = createRenoDxStore(api);

    await store.load('steam:1091500');

    expect(store.hostUpdate).toBe('current');
    expect(store.freshness).toBe('current');
    expect(store.updateAvailable).toBe(false);
  });

  it('update() no-ops when only a channel switch action is available', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'current',
          host: 'current',
          dlssFix: null,
          overall: 'current',
        } as RenoDxUpdateReport),
      ),
      switchChannel: vi.fn(() => Promise.resolve(installedWithChannel('nightly').state)),
      update: vi.fn(() => Promise.resolve(INSTALLED.state)),
    });
    const store = createRenoDxStore(api);

    await store.load('steam:1091500');
    const ok = await store.update('steam:1091500');

    expect(ok).toBe(false);
    expect(api.switchChannel).not.toHaveBeenCalled();
    expect(api.update).not.toHaveBeenCalled();
    expect(store.updateAvailable).toBe(false);
  });

  it('update() applies a normal RenoDX update without switching channels', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'available',
          host: 'current',
          dlssFix: null,
          overall: 'available',
        } as RenoDxUpdateReport),
      ),
      switchChannel: vi.fn(() => Promise.resolve(installedWithChannel('nightly').state)),
      update: vi.fn(() => Promise.resolve(installedWithChannel('nightly').state)),
    });
    const store = createRenoDxStore(api);

    await store.load('steam:1091500');
    const ok = await store.update('steam:1091500');

    expect(ok).toBe(true);
    expect(api.switchChannel).not.toHaveBeenCalled();
    expect(api.update).toHaveBeenCalledWith('steam:1091500');
    expect(store.updateAvailable).toBe(false);
  });

  it('does not count Vulkan channel mismatch as a per-game RenoDX update', async () => {
    const installedVulkan: AvailabilityReport = {
      ...INSTALLED,
      state: {
        ...(INSTALLED.state as Extract<RenoDxInstallState, { status: 'installed' }>),
        host_kind: 'vulkan',
      },
      actions: {
        use_existing: action(),
        switch_channel: action({ target_channel: 'nightly' }),
      },
    };
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(installedVulkan)),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'current',
          host: 'current',
          dlssFix: null,
          overall: 'current',
        } as RenoDxUpdateReport),
      ),
    });
    const store = createRenoDxStore(api);

    await store.load('steam:1091500');

    expect(store.updateAvailable).toBe(false);
    expect(store.hostUpdate).toBe('current');
  });

  it('switchChannel() calls the backend and updates the current channel', async () => {
    let switched = false;
    const installedNightly: AvailabilityReport = {
      ...INSTALLED,
      host_facts: {
        ...PRESENT_HOST_FACTS,
        channel: {
          selected: 'nightly',
          effective: 'nightly',
          detected: 'nightly',
        },
      },
      actions: {
        use_existing: action(),
        switch_channel: action({ target_channel: 'stable' }),
      },
    };
    const installedStable: AvailabilityReport = {
      ...installedNightly,
      host_facts: {
        ...PRESENT_HOST_FACTS,
        channel: {
          selected: 'stable',
          effective: 'stable',
          detected: 'stable',
        },
      },
      actions: {
        use_existing: action(),
        switch_channel: action({ target_channel: 'nightly' }),
      },
    };
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(switched ? installedStable : installedNightly)),
      switchChannel: vi.fn(() => {
        switched = true;
        return Promise.resolve(INSTALLED.state);
      }),
    });
    const store = createRenoDxStore(api);
    await store.load('steam:1091500');

    const ok = await store.switchChannel('steam:1091500', 'stable');

    expect(ok).toBe(true);
    expect(api.switchChannel).toHaveBeenCalledWith('steam:1091500', 'stable');
    expect(store.reshadeChannel).toBe('stable');
  });

  it('switchChannel() no-ops when the requested channel is already active', async () => {
    const installedStable: AvailabilityReport = {
      ...INSTALLED,
      host_facts: {
        ...PRESENT_HOST_FACTS,
        channel: {
          selected: 'stable',
          effective: 'stable',
          detected: 'stable',
        },
      },
      actions: {
        use_existing: action(),
        switch_channel: action({ target_channel: 'nightly' }),
      },
    };
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(installedStable)),
    });
    const store = createRenoDxStore(api);
    await store.load('steam:1091500');

    const ok = await store.switchChannel('steam:1091500', 'stable');

    expect(ok).toBe(false);
    expect(api.switchChannel).not.toHaveBeenCalled();
  });

  it('switchChannel() preserves the current channel on backend failure', async () => {
    const installedNightly: AvailabilityReport = {
      ...INSTALLED,
      host_facts: {
        ...PRESENT_HOST_FACTS,
        channel: {
          selected: 'nightly',
          effective: 'nightly',
          detected: 'nightly',
        },
      },
      actions: {
        use_existing: action(),
        switch_channel: action({ target_channel: 'stable' }),
      },
    };
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(installedNightly)),
      switchChannel: vi.fn(() => Promise.reject(new Error('boom'))),
    });
    const store = createRenoDxStore(api);
    await store.load('steam:1091500');

    const ok = await store.switchChannel('steam:1091500', 'stable');

    expect(ok).toBe(false);
    expect(api.switchChannel).toHaveBeenCalledWith('steam:1091500', 'stable');
    expect(store.reshadeChannel).toBe('nightly');
  });

  it('exposes the file-install offer for a compatible external game', async () => {
    const EXTERNAL: AvailabilityReport = availability({
      state: { status: 'not_installed' },
      outcome: {
        kind: 'external',
        url: 'https://discord.gg/example',
        label_key: 'renodx.external.discord',
        file_install: {
          confidence: 'verified',
          risk: {
            severity: 'info',
            anticheat_engine: 'none',
            online: 'singleplayer',
            message_key: 'renodx.risk.sp_safe',
            confidence: 'high',
            reference_url: null,
            detected_locally: false,
          },
          notes_keys: [],
          host_kind: 'proxy',
        },
      },
      manual_install: null,
    });
    let installed = false;
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(installed ? INSTALLED : EXTERNAL)),
      installFromFile: vi.fn(() => {
        installed = true;
        return Promise.resolve(INSTALLED.state);
      }),
    });
    const store = createRenoDxStore(api);
    await store.load('steam:1091500');

    expect(store.isExternal).toBe(true);
    expect(store.externalFileInstallable).toBe(true);
    expect(store.externalConfidence).toBe('verified');

    const ok = await store.installFromFile(
      'steam:1091500',
      'C:\\dl\\renodx-x.addon64',
      'nightly',
      false,
    );

    expect(ok).toBe(true);
    expect(api.installFromFile).toHaveBeenCalledWith(
      'steam:1091500',
      'C:\\dl\\renodx-x.addon64',
      'nightly',
      false,
    );
    expect(store.isInstalled).toBe(true);
  });

  it('treats a link-only external game as not file-installable', async () => {
    const LINK_ONLY: AvailabilityReport = availability({
      state: { status: 'not_installed' },
      outcome: {
        kind: 'external',
        url: 'https://discord.gg/example',
        label_key: 'renodx.external.discord',
        file_install: null,
      },
      manual_install: null,
    });
    const store = createRenoDxStore(
      fakeApi({ getAvailability: vi.fn(() => Promise.resolve(LINK_ONLY)) }),
    );
    await store.load('steam:1091500');

    expect(store.isExternal).toBe(true);
    expect(store.externalFileInstallable).toBe(false);
    expect(store.externalConfidence).toBeNull();
  });

  it('records a load error when the backend fails', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.reject(new Error('boom'))),
    });
    const store = createRenoDxStore(api);

    await store.load('steam:1091500');

    expect(store.loadError).not.toBeNull();
    expect(store.loaded).toBe(false);
    expect(store.loading).toBe(false);
  });

  it('install() resolves false, clears busy, and notifies when the backend fails', async () => {
    vi.mocked(publishErrorNotification).mockClear();
    const api = fakeApi({
      install: vi.fn(() => Promise.reject(new Error('boom'))),
    });
    const store = createRenoDxStore(api);

    const ok = await store.install('steam:1091500', 'stable', false);

    expect(ok).toBe(false);
    expect(store.busy).toBe(false);
    expect(store.isInstalled).toBe(false);
    expect(publishErrorNotification).toHaveBeenCalledTimes(1);
  });

  it('installFromFile() resolves false and clears busy when the backend fails', async () => {
    const api = fakeApi({
      installFromFile: vi.fn(() => Promise.reject(new Error('boom'))),
    });
    const store = createRenoDxStore(api);

    const ok = await store.installFromFile(
      'steam:1091500',
      'C:\\dl\\renodx-x.addon64',
      'stable',
      false,
    );

    expect(ok).toBe(false);
    expect(store.busy).toBe(false);
    expect(store.isInstalled).toBe(false);
  });

  it('update() resolves false and leaves state untouched when the backend fails', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'available',
          host: 'current',
          dlssFix: null,
          overall: 'available',
        } as RenoDxUpdateReport),
      ),
      update: vi.fn(() => Promise.reject(new Error('boom'))),
    });
    const store = createRenoDxStore(api);
    await store.load('steam:1091500');
    expect(store.updateAvailable).toBe(true);

    const ok = await store.update('steam:1091500');

    expect(ok).toBe(false);
    expect(store.busy).toBe(false);
    // The failed update must not clear the pending verdict it was meant to resolve.
    expect(store.updateAvailable).toBe(true);
  });

  it('uninstall() resolves false and leaves the installed state untouched when the backend fails', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      uninstall: vi.fn(() => Promise.reject(new Error('boom'))),
    });
    const store = createRenoDxStore(api);
    await store.load('steam:1091500');
    expect(store.isInstalled).toBe(true);

    const ok = await store.uninstall('steam:1091500');

    expect(ok).toBe(false);
    expect(store.busy).toBe(false);
    expect(store.isInstalled).toBe(true);
  });

  it('installDlssFix() resolves false and leaves dlssFixInstalled untouched when the backend fails', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'current',
          host: 'current',
          dlssFix: null,
          overall: 'current',
        } as RenoDxUpdateReport),
      ),
      dlssFixAvailability: vi.fn(() => Promise.resolve(true)),
      installDlssFix: vi.fn(() => Promise.reject(new Error('boom'))),
    });
    const store = createRenoDxStore(api);
    await store.load('steam:1091500');
    expect(store.dlssFixAvailable).toBe(true);

    const ok = await store.installDlssFix('steam:1091500');

    expect(ok).toBe(false);
    expect(store.busy).toBe(false);
    expect(store.dlssFixInstalled).toBe(false);
  });

  it('uninstallDlssFix() resolves false and leaves dlssFixInstalled untouched when the backend fails', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() =>
        Promise.resolve({ ...INSTALLED, state: INSTALLED_WITH_DLSS_FIX }),
      ),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'current',
          host: 'current',
          dlssFix: 'current',
          overall: 'current',
        } as RenoDxUpdateReport),
      ),
      uninstallDlssFix: vi.fn(() => Promise.reject(new Error('boom'))),
    });
    const store = createRenoDxStore(api);
    await store.load('steam:1091500');
    expect(store.dlssFixInstalled).toBe(true);

    const ok = await store.uninstallDlssFix('steam:1091500');

    expect(ok).toBe(false);
    expect(store.busy).toBe(false);
    expect(store.dlssFixInstalled).toBe(true);
  });

  it('checkForUpdates() records a failed probe when the backend rejects', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() => Promise.reject(new Error('network down'))),
    });
    const store = createRenoDxStore(api);
    await store.load('steam:1091500');

    await store.checkForUpdates('steam:1091500');

    expect(store.freshness).toBe('unknown');
    expect(store.updateProbing).toBe(false);
    expect(store.lastCheckedAt).not.toBeNull();
  });

  it('discards a stale post-mutation host refresh superseded by a newer load', async () => {
    // install() on game1 triggers a background refreshHostInfo scan that we hold
    // open; before it resolves, the user switches to game2 and load() completes
    // first. The stale game1 refresh landing afterward must not clobber game2's
    // freshly loaded host state.
    let releaseGame1Refresh: (report: AvailabilityReport) => void = () => undefined;
    const slowGame1Refresh = new Promise<AvailabilityReport>((resolve) => {
      releaseGame1Refresh = resolve;
    });
    let getAvailabilityCalls = 0;
    const api = fakeApi({
      getAvailability: vi.fn((gameId: string) => {
        getAvailabilityCalls += 1;
        // Call 1: game1's initial load(). Call 2: game1's post-install refresh
        // (held open). Call 3: game2's load().
        if (getAvailabilityCalls === 2) {
          return slowGame1Refresh;
        }
        return Promise.resolve(gameId === 'game2' ? INSTALLED : NOT_INSTALLED_SAFE);
      }),
      install: vi.fn(() => Promise.resolve(INSTALLED.state)),
    });
    const store = createRenoDxStore(api);
    await store.load('game1');

    const installDone = store.install('game1', 'stable', false); // starts the held-open refresh
    await installDone;
    await store.load('game2'); // resolves and commits before the stale refresh lands
    expect(store.hostDetection).toBe('present');

    // Now let the stale game1 refresh resolve with a different host state.
    releaseGame1Refresh(NOT_INSTALLED_SAFE);
    await Promise.resolve();
    await Promise.resolve();

    // game2's freshly loaded host state must survive the late game1 refresh.
    expect(store.hostDetection).toBe('present');
  });

  it('discards a stale load when gameId changes mid-request', async () => {
    // game1's availability never resolves until we release it; game2's resolves
    // immediately. The late game1 result must not overwrite game2's state.
    let releaseGame1: (report: AvailabilityReport) => void = () => undefined;
    const slowGame1 = new Promise<AvailabilityReport>((resolve) => {
      releaseGame1 = resolve;
    });
    const api = fakeApi({
      getAvailability: vi.fn((gameId: string) =>
        gameId === 'game1' ? slowGame1 : Promise.resolve(INSTALLED),
      ),
    });
    const store = createRenoDxStore(api);

    const load1 = store.load('game1'); // in-flight, unresolved
    await store.load('game2'); // newer, resolves first → installed
    expect(store.isInstalled).toBe(true);

    // Now resolve the stale game1 load with a different (not-installed) result.
    releaseGame1(NOT_INSTALLED_SAFE);
    await load1;

    // game2's state must survive; the stale game1 response is dropped.
    expect(store.isInstalled).toBe(true);
  });

  it('reports DLSS-Fix availability for an installed game without one', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'current',
          host: 'current',
          dlssFix: null,
          overall: 'current',
        } as RenoDxUpdateReport),
      ),
      dlssFixAvailability: vi.fn(() => Promise.resolve(true)),
    });
    const store = createRenoDxStore(api);

    await store.load('steam:1091500');

    expect(store.isInstalled).toBe(true);
    expect(store.dlssFixInstalled).toBe(false);
    expect(store.dlssFixAvailable).toBe(true);
  });

  it('installDlssFix clears availability once the companion is tracked', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'current',
          host: 'current',
          dlssFix: null,
          overall: 'current',
        } as RenoDxUpdateReport),
      ),
      dlssFixAvailability: vi.fn(() => Promise.resolve(true)),
      installDlssFix: vi.fn(() => Promise.resolve(INSTALLED_WITH_DLSS_FIX)),
    });
    const store = createRenoDxStore(api);

    await store.load('steam:1091500');
    expect(store.dlssFixAvailable).toBe(true);

    const ok = await store.installDlssFix('steam:1091500');

    expect(ok).toBe(true);
    expect(api.installDlssFix).toHaveBeenCalledWith('steam:1091500');
    // After install, the backend reports a DlssFix tracked source, so the state
    // carries `dlss_fix_installed` and the companion reads as installed; it is no
    // longer "available" to install (the stale flag must not linger).
    expect(store.dlssFixInstalled).toBe(true);
    expect(store.dlssFixAvailable).toBe(false);
  });

  it('install() clears stale download progress before starting', async () => {
    const api = fakeApi();
    const store = createRenoDxStore(api);
    vi.mocked(clearDownloadProgress).mockClear();

    await store.install('steam:1091500', 'stable', false);

    expect(vi.mocked(clearDownloadProgress)).toHaveBeenCalledWith(['steam:1091500']);
  });

  it('install() refreshes host state but does not re-probe updates after the mutation', async () => {
    // A mutation re-reads the host (one local getAvailability scan, so the freshly
    // installed ReShade replaces the pre-install host state) but must NOT trigger a
    // checkUpdate (which would re-download the add-on + ReShade host to compare
    // digests) — we just installed, so every tracked source is current.
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'current',
          host: 'current',
          dlssFix: null,
          overall: 'current',
        } as RenoDxUpdateReport),
      ),
      install: vi.fn(() => Promise.resolve(INSTALLED.state)),
    });
    const store = createRenoDxStore(api);

    await store.load('steam:1091500');
    const availabilityCallsAfterLoad = vi.mocked(api.getAvailability).mock.calls.length;
    const updateCallsAfterLoad = vi.mocked(api.checkUpdate).mock.calls.length;

    await store.install('steam:1091500', 'stable', false);

    // Exactly one extra availability scan (the host refresh) and no update-check:
    // the install verdict is derived from the command's nextState.
    expect(vi.mocked(api.getAvailability).mock.calls.length).toBe(availabilityCallsAfterLoad + 1);
    expect(vi.mocked(api.checkUpdate).mock.calls.length).toBe(updateCallsAfterLoad);
    expect(store.isInstalled).toBe(true);
    expect(store.updateStatus).toBe('current');
    expect(store.updateProbing).toBe(false);
  });

  it('mutation host refresh applies the availability snapshot without changing state', async () => {
    let installed = false;
    const afterInstall: AvailabilityReport = {
      ...INSTALLED,
      host_detection: 'present',
      host_facts: PRESENT_HOST_FACTS,
      actions: {
        use_existing: action(),
        switch_channel: action({ target_channel: 'nightly' }),
      },
      reshade_stable_supported: true,
      renodx_addon: {
        present_on_disk: true,
        expected_path: 'C:\\Games\\Game\\renodx.addon64',
        discovered_path: 'C:\\Games\\Game\\renodx.addon64',
        enabled_by_config: true,
        load_mode: 'auto_search',
      },
    };
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(installed ? afterInstall : NOT_INSTALLED_SAFE)),
      install: vi.fn(() => {
        installed = true;
        return Promise.resolve(INSTALLED.state);
      }),
    });
    const store = createRenoDxStore(api);
    await store.load('steam:1091500');

    const ok = await store.install('steam:1091500', 'stable', false);
    await Promise.resolve();

    expect(ok).toBe(true);
    expect(store.isInstalled).toBe(true);
    expect(store.hostDetection).toBe('present');
    expect(store.hostFacts).toEqual(afterInstall.host_facts);
    expect(store.reshadeChannel).toBe('stable');
    expect(store.renodxAddon).toEqual(afterInstall.renodx_addon);
  });

  it('mutation host refresh failures keep the optimistic install state', async () => {
    let calls = 0;
    const api = fakeApi({
      getAvailability: vi.fn(() => {
        calls += 1;
        return calls === 1
          ? Promise.resolve(NOT_INSTALLED_SAFE)
          : Promise.reject(new Error('scan failed'));
      }),
      install: vi.fn(() => Promise.resolve(INSTALLED.state)),
    });
    const store = createRenoDxStore(api);
    await store.load('steam:1091500');

    const ok = await store.install('steam:1091500', 'stable', false);
    await Promise.resolve();

    expect(ok).toBe(true);
    expect(store.isInstalled).toBe(true);
    expect(store.hostDetection).toBe('absent');
  });

  it('uninstall() refreshes host state and clears the update report', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      uninstall: vi.fn(() => Promise.resolve(NOT_INSTALLED_SAFE.state)),
    });
    const store = createRenoDxStore(api);
    await store.load('steam:1091500');
    expect(store.isInstalled).toBe(true);

    const availabilityCallsAfterLoad = vi.mocked(api.getAvailability).mock.calls.length;
    const ok = await store.uninstall('steam:1091500');

    expect(ok).toBe(true);
    expect(store.isInstalled).toBe(false);
    expect(store.updateStatus).toBeNull();
    // One extra availability scan refreshes the host after removal (no update probe).
    expect(vi.mocked(api.getAvailability).mock.calls.length).toBe(availabilityCallsAfterLoad + 1);
  });

  it('surfaces the add-on date and install timestamps when installed', async () => {
    const api = fakeApi({ getAvailability: vi.fn(() => Promise.resolve(INSTALLED)) });
    const store = createRenoDxStore(api);

    await store.load('steam:1091500');

    expect(store.addonDated).toBe('Wed, 18 Jun 2026 12:00:00 GMT');
    expect(store.installedAt).toBe(1_700_000_000_000);
    expect(store.updatedAt).toBe(1_700_000_500_000);
    // A completed load stamps the last-checked time and resolves freshness.
    expect(store.lastCheckedAt).not.toBeNull();
    expect(store.freshness).toBe('current');
  });

  it('checkForUpdates re-probes upstream and re-stamps lastCheckedAt', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'available',
          host: 'current',
          dlssFix: null,
          overall: 'available',
        } as RenoDxUpdateReport),
      ),
    });
    const store = createRenoDxStore(api);
    await store.load('steam:1091500');
    const checksAfterLoad = vi.mocked(api.checkUpdate).mock.calls.length;
    const stampAfterLoad = store.lastCheckedAt;

    await store.checkForUpdates('steam:1091500');

    // A second probe ran and the verdict + freshness reflect it.
    expect(vi.mocked(api.checkUpdate).mock.calls.length).toBe(checksAfterLoad + 1);
    expect(store.updateAvailable).toBe(true);
    expect(store.freshness).toBe('available');
    expect(store.lastCheckedAt).not.toBeNull();
    expect(store.lastCheckedAt).toBeGreaterThanOrEqual(stampAfterLoad ?? 0);
  });

  it('install() stamps optimistic install timestamps and a current freshness', async () => {
    // The mutation command returns a state with null timestamps (built from an
    // in-memory record); the store fills them optimistically so the card shows
    // "Installed just now / Up to date" without waiting for a reload.
    const installedWithoutDates: RenoDxInstallState = {
      status: 'installed',
      host_kind: 'proxy',
      version: null,
      addon_dated: null,
      installed_at: null,
      updated_at: null,
      dlss_fix_installed: false,
      addon_tracked: true,
    };
    const api = fakeApi({ install: vi.fn(() => Promise.resolve(installedWithoutDates)) });
    const store = createRenoDxStore(api);

    await store.install('steam:1091500', 'stable', false);

    expect(store.isInstalled).toBe(true);
    expect(store.installedAt).not.toBeNull();
    expect(store.updatedAt).not.toBeNull();
    expect(store.freshness).toBe('current');
    expect(store.lastCheckedAt).not.toBeNull();
  });

  it('reports "untracked" freshness for a local file install', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: null,
          host: null,
          dlssFix: null,
          overall: 'unknown',
        } as RenoDxUpdateReport),
      ),
    });
    const store = createRenoDxStore(api);

    await store.load('steam:1091500');

    expect(store.freshness).toBe('untracked');
  });

  it('reports "unknown" (not "untracked") freshness when the update probe fails', async () => {
    // A network failure writes the same { addon: null, host: null } report as a
    // successful untracked probe; without the probeFailed guard the card would
    // mislabel a network failure as "Updates not tracked".
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() => Promise.reject(new Error('network down'))),
    });
    const store = createRenoDxStore(api);

    await store.load('steam:1091500');

    expect(store.freshness).toBe('unknown');
    expect(store.lastCheckedAt).not.toBeNull();
  });

  it('checkForUpdates recovers from a failed probe to a current verdict', async () => {
    // First check fails (unknown), second succeeds (current): probeFailed must
    // clear so freshness does not stay stuck on "unknown" after recovery.
    let fail = true;
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() => {
        if (fail) {
          fail = false;
          return Promise.reject(new Error('network down'));
        }
        return Promise.resolve({
          addon: 'current',
          host: 'current',
          dlssFix: null,
          overall: 'current',
        } as RenoDxUpdateReport);
      }),
    });
    const store = createRenoDxStore(api);

    await store.load('steam:1091500');
    expect(store.freshness).toBe('unknown');

    await store.checkForUpdates('steam:1091500');

    expect(store.freshness).toBe('current');
  });

  it('dlssFixInstalled survives an update-probe failure (read off the state)', async () => {
    // The companion is tracked (the state carries dlss_fix_installed), but the
    // update probe fails. The UI must still show the "Remove" button — the
    // presence is read off the state, not the (null) update report.
    const api = fakeApi({
      getAvailability: vi.fn(() =>
        Promise.resolve({ ...INSTALLED, state: INSTALLED_WITH_DLSS_FIX }),
      ),
      checkUpdate: vi.fn(() => Promise.reject(new Error('network down'))),
    });
    const store = createRenoDxStore(api);

    await store.load('steam:1091500');

    expect(store.dlssFixInstalled).toBe(true);
    expect(store.freshness).toBe('unknown');
  });

  it('addonTracked is read off the state and survives a probe failure', async () => {
    // A user-file install records no add-on source (addon_tracked: false) and its
    // probe fails. `addonTracked` must stay false — read off the state, not the
    // (null) update report — so the "installed from a file" hint is correct.
    const fileInstall: RenoDxInstallState = {
      ...(INSTALLED.state as Extract<RenoDxInstallState, { status: 'installed' }>),
      addon_tracked: false,
    };
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve({ ...INSTALLED, state: fileInstall })),
      checkUpdate: vi.fn(() => Promise.reject(new Error('network down'))),
    });
    const store = createRenoDxStore(api);

    await store.load('steam:1091500');

    expect(store.addonTracked).toBe(false);
    expect(store.freshness).toBe('unknown');
  });
});

describe('deriveFreshness', () => {
  const report = (over: Partial<RenoDxUpdateReport> = {}): RenoDxUpdateReport => ({
    addon: 'current',
    host: 'current',
    dlssFix: null,
    overall: 'current',
    ...over,
  });

  it('reports checking while a probe is in flight, regardless of report', () => {
    expect(deriveFreshness(true, false, null)).toBe('checking');
    expect(deriveFreshness(true, true, report())).toBe('checking');
  });

  it('reports unknown on a failed probe or a missing report', () => {
    expect(
      deriveFreshness(false, true, report({ addon: null, host: null, overall: 'unknown' })),
    ).toBe('unknown');
    expect(deriveFreshness(false, false, null)).toBe('unknown');
  });

  it('reports available when any source changed', () => {
    expect(deriveFreshness(false, false, report({ overall: 'available' }))).toBe('available');
    expect(deriveFreshness(false, false, report({ overall: 'channel_mismatch' }))).toBe(
      'available',
    );
  });

  it('reports untracked only on a successful probe with no tracked sources', () => {
    expect(
      deriveFreshness(false, false, report({ addon: null, host: null, overall: 'unknown' })),
    ).toBe('untracked');
  });

  it('reports current when every source is up to date', () => {
    expect(deriveFreshness(false, false, report())).toBe('current');
  });
});
