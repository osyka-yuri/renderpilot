import { describe, expect, it, vi } from 'vitest';

vi.mock('@shared/notifications', () => ({
  publishPresentedErrorNotification: vi.fn(),
}));

import { publishPresentedErrorNotification } from '@shared/notifications';
import { createLumaStore } from './create-luma-store.svelte';
import type { LumaUpdateReport } from './types';
import {
  availability,
  DGVOODOO_REQUIREMENT,
  fakeApi,
  INSTALLABLE_OUTCOME,
  INSTALLED,
} from './luma-store-test-fixtures';

describe('createLumaStore', () => {
  it('install() passes the confirmation flag and refreshes state', async () => {
    const api = fakeApi();
    const store = createLumaStore({ api });

    const ok = await store.install('steam:403640', true);

    expect(ok).toBe('ok');
    expect(api.install).toHaveBeenCalledWith('steam:403640', true);
    expect(store.isInstalled).toBe(true);
    expect(store.installedAt).not.toBeNull();
    expect(store.updatedAt).not.toBeNull();
  });

  it('install() keeps overall current when passive probe returns unknown', async () => {
    // Passive probes skip multi-MB ZIP digests and often report overall unknown;
    // after a successful install the synthetic report must win so the badge
    // does not flash "Couldn't check".
    const api = fakeApi({
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: null,
          host: null,
          dgvoodoo: null,
          overall: 'unknown',
        } as LumaUpdateReport),
      ),
    });
    const store = createLumaStore({ api });

    const ok = await store.install('steam:403640', false);

    expect(ok).toBe('ok');
    expect(store.freshness).toBe('current');
    expect(store.updateAvailable).toBe(false);
  });

  it('install() adopts dgVoodoo available from passive probe while keeping current overall', async () => {
    const api = fakeApi({
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: null,
          host: null,
          dgvoodoo: 'available',
          overall: 'unknown',
        } as LumaUpdateReport),
      ),
    });
    const store = createLumaStore({ api });

    await store.install('steam:403640', false);

    expect(store.dgvoodooUpdate).toBe('available');
    expect(store.freshness).toBe('available');
  });

  it('install() does not optimistically mark dgVoodoo current from external_requirement', async () => {
    const preInstall = availability({
      state: { status: 'not_installed' },
      outcome: {
        ...INSTALLABLE_OUTCOME,
        external_requirement: DGVOODOO_REQUIREMENT,
      },
    });
    const api = fakeApi({
      getAvailability: vi
        .fn()
        .mockResolvedValueOnce(preInstall)
        .mockResolvedValue(
          availability({
            ...INSTALLED,
            outcome: {
              ...INSTALLABLE_OUTCOME,
              external_requirement: DGVOODOO_REQUIREMENT,
            },
          }),
        ),
      // Post-commit probe populates managed-dependency freshness.
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'current',
          host: 'current',
          dgvoodoo: 'current',
          overall: 'current',
        } as LumaUpdateReport),
      ),
    });
    const store = createLumaStore({ api });
    await store.load('steam:49520');
    expect(store.externalRequirement?.kind).toBe('dgvoodoo2');

    const ok = await store.install('steam:49520', false);

    expect(ok).toBe('ok');
    expect(api.checkUpdate).toHaveBeenCalled();
    expect(store.dgvoodooUpdate).toBe('current');
  });

  it('install() refreshes outcome from host refresh after commit', async () => {
    const preInstall = availability({
      state: { status: 'not_installed' },
      outcome: {
        ...INSTALLABLE_OUTCOME,
        guidance: [{ id: 'pre', kind: 'warning', fallback_text: 'pre-install' }],
        features: { dlss_fsr: 'unknown', hdr: 'unknown' },
      },
    });
    const postInstall = availability({
      ...INSTALLED,
      outcome: {
        ...INSTALLABLE_OUTCOME,
        guidance: [{ id: 'post', kind: 'game_setting', fallback_text: 'post-install' }],
        features: { dlss_fsr: 'supported', hdr: 'supported' },
        external_requirement: DGVOODOO_REQUIREMENT,
      },
    });
    const api = fakeApi({
      getAvailability: vi.fn().mockResolvedValueOnce(preInstall).mockResolvedValue(postInstall),
    });
    const store = createLumaStore({ api });
    await store.load('steam:49520');
    expect(store.features).toEqual({ dlss_fsr: 'unknown', hdr: 'unknown' });

    await store.install('steam:49520', false);

    expect(store.isInstalled).toBe(true);
    expect(store.features).toEqual({ dlss_fsr: 'supported', hdr: 'supported' });
    expect(store.externalRequirement).toEqual(DGVOODOO_REQUIREMENT);
  });

  it('install() resolves false, clears busy, and notifies when the backend fails', async () => {
    vi.mocked(publishPresentedErrorNotification).mockClear();
    const api = fakeApi({ install: vi.fn(() => Promise.reject(new Error('boom'))) });
    const store = createLumaStore({ api });

    const ok = await store.install('steam:403640', false);

    expect(ok).toBe('failed');
    expect(store.busy).toBe(false);
    expect(store.isInstalled).toBe(false);
    expect(publishPresentedErrorNotification).toHaveBeenCalledTimes(1);
  });

  it('surfaces an available update for an installed game', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'available',
          host: 'current',
          dgvoodoo: null,
          overall: 'available',
        } as LumaUpdateReport),
      ),
    });
    const store = createLumaStore({ api });

    await store.load('steam:403640');

    expect(store.updateAvailable).toBe(true);
    expect(store.addonUpdate).toBe('available');
  });

  it('surfaces an available dgVoodoo update for an installed game', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'current',
          host: 'current',
          dgvoodoo: 'available',
          overall: 'available',
        } as LumaUpdateReport),
      ),
    });
    const store = createLumaStore({ api });

    await store.load('steam:49520');

    expect(store.updateAvailable).toBe(true);
    expect(store.dgvoodooUpdate).toBe('available');
  });

  it('update() applies the update and refreshes, clearing the flag', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi
        .fn()
        .mockResolvedValueOnce({
          addon: 'available',
          host: 'current',
          dgvoodoo: null,
          overall: 'available',
        } satisfies LumaUpdateReport)
        // Post-commit probe after a successful update.
        .mockResolvedValue({
          addon: 'current',
          host: 'current',
          dgvoodoo: null,
          overall: 'current',
        } satisfies LumaUpdateReport),
    });
    const store = createLumaStore({ api });
    await store.load('steam:403640');
    expect(store.updateAvailable).toBe(true);

    const ok = await store.update('steam:403640');

    expect(ok).toBe('ok');
    expect(api.update).toHaveBeenCalledWith('steam:403640', { forceFull: false });
    expect(api.checkUpdate).toHaveBeenCalledTimes(2);
    expect(store.updateAvailable).toBe(false);
  });

  it('update() no-ops when no update is available', async () => {
    const api = fakeApi();
    const store = createLumaStore({ api });

    const ok = await store.update('steam:403640');

    expect(ok).toBe('skipped');
    expect(api.update).not.toHaveBeenCalled();
  });

  it('update() resolves false and leaves state untouched when the backend fails', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'available',
          host: 'current',
          dgvoodoo: null,
          overall: 'available',
        } as LumaUpdateReport),
      ),
      update: vi.fn(() => Promise.reject(new Error('boom'))),
    });
    const store = createLumaStore({ api });
    await store.load('steam:403640');
    expect(store.updateAvailable).toBe(true);

    const ok = await store.update('steam:403640');

    expect(ok).toBe('failed');
    expect(store.busy).toBe(false);
    expect(store.updateAvailable).toBe(true);
  });

  it('repair() calls the update backend and refreshes state even with no update pending', async () => {
    const api = fakeApi({ getAvailability: vi.fn(() => Promise.resolve(INSTALLED)) });
    const store = createLumaStore({ api });
    await store.load('steam:403640');
    expect(store.updateAvailable).toBe(false);
    const probesBefore = vi.mocked(api.checkUpdate).mock.calls.length;

    const ok = await store.repair('steam:403640');

    expect(ok).toBe('ok');
    expect(api.update).toHaveBeenCalledWith('steam:403640', { forceFull: true });
    expect(store.isInstalled).toBe(true);
    expect(vi.mocked(api.checkUpdate).mock.calls.length).toBeGreaterThan(probesBefore);
  });

  it('repair() resolves false, clears busy, and notifies when the backend fails', async () => {
    vi.mocked(publishPresentedErrorNotification).mockClear();
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      update: vi.fn(() => Promise.reject(new Error('boom'))),
    });
    const store = createLumaStore({ api });
    await store.load('steam:403640');

    const ok = await store.repair('steam:403640');

    expect(ok).toBe('failed');
    expect(store.busy).toBe(false);
    expect(publishPresentedErrorNotification).toHaveBeenCalledTimes(1);
  });

  it('uninstall() refreshes to not-installed', async () => {
    const api = fakeApi({ getAvailability: vi.fn(() => Promise.resolve(INSTALLED)) });
    const store = createLumaStore({ api });
    await store.load('steam:403640');
    expect(store.isInstalled).toBe(true);

    const ok = await store.uninstall('steam:403640');

    expect(ok).toBe('ok');
    expect(store.isInstalled).toBe(false);
  });

  it('invalidates game component state after every committed Luma mutation', async () => {
    const onGameDetailsInvalidate = vi.fn(() => Promise.resolve());
    const store = createLumaStore({ api: fakeApi(), onGameDetailsInvalidate });

    expect(await store.install('steam:403640', false)).toBe('ok');
    expect(await store.repair('steam:403640')).toBe('ok');
    expect(await store.uninstall('steam:403640')).toBe('ok');

    expect(onGameDetailsInvalidate).toHaveBeenCalledTimes(3);
    expect(onGameDetailsInvalidate).toHaveBeenNthCalledWith(1, 'steam:403640');
    expect(onGameDetailsInvalidate).toHaveBeenNthCalledWith(2, 'steam:403640');
    expect(onGameDetailsInvalidate).toHaveBeenNthCalledWith(3, 'steam:403640');
  });

  it('uninstall() resolves false and leaves the installed state untouched when the backend fails', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      uninstall: vi.fn(() => Promise.reject(new Error('boom'))),
    });
    const store = createLumaStore({ api });
    await store.load('steam:403640');
    expect(store.isInstalled).toBe(true);

    const ok = await store.uninstall('steam:403640');

    expect(ok).toBe('failed');
    expect(store.busy).toBe(false);
    expect(store.isInstalled).toBe(true);
  });

  it('checkForUpdates() records a failed probe when the backend rejects', async () => {
    const api = fakeApi({
      getAvailability: vi.fn(() => Promise.resolve(INSTALLED)),
      checkUpdate: vi.fn(() => Promise.reject(new Error('network down'))),
    });
    const store = createLumaStore({ api });
    await store.load('steam:403640');

    await store.checkForUpdates('steam:403640');

    expect(store.freshness).toBe('unknown');
    expect(store.updateProbing).toBe(false);
  });

  it('checkForUpdates re-probes upstream and re-stamps lastCheckedAt', async () => {
    const api = fakeApi({ getAvailability: vi.fn(() => Promise.resolve(INSTALLED)) });
    const store = createLumaStore({ api });
    await store.load('steam:403640');
    const firstChecked = store.lastCheckedAt;

    await store.checkForUpdates('steam:403640');

    expect(store.lastCheckedAt).not.toBeNull();
    expect(api.checkUpdate).toHaveBeenCalledTimes(2);
    expect(typeof firstChecked).toBe('number');
  });
});
