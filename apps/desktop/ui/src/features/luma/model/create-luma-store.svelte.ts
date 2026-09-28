import {
  addonCoreApi,
  commonOutcomeApi,
  createAddonStore,
  hostSnapshotApi,
  mergeAddonApis,
  requestMutationSafetyTokens,
  type AddonMutationResult,
  type MutationSafetyCapture,
  type MutationSafetyTokens,
} from '@entities/addon';
import { isFileSafetyContextError } from '@shared/errors';
import { t, type MessageKeyWithoutParams } from '@shared/i18n';
import { publishPresentedErrorNotification } from '@shared/notifications';

import { lumaApi, type LumaApi } from '../api/desktop';
import {
  availabilitySnapshotFromReport,
  defaultHostFacts,
  type AvailabilitySnapshot,
} from './luma-store-helpers';
import type {
  AvailabilityOutcome,
  AvailabilityReport,
  LumaFeatures,
  LumaGuidance,
  LumaInstallState,
  LumaManagedDependencySummary,
  LumaProfile,
  LumaUpdateReport,
} from './types';

/** Reactive store backing the Luma card for a single game. */
export type LumaStore = ReturnType<typeof createLumaStore>;

export type LumaStoreOptions = {
  api?: LumaApi;
  /**
   * Called after a successful install/uninstall changes whether this game
   * blocks RenoDX. The caller owns any peer-store reload; failures there must
   * not make the completed Luma mutation look failed.
   */
  onExclusivityChange?: (gameId: string) => void;
  /**
   * Reloads game-details catalog/component state after Luma mutations that may
   * cascade owned `nvngx_dlss.dll` into library swaps. RenoDX DLSS-Fix is a
   * separate companion add-on and does not need this path.
   */
  onGameDetailsInvalidate?: (gameId: string) => void | Promise<void>;
  requireSafetyTokens?: (gameId: string, scope: 'game') => Promise<MutationSafetyTokens | null>;
};

/**
 * Creates the Luma store. The backend API is injected so tests can drive the
 * store with fakes; production code uses the default Tauri-bound [`lumaApi`].
 */
export function createLumaStore(options: LumaStoreOptions = {}) {
  const api = options.api ?? lumaApi;
  const onExclusivityChange = options.onExclusivityChange;
  const onGameDetailsInvalidate = options.onGameDetailsInvalidate;
  const requireSafetyTokens = options.requireSafetyTokens;
  let safetyContextError = $state.raw<unknown>(null);
  let safetyAttemptGeneration = 0;

  function beginSafetyAttempt(): number {
    safetyContextError = null;
    return ++safetyAttemptGeneration;
  }

  function recordSafetyContextError(owner: number, error: unknown): void {
    if (owner === safetyAttemptGeneration) {
      safetyContextError = isFileSafetyContextError(error) ? error : null;
    }
  }

  function captureSafetyTokens(gameId: string): Promise<MutationSafetyCapture> {
    if (core.busy) {
      return Promise.resolve({ kind: 'cancelled' });
    }

    return requestMutationSafetyTokens(requireSafetyTokens, gameId, 'game');
  }

  function handleSafetyCaptureError(
    error: unknown,
    errorKey: MessageKeyWithoutParams,
    safetyAttempt: number,
  ): AddonMutationResult {
    if (safetyAttempt !== safetyAttemptGeneration) {
      return 'skipped';
    }
    publishPresentedErrorNotification(t(errorKey), error);
    recordSafetyContextError(safetyAttempt, error);
    return 'failed';
  }

  function runWithSafetyTokens(
    gameId: string,
    errorKey: MessageKeyWithoutParams,
    safetyAttempt: number,
    run: (tokens?: MutationSafetyTokens) => Promise<AddonMutationResult>,
  ): Promise<AddonMutationResult> {
    const runAndRecordOutcome = (tokens?: MutationSafetyTokens) =>
      run(tokens).then((result) => {
        recordSafetyContextError(
          safetyAttempt,
          result === 'failed' ? core.safetyContextError : null,
        );
        return result;
      });

    if (!requireSafetyTokens) {
      return runAndRecordOutcome();
    }

    return captureSafetyTokens(gameId).then(
      (safety) => {
        if (safetyAttempt !== safetyAttemptGeneration || safety.kind === 'cancelled') {
          return 'skipped';
        }
        return runAndRecordOutcome(safety.tokens);
      },
      (error: unknown) => handleSafetyCaptureError(error, errorKey, safetyAttempt),
    );
  }

  type RetainedProfileMeta = {
    profile: LumaProfile | null;
    features: LumaFeatures | null;
    guidance: LumaGuidance[];
    externalRequirement: LumaManagedDependencySummary | null;
  };

  let availabilitySnapshot = $state<AvailabilitySnapshot>({
    engineConfig: {
      status: 'not_applicable',
      path: null,
      can_apply: false,
    },
    hostDetection: 'absent',
    hostFacts: defaultHostFacts(),
    actions: {},
    vcredistPresent: null,
    vcredistInstallerUrl: '',
    installTorn: false,
    uninstallBlockedBy: null,
  });
  let outcome = $state<AvailabilityOutcome | null>(null);
  /** Last installable profile metadata — retained while installed if resolution drifts. */
  let retainedProfileMeta = $state<RetainedProfileMeta | null>(null);
  /** Game id of the last navigation load begin — used to keep same-game profile meta. */
  let lastLoadGameId: string | null = null;

  function applyOutcome(report: AvailabilityReport): void {
    outcome = report.outcome;
    if (report.outcome.kind === 'installable') {
      retainedProfileMeta = {
        profile: report.outcome.profile,
        features: report.outcome.features,
        guidance: report.outcome.guidance,
        externalRequirement: report.outcome.external_requirement,
      };
    } else if (report.state.status !== 'installed') {
      retainedProfileMeta = null;
    }
  }

  const core = createAddonStore<LumaInstallState, LumaUpdateReport, AvailabilityReport>({
    api: {
      getAvailability: api.getAvailability,
      checkUpdate: (gameId, kind) => api.checkUpdate(gameId, { deep: kind === 'user' }),
    },
    messages: { loadFailed: 'addon.availability.loadFailed' },
    onExclusivityChange,
    // Advisory ZIP / dgVoodoo ownership need a passive probe after mutations.
    postMutationProbe: 'passive',
    onMutationSideEffect: onGameDetailsInvalidate
      ? (gameId) => onGameDetailsInvalidate(gameId)
      : undefined,
    applyLoadReport: (report) => {
      availabilitySnapshot = availabilitySnapshotFromReport(report);
      applyOutcome(report);
    },
    applyHostRefresh: (report) => {
      availabilitySnapshot = availabilitySnapshotFromReport(report);
      // Keep guidance/features/external_requirement in sync after mutations
      // (load-only path already assigns outcome via applyLoadReport).
      applyOutcome(report);
    },
    resetToolState: (gameId) => {
      safetyContextError = null;
      safetyAttemptGeneration += 1;
      availabilitySnapshot = {
        engineConfig: {
          status: 'not_applicable',
          path: null,
          can_apply: false,
        },
        hostDetection: 'absent',
        hostFacts: defaultHostFacts(),
        actions: {},
        vcredistPresent: null,
        vcredistInstallerUrl: '',
        installTorn: false,
        uninstallBlockedBy: null,
      };
      outcome = null;
      // Same-game reload keeps retained profile meta so installable features
      // survive resolution drift. Switching games drops it immediately.
      if (gameId === null || (lastLoadGameId !== null && lastLoadGameId !== gameId)) {
        retainedProfileMeta = null;
      }
      lastLoadGameId = gameId;
    },
    buildUpdateReportForInstall: (nextState) => {
      if (nextState.status !== 'installed') {
        return null;
      }
      // dgVoodoo ownership is only known after checkUpdate (reused runtimes
      // report null). Do not optimistically claim "current" from the installable
      // external_requirement — that overstates managed status until the probe.
      return {
        addon: 'current',
        host: 'current',
        dgvoodoo: null,
        overall: 'current',
      };
    },
    buildProbeFailureReport: () => ({
      addon: null,
      host: null,
      dgvoodoo: null,
      overall: 'unknown',
    }),
    /**
     * After install/update the synthetic report is overall `current`. A passive
     * probe often returns `unknown` without downloading the ZIP — keep the
     * optimistic current for addon/host and adopt any concrete dgVoodoo signal.
     */
    coalesceUpdateReport: (previous, probed) => {
      if (probed.overall !== 'unknown' || previous?.overall !== 'current') {
        return probed;
      }
      const addon = probed.addon ?? previous.addon;
      const host = probed.host ?? previous.host;
      const dgvoodoo = probed.dgvoodoo ?? previous.dgvoodoo;
      const anyAvailable =
        addon === 'available' || host === 'available' || dgvoodoo === 'available';
      return {
        addon,
        host,
        dgvoodoo,
        overall: anyAvailable ? 'available' : 'current',
      };
    },
    freshnessExtraSources: (report) => [report.dgvoodoo],
  });

  const externalRequirement = $derived<LumaManagedDependencySummary | null>(
    outcome?.kind === 'installable'
      ? outcome.external_requirement
      : core.state?.status === 'installed'
        ? (retainedProfileMeta?.externalRequirement ?? null)
        : null,
  );
  const profile = $derived<LumaProfile | null>(
    outcome?.kind === 'installable'
      ? outcome.profile
      : core.state?.status === 'installed'
        ? (retainedProfileMeta?.profile ?? null)
        : null,
  );
  const features = $derived<LumaFeatures | null>(
    outcome?.kind === 'installable'
      ? outcome.features
      : core.state?.status === 'installed'
        ? (retainedProfileMeta?.features ?? null)
        : null,
  );
  const guidance = $derived<LumaGuidance[]>(
    outcome?.kind === 'installable'
      ? outcome.guidance
      : core.state?.status === 'installed'
        ? (retainedProfileMeta?.guidance ?? [])
        : [],
  );
  const dgvoodooUpdate = $derived(core.updateReport?.dgvoodoo ?? null);
  const reshadeChannel = $derived(
    core.state?.status === 'installed' ? core.state.reshade_channel : null,
  );
  const launchArgs = $derived<string[]>(
    core.state?.status === 'installed'
      ? core.state.launch_args
      : outcome?.kind === 'installable'
        ? outcome.launch_args
        : [],
  );

  async function install(gameId: string): Promise<AddonMutationResult> {
    const safetyAttempt = beginSafetyAttempt();
    return runWithSafetyTokens(gameId, 'gameDetails.luma.installError', safetyAttempt, (tokens) =>
      core.runBusyMutation(
        gameId,
        () => (tokens ? api.install(gameId, tokens.gameContextToken) : api.install(gameId)),
        {
          errorKey: 'gameDetails.luma.installError',
          safetyScope: 'game',
          notifyExclusivity: true,
        },
      ),
    );
  }

  async function mutateUpdate(
    gameId: string,
    errorKey: 'gameDetails.luma.updateError' | 'gameDetails.luma.repairError',
    requireUpdateAvailable: boolean,
    forceFull: boolean,
  ): Promise<AddonMutationResult> {
    const safetyAttempt = beginSafetyAttempt();
    if (requireUpdateAvailable && !core.updateAvailable) {
      return 'skipped';
    }
    return runWithSafetyTokens(gameId, errorKey, safetyAttempt, (tokens) =>
      core.runBusyMutation(
        gameId,
        () =>
          tokens
            ? api.update(gameId, { forceFull, gameContextToken: tokens.gameContextToken })
            : api.update(gameId, { forceFull }),
        {
          errorKey,
          safetyScope: 'game',
          requireUpdateAvailable,
        },
      ),
    );
  }

  async function update(gameId: string): Promise<AddonMutationResult> {
    return mutateUpdate(gameId, 'gameDetails.luma.updateError', true, false);
  }

  async function repair(gameId: string): Promise<AddonMutationResult> {
    // Repair must force a full payload reconverge, not HostOnly when ETag matches.
    return mutateUpdate(gameId, 'gameDetails.luma.repairError', false, true);
  }

  async function uninstall(gameId: string): Promise<AddonMutationResult> {
    const safetyAttempt = beginSafetyAttempt();
    const result = await core.runBusyMutation(gameId, () => api.uninstall(gameId), {
      errorKey: 'gameDetails.luma.uninstallError',
      clearDownloadProgress: false,
      notifyExclusivity: true,
    });
    recordSafetyContextError(safetyAttempt, result === 'failed' ? core.safetyContextError : null);
    return result;
  }

  function checkForUpdates(gameId: string): Promise<void> {
    beginSafetyAttempt();
    return core.checkForUpdates(gameId);
  }

  async function applyEngineConfig(gameId: string): Promise<AddonMutationResult> {
    const safetyAttempt = beginSafetyAttempt();
    const apply = api.applyEngineConfig;
    if (!apply) {
      return 'skipped';
    }
    return runWithSafetyTokens(
      gameId,
      'gameDetails.luma.engineConfigApplyError',
      safetyAttempt,
      (tokens) =>
        core.runSidecarMutation(
          gameId,
          () => (tokens ? apply(gameId, tokens.gameContextToken) : apply(gameId)),
          { errorKey: 'gameDetails.luma.engineConfigApplyError', safetyScope: 'game' },
        ),
    );
  }

  return mergeAddonApis(
    addonCoreApi(core),
    commonOutcomeApi(() => outcome),
    hostSnapshotApi(() => availabilitySnapshot),
    {
      load: core.load,
      retry: core.retry,
      refreshAvailability: core.refreshAvailability,
      checkForUpdates,
      get vcredistPresent() {
        return availabilitySnapshot.vcredistPresent;
      },
      get engineConfig() {
        return availabilitySnapshot.engineConfig;
      },
      get vcredistInstallerUrl() {
        return availabilitySnapshot.vcredistInstallerUrl;
      },
      get installTorn() {
        return availabilitySnapshot.installTorn;
      },
      get uninstallBlockedBy() {
        return availabilitySnapshot.uninstallBlockedBy;
      },
      get safetyContextError() {
        return safetyContextError;
      },
      get externalRequirement() {
        return externalRequirement;
      },
      get profile() {
        return profile;
      },
      get features() {
        return features;
      },
      get guidance() {
        return guidance;
      },
      get launchArgs() {
        return launchArgs;
      },
      get reshadeChannel() {
        return reshadeChannel;
      },
      get dgvoodooUpdate() {
        return dgvoodooUpdate;
      },
      install,
      update,
      repair,
      uninstall,
      applyEngineConfig,
    },
  );
}
