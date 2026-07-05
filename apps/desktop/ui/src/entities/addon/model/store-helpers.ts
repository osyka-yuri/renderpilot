import type { Freshness, HostFacts, ReshadeChannel, UpdateStatus } from './types';

/**
 * Minimal install-state shape every add-on tool shares.
 *
 * **Strict honest modeling (no legacy, no compromises)**:
 * - `not_installed` is pure — it must never carry installation timestamps.
 * - `installed` **always** has numeric `installed_at` and `updated_at`.
 * Tool-specific states are the full IO shapes; this base + canonicalize
 * guarantee that business logic, stores and pages only ever see strict shapes.
 */
export type AddonInstallStateBase =
  | { status: 'not_installed' }
  | {
      status: 'installed';
      addon_dated: string | null;
      installed_at: number;
      updated_at: number;
      addon_tracked: boolean | null;
    };

/** The installed arm of the base (used for narrowing). */
export type InstalledAddonInstallState = Extract<AddonInstallStateBase, { status: 'installed' }>;

/** The minimal update-report shape {@link deriveFreshness} reads. Every tool's
 * own (larger) update-report type structurally satisfies this. */
export type FreshnessSource = {
  addon: UpdateStatus | null;
  host: UpdateStatus | null;
  overall: UpdateStatus;
};

/**
 * Canonical form of a **typed** install state after load or mutation.
 *
 * - `not_installed` collapses to exactly `{ status: 'not_installed' }` (no stray
 *   timestamp keys), even if the value carried extra runtime properties.
 * - `installed` is returned as-is so tool-specific fields stay intact.
 *
 * Unlike {@link normalizeInstallState}, this does not go through `unknown` and
 * does not strip tool fields from the installed arm.
 *
 * The `as T` on the not_installed rebuild is the one intentional cast in this
 * module: pure `{ status: 'not_installed' }` is always a valid member of any
 * tool state union `T`, but TypeScript cannot prove that for arbitrary `T`.
 */
export function canonicalizeInstallState<T extends AddonInstallStateBase>(state: T): T {
  if (state.status === 'not_installed') {
    return { status: 'not_installed' } as T;
  }
  return state;
}

/**
 * Full reactive core snapshot for the add-on store. Transitions always produce
 * a new object; the Svelte shell only reassigns `core = next`.
 */
export type AddonCoreSnapshot<TState, TUpdateReport> = {
  state: TState | null;
  loading: boolean;
  loaded: boolean;
  busy: boolean;
  loadError: string | null;
  updateReport: TUpdateReport | null;
  updateProbing: boolean;
  probeFailed: boolean;
  lastCheckedAt: number | null;
  requestId: number;
};

/** Result of any transition that also advances the request token. */
export type SnapshotTransition<TState, TUpdateReport> = {
  next: AddonCoreSnapshot<TState, TUpdateReport>;
  token: number;
};

function nextRequestId(requestId: number): number {
  return requestId + 1;
}

export function createInitialAddonCoreSnapshot<
  TState extends AddonInstallStateBase,
  TUpdateReport,
>(): AddonCoreSnapshot<TState, TUpdateReport> {
  return {
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
  };
}

/** Bump the request token; return the next snapshot and the new token. */
export function beginRequest<TState, TUpdateReport>(
  core: AddonCoreSnapshot<TState, TUpdateReport>,
): SnapshotTransition<TState, TUpdateReport> {
  const token = nextRequestId(core.requestId);
  return { next: { ...core, requestId: token }, token };
}

export function withLoadBegin<TState, TUpdateReport>(
  core: AddonCoreSnapshot<TState, TUpdateReport>,
): SnapshotTransition<TState, TUpdateReport> {
  const token = nextRequestId(core.requestId);
  return {
    token,
    next: {
      ...core,
      requestId: token,
      loading: true,
      loadError: null,
      updateReport: null,
      updateProbing: false,
      probeFailed: false,
      lastCheckedAt: null,
    },
  };
}

export function withLoadSuccess<TState extends AddonInstallStateBase, TUpdateReport>(
  core: AddonCoreSnapshot<TState, TUpdateReport>,
  nextState: TState,
): AddonCoreSnapshot<TState, TUpdateReport> {
  return {
    ...core,
    state: canonicalizeInstallState(nextState),
    loaded: true,
  };
}

export function withLoadError<TState, TUpdateReport>(
  core: AddonCoreSnapshot<TState, TUpdateReport>,
  loadError: string,
): AddonCoreSnapshot<TState, TUpdateReport> {
  return { ...core, loadError };
}

export function withLoading<TState, TUpdateReport>(
  core: AddonCoreSnapshot<TState, TUpdateReport>,
  loading: boolean,
): AddonCoreSnapshot<TState, TUpdateReport> {
  return core.loading === loading ? core : { ...core, loading };
}

export function withBusy<TState, TUpdateReport>(
  core: AddonCoreSnapshot<TState, TUpdateReport>,
  busy: boolean,
): AddonCoreSnapshot<TState, TUpdateReport> {
  return core.busy === busy ? core : { ...core, busy };
}

export function withProbeBegin<TState, TUpdateReport>(
  core: AddonCoreSnapshot<TState, TUpdateReport>,
): AddonCoreSnapshot<TState, TUpdateReport> {
  return { ...core, updateProbing: true, probeFailed: false };
}

export function withProbeSuccess<TState, TUpdateReport>(
  core: AddonCoreSnapshot<TState, TUpdateReport>,
  updateReport: TUpdateReport,
): AddonCoreSnapshot<TState, TUpdateReport> {
  return { ...core, updateReport };
}

export function withProbeFailure<TState, TUpdateReport>(
  core: AddonCoreSnapshot<TState, TUpdateReport>,
  updateReport: TUpdateReport,
): AddonCoreSnapshot<TState, TUpdateReport> {
  return { ...core, updateReport, probeFailed: true };
}

export function withProbeEnd<TState, TUpdateReport>(
  core: AddonCoreSnapshot<TState, TUpdateReport>,
  now: number = Date.now(),
): AddonCoreSnapshot<TState, TUpdateReport> {
  return { ...core, updateProbing: false, lastCheckedAt: now };
}

/**
 * Immutable post-mutation core fields: install state, synthetic update report,
 * and cleared load/probe flags. No side effects.
 *
 * Timestamps come from the backend response — the client does not re-stamp.
 */
export type MutationCommitPatch<TState, TUpdateReport> = {
  state: TState;
  loading: false;
  loadError: null;
  updateProbing: false;
  probeFailed: false;
  updateReport: TUpdateReport | null;
  lastCheckedAt: number | null;
};

export function materializeMutationCommit<TState extends AddonInstallStateBase, TUpdateReport>(
  nextState: TState,
  buildUpdateReport: (state: TState) => TUpdateReport | null,
  now: number = Date.now(),
): MutationCommitPatch<TState, TUpdateReport> {
  const state = canonicalizeInstallState(nextState);
  return {
    state,
    loading: false,
    loadError: null,
    updateProbing: false,
    probeFailed: false,
    updateReport: buildUpdateReport(state),
    lastCheckedAt: state.status === 'installed' ? now : null,
  };
}

/** Apply a mutation commit patch and bump the request token. */
export function withMutationCommit<TState extends AddonInstallStateBase, TUpdateReport>(
  core: AddonCoreSnapshot<TState, TUpdateReport>,
  nextState: TState,
  buildUpdateReport: (state: TState) => TUpdateReport | null,
  now: number = Date.now(),
): SnapshotTransition<TState, TUpdateReport> {
  const token = nextRequestId(core.requestId);
  const patch = materializeMutationCommit(nextState, buildUpdateReport, now);
  return {
    token,
    next: {
      ...core,
      ...patch,
      requestId: token,
    },
  };
}

/**
 * Converts untrusted/raw install state (unknown wire shape) into the strict
 * base type. Prefer {@link canonicalizeInstallState} for already-typed IO.
 *
 * - `not_installed` is always exactly `{ status: 'not_installed' }`.
 * - `installed` requires numeric `installed_at` and `updated_at` or throws.
 */
export function normalizeInstallState(raw: unknown): AddonInstallStateBase {
  if (!raw || typeof raw !== 'object') {
    return { status: 'not_installed' };
  }
  const r = raw as Record<string, unknown>;
  if (r.status === 'not_installed') {
    return { status: 'not_installed' };
  }
  if (r.status === 'installed') {
    const addon_dated = typeof r.addon_dated === 'string' ? r.addon_dated : null;
    const installed_at = r.installed_at;
    const updated_at = r.updated_at;
    const addon_tracked = typeof r.addon_tracked === 'boolean' ? r.addon_tracked : null;

    if (typeof installed_at !== 'number' || typeof updated_at !== 'number') {
      throw new Error(
        'normalizeInstallState: "installed" state must have numeric installed_at and updated_at (no legacy nulls allowed)',
      );
    }

    return {
      status: 'installed',
      addon_dated,
      installed_at,
      updated_at,
      addon_tracked,
    };
  }
  return { status: 'not_installed' };
}

/** Default (nothing detected yet) host facts, before an availability report
 * has loaded. `defaultChannel` is the tool's own default ReShade channel
 * (for example RenoDX: `stable`). */
export function defaultHostFacts(defaultChannel: ReshadeChannel): HostFacts {
  return {
    slot: null,
    active: false,
    path: null,
    version: null,
    addon_support: 'unknown',
    channel: {
      selected: defaultChannel,
      effective: defaultChannel,
      detected: null,
    },
    update_status: 'unknown_needs_validation',
    is_custom_build: false,
  };
}

/**
 * Maps the probe state + update report to the single freshness verdict the card
 * renders as a pill. Order matters:
 * - a probe in flight wins, suppressing a transient verdict;
 * - a failed probe reads `unknown` because it writes the same report shape as a
 *   successful untracked probe;
 * - `available` outranks the per-source breakdown.
 */
export function deriveFreshness(
  updateProbing: boolean,
  probeFailed: boolean,
  updateReport: FreshnessSource | null,
  extraSources: readonly (UpdateStatus | null)[] = [],
): Freshness {
  if (updateProbing) {
    return 'checking';
  }
  if (probeFailed || updateReport === null) {
    return 'unknown';
  }
  if (updateReport.overall === 'available' || updateReport.overall === 'channel_mismatch') {
    return 'available';
  }
  const allSources = [updateReport.addon, updateReport.host, ...extraSources];
  if (allSources.every((source) => source === null)) {
    return 'untracked';
  }
  if (updateReport.overall === 'current') {
    return 'current';
  }
  return 'unknown';
}
