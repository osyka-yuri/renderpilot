import { describe, expect, it, vi } from 'vitest';

vi.mock('@shared/notifications', () => ({
  publishPresentedErrorNotification: vi.fn(),
}));

vi.mock('@shared/lib', async (importOriginal) => {
  const actual = await importOriginal<unknown>();
  return { ...(actual as Record<string, unknown>), clearDownloadProgress: vi.fn() };
});

import { clearDownloadProgress } from '@shared/lib';
import { publishPresentedErrorNotification } from '@shared/notifications';
import { createRenoDxStore } from './create-renodx-store.svelte';
import type { AvailabilityReport, RenoDxInstallState, RenoDxUpdateReport } from './types';
import {
  action,
  availability,
  fakeApi,
  INSTALLED,
  INSTALLED_WITH_DLSS_FIX,
  installedWithChannel,
  NOT_INSTALLED_SAFE,
  PRESENT_HOST_FACTS,
  VULKAN_INSTALLED,
  VULKAN_NOT_INSTALLED,
} from './renodx-store-test-fixtures';

describe('createRenoDxStore', () => {
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
    const store = createRenoDxStore({ api });

    await store.install('steam:1091500', 'stable', true);

    expect(api.install).toHaveBeenCalledWith('steam:1091500', 'stable', true);
    expect(store.isInstalled).toBe(true);
    expect(store.hostActions.use_existing?.enabled).toBe(true);
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
    const store = createRenoDxStore({ api });

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
    const store = createRenoDxStore({ api });

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
    const store = createRenoDxStore({ api });

    await store.load('steam:1091500');
    expect(store.updateAvailable).toBe(true);

    const ok = await store.update('steam:1091500');

    expect(ok).toBe('ok');
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
    const store = createRenoDxStore({ api });

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
    const store = createRenoDxStore({ api });

    await store.load('steam:1091500');
    const ok = await store.update('steam:1091500');

    expect(ok).toBe('skipped');
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
    const store = createRenoDxStore({ api });

    await store.load('steam:1091500');
    const ok = await store.update('steam:1091500');

    expect(ok).toBe('ok');
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
    const store = createRenoDxStore({ api });

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
    const store = createRenoDxStore({ api });
    await store.load('steam:1091500');

    const ok = await store.switchChannel('steam:1091500', 'stable');

    expect(ok).toBe('ok');
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
    const store = createRenoDxStore({ api });
    await store.load('steam:1091500');

    const ok = await store.switchChannel('steam:1091500', 'stable');

    expect(ok).toBe('skipped');
    expect(api.switchChannel).not.toHaveBeenCalled();
  });

  it('switchChannel() preserves the current channel on backend failure', async () => {
    const installedNightly: AvailabilityReport = {
      ...INSTALLED,
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
    };
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(installedNightly)),
      switchChannel: vi.fn(() => Promise.reject(new Error('boom'))),
    });
    const store = createRenoDxStore({ api });
    await store.load('steam:1091500');

    const ok = await store.switchChannel('steam:1091500', 'stable');

    expect(ok).toBe('failed');
    expect(api.switchChannel).toHaveBeenCalledWith('steam:1091500', 'stable');
    expect(store.reshadeChannel).toBe('nightly');
  });

  it('exposes the file-install offer for a compatible external game', async () => {
    const EXTERNAL: AvailabilityReport = availability({
      state: { status: 'not_installed' },
      outcome: {
        kind: 'external',
        url: 'https://discord.gg/example',
        message: {
          id: 'renodx.external.discord',
          fallback_text: 'Open the RenoDX Discord',
        },
        file_install: {
          confidence: 'verified',
          risk: {
            severity: 'info',
            message_key: 'addon.risk.sp_safe',
          },
          host_kind: 'proxy',
          generic_profile: null,
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
    const store = createRenoDxStore({ api });
    await store.load('steam:1091500');

    expect(store.isExternal).toBe(true);
    expect(store.externalFileInstallable).toBe(true);
    expect(store.externalConfidence).toBe('verified');
    expect(store.externalMessage).toEqual({
      id: 'renodx.external.discord',
      fallback_text: 'Open the RenoDX Discord',
    });

    const ok = await store.installFromFile(
      'steam:1091500',
      'C:\\dl\\renodx-x.addon64',
      'nightly',
      false,
    );

    expect(ok).toBe('ok');
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
        message: {
          id: 'renodx.external.discord',
          fallback_text: 'Open the RenoDX Discord',
        },
        file_install: null,
      },
      manual_install: null,
    });
    const store = createRenoDxStore({
      api: fakeApi({ getAvailability: vi.fn(() => Promise.resolve(LINK_ONLY)) }),
    });
    await store.load('steam:1091500');

    expect(store.isExternal).toBe(true);
    expect(store.externalFileInstallable).toBe(false);
    expect(store.externalConfidence).toBeNull();
  });

  it('records a load error when the backend fails', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.reject(new Error('boom'))),
    });
    const store = createRenoDxStore({ api });

    await store.load('steam:1091500');

    expect(store.loadError).not.toBeNull();
    expect(store.loaded).toBe(false);
    expect(store.loading).toBe(false);
  });

  it('install() resolves false, clears busy, and notifies when the backend fails', async () => {
    vi.mocked(publishPresentedErrorNotification).mockClear();
    const api = fakeApi({
      install: vi.fn(() => Promise.reject(new Error('boom'))),
    });
    const store = createRenoDxStore({ api });

    const ok = await store.install('steam:1091500', 'stable', false);

    expect(ok).toBe('failed');
    expect(store.busy).toBe(false);
    expect(store.isInstalled).toBe(false);
    expect(publishPresentedErrorNotification).toHaveBeenCalledTimes(1);
  });

  it('installFromFile() resolves false and clears busy when the backend fails', async () => {
    const api = fakeApi({
      installFromFile: vi.fn(() => Promise.reject(new Error('boom'))),
    });
    const store = createRenoDxStore({ api });

    const ok = await store.installFromFile(
      'steam:1091500',
      'C:\\dl\\renodx-x.addon64',
      'stable',
      false,
    );

    expect(ok).toBe('failed');
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
    const store = createRenoDxStore({ api });
    await store.load('steam:1091500');
    expect(store.updateAvailable).toBe(true);

    const ok = await store.update('steam:1091500');

    expect(ok).toBe('failed');
    expect(store.busy).toBe(false);
    // The failed update must not clear the pending verdict it was meant to resolve.
    expect(store.updateAvailable).toBe(true);
  });

  it('uninstall() resolves false and leaves the installed state untouched when the backend fails', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      uninstall: vi.fn(() => Promise.reject(new Error('boom'))),
    });
    const store = createRenoDxStore({ api });
    await store.load('steam:1091500');
    expect(store.isInstalled).toBe(true);

    const ok = await store.uninstall('steam:1091500');

    expect(ok).toBe('failed');
    expect(store.busy).toBe(false);
    expect(store.isInstalled).toBe(true);
  });

  it('does not notify peer exclusivity when uninstall fails', async () => {
    const onExclusivityChange = vi.fn();
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      uninstall: vi.fn(() => Promise.reject(new Error('boom'))),
    });
    const store = createRenoDxStore({ api, onExclusivityChange });
    await store.load('steam:1091500');

    const ok = await store.uninstall('steam:1091500');

    expect(ok).toBe('failed');
    expect(onExclusivityChange).not.toHaveBeenCalled();
  });

  it('checkForUpdates() records a failed probe when the backend rejects', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() => Promise.reject(new Error('network down'))),
    });
    const store = createRenoDxStore({ api });
    await store.load('steam:1091500');

    await store.checkForUpdates('steam:1091500');

    expect(store.freshness).toBe('unknown');
    expect(store.updateProbing).toBe(false);
    expect(store.lastCheckedAt).not.toBeNull();
  });

  it('discards a stale post-mutation host refresh superseded by a newer load', async () => {
    // install() on game1 enters its awaited post-commit refresh, which we hold
    // open. Before it resolves, the user switches to game2 and load() completes.
    // The stale game1 refresh landing afterward must not clobber game2's freshly
    // loaded host state, even though install() correctly waits for post-commit.
    const slowGame1Refresh = Promise.withResolvers<AvailabilityReport>();
    let getAvailabilityCalls = 0;
    const api = fakeApi({
      getAvailability: vi.fn((gameId: string) => {
        getAvailabilityCalls += 1;
        // Call 1: game1's initial load(). Call 2: game1's post-install refresh
        // (held open). Call 3: game2's load().
        if (getAvailabilityCalls === 2) {
          return slowGame1Refresh.promise;
        }
        return Promise.resolve(gameId === 'game2' ? INSTALLED : NOT_INSTALLED_SAFE);
      }),
      install: vi.fn(() => Promise.resolve(INSTALLED.state)),
    });
    const store = createRenoDxStore({ api });
    await store.load('game1');

    const installDone = store.install('game1', 'stable', false);
    await vi.waitFor(() => {
      expect(api.getAvailability).toHaveBeenCalledTimes(2);
    });
    await store.load('game2'); // resolves and commits before the stale refresh lands
    expect(store.hostDetection).toBe('present');

    // Now let the stale game1 refresh resolve with a different host state.
    slowGame1Refresh.resolve(NOT_INSTALLED_SAFE);
    await installDone;

    // game2's freshly loaded host state must survive the late game1 refresh.
    expect(store.hostDetection).toBe('present');
  });

  it('discards a stale load when gameId changes mid-request', async () => {
    // game1's availability never resolves until we release it; game2's resolves
    // immediately. The late game1 result must not overwrite game2's state.
    const slowGame1 = Promise.withResolvers<AvailabilityReport>();
    const api = fakeApi({
      getAvailability: vi.fn((gameId: string) =>
        gameId === 'game1' ? slowGame1.promise : Promise.resolve(INSTALLED),
      ),
    });
    const store = createRenoDxStore({ api });

    const load1 = store.load('game1'); // in-flight, unresolved
    await store.load('game2'); // newer, resolves first → installed
    expect(store.isInstalled).toBe(true);

    // Now resolve the stale game1 load with a different (not-installed) result.
    slowGame1.resolve(NOT_INSTALLED_SAFE);
    await load1;

    // game2's state must survive; the stale game1 response is dropped.
    expect(store.isInstalled).toBe(true);
    expect(api.dlssFixAvailability).toHaveBeenCalledOnce();
    expect(api.dlssFixAvailability).toHaveBeenCalledWith('game2');
  });

  it('install() clears stale download progress before starting', async () => {
    const api = fakeApi();
    const store = createRenoDxStore({ api });
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
    const store = createRenoDxStore({ api });

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
      // Distinct from NOT_INSTALLED_SAFE so stale pre-mutation fields would fail.
      outcome: {
        kind: 'installable',
        confidence: 'untested',
        risk: {
          severity: 'info',
          message_key: 'addon.risk.sp_safe',
        },
        generic_profile: {
          engine: 'unreal',
          message: {
            id: 'renodx.generic.universal',
            fallback_text: 'Uses the shared Unreal Engine profile.',
          },
        },
        host_kind: 'proxy',
      },
      manual_install: {
        risk: {
          severity: 'info',
          message_key: 'addon.risk.sp_safe',
        },
        host_kind: 'proxy',
        expected_addon_name: 'renodx-example',
        game_arch: 'x64',
      },
      vulkan_layer: VULKAN_INSTALLED,
    };
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(installed ? afterInstall : NOT_INSTALLED_SAFE)),
      install: vi.fn(() => {
        installed = true;
        return Promise.resolve(INSTALLED.state);
      }),
      // Avoid afterCommit vulkan status call overwriting the availability field.
      vulkanLayerStatus: vi.fn(() => Promise.resolve(VULKAN_INSTALLED)),
    });
    const store = createRenoDxStore({ api });
    await store.load('steam:1091500');
    expect(store.outcome).toEqual(NOT_INSTALLED_SAFE.outcome);
    expect(store.manualInstall).toBeNull();
    expect(store.vulkanLayer).toEqual(VULKAN_NOT_INSTALLED);

    const ok = await store.install('steam:1091500', 'stable', false);
    await Promise.resolve();

    expect(ok).toBe('ok');
    expect(store.isInstalled).toBe(true);
    expect(store.hostDetection).toBe('present');
    expect(store.hostFacts).toEqual(afterInstall.host_facts);
    expect(store.reshadeChannel).toBe('stable');
    expect(store.renodxAddon).toEqual(afterInstall.renodx_addon);
    expect(store.outcome).toEqual(afterInstall.outcome);
    expect(store.manualInstall).toEqual(afterInstall.manual_install);
    expect(store.vulkanLayer).toEqual(VULKAN_INSTALLED);
  });

  it('uninstall host refresh updates outcome-derived fields from availability', async () => {
    let installed = true;
    const afterUninstall = availability({
      state: { status: 'not_installed' },
      outcome: {
        kind: 'external',
        url: 'https://example.test/renodx',
        message: {
          id: 'renodx.external.example',
          fallback_text: 'Use the external package',
        },
        file_install: null,
      },
      manual_install: {
        risk: {
          severity: 'info',
          message_key: 'addon.risk.sp_safe',
        },
        host_kind: 'proxy',
        expected_addon_name: 'renodx-example',
        game_arch: 'x64',
      },
      vulkan_layer: VULKAN_NOT_INSTALLED,
    });
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(installed ? INSTALLED : afterUninstall)),
      uninstall: vi.fn(() => {
        installed = false;
        return Promise.resolve(NOT_INSTALLED_SAFE.state);
      }),
    });
    const store = createRenoDxStore({ api });
    await store.load('steam:1091500');
    expect(store.isInstalled).toBe(true);
    expect(store.outcome?.kind).toBe('unsupported');

    const ok = await store.uninstall('steam:1091500');
    await Promise.resolve();

    expect(ok).toBe('ok');
    expect(store.isInstalled).toBe(false);
    expect(store.outcome).toEqual(afterUninstall.outcome);
    expect(store.manualInstall).toEqual(afterUninstall.manual_install);
    expect(store.vulkanLayer).toEqual(VULKAN_NOT_INSTALLED);
    expect(store.isExternal).toBe(true);
  });

  it('mutation host refresh failures keep the committed install state', async () => {
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
    const store = createRenoDxStore({ api });
    await store.load('steam:1091500');

    const ok = await store.install('steam:1091500', 'stable', false);
    await Promise.resolve();

    expect(ok).toBe('ok');
    expect(store.isInstalled).toBe(true);
    expect(store.hostDetection).toBe('absent');
  });

  it('uninstall() refreshes host state and clears the update report', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      uninstall: vi.fn(() => Promise.resolve(NOT_INSTALLED_SAFE.state)),
    });
    const store = createRenoDxStore({ api });
    await store.load('steam:1091500');
    expect(store.isInstalled).toBe(true);

    const availabilityCallsAfterLoad = vi.mocked(api.getAvailability).mock.calls.length;
    const ok = await store.uninstall('steam:1091500');

    expect(ok).toBe('ok');
    expect(store.isInstalled).toBe(false);
    expect(store.updateStatus).toBeNull();
    // One extra availability scan refreshes the host after removal (no update probe).
    expect(vi.mocked(api.getAvailability).mock.calls.length).toBe(availabilityCallsAfterLoad + 1);
  });

  it('surfaces the add-on date and install timestamps when installed', async () => {
    const api = fakeApi({ getAvailability: vi.fn(() => Promise.resolve(INSTALLED)) });
    const store = createRenoDxStore({ api });

    await store.load('steam:1091500');

    expect(store.addonDated).toBe(Date.UTC(2026, 5, 18, 12));
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
    const store = createRenoDxStore({ api });
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

  it('install() commits backend install state and a synthetic current freshness', async () => {
    // Timestamps come from the backend; the client only applies a synthetic
    // "everything current" update report until the next real probe.
    const installedFromBackend: RenoDxInstallState = {
      status: 'installed',
      host_kind: 'proxy',
      version: null,
      addon_dated: null,
      installed_at: 1_000_000_000_000,
      updated_at: 1_000_000_000_000,
      dlss_fix_installed: false,
      addon_tracked: true,
    };
    const api = fakeApi({ install: vi.fn(() => Promise.resolve(installedFromBackend)) });
    const store = createRenoDxStore({ api });

    await store.install('steam:1091500', 'stable', false);

    expect(store.isInstalled).toBe(true);
    expect(store.installedAt).toBe(1_000_000_000_000);
    expect(store.updatedAt).toBe(1_000_000_000_000);
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
    const store = createRenoDxStore({ api });

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
    const store = createRenoDxStore({ api });

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
    const store = createRenoDxStore({ api });

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
    const store = createRenoDxStore({ api });

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
    const store = createRenoDxStore({ api });

    await store.load('steam:1091500');

    expect(store.addonTracked).toBe(false);
    expect(store.freshness).toBe('unknown');
  });
});
