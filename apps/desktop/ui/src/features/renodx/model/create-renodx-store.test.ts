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
import type { RenoDxApi } from '../api/desktop';
import { createRenoDxStore, deriveFreshness } from './create-renodx-store.svelte';
import type { AvailabilityReport, RenoDxInstallState, RenoDxUpdateReport } from './types';

function availability(
  report: Omit<
    AvailabilityReport,
    | 'reshade_host'
    | 'reshade_host_action'
    | 'reshade_conflict'
    | 'reshade_channel'
    | 'reshade_stable_supported'
    | 'reshade_ownership'
    | 'renodx_addon'
  >,
): AvailabilityReport {
  return {
    reshade_host: { status: 'absent' },
    reshade_host_action: 'update_host',
    reshade_conflict: false,
    reshade_channel: null,
    reshade_stable_supported: true,
    reshade_ownership: { kind: 'missing' },
    renodx_addon: null,
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
      source: null,
      detected_locally: false,
    },
    notes_keys: [],
  },
  manual_install: null,
});

const INSTALLED: AvailabilityReport = availability({
  state: {
    status: 'installed',
    version: 'snapshot-2026.06',
    reshade_managed_by_us: true,
    addon_dated: 'Wed, 18 Jun 2026 12:00:00 GMT',
    installed_at: 1_700_000_000_000,
    updated_at: 1_700_000_500_000,
    dlss_fix_installed: false,
    addon_tracked: true,
  },
  outcome: { kind: 'unsupported' },
  manual_install: null,
});

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
    ...overrides,
  };
}

describe('createRenoDxStore', () => {
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
    };
    const store = createRenoDxStore(
      fakeApi({ getAvailability: vi.fn(() => Promise.resolve(withoutStable)) }),
    );

    await store.load('steam:1091500');
    store.setSelectedReshadeChannel('stable');

    expect(store.reshadeStableSupported).toBe(false);
    expect(store.selectedReshadeChannel).toBe('nightly');
  });

  it('applies the availability snapshot consistently on load', async () => {
    const report: AvailabilityReport = {
      ...INSTALLED,
      reshade_host: {
        status: 'present',
        path: 'C:\\Games\\Game\\dxgi.dll',
        slot: 'dxgi.dll',
        version: '6.5.1',
        addon_support: 'full',
        identity: 'confirmed',
        active: {
          state: 'active',
          reason: 'detected_by_matcher',
        },
      },
      reshade_host_action: 'up_to_date',
      reshade_conflict: false,
      reshade_channel: 'nightly',
      reshade_stable_supported: false,
      reshade_ownership: { kind: 'managed', health: 'healthy' },
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

    expect(store.reshadeHost).toEqual(report.reshade_host);
    expect(store.reshadeHostAction).toBe('up_to_date');
    expect(store.reshadeConflict).toBe(false);
    expect(store.reshadeChannel).toBe('nightly');
    expect(store.reshadeStableSupported).toBe(false);
    expect(store.reshadeOwnership).toEqual({ kind: 'managed', health: 'healthy' });
    expect(store.renodxAddon).toEqual(report.renodx_addon);
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
          source: null,
          detected_locally: true,
        },
        notes_keys: [],
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
    expect(store.isManaged).toBe(true);
    expect(store.busy).toBe(false);
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

  it('switchChannel() calls the backend and updates the current channel', async () => {
    let switched = false;
    const installedNightly: AvailabilityReport = {
      ...INSTALLED,
      reshade_channel: 'nightly',
      reshade_ownership: { kind: 'managed', health: 'healthy' },
    };
    const installedStable: AvailabilityReport = {
      ...installedNightly,
      reshade_channel: 'stable',
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
      reshade_channel: 'stable',
      reshade_ownership: { kind: 'managed', health: 'healthy' },
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
      reshade_channel: 'nightly',
      reshade_ownership: { kind: 'managed', health: 'healthy' },
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
            source: null,
            detected_locally: false,
          },
          notes_keys: [],
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

  it('install() resolves false when the backend fails', async () => {
    const api = fakeApi({
      install: vi.fn(() => Promise.reject(new Error('boom'))),
    });
    const store = createRenoDxStore(api);

    const ok = await store.install('steam:1091500', 'stable', false);

    expect(ok).toBe(false);
    expect(store.busy).toBe(false);
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
      reshade_host: {
        status: 'present',
        path: 'C:\\Games\\Game\\dxgi.dll',
        slot: 'dxgi.dll',
        version: '6.5.1',
        addon_support: 'full',
        identity: 'confirmed',
        active: {
          state: 'active',
          reason: 'detected_by_matcher',
        },
      },
      reshade_host_action: 'up_to_date',
      reshade_conflict: false,
      reshade_channel: 'stable',
      reshade_stable_supported: true,
      reshade_ownership: { kind: 'managed', health: 'healthy' },
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
    expect(store.reshadeHost).toEqual(afterInstall.reshade_host);
    expect(store.reshadeHostAction).toBe('up_to_date');
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
    expect(store.reshadeHost).toEqual({ status: 'absent' });
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
      version: null,
      reshade_managed_by_us: true,
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

  it('reports "untracked" freshness for a file install with a foreign host', async () => {
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
