import type { Freshness, HostDetection, HostFacts, ReshadeChannel, UpdateStatus } from './types';

/** Shared host fields every tool's availability wire report carries. */
export type HostSnapshotWire = {
  host_detection: HostDetection;
  host_facts: HostFacts;
  actions: unknown;
};

/** CamelCase host snapshot core shared by Luma/RenoDX store helpers. */
export type HostSnapshotCore<TActions> = {
  hostDetection: HostDetection;
  hostFacts: HostFacts;
  actions: TActions;
};

/** Maps shared host wire fields into the store snapshot core. */
export function mapHostSnapshotCore<TActions>(
  report: HostSnapshotWire & { actions: TActions },
): HostSnapshotCore<TActions> {
  return {
    hostDetection: report.host_detection,
    hostFacts: report.host_facts,
    actions: report.actions,
  };
}

/**
 * Builds a tool availability snapshot from shared host fields plus tool-only
 * extras. Keeps Luma/RenoDX store helpers as thin field maps.
 */
export function mapAvailabilitySnapshot<TActions, TExtra extends object>(
  report: HostSnapshotWire & { actions: TActions },
  extra: TExtra,
): HostSnapshotCore<TActions> & TExtra {
  return {
    ...mapHostSnapshotCore(report),
    ...extra,
  };
}

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
    };

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

/**
 * Starts a load and invalidates every in-flight request (including mutations).
 * Clears `busy` so navigation / reload owns the UI chrome: a discarded mutation
 * cannot leave the spinner stuck after its token is superseded.
 *
 * Navigation loads (`preserveLoadError === false`) clear install chrome
 * (`state` / `loaded`) so switching games cannot flash the previous game's
 * installed panel while the new availability is in flight. Same-game retry
 * (`preserveLoadError === true`) retains prior state/error so installed chrome
 * stays honest under a load failure.
 */
export function withLoadBegin<TState, TUpdateReport>(
  core: AddonCoreSnapshot<TState, TUpdateReport>,
  preserveLoadError = false,
): SnapshotTransition<TState, TUpdateReport> {
  const token = nextRequestId(core.requestId);
  const retainChrome = preserveLoadError;
  return {
    token,
    next: {
      ...core,
      requestId: token,
      loading: true,
      busy: false,
      // Navigation: drop previous game chrome. Retry: keep installed + error.
      state: retainChrome ? core.state : null,
      loaded: retainChrome ? core.loaded : false,
      loadError: preserveLoadError ? core.loadError : null,
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
    loadError: null,
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

/** Starts a mutation and invalidates every in-flight load/probe. Stale
 * requests can no longer overwrite the mutation, and their spinners are
 * cleared because their `finally` blocks will intentionally be token-stale. */
export function withMutationBegin<TState, TUpdateReport>(
  core: AddonCoreSnapshot<TState, TUpdateReport>,
): SnapshotTransition<TState, TUpdateReport> {
  const token = nextRequestId(core.requestId);
  return {
    token,
    next: {
      ...core,
      requestId: token,
      busy: true,
      loading: false,
      updateProbing: false,
    },
  };
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

/** Default (nothing detected yet) host facts, before an availability report
 * has loaded. `defaultChannel` is the tool's own default ReShade channel
 * (RenoDX: `stable`; Luma: always `nightly`, since it has no channel switch). */
export function defaultHostFacts(defaultChannel: ReshadeChannel): HostFacts {
  return {
    slot: null,
    active: false,
    path: null,
    version: null,
    addon_support: 'unknown',
    channel: {
      selected: defaultChannel,
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
