import {
  addonCoreApi,
  commonOutcomeApi,
  createAddonStore,
  hostSnapshotApi,
  mergeAddonApis,
  type AddonMutationResult,
  type MutationSafetyTokens,
} from '@entities/addon';

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
  requireSafetyTokens?: (gameId: string, scope: 'game') => Promise<MutationSafetyTokens>;
  onSafetyContextError?: (error: unknown, scope: 'game') => void | Promise<void>;
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
  const onSafetyContextError = options.onSafetyContextError;

  type RetainedProfileMeta = {
    profile: LumaProfile | null;
    features: LumaFeatures | null;
    guidance: LumaGuidance[];
    externalRequirement: LumaManagedDependencySummary | null;
  };

  let availabilitySnapshot = $state<AvailabilitySnapshot>({
    hostDetection: 'absent',
    hostFacts: defaultHostFacts(),
    actions: {},
    vcredistPresent: null,
    vcredistInstallerUrl: '',
    installTorn: false,
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
    onMutationError: (error) => {
      if (requireSafetyTokens) {
        void onSafetyContextError?.(error, 'game');
      }
    },
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
      availabilitySnapshot = {
        hostDetection: 'absent',
        hostFacts: defaultHostFacts(),
        actions: {},
        vcredistPresent: null,
        vcredistInstallerUrl: '',
        installTorn: false,
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
    return core.runBusyMutation(
      gameId,
      async () => {
        const tokens = await requireSafetyTokens?.(gameId, 'game');
        return tokens ? api.install(gameId, tokens.gameContextToken) : api.install(gameId);
      },
      {
        errorKey: 'gameDetails.luma.installError',
        safetyScope: 'game',
        notifyExclusivity: true,
      },
    );
  }

  async function mutateUpdate(
    gameId: string,
    errorKey: 'gameDetails.luma.updateError' | 'gameDetails.luma.repairError',
    requireUpdateAvailable: boolean,
    forceFull: boolean,
  ): Promise<AddonMutationResult> {
    return core.runBusyMutation(
      gameId,
      async () => {
        const tokens = await requireSafetyTokens?.(gameId, 'game');
        return tokens
          ? api.update(gameId, { forceFull, gameContextToken: tokens.gameContextToken })
          : api.update(gameId, { forceFull });
      },
      {
        errorKey,
        safetyScope: 'game',
        requireUpdateAvailable,
      },
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
    return core.runBusyMutation(gameId, () => api.uninstall(gameId), {
      errorKey: 'gameDetails.luma.uninstallError',
      clearDownloadProgress: false,
      notifyExclusivity: true,
    });
  }

  return mergeAddonApis(
    addonCoreApi(core),
    commonOutcomeApi(() => outcome),
    hostSnapshotApi(() => availabilitySnapshot),
    {
      load: core.load,
      retry: core.retry,
      checkForUpdates: core.checkForUpdates,
      get vcredistPresent() {
        return availabilitySnapshot.vcredistPresent;
      },
      get vcredistInstallerUrl() {
        return availabilitySnapshot.vcredistInstallerUrl;
      },
      get installTorn() {
        return availabilitySnapshot.installTorn;
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
    },
  );
}
