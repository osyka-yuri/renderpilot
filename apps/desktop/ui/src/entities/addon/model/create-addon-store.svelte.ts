import { formatPresentedError } from '@shared/error-presentation';
import { parseHttpDateTimestamp } from '@shared/date';
import { t, type MessageKeyWithoutParams } from '@shared/i18n';
import { publishPresentedErrorNotification } from '@shared/notifications';

import {
  runBusyMutation as runBusyMutationImpl,
  type BusyMutationContext,
  type BusyMutationOptions,
  type CheckUpdateKind,
  type PostMutationProbe,
} from './busy-mutation';
import {
  beginRequest,
  createInitialAddonCoreSnapshot,
  deriveFreshness,
  withDeactivation,
  withLoadBegin,
  withLoadError,
  withLoadSuccess,
  withLoading,
  withMutationCommit,
  withProbeBegin,
  withProbeEnd,
  withProbeFailure,
  withProbeSuccess,
  type AddonInstallStateBase,
  type FreshnessSource,
} from './store-helpers';
import type { MutationSafetyScope, UpdateStatus } from './types';

export type AddonStoreApi<
  TState extends AddonInstallStateBase,
  TUpdateReport extends FreshnessSource,
  TAvailabilityReport extends { state: TState },
> = {
  getAvailability: (gameId: string) => Promise<TAvailabilityReport>;
  checkUpdate: (gameId: string, kind: CheckUpdateKind) => Promise<TUpdateReport>;
};

export type AddonStoreMessages = {
  loadFailed: MessageKeyWithoutParams;
};

export type CreateAddonStoreConfig<
  TState extends AddonInstallStateBase,
  TUpdateReport extends FreshnessSource,
  TAvailabilityReport extends { state: TState },
> = {
  api: AddonStoreApi<TState, TUpdateReport, TAvailabilityReport>;
  messages: AddonStoreMessages;
  onExclusivityChange?: (gameId: string) => void;
  /** Tool-specific fields to apply after `state` is set from a load report. */
  applyLoadReport: (report: TAvailabilityReport) => void;
  /** Tool-specific host snapshot refresh after a mutation (local scan). */
  applyHostRefresh: (report: TAvailabilityReport) => void;
  /**
   * Clears tool-owned state when navigation starts or the store is deactivated.
   * A normalized game id permits same-game cache retention; `null` requires a
   * complete reset. Explicit retry retains the current tool state.
   */
  resetToolState?: (gameId: string | null) => void;
  buildUpdateReportForInstall: (nextState: TState) => TUpdateReport | null;
  buildProbeFailureReport: () => TUpdateReport;
  /** Tool-specific tracked sources for the untracked freshness branch (e.g. dgvoodoo, dlssFix). */
  freshnessExtraSources?: (report: TUpdateReport) => readonly (UpdateStatus | null)[];
  /**
   * Optional merge of a probe result with the report already on the store
   * (e.g. optimistic post-install "current"). Default: use the probe as-is.
   */
  coalesceUpdateReport?: (previous: TUpdateReport | null, probed: TUpdateReport) => TUpdateReport;
  /**
   * Default probe policy after a successful mutation when `probeUpdates` is
   * omitted on the call. Defaults to `never`.
   */
  postMutationProbe?: PostMutationProbe;
  /**
   * Optional page/game-details side effect after every successful mutation
   * (host refresh + optional probe + tool `afterCommit`). Token-guarded by the
   * mutation flow; Luma uses this for cascade library invalidation.
   */
  onMutationSideEffect?: (gameId: string, token: number) => void | Promise<void>;
  onMutationError?: (error: unknown, scope: MutationSafetyScope) => void;
};

export type AddonStoreCore<
  TState extends AddonInstallStateBase,
  TUpdateReport extends FreshnessSource,
> = ReturnType<typeof createAddonStore<TState, TUpdateReport, { state: TState }>>;

/**
 * Shared reactive skeleton for per-game add-on stores: request-token guarding,
 * load/probe flow, post-mutation commit, and common install/update timestamp
 * getters. Tool stores compose this and add their own availability fields,
 * derived outcome getters, and mutation entry points.
 *
 * Internal state is a single immutable `AddonCoreSnapshot`; every transition
 * reassigns `core = next` via pure helpers in `store-helpers`.
 */
export function createAddonStore<
  TState extends AddonInstallStateBase,
  TUpdateReport extends FreshnessSource,
  TAvailabilityReport extends { state: TState },
>(config: CreateAddonStoreConfig<TState, TUpdateReport, TAvailabilityReport>) {
  const {
    api,
    messages,
    onExclusivityChange,
    applyLoadReport,
    applyHostRefresh,
    resetToolState,
    buildUpdateReportForInstall,
    buildProbeFailureReport,
    freshnessExtraSources,
    coalesceUpdateReport,
    postMutationProbe = 'never',
    onMutationSideEffect,
    onMutationError,
  } = config;

  // `$state.raw`: whole-snapshot replace only (immutable reducers). Avoids deep
  // proxy tracking on a structure we never mutate in place.
  let core = $state.raw(createInitialAddonCoreSnapshot<TState, TUpdateReport>());
  let safetyContextError = $state.raw<unknown>(null);

  const isInstalled = $derived(core.state?.status === 'installed');
  const updateAvailable = $derived(
    core.updateReport?.overall === 'available' || core.updateReport?.overall === 'channel_mismatch',
  );
  const addonUpdate = $derived(core.updateReport?.addon ?? null);
  const hostUpdate = $derived(core.updateReport?.host ?? null);
  const addonDated = $derived(
    core.state?.status === 'installed' ? parseHttpDateTimestamp(core.state.addon_dated) : null,
  );
  const installedAt = $derived(core.state?.status === 'installed' ? core.state.installed_at : null);
  const updatedAt = $derived(core.state?.status === 'installed' ? core.state.updated_at : null);
  const freshness = $derived.by(() =>
    deriveFreshness(
      core.updateProbing,
      core.probeFailed,
      core.updateReport,
      core.updateReport && freshnessExtraSources ? freshnessExtraSources(core.updateReport) : [],
    ),
  );

  function isCurrentRequest(token: number): boolean {
    return token === core.requestId;
  }

  function notifyExclusivityChange(gameId: string): void {
    onExclusivityChange?.(gameId);
  }

  async function loadAvailability(gameId: string, preserveLoadError: boolean): Promise<void> {
    const normalizedGameId = gameId.trim();
    const { next, token } = withLoadBegin(core, preserveLoadError);
    core = next;
    // Navigation loads clear tool chrome so outcome flags from the previous game
    // cannot drive the card while the new game's availability is in flight.
    if (!preserveLoadError) {
      resetToolState?.(normalizedGameId);
    }
    let succeeded = false;
    try {
      const report = await api.getAvailability(normalizedGameId);
      if (token !== core.requestId) {
        return;
      }
      core = withLoadSuccess(core, report.state);
      applyLoadReport(report);
      succeeded = true;
    } catch (error) {
      if (token !== core.requestId) {
        return;
      }
      const loadError = formatPresentedError(error);
      core = withLoadError(core, loadError);
      publishPresentedErrorNotification(t(messages.loadFailed), error);
    } finally {
      if (token === core.requestId) {
        core = withLoading(core, false);
      }
    }

    if (!succeeded) {
      return;
    }
    await probeUpdateStatus(normalizedGameId, token, 'passive');
  }

  async function load(gameId: string): Promise<void> {
    await loadAvailability(gameId, false);
  }

  /** Invalidates pending work and clears core state without issuing I/O. */
  function deactivate(): void {
    core = withDeactivation(core);
    resetToolState?.(null);
  }

  /** Keeps the previous failure visible while this explicit retry is in progress. */
  async function retry(gameId: string): Promise<void> {
    await loadAvailability(gameId, true);
  }

  /**
   * Applies a successful mutation response and returns the request token for
   * the ordered post-commit sequence.
   */
  function commitMutationResult(nextState: TState): number {
    const { next, token } = withMutationCommit(core, nextState, buildUpdateReportForInstall);
    core = next;
    return token;
  }

  /**
   * Re-reads host/availability after a mutation (local scan, no upstream probe).
   * Best-effort and token-guarded; does not overwrite the committed install `state`.
   */
  async function refreshHostInfo(gameId: string, token: number): Promise<void> {
    try {
      const report = await api.getAvailability(gameId);
      if (token === core.requestId) {
        applyHostRefresh(report);
      }
    } catch {
      // Best-effort: a failed host refresh leaves the committed install state in place.
    }
  }

  async function probeUpdateStatus(
    gameId: string,
    token: number,
    kind: CheckUpdateKind,
  ): Promise<void> {
    if (token !== core.requestId || core.state?.status !== 'installed') {
      return;
    }
    // Idempotent: post-mutation path may already have set updateProbing so
    // freshness stays `checking` across refreshHostInfo.
    if (!core.updateProbing) {
      core = withProbeBegin(core);
    }
    try {
      const report = await api.checkUpdate(gameId, kind);
      if (token === core.requestId) {
        const previous = core.updateReport;
        const resolved = coalesceUpdateReport?.(previous, report) ?? report;
        core = withProbeSuccess(core, resolved);
      }
    } catch {
      if (token === core.requestId) {
        core = withProbeFailure(core, buildProbeFailureReport());
      }
    } finally {
      if (token === core.requestId) {
        core = withProbeEnd(core);
      }
    }
  }

  async function checkForUpdates(gameId: string): Promise<void> {
    const { next, token } = beginRequest(core);
    core = next;
    await probeUpdateStatus(gameId, token, 'user');
  }

  const mutationCtx: BusyMutationContext<TState, TUpdateReport> = {
    getCore: () => core,
    setCore: (next) => {
      core = next;
    },
    getUpdateAvailable: () => updateAvailable,
    commitMutationResult,
    refreshHostInfo,
    probeUpdateStatus,
    notifyExclusivityChange,
    postMutationProbe,
    onMutationSideEffect,
    onMutationError: (error, scope) => {
      safetyContextError = error;
      onMutationError?.(error, scope);
    },
  };

  async function runBusyMutation(
    gameId: string,
    fn: () => Promise<TState>,
    options: BusyMutationOptions,
  ) {
    safetyContextError = null;
    return runBusyMutationImpl(mutationCtx, gameId, fn, options);
  }

  return {
    get state() {
      return core.state;
    },
    get loading() {
      return core.loading;
    },
    get loaded() {
      return core.loaded;
    },
    get busy() {
      return core.busy;
    },
    get loadError() {
      return core.loadError;
    },
    get updateReport() {
      return core.updateReport;
    },
    get updateStatus() {
      return core.updateReport?.overall ?? null;
    },
    get updateProbing() {
      return core.updateProbing;
    },
    get freshness() {
      return freshness;
    },
    get lastCheckedAt() {
      return core.lastCheckedAt;
    },
    get isInstalled() {
      return isInstalled;
    },
    get addonDated() {
      return addonDated;
    },
    get installedAt() {
      return installedAt;
    },
    get updatedAt() {
      return updatedAt;
    },
    get addonUpdate() {
      return addonUpdate;
    },
    get hostUpdate() {
      return hostUpdate;
    },
    get updateAvailable() {
      return updateAvailable;
    },
    get requestToken() {
      return core.requestId;
    },
    get safetyContextError() {
      return safetyContextError;
    },
    load,
    deactivate,
    retry,
    checkForUpdates,
    isCurrentRequest,
    runBusyMutation,
    notifyExclusivityChange,
  };
}
