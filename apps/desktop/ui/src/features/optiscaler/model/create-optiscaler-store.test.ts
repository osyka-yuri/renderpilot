import { describe, expect, it, vi } from 'vitest';

vi.mock('@shared/notifications', () => ({
  publishPresentedErrorNotification: vi.fn(),
}));

import type { OptiScalerApi } from '../api/desktop';
import { createOptiScalerStore } from './create-optiscaler-store.svelte';
import type { OptiScalerAvailability, OptiScalerOperationResult } from './types';
import {
  buildOptiScalerAvailability,
  buildOptiScalerOperationResult,
} from './optiscaler-test-fixtures';

function availability(gameId: string): OptiScalerAvailability {
  return buildOptiScalerAvailability({
    game_id: gameId,
  });
}

function operation(gameId: string): OptiScalerOperationResult {
  return buildOptiScalerOperationResult({
    installed: true,
    release: gameId,
    changed_count: 1,
  });
}

function fakeApi(overrides: Partial<OptiScalerApi> = {}): OptiScalerApi {
  const availabilityApi = vi.fn<OptiScalerApi['availability']>((gameId) =>
    Promise.resolve(availability(gameId)),
  );
  const install = vi.fn<OptiScalerApi['install']>((gameId) => Promise.resolve(operation(gameId)));
  const update = vi.fn<OptiScalerApi['update']>((gameId) => Promise.resolve(operation(gameId)));
  const repair = vi.fn<OptiScalerApi['repair']>((gameId) => Promise.resolve(operation(gameId)));
  const setModules = vi.fn<OptiScalerApi['setModules']>((gameId) =>
    Promise.resolve(operation(gameId)),
  );
  const relocate = vi.fn<OptiScalerApi['relocate']>((gameId) => Promise.resolve(operation(gameId)));
  const uninstall = vi.fn<OptiScalerApi['uninstall']>((gameId) =>
    Promise.resolve(operation(gameId)),
  );
  const checkUpdate = vi.fn<OptiScalerApi['checkUpdate']>(() =>
    Promise.resolve({
      overall: 'current',
      installed_release: null,
      available_release: null,
      update_available: false,
      repair_required: false,
      drifted: false,
    }),
  );
  return {
    availability: availabilityApi,
    install,
    checkUpdate,
    update,
    repair,
    setModules,
    relocate,
    uninstall,
    ...overrides,
  };
}

describe('createOptiScalerStore', () => {
  it('clears the previous game report immediately when navigation changes game', async () => {
    let resolveSecond!: (report: OptiScalerAvailability) => void;
    const second = new Promise<OptiScalerAvailability>((resolve) => {
      resolveSecond = resolve;
    });
    const availabilityApi = vi.fn<OptiScalerApi['availability']>((gameId) =>
      gameId === 'steam:2' ? second : Promise.resolve(availability(gameId)),
    );
    const api = fakeApi({ availability: availabilityApi });
    const store = createOptiScalerStore({ api });
    await store.load('steam:1');
    expect(store.report?.game_id).toBe('steam:1');

    const loadingSecond = store.load('steam:2');

    expect(store.loaded).toBe(false);
    expect(store.report).toBeNull();
    resolveSecond(availability('steam:2'));
    await loadingSecond;
    expect(store.report?.game_id).toBe('steam:2');
  });

  it('does not let a stale availability response overwrite the current game', async () => {
    let resolveFirst!: (report: OptiScalerAvailability) => void;
    const first = new Promise<OptiScalerAvailability>((resolve) => {
      resolveFirst = resolve;
    });
    const availabilityApi = vi.fn<OptiScalerApi['availability']>((gameId) =>
      gameId === 'steam:1' ? first : Promise.resolve(availability(gameId)),
    );
    const api = fakeApi({ availability: availabilityApi });
    const store = createOptiScalerStore({ api });

    const loadingFirst = store.load('steam:1');
    await store.load('steam:2');
    resolveFirst(availability('steam:1'));
    await loadingFirst;

    expect(store.report?.game_id).toBe('steam:2');
  });

  it('keeps the last successful report usable when refresh fails', async () => {
    let rejectRefresh = false;
    const availabilityApi = vi.fn<OptiScalerApi['availability']>((gameId) => {
      if (rejectRefresh) {
        return Promise.reject(new Error('stable catalogue is temporarily unavailable'));
      }
      return Promise.resolve(availability(gameId));
    });
    const store = createOptiScalerStore({ api: fakeApi({ availability: availabilityApi }) });

    await store.load('steam:1');
    rejectRefresh = true;
    await store.load('steam:1');

    expect(store.loadError).not.toBeNull();
    expect(store.report?.selected_release).toBe('stable');
    expect(store.loaded).toBe(true);
  });

  it('serializes mutations and invalidates peer add-on state after success', async () => {
    let resolveInstall!: (result: OptiScalerOperationResult) => void;
    const install = new Promise<OptiScalerOperationResult>((resolve) => {
      resolveInstall = resolve;
    });
    const onAddonStateChange = vi.fn();
    const api = fakeApi({ install: vi.fn<OptiScalerApi['install']>(() => install) });
    const store = createOptiScalerStore({ api, onAddonStateChange });
    await store.load('steam:1');

    const first = store.install('steam:1', ['core']);
    const second = await store.install('steam:1', ['core']);
    expect(second).toBe('skipped');
    resolveInstall(operation('steam:1'));

    expect(await first).toBe('ok');
    expect(onAddonStateChange).toHaveBeenCalledWith('steam:1');
  });

  it('uses the same install contract for every installation attempt', async () => {
    const currentApi = fakeApi();
    const store = createOptiScalerStore({ api: currentApi });
    await store.load('steam:1');

    await store.install('steam:1', ['core']);
    await store.install('steam:1', ['core']);

    expect(currentApi.install).toHaveBeenNthCalledWith(1, 'steam:1', ['core']);
    expect(currentApi.install).toHaveBeenNthCalledWith(2, 'steam:1', ['core']);
    expect(currentApi.availability).toHaveBeenCalledWith('steam:1');
  });

  it('does not call the install command when the shared install warning is rejected', async () => {
    const currentApi = fakeApi();
    const requireInstallSafetyTokens = vi.fn(() => Promise.resolve(null));
    const store = createOptiScalerStore({ api: currentApi, requireInstallSafetyTokens });

    await expect(store.install('steam:1', ['core'])).resolves.toBe('skipped');

    expect(requireInstallSafetyTokens).toHaveBeenCalledWith('steam:1', 'game');
    expect(currentApi.install).not.toHaveBeenCalled();
  });

  it('acquires a fresh game-scoped safety token for every file mutation', async () => {
    const currentApi = fakeApi();
    const requireSafetyTokens = vi.fn(() =>
      Promise.resolve({ gameContextToken: 'fresh-game-token' }),
    );
    const store = createOptiScalerStore({ api: currentApi, requireSafetyTokens });
    await store.load('steam:1');

    await store.install('steam:1', ['core']);
    await store.repair('steam:1');

    expect(requireSafetyTokens).toHaveBeenNthCalledWith(1, 'steam:1', 'game');
    expect(requireSafetyTokens).toHaveBeenNthCalledWith(2, 'steam:1', 'game');
    expect(currentApi.install).toHaveBeenCalledWith('steam:1', ['core'], 'fresh-game-token');
    expect(currentApi.repair).toHaveBeenCalledWith('steam:1', 'fresh-game-token');
  });

  it('does not return to a previous game when its mutation finishes after navigation', async () => {
    let resolveInstall!: (result: OptiScalerOperationResult) => void;
    const install = new Promise<OptiScalerOperationResult>((resolve) => {
      resolveInstall = resolve;
    });
    const availabilityApi = vi.fn<OptiScalerApi['availability']>((gameId) =>
      Promise.resolve(availability(gameId)),
    );
    const api = fakeApi({
      availability: availabilityApi,
      install: vi.fn<OptiScalerApi['install']>(() => install),
    });
    const store = createOptiScalerStore({ api });
    await store.load('steam:1');

    const installing = store.install('steam:1', ['core']);
    await store.load('steam:2');
    resolveInstall(operation('steam:1'));

    expect(await installing).toBe('ok');
    expect(store.report?.game_id).toBe('steam:2');
    expect(store.report?.install.installed).toBe(false);
    expect(store.report?.install.release).toBeNull();
    expect(store.state).toBeNull();
    expect(availabilityApi).toHaveBeenCalledTimes(2);
  });

  it('keeps an in-flight mutation busy across deactivation and reactivation', async () => {
    let resolveInstall!: (result: OptiScalerOperationResult) => void;
    const install = new Promise<OptiScalerOperationResult>((resolve) => {
      resolveInstall = resolve;
    });
    const api = fakeApi({ install: vi.fn<OptiScalerApi['install']>(() => install) });
    const store = createOptiScalerStore({ api });
    await store.load('steam:1');

    const installing = store.install('steam:1', ['core']);
    store.deactivate();
    await store.load('steam:2');

    expect(store.busy).toBe(true);
    expect(await store.repair('steam:2')).toBe('skipped');

    resolveInstall(operation('steam:1'));
    expect(await installing).toBe('ok');
    expect(store.busy).toBe(false);
  });

  it('triggers onGameDetailsInvalidate after successful mutation commit', async () => {
    const api = fakeApi();
    const onGameDetailsInvalidate = vi.fn();
    const store = createOptiScalerStore({ api, onGameDetailsInvalidate });

    await store.install('steam:1', ['core']);
    expect(onGameDetailsInvalidate).toHaveBeenCalledTimes(1);
    expect(onGameDetailsInvalidate).toHaveBeenCalledWith('steam:1');

    await store.uninstall('steam:1');
    expect(onGameDetailsInvalidate).toHaveBeenCalledTimes(2);
    expect(onGameDetailsInvalidate).toHaveBeenLastCalledWith('steam:1');
  });

  it('awaits onGameDetailsInvalidate and handles rejected promises gracefully', async () => {
    const api = fakeApi();
    let invalidateSettled = false;
    const onGameDetailsInvalidate = vi.fn(
      () =>
        new Promise<void>((_, reject) => {
          setTimeout(() => {
            invalidateSettled = true;
            reject(new Error('invalidation error'));
          }, 10);
        }),
    );
    const store = createOptiScalerStore({ api, onGameDetailsInvalidate });

    const result = await store.install('steam:1', ['core']);
    expect(invalidateSettled).toBe(true);
    expect(result).toBe('ok');
    expect(store.busy).toBe(false);
  });

  it('projects uninstalled state immediately after backend mutation before reload finishes', async () => {
    let resolveReload!: (report: OptiScalerAvailability) => void;
    const reload = new Promise<OptiScalerAvailability>((resolve) => {
      resolveReload = resolve;
    });
    let callCount = 0;
    const availabilityApi = vi.fn<OptiScalerApi['availability']>(() => {
      callCount += 1;
      return callCount === 1
        ? Promise.resolve(
            buildOptiScalerAvailability({
              game_id: 'steam:1',
              install: { installed: true, release: 'v0.9.3' },
            }),
          )
        : reload;
    });
    const uninstallApi = vi.fn<OptiScalerApi['uninstall']>(() =>
      Promise.resolve(
        buildOptiScalerOperationResult({
          installed: false,
          release: null,
          changed_count: 1,
        }),
      ),
    );
    const api = fakeApi({ availability: availabilityApi, uninstall: uninstallApi });
    const store = createOptiScalerStore({ api });
    await store.load('steam:1');
    expect(store.state).not.toBeNull();
    expect(store.state?.installed).toBe(true);

    const uninstalling = store.uninstall('steam:1');
    await vi.waitFor(() => {
      expect(uninstallApi).toHaveBeenCalledOnce();
      expect(store.state).toBeNull();
      expect(store.report?.install.installed).toBe(false);
    });

    resolveReload(
      buildOptiScalerAvailability({
        game_id: 'steam:1',
        install: { installed: false, release: null },
      }),
    );
    await uninstalling;
    expect(store.state).toBeNull();
  });

  it('projects installed state immediately after install mutation before reload finishes', async () => {
    let resolveReload!: (report: OptiScalerAvailability) => void;
    const reload = new Promise<OptiScalerAvailability>((resolve) => {
      resolveReload = resolve;
    });
    let callCount = 0;
    const availabilityApi = vi.fn<OptiScalerApi['availability']>(() => {
      callCount += 1;
      return callCount === 1
        ? Promise.resolve(
            buildOptiScalerAvailability({
              game_id: 'steam:1',
              install: { installed: false, release: null },
            }),
          )
        : reload;
    });
    const installApi = vi.fn<OptiScalerApi['install']>(() =>
      Promise.resolve(
        buildOptiScalerOperationResult({
          installed: true,
          release: 'v0.9.5',
          changed_count: 1,
        }),
      ),
    );
    const api = fakeApi({ availability: availabilityApi, install: installApi });
    const store = createOptiScalerStore({ api });
    await store.load('steam:1');
    expect(store.state).toBeNull();

    const installing = store.install('steam:1', ['core']);
    await vi.waitFor(() => {
      expect(installApi).toHaveBeenCalledOnce();
      expect(store.state).not.toBeNull();
      expect(store.state?.installed).toBe(true);
      expect(store.state?.release).toBe('v0.9.5');
    });

    resolveReload(
      buildOptiScalerAvailability({
        game_id: 'steam:1',
        install: { installed: true, release: 'v0.9.5' },
      }),
    );
    await installing;
    expect(store.state?.installed).toBe(true);
  });

  it('resets checkingUpdates to false when checkUpdate rejects', async () => {
    let rejectCheck!: (error: Error) => void;
    const checkPromise = new Promise<never>((_, reject) => {
      rejectCheck = reject;
    });
    const checkUpdateApi = vi.fn(() => checkPromise);
    const api = fakeApi({ checkUpdate: checkUpdateApi });
    const store = createOptiScalerStore({ api });

    expect(store.checkingUpdates).toBe(false);

    const checking = store.checkForUpdates('steam:1');
    expect(store.checkingUpdates).toBe(true);
    expect(store.busy).toBe(true);

    rejectCheck(new Error('network error'));
    const result = await checking;

    expect(result).toBe('failed');
    expect(store.checkingUpdates).toBe(false);
    expect(store.busy).toBe(false);
  });

  it('skips concurrent checkForUpdates without resetting an in-flight check', async () => {
    let resolveCheck!: () => void;
    const checkPromise = new Promise<{
      overall: 'current';
      installed_release: null;
      available_release: null;
      update_available: boolean;
      repair_required: boolean;
      drifted: boolean;
    }>((resolve) => {
      resolveCheck = () => {
        resolve({
          overall: 'current',
          installed_release: null,
          available_release: null,
          update_available: false,
          repair_required: false,
          drifted: false,
        });
      };
    });
    const checkUpdateApi = vi.fn(() => checkPromise);
    const api = fakeApi({ checkUpdate: checkUpdateApi });
    const store = createOptiScalerStore({ api });

    const first = store.checkForUpdates('steam:1');
    expect(store.checkingUpdates).toBe(true);

    const second = await store.checkForUpdates('steam:1');
    expect(second).toBe('skipped');
    // Ensure in-flight check state is preserved
    expect(store.checkingUpdates).toBe(true);
    expect(store.busy).toBe(true);

    resolveCheck();
    await first;

    expect(store.checkingUpdates).toBe(false);
    expect(store.busy).toBe(false);
  });
});
