import { describe, expect, it, vi } from 'vitest';

vi.mock('@shared/notifications', () => ({
  publishPresentedErrorNotification: vi.fn(),
}));

import { createLumaStore } from './create-luma-store.svelte';
import type { AvailabilityReport, LumaInstallState } from './types';
import {
  availability,
  DGVOODOO_REQUIREMENT,
  fakeApi,
  INSTALLABLE_OUTCOME,
  INSTALLED,
  NOT_INSTALLED_SAFE,
} from './luma-store-test-fixtures';

describe('createLumaStore', () => {
  it('starts empty before loading', () => {
    const store = createLumaStore({ api: fakeApi() });
    expect(store.loaded).toBe(false);
    expect(store.isInstalled).toBe(false);
    expect(store.isInstallable).toBe(false);
  });

  it('load() reflects an installable, safe game', async () => {
    const store = createLumaStore({ api: fakeApi() });
    await store.load('steam:403640');

    expect(store.loaded).toBe(true);
    expect(store.isInstallable).toBe(true);
    expect(store.requiresConfirmation).toBe(false);
    expect(store.risk?.severity).toBe('info');
    expect(store.features).toEqual({ dlss_fsr: 'unknown', hdr: 'unknown' });
  });

  it('surfaces vcredist advisory and torn-install flag from availability', async () => {
    const report: AvailabilityReport = {
      ...NOT_INSTALLED_SAFE,
      vcredist_present: false,
      vcredist_installer_url: 'https://aka.ms/vs/17/release/vc_redist.x86.exe',
      install_torn: true,
    };
    const store = createLumaStore({
      api: fakeApi({ getAvailability: vi.fn(() => Promise.resolve(report)) }),
    });

    await store.load('steam:403640');

    expect(store.vcredistPresent).toBe(false);
    expect(store.vcredistInstallerUrl).toBe('https://aka.ms/vs/17/release/vc_redist.x86.exe');
    expect(store.installTorn).toBe(true);
  });

  it('surfaces launch arguments from the installable outcome before install', async () => {
    const report: AvailabilityReport = {
      ...NOT_INSTALLED_SAFE,
      outcome: { ...INSTALLABLE_OUTCOME, launch_args: ['-dx11'] },
    };
    const store = createLumaStore({
      api: fakeApi({ getAvailability: vi.fn(() => Promise.resolve(report)) }),
    });

    await store.load('steam:403640');

    expect(store.launchArgs).toEqual(['-dx11']);
  });

  it('retains the profile features for an installed game', async () => {
    const report: AvailabilityReport = {
      ...INSTALLED,
      outcome: {
        ...INSTALLABLE_OUTCOME,
        features: { dlss_fsr: 'supported', hdr: 'unsupported' },
      },
    };
    const store = createLumaStore({
      api: fakeApi({ getAvailability: vi.fn(() => Promise.resolve(report)) }),
    });

    await store.load('steam:403640');

    expect(store.isInstalled).toBe(true);
    expect(store.features).toEqual({ dlss_fsr: 'supported', hdr: 'unsupported' });
  });

  it('retains installable profile metadata when resolution drifts while installed', async () => {
    const installable: AvailabilityReport = {
      ...INSTALLED,
      outcome: {
        ...INSTALLABLE_OUTCOME,
        features: { dlss_fsr: 'supported', hdr: 'unsupported' },
        guidance: [{ id: 'g1', kind: 'warning', fallback_text: 'keep me' }],
        external_requirement: DGVOODOO_REQUIREMENT,
      },
    };
    const drifted: AvailabilityReport = {
      ...INSTALLED,
      outcome: { kind: 'unsupported' },
    };
    const getAvailability = vi
      .fn()
      .mockResolvedValueOnce(installable)
      .mockResolvedValueOnce(drifted);
    const store = createLumaStore({
      api: fakeApi({ getAvailability }),
    });

    await store.load('steam:403640');
    await store.load('steam:403640');

    expect(store.isInstalled).toBe(true);
    expect(store.features).toEqual({ dlss_fsr: 'supported', hdr: 'unsupported' });
    expect(store.guidance).toEqual([{ id: 'g1', kind: 'warning', fallback_text: 'keep me' }]);
    expect(store.externalRequirement?.kind).toBe('dgvoodoo2');
  });

  it('deactivate() clears retained profile metadata before same-game reactivation', async () => {
    const installable: AvailabilityReport = {
      ...INSTALLED,
      outcome: {
        ...INSTALLABLE_OUTCOME,
        features: { dlss_fsr: 'supported', hdr: 'unsupported' },
        guidance: [{ id: 'g1', kind: 'warning', fallback_text: 'do not retain me' }],
        external_requirement: DGVOODOO_REQUIREMENT,
      },
      vcredist_present: false,
      install_torn: true,
    };
    const drifted: AvailabilityReport = {
      ...INSTALLED,
      outcome: { kind: 'unsupported' },
    };
    const getAvailability = vi
      .fn()
      .mockResolvedValueOnce(installable)
      .mockResolvedValueOnce(drifted);
    const store = createLumaStore({ api: fakeApi({ getAvailability }) });

    await store.load('steam:403640');
    store.deactivate();

    expect(store.loaded).toBe(false);
    expect(store.isInstalled).toBe(false);
    expect(store.isInstallable).toBe(false);
    expect(store.features).toBeNull();
    expect(store.guidance).toEqual([]);
    expect(store.externalRequirement).toBeNull();
    expect(store.vcredistPresent).toBeNull();
    expect(store.installTorn).toBe(false);

    await store.load('steam:403640');
    expect(store.isInstalled).toBe(true);
    expect(store.features).toBeNull();
    expect(store.guidance).toEqual([]);
    expect(store.externalRequirement).toBeNull();
  });

  it('surfaces an external requirement from the installable outcome', async () => {
    const report: AvailabilityReport = {
      ...NOT_INSTALLED_SAFE,
      outcome: {
        ...INSTALLABLE_OUTCOME,
        external_requirement: DGVOODOO_REQUIREMENT,
      },
    };
    const store = createLumaStore({
      api: fakeApi({ getAvailability: vi.fn(() => Promise.resolve(report)) }),
    });

    await store.load('steam:49520');

    expect(store.externalRequirement?.kind).toBe('dgvoodoo2');
    expect(store.externalRequirement?.version).toBe('2.87.3');
  });

  it('keeps an external requirement available while the game is installed', async () => {
    const report: AvailabilityReport = {
      ...INSTALLED,
      outcome: {
        ...INSTALLABLE_OUTCOME,
        external_requirement: DGVOODOO_REQUIREMENT,
      },
    };
    const store = createLumaStore({
      api: fakeApi({ getAvailability: vi.fn(() => Promise.resolve(report)) }),
    });

    await store.load('steam:49520');

    expect(store.isInstalled).toBe(true);
    expect(store.externalRequirement).toEqual(DGVOODOO_REQUIREMENT);
  });

  it('surfaces launch arguments and the reshade channel from the installed state', async () => {
    const report: AvailabilityReport = {
      ...INSTALLED,
      state: { ...INSTALLED.state, launch_args: ['-dx11'] } as LumaInstallState,
    };
    const store = createLumaStore({
      api: fakeApi({ getAvailability: vi.fn(() => Promise.resolve(report)) }),
    });

    await store.load('steam:403640');

    expect(store.launchArgs).toEqual(['-dx11']);
    expect(store.reshadeChannel).toBe('nightly');
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
        launch_args: [],
        profile: { scope: 'game' },
        features: { dlss_fsr: 'unknown', hdr: 'unknown' },
        guidance: [],
        external_requirement: null,
      },
    });
    const store = createLumaStore({
      api: fakeApi({ getAvailability: vi.fn(() => Promise.resolve(warn)) }),
    });

    await store.load('steam:403640');

    expect(store.requiresConfirmation).toBe(true);
  });

  it('reports blocked by a tracked other-addon record', async () => {
    const blocked: AvailabilityReport = availability({
      state: { status: 'not_installed' },
      outcome: { kind: 'blocked_by_other_addon', other_kind: 'renodx', unmanaged: false },
    });
    const store = createLumaStore({
      api: fakeApi({ getAvailability: vi.fn(() => Promise.resolve(blocked)) }),
    });

    await store.load('steam:403640');

    expect(store.isBlockedByOtherAddon).toBe(true);
    expect(store.otherAddonKind).toBe('renodx');
    expect(store.otherAddonUnmanaged).toBe(false);
  });

  it('reports blocked by unmanaged other-addon files', async () => {
    const blocked: AvailabilityReport = availability({
      state: { status: 'not_installed' },
      outcome: { kind: 'blocked_by_other_addon', other_kind: 'renodx', unmanaged: true },
    });
    const store = createLumaStore({
      api: fakeApi({ getAvailability: vi.fn(() => Promise.resolve(blocked)) }),
    });

    await store.load('steam:403640');

    expect(store.isBlockedByOtherAddon).toBe(true);
    expect(store.otherAddonUnmanaged).toBe(true);
  });

  it('reports Luma-shaped files present with no tracked record', async () => {
    const unmanaged: AvailabilityReport = availability({
      state: { status: 'not_installed' },
      outcome: { kind: 'unmanaged_present' },
    });
    const store = createLumaStore({
      api: fakeApi({ getAvailability: vi.fn(() => Promise.resolve(unmanaged)) }),
    });

    await store.load('steam:403640');

    expect(store.isUnmanagedPresent).toBe(true);
    expect(store.isBlockedByOtherAddon).toBe(false);
  });

  it('records a load error when the backend fails', async () => {
    const api = fakeApi({ getAvailability: vi.fn(() => Promise.reject(new Error('boom'))) });
    const store = createLumaStore({ api });

    await store.load('steam:403640');

    expect(store.loadError).not.toBeNull();
    expect(store.loaded).toBe(false);
    expect(store.loading).toBe(false);
  });

  it('discards a stale load when gameId changes mid-request', async () => {
    const slowGame1 = Promise.withResolvers<AvailabilityReport>();
    const api = fakeApi({
      getAvailability: vi.fn((gameId: string) =>
        gameId === 'game1' ? slowGame1.promise : Promise.resolve(INSTALLED),
      ),
    });
    const store = createLumaStore({ api });

    const load1 = store.load('game1'); // in-flight, unresolved
    await store.load('game2'); // newer, resolves first → installed
    expect(store.isInstalled).toBe(true);

    slowGame1.resolve(NOT_INSTALLED_SAFE);
    await load1;

    // game2's state must survive; the stale game1 response is dropped.
    expect(store.isInstalled).toBe(true);
  });
});
