import { describeCommandErrorTechnical } from '@shared/api';
import { t, type MessageKey } from '@shared/i18n';
import { publishErrorNotification } from '@shared/notifications';
import { clearDownloadProgress } from '@shared/lib';

import {
  beginRequest,
  createInitialAddonCoreSnapshot,
  deriveFreshness,
  withBusy,
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
import type { UpdateStatus } from './types';

export type AddonStoreApi<
  TState extends AddonInstallStateBase,
  TUpdateReport extends FreshnessSource,
  TAvailabilityReport extends { state: TState },
> = {
  getAvailability: (gameId: string) => Promise<TAvailabilityReport>;
  checkUpdate: (gameId: string) => Promise<TUpdateReport>;
};

export type AddonStoreMessages = {
  loadFailed: MessageKey;
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
  buildUpdateReportForInstall: (nextState: TState) => TUpdateReport | null;
  buildProbeFailureReport: () => TUpdateReport;
  /** Tool-specific tracked sources for the untracked freshness branch (e.g. dgvoodoo, dlssFix). */
  freshnessExtraSources?: (report: TUpdateReport) => readonly (UpdateStatus | null)[];
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
    buildUpdateReportForInstall,
    buildProbeFailureReport,
    freshnessExtraSources,
  } = config;

  // `$state.raw`: whole-snapshot replace only (immutable reducers). Avoids deep
  // proxy tracking on a structure we never mutate in place.
  let core = $state.raw(createInitialAddonCoreSnapshot<TState, TUpdateReport>());

  const isInstalled = $derived(core.state?.status === 'installed');
  const updateAvailable = $derived(
    core.updateReport?.overall === 'available' || core.updateReport?.overall === 'channel_mismatch',
  );
  const addonUpdate = $derived(core.updateReport?.addon ?? null);
  const hostUpdate = $derived(core.updateReport?.host ?? null);
  const addonDated = $derived(core.state?.status === 'installed' ? core.state.addon_dated : null);
  const installedAt = $derived(core.state?.status === 'installed' ? core.state.installed_at : null);
  const updatedAt = $derived(core.state?.status === 'installed' ? core.state.updated_at : null);
  const addonTracked = $derived(
    core.state?.status === 'installed' ? core.state.addon_tracked : null,
  );
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

  async function load(gameId: string): Promise<void> {
    const { next, token } = withLoadBegin(core);
    core = next;
    try {
      const report = await api.getAvailability(gameId);
      if (token !== core.requestId) {
        return;
      }
      core = withLoadSuccess(core, report.state);
      applyLoadReport(report);
    } catch (error) {
      if (token !== core.requestId) {
        return;
      }
      const loadError = describeCommandErrorTechnical(error);
      core = withLoadError(core, loadError);
      publishErrorNotification(t(messages.loadFailed), loadError);
    } finally {
      if (token === core.requestId) {
        core = withLoading(core, false);
      }
    }

    await probeUpdateStatus(gameId, token);
  }

  /**
   * Applies a successful mutation response: pure commit into the core snapshot
   * + best-effort host re-scan. Returns the request token for tool `onSuccess` hooks.
   */
  function commitMutationResult(gameId: string, nextState: TState): number {
    const { next, token } = withMutationCommit(core, nextState, buildUpdateReportForInstall);
    core = next;
    void refreshHostInfo(gameId, token);
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

  async function probeUpdateStatus(gameId: string, token: number): Promise<void> {
    if (token !== core.requestId || core.state?.status !== 'installed') {
      return;
    }
    core = withProbeBegin(core);
    try {
      const report = await api.checkUpdate(gameId);
      if (token === core.requestId) {
        core = withProbeSuccess(core, report);
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
    await probeUpdateStatus(gameId, token);
  }

  type BusyMutationOptions = {
    errorKey: MessageKey;
    clearDownloadProgress?: boolean;
    requireUpdateAvailable?: boolean;
    onSuccess?: (token: number) => void | Promise<void>;
    notifyExclusivity?: boolean;
  };

  async function runBusyMutation(
    gameId: string,
    fn: () => Promise<TState>,
    options: BusyMutationOptions,
  ): Promise<boolean> {
    if (core.busy) {
      return false;
    }
    if (options.requireUpdateAvailable && !updateAvailable) {
      return false;
    }
    core = withBusy(core, true);
    if (options.clearDownloadProgress !== false) {
      clearDownloadProgress([gameId]);
    }
    try {
      const nextState = await fn();
      const token = commitMutationResult(gameId, nextState);
      await options.onSuccess?.(token);
      if (options.notifyExclusivity) {
        notifyExclusivityChange(gameId);
      }
      return true;
    } catch (error) {
      publishErrorNotification(t(options.errorKey), describeCommandErrorTechnical(error));
      return false;
    } finally {
      core = withBusy(core, false);
    }
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
    get addonTracked() {
      return addonTracked;
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
    load,
    checkForUpdates,
    isCurrentRequest,
    runBusyMutation,
    notifyExclusivityChange,
  };
}
