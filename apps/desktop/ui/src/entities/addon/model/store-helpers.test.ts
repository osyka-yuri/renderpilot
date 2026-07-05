import { describe, expect, it } from 'vitest';

import type { AddonInstallStateBase, FreshnessSource } from './store-helpers';
import {
  beginRequest,
  canonicalizeInstallState,
  createInitialAddonCoreSnapshot,
  deriveFreshness,
  materializeMutationCommit,
  normalizeInstallState,
  withBusy,
  withLoadBegin,
  withLoadSuccess,
  withMutationCommit,
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
      addon_tracked: boolean | null;
      extra: string;
    };
    const installed: ToolState = {
      status: 'installed',
      addon_dated: 'Wed, 01 Jan 2025',
      installed_at: 1_700_000_000_000,
      updated_at: 1_700_000_500_000,
      addon_tracked: true,
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
    addon_tracked: true,
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
    addon_tracked: true,
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

  it('withLoadBegin bumps the token and clears probe fields without mutating prior', () => {
    const prior = createInitialAddonCoreSnapshot();
    const { next, token } = withLoadBegin(prior);
    expect(token).toBe(1);
    expect(next.requestId).toBe(1);
    expect(next.loading).toBe(true);
    expect(prior.requestId).toBe(0);
    expect(prior.loading).toBe(false);
  });

  it('withLoadSuccess sets canonical state and loaded', () => {
    const core = withLoadSuccess(createInitialAddonCoreSnapshot(), installed);
    expect(core.state).toEqual(installed);
    expect(core.loaded).toBe(true);
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

describe('normalizeInstallState', () => {
  it('drops timestamps from not_installed (even if present in raw)', () => {
    expect(normalizeInstallState({ status: 'not_installed' })).toEqual({ status: 'not_installed' });
    expect(
      normalizeInstallState({ status: 'not_installed', installed_at: 123, updated_at: 456 }),
    ).toEqual({ status: 'not_installed' });
    expect(
      normalizeInstallState({ status: 'not_installed', installed_at: null, updated_at: null }),
    ).toEqual({ status: 'not_installed' });
  });

  it('preserves numeric timestamps on installed', () => {
    const raw = {
      status: 'installed',
      addon_dated: 'Wed, 01 Jan 2025',
      installed_at: 1_700_000_000_000,
      updated_at: 1_700_000_500_000,
      addon_tracked: true,
    };
    expect(normalizeInstallState(raw)).toEqual({
      status: 'installed',
      addon_dated: 'Wed, 01 Jan 2025',
      installed_at: 1_700_000_000_000,
      updated_at: 1_700_000_500_000,
      addon_tracked: true,
    });
  });

  it('JSON roundtrip of normalized not_installed never contains timestamp keys', () => {
    const clean = normalizeInstallState({
      status: 'not_installed',
      installed_at: 123,
      updated_at: 456,
    });
    expect(JSON.stringify(clean)).not.toContain('installed_at');
    expect(JSON.stringify(clean)).not.toContain('updated_at');
  });

  it('throws on installed state with missing or non-numeric timestamps (strict, no legacy)', () => {
    expect(() =>
      normalizeInstallState({ status: 'installed', installed_at: null, updated_at: 123 }),
    ).toThrow(/numeric installed_at and updated_at/);

    expect(() =>
      normalizeInstallState({ status: 'installed', installed_at: 123, updated_at: undefined }),
    ).toThrow(/numeric installed_at and updated_at/);
  });
});
