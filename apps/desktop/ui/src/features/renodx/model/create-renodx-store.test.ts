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

const NOT_INSTALLED_SAFE: AvailabilityReport = {
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
};

const INSTALLED: AvailabilityReport = {
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
};

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

  it('flags a warn-risk game as requiring confirmation', async () => {
    const warn: AvailabilityReport = {
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
    };
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

    await store.install('steam:1091500', true);

    expect(api.install).toHaveBeenCalledWith('steam:1091500', true);
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

  it('exposes the file-install offer for a compatible external game', async () => {
    const EXTERNAL: AvailabilityReport = {
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
    };
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

    const ok = await store.installFromFile('steam:1091500', 'C:\\dl\\renodx-x.addon64', false);

    expect(ok).toBe(true);
    expect(api.installFromFile).toHaveBeenCalledWith(
      'steam:1091500',
      'C:\\dl\\renodx-x.addon64',
      false,
    );
    expect(store.isInstalled).toBe(true);
  });

  it('treats a link-only external game as not file-installable', async () => {
    const LINK_ONLY: AvailabilityReport = {
      state: { status: 'not_installed' },
      outcome: {
        kind: 'external',
        url: 'https://discord.gg/example',
        label_key: 'renodx.external.discord',
        file_install: null,
      },
      manual_install: null,
    };
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

    const ok = await store.install('steam:1091500', false);

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

    await store.install('steam:1091500', false);

    expect(vi.mocked(clearDownloadProgress)).toHaveBeenCalledWith(['steam:1091500']);
  });

  it('install() does not re-fetch availability or re-probe updates after the mutation', async () => {
    // The perf regression guard: a mutation must not trigger a second
    // getAvailability (which re-reads the game executable) nor a checkUpdate
    // (which would re-download the add-on + ReShade host to compare digests).
    // We just installed, so every tracked source is current by construction.
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

    await store.install('steam:1091500', false);

    // No additional availability or update-check calls from the post-mutation
    // refresh; the install verdict is derived from the command's nextState.
    expect(vi.mocked(api.getAvailability).mock.calls.length).toBe(availabilityCallsAfterLoad);
    expect(vi.mocked(api.checkUpdate).mock.calls.length).toBe(updateCallsAfterLoad);
    expect(store.isInstalled).toBe(true);
    expect(store.updateStatus).toBe('current');
    expect(store.updateProbing).toBe(false);
  });

  it('uninstall() clears the update report without re-fetching availability', async () => {
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
    expect(vi.mocked(api.getAvailability).mock.calls.length).toBe(availabilityCallsAfterLoad);
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

    await store.install('steam:1091500', false);

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
