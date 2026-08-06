import { describe, expect, it } from 'vitest';

import type { AddonInstallStateBase, FreshnessSource } from './store-helpers';
import {
  beginRequest,
  canonicalizeInstallState,
  createInitialAddonCoreSnapshot,
  defaultHostFacts,
  deriveFreshness,
  mapAvailabilitySnapshot,
  mapHostSnapshotCore,
  materializeMutationCommit,
  withBusy,
  withDeactivation,
  withLoadBegin,
  withLoadSuccess,
  withMutationCommit,
  withMutationBegin,
  withProbeBegin,
  withProbeEnd,
  withProbeSuccess,
} from './store-helpers';

/** Mirrors production `buildUpdateReportForInstall` branching. */
function buildUpdateReportForInstall(state: AddonInstallStateBase): FreshnessSource | null {
  return state.status === 'installed'
    ? { addon: 'current', host: 'current', overall: 'current' }
    : null;
}

describe('deriveFreshness', () => {
  const current = (over: Partial<FreshnessSource> = {}): FreshnessSource => ({
    addon: 'current',
    host: 'current',
    overall: 'current',
    ...over,
  });

  it('reports checking while a probe is in flight, regardless of report', () => {
    expect(deriveFreshness(true, false, null)).toBe('checking');
    expect(deriveFreshness(true, false, current())).toBe('checking');
    expect(deriveFreshness(true, true, current())).toBe('checking');
  });

  it('reports unknown on a failed probe or a missing report', () => {
    expect(deriveFreshness(false, true, current())).toBe('unknown');
    expect(
      deriveFreshness(false, true, current({ addon: null, host: null, overall: 'unknown' })),
    ).toBe('unknown');
    expect(deriveFreshness(false, false, null)).toBe('unknown');
  });

  it('reports available when any source changed', () => {
    expect(deriveFreshness(false, false, current({ overall: 'available' }))).toBe('available');
    expect(deriveFreshness(false, false, current({ overall: 'channel_mismatch' }))).toBe(
      'available',
    );
  });

  it('reports untracked only on a successful probe with no tracked sources', () => {
    expect(
      deriveFreshness(false, false, {
        addon: null,
        host: null,
        overall: 'unknown',
      }),
    ).toBe('untracked');

    expect(
      deriveFreshness(
        false,
        false,
        {
          addon: null,
          host: null,
          overall: 'unknown',
        },
        [null],
      ),
    ).toBe('untracked');
  });

  it('does not report untracked when an extra tracked source is present', () => {
    expect(
      deriveFreshness(
        false,
        false,
        {
          addon: null,
          host: null,
          overall: 'current',
        },
        ['current'],
      ),
    ).toBe('current');
  });

  it('reports current when every source is up to date', () => {
    expect(deriveFreshness(false, false, current())).toBe('current');
  });
});

describe('canonicalizeInstallState', () => {
  it('collapses not_installed to a pure shape (strips stray keys)', () => {
    // Extra keys are structural noise, not part of the typed not_installed arm.
    const dirty = Object.assign(
      { status: 'not_installed' as const },
      {
        installed_at: 123,
        updated_at: 456,
      },
    );
    const clean = canonicalizeInstallState(dirty);
    expect(clean).toEqual({ status: 'not_installed' });
    expect(JSON.stringify(clean)).not.toContain('installed_at');
  });

  it('preserves tool-specific fields on installed', () => {
    type ToolState = {
      status: 'installed';
      addon_dated: string | null;
      installed_at: number;
      updated_at: number;
      source_receipt: string;
      extra: string;
    };
    const installed: ToolState = {
      status: 'installed',
      addon_dated: 'Wed, 01 Jan 2025',
      installed_at: 1_700_000_000_000,
      updated_at: 1_700_000_500_000,
      source_receipt: 'receipt',
      extra: 'keep-me',
    };
    expect(canonicalizeInstallState(installed)).toBe(installed);
  });
});

describe('materializeMutationCommit', () => {
  const installed: AddonInstallStateBase = {
    status: 'installed',
    addon_dated: null,
    installed_at: 1_000,
    updated_at: 2_000,
  };

  it('commits installed state with a synthetic update report and lastCheckedAt', () => {
    const patch = materializeMutationCommit<AddonInstallStateBase, FreshnessSource>(
      installed,
      buildUpdateReportForInstall,
      9_999,
    );

    expect(patch).toEqual({
      state: installed,
      loading: false,
      loadError: null,
      updateProbing: false,
      probeFailed: false,
      updateReport: { addon: 'current', host: 'current', overall: 'current' },
      lastCheckedAt: 9_999,
    });
  });

  it('commits uninstall with null report, pure not_installed, and no lastCheckedAt', () => {
    const dirtyNotInstalled = Object.assign(
      { status: 'not_installed' as const },
      {
        installed_at: 1,
      },
    );
    const patch = materializeMutationCommit<AddonInstallStateBase, FreshnessSource>(
      dirtyNotInstalled,
      buildUpdateReportForInstall,
      9_999,
    );

    expect(patch.state).toEqual({ status: 'not_installed' });
    expect(JSON.stringify(patch.state)).not.toContain('installed_at');
    expect(patch.updateReport).toBeNull();
    expect(patch.lastCheckedAt).toBeNull();
  });

  it('uses buildUpdateReport against the canonicalized state', () => {
    const patch = materializeMutationCommit<AddonInstallStateBase, FreshnessSource>(
      { status: 'not_installed' },
      buildUpdateReportForInstall,
    );
    expect(patch.updateReport).toBeNull();
  });
});

describe('AddonCoreSnapshot transitions', () => {
  const currentReport: FreshnessSource = {
    addon: 'current',
    host: 'current',
    overall: 'current',
  };

  const installed: AddonInstallStateBase = {
    status: 'installed',
    addon_dated: null,
    installed_at: 1_000,
    updated_at: 2_000,
  };

  it('createInitialAddonCoreSnapshot starts empty', () => {
    const core = createInitialAddonCoreSnapshot();
    expect(core).toEqual({
      state: null,
      loading: false,
      loaded: false,
      busy: false,
      loadError: null,
      updateReport: null,
      updateProbing: false,
      probeFailed: false,
      lastCheckedAt: null,
      requestId: 0,
    });
  });

  it('withLoadBegin only retains an error for an explicit retry', () => {
    const prior = { ...createInitialAddonCoreSnapshot(), loadError: 'previous failure' };
    const { next, token } = withLoadBegin(prior);
    expect(token).toBe(1);
    expect(next.requestId).toBe(1);
    expect(next.loading).toBe(true);
    expect(next.loadError).toBeNull();
    expect(prior.requestId).toBe(0);
    expect(prior.loading).toBe(false);

    const retry = withLoadBegin(prior, true);
    expect(retry.next.loadError).toBe('previous failure');
  });

  it('withDeactivation resets state and advances the request token', () => {
    const prior = {
      ...createInitialAddonCoreSnapshot<AddonInstallStateBase, FreshnessSource>(),
      state: installed,
      loaded: true,
      busy: true,
      loadError: 'stale',
      updateReport: currentReport,
      requestId: 4,
    };

    const next = withDeactivation(prior);

    expect(next).toEqual({
      ...createInitialAddonCoreSnapshot<AddonInstallStateBase, FreshnessSource>(),
      requestId: 5,
    });
    expect(prior.requestId).toBe(4);
    expect(prior.state).toBe(installed);
  });

  it('withLoadBegin clears install chrome on navigation load', () => {
    const prior = {
      ...createInitialAddonCoreSnapshot<AddonInstallStateBase, FreshnessSource>(),
      state: installed,
      loaded: true,
      requestId: 2,
    };
    const { next } = withLoadBegin(prior);
    expect(next.state).toBeNull();
    expect(next.loaded).toBe(false);
    expect(next.loading).toBe(true);
    expect(next.requestId).toBe(3);
  });

  it('withLoadBegin retains install chrome on same-game retry', () => {
    const prior = {
      ...createInitialAddonCoreSnapshot<AddonInstallStateBase, FreshnessSource>(),
      state: installed,
      loaded: true,
      loadError: 'offline',
      requestId: 2,
    };
    const { next } = withLoadBegin(prior, true);
    expect(next.state).toEqual(installed);
    expect(next.loaded).toBe(true);
    expect(next.loadError).toBe('offline');
    expect(next.loading).toBe(true);
  });

  it('withLoadBegin clears busy so a superseded mutation cannot leave a stuck spinner', () => {
    const prior = { ...createInitialAddonCoreSnapshot(), busy: true, requestId: 3 };
    const { next } = withLoadBegin(prior);
    expect(next.busy).toBe(false);
    expect(next.loading).toBe(true);
    expect(next.requestId).toBe(4);
  });

  it('withLoadSuccess sets canonical state, loaded, and clears a retained error', () => {
    const core = withLoadSuccess(
      { ...createInitialAddonCoreSnapshot(), loadError: 'previous failure' },
      installed,
    );
    expect(core.state).toEqual(installed);
    expect(core.loaded).toBe(true);
    expect(core.loadError).toBeNull();
  });

  it('beginRequest only bumps the token', () => {
    const prior = createInitialAddonCoreSnapshot();
    const { next, token } = beginRequest(prior);
    expect(token).toBe(1);
    expect(next).toEqual({ ...prior, requestId: 1 });
  });

  it('withMutationCommit merges patch, bumps token, preserves loaded/busy', () => {
    let core = withLoadSuccess(
      { ...createInitialAddonCoreSnapshot<AddonInstallStateBase, FreshnessSource>(), loaded: true },
      installed,
    );
    core = withBusy(core, true);
    const { next, token } = withMutationCommit(
      core,
      { status: 'not_installed' },
      buildUpdateReportForInstall,
      9_999,
    );

    expect(token).toBe(1);
    expect(next.state).toEqual({ status: 'not_installed' });
    expect(next.loaded).toBe(true);
    expect(next.busy).toBe(true);
    expect(next.loading).toBe(false);
    expect(next.updateReport).toBeNull();
    expect(next.lastCheckedAt).toBeNull();
    expect(next.requestId).toBe(1);
    // prior snapshot unchanged
    expect(core.state?.status).toBe('installed');
  });

  it('withMutationBegin invalidates requests and owns busy/loading state', () => {
    const prior = {
      ...createInitialAddonCoreSnapshot(),
      loading: true,
      updateProbing: true,
    };

    const { next, token } = withMutationBegin(prior);

    expect(token).toBe(1);
    expect(next.requestId).toBe(1);
    expect(next.busy).toBe(true);
    expect(next.loading).toBe(false);
    expect(next.updateProbing).toBe(false);
  });

  it('probe helpers replace fields immutably', () => {
    const started = withProbeBegin(createInitialAddonCoreSnapshot());
    expect(started.updateProbing).toBe(true);
    expect(started.probeFailed).toBe(false);

    const success = withProbeSuccess(started, currentReport);
    expect(success.updateReport).toEqual(currentReport);

    const ended = withProbeEnd(success, 42);
    expect(ended.updateProbing).toBe(false);
    expect(ended.lastCheckedAt).toBe(42);
    expect(started.updateProbing).toBe(true);
  });
});

describe('mapHostSnapshotCore / mapAvailabilitySnapshot', () => {
  const wire = {
    host_detection: 'absent' as const,
    host_facts: defaultHostFacts('nightly'),
    actions: { install: null, uninstall: null },
  };

  it('maps shared host wire fields into camelCase core', () => {
    expect(mapHostSnapshotCore(wire)).toEqual({
      hostDetection: 'absent',
      hostFacts: wire.host_facts,
      actions: wire.actions,
    });
  });

  it('merges tool-specific extras without dropping the host core', () => {
    expect(
      mapAvailabilitySnapshot(wire, {
        installTorn: true,
        vcredistPresent: false,
      }),
    ).toEqual({
      hostDetection: 'absent',
      hostFacts: wire.host_facts,
      actions: wire.actions,
      installTorn: true,
      vcredistPresent: false,
    });
  });
});
