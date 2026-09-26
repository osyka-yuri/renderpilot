import {
  addonCoreApi,
  commonOutcomeApi,
  createAddonStore,
  hostSnapshotApi,
  mergeAddonApis,
  type MatchConfidence,
  type ReshadeChannel,
  type MutationSafetyTokens,
  type AddonMutationResult,
} from '@entities/addon';

import { renodxApi, type RenoDxApi } from '../api/desktop';
import {
  availabilitySnapshotFromReport,
  currentHostChannel,
  defaultHostFacts,
  type AvailabilitySnapshot,
} from './renodx-store-helpers';
import { createRenoDxCompanionStore } from './renodx-store-companion.svelte';
import { createRenoDxDlssFixMutations } from './renodx-store-dlss-fix-mutations';
import { createRenoDxHostMutations } from './renodx-store-host-mutations';
import type {
  AvailabilityOutcome,
  AvailabilityReport,
  ManualFileInstall,
  RenoDxGenericProfile,
  RenoDxGuidance,
  RenoDxInstallState,
  RenoDxLaunchRequirement,
  RenoDxUpdateReport,
  VulkanLayerReport,
} from './types';
import { presentDlssFix } from './dlss-fix-presentation';

/** Reactive store backing the RenoDX card for a single game. */
export type RenoDxStore = ReturnType<typeof createRenoDxStore>;

export type RenoDxStoreOptions = {
  api?: RenoDxApi;
  /** Called after a successful install/uninstall changes whether this game blocks Luma. */
  onExclusivityChange?: (gameId: string) => void;
  /** Refreshes the details capability projection after install/uninstall. */
  onGameDetailsInvalidate?: (gameId: string) => void | Promise<void>;
  requireSafetyTokens?: (
    gameId: string,
    scope: 'game' | 'game_and_shared',
  ) => Promise<MutationSafetyTokens>;
  requireInstallSafetyTokens?: (
    gameId: string,
    scope: 'game' | 'game_and_shared',
  ) => Promise<MutationSafetyTokens | null>;
  onSafetyContextError?: (
    error: unknown,
    scope: 'game' | 'game_and_shared',
  ) => void | Promise<void>;
};

/**
 * Creates the RenoDX store. The backend API is injected so tests can drive the
 * store with fakes; production code uses the default Tauri-bound [`renodxApi`].
 */
export function createRenoDxStore(options: RenoDxStoreOptions = {}) {
  const api = options.api ?? renodxApi;
  const onExclusivityChange = options.onExclusivityChange;
  const requireSafetyTokens = options.requireSafetyTokens;

  let availabilitySnapshot = $state<AvailabilitySnapshot>({
    engineConfig: {
      status: 'not_applicable',
      path: null,
      can_apply: false,
    },
    hostDetection: 'absent',
    hostFacts: defaultHostFacts(),
    actions: {},
    reshadeStableSupported: true,
    renodxAddon: null,
    installTorn: false,
  });
  let selectedReshadeChannel = $state<ReshadeChannel>('stable');
  let outcome = $state<AvailabilityOutcome | null>(null);
  type RetainedInstallMeta = {
    genericProfile: RenoDxGenericProfile | null;
    profileId: string | null;
    guidance: RenoDxGuidance[];
    launch: RenoDxLaunchRequirement | null;
  };
  let retainedInstallMeta = $state<RetainedInstallMeta | null>(null);
  let lastLoadGameId: string | null = null;
  let manualInstall = $state<ManualFileInstall | null>(null);
  let vulkanLayer = $state<VulkanLayerReport | null>(null);

  function clearRetainedInstallMeta(): void {
    retainedInstallMeta = null;
  }

  function resetAvailabilityPresentation(): void {
    availabilitySnapshot = {
      engineConfig: {
        status: 'not_applicable',
        path: null,
        can_apply: false,
      },
      hostDetection: 'absent',
      hostFacts: defaultHostFacts(),
      actions: {},
      reshadeStableSupported: true,
      renodxAddon: null,
      installTorn: false,
    };
    selectedReshadeChannel = 'stable';
    outcome = null;
    manualInstall = null;
    vulkanLayer = null;
  }

  function applyOutcome(report: AvailabilityReport): void {
    outcome = report.outcome;
    const install =
      report.outcome.kind === 'installable'
        ? report.outcome
        : report.outcome.kind === 'external'
          ? report.outcome.file_install
          : null;
    if (install) {
      retainedInstallMeta = {
        genericProfile: install.generic_profile,
        profileId: install.profile_id,
        guidance: install.guidance,
        launch: install.launch,
      };
    } else if (report.state.status !== 'installed') {
      clearRetainedInstallMeta();
    }
  }

  function applyAvailabilitySnapshot(
    report: Parameters<typeof availabilitySnapshotFromReport>[0],
    mode: 'resetSelection' | 'preserveSelection',
  ): void {
    const nextSnapshot = availabilitySnapshotFromReport(report);
    availabilitySnapshot = nextSnapshot;
    if (mode === 'resetSelection') {
      selectedReshadeChannel = nextSnapshot.hostFacts.channel.selected;
    }
  }

  function channelIsSupported(channel: ReshadeChannel): boolean {
    return channel !== 'stable' || availabilitySnapshot.reshadeStableSupported;
  }

  const core = createAddonStore<RenoDxInstallState, RenoDxUpdateReport, AvailabilityReport>({
    api: {
      getAvailability: api.getAvailability,
      checkUpdate: (gameId) => api.checkUpdate(gameId),
    },
    messages: { loadFailed: 'addon.availability.loadFailed' },
    onExclusivityChange,
    onMutationError: (error, scope) => {
      if (requireSafetyTokens) {
        void options.onSafetyContextError?.(error, scope);
      }
    },
    postMutationProbe: 'never',
    applyLoadReport: (report) => {
      applyAvailabilitySnapshot(report, 'resetSelection');
      applyOutcome(report);
      manualInstall = report.manual_install;
      vulkanLayer = report.vulkan_layer;
    },
    applyHostRefresh: (report) => {
      applyAvailabilitySnapshot(report, 'preserveSelection');
      applyOutcome(report);
      manualInstall = report.manual_install;
      vulkanLayer = report.vulkan_layer;
    },
    invalidateAvailabilityForCommittedState: (state) => {
      if (state.status === 'not_installed') {
        resetAvailabilityPresentation();
        clearRetainedInstallMeta();
      }
    },
    resetToolState: (gameId) => {
      resetAvailabilityPresentation();
      if (gameId === null || (lastLoadGameId !== null && lastLoadGameId !== gameId)) {
        clearRetainedInstallMeta();
      }
      lastLoadGameId = gameId;
    },
    buildUpdateReportForInstall: (nextState) => {
      if (nextState.status !== 'installed') {
        return null;
      }
      return {
        addon: 'current',
        host: 'current',
        dlssFix: nextState.dlss_fix_evidence_present ? 'current' : null,
        overall: 'current',
      };
    },
    buildProbeFailureReport: () => ({
      addon: null,
      host: null,
      dlssFix: null,
      overall: 'unknown',
    }),
    freshnessExtraSources: (report) => [report.dlssFix],
  });

  const companion = createRenoDxCompanionStore({
    api,
    core,
    setSelectedChannel: (channel) => {
      selectedReshadeChannel = channel;
    },
    setVulkanLayer: (report) => {
      vulkanLayer = report;
    },
    onGameDetailsInvalidate: options.onGameDetailsInvalidate,
  });

  const isExternal = $derived(outcome?.kind === 'external');
  const isNativeHdr = $derived(outcome?.kind === 'native_hdr');
  const hasUnsupportedSettings = $derived(outcome?.kind === 'unsupported_settings');
  const externalUrl = $derived(outcome?.kind === 'external' ? outcome.url : null);
  const externalMessage = $derived(outcome?.kind === 'external' ? outcome.message : null);
  const externalFileInstall = $derived(outcome?.kind === 'external' ? outcome.file_install : null);
  const externalFileInstallable = $derived(externalFileInstall !== null);
  const externalConfidence = $derived<MatchConfidence | null>(
    externalFileInstall?.confidence ?? null,
  );
  const activeInstall = $derived(
    outcome?.kind === 'installable'
      ? outcome
      : outcome?.kind === 'external'
        ? outcome.file_install
        : null,
  );
  const genericProfile = $derived(
    activeInstall?.generic_profile ??
      (core.state?.status === 'installed' ? retainedInstallMeta?.genericProfile : null) ??
      null,
  );
  const profileId = $derived(
    activeInstall?.profile_id ??
      (core.state?.status === 'installed' ? retainedInstallMeta?.profileId : null) ??
      null,
  );
  const guidance = $derived<RenoDxGuidance[]>(
    activeInstall?.guidance ??
      (core.state?.status === 'installed' ? retainedInstallMeta?.guidance : null) ??
      [],
  );
  const launch = $derived<RenoDxLaunchRequirement | null>(
    activeInstall?.launch ??
      (core.state?.status === 'installed' ? retainedInstallMeta?.launch : null) ??
      null,
  );
  const vulkanUpdateDiagnostics = $derived(core.updateReport?.vulkan_diagnostics ?? []);
  const dlssFix = $derived(
    presentDlssFix({
      availability: companion.dlssFixAvailability,
      fallbackEvidencePresent:
        core.state?.status === 'installed' && core.state.dlss_fix_evidence_present,
      updateStatus: core.updateReport?.dlssFix ?? null,
    }),
  );
  const addonTracked = $derived(
    core.state?.status === 'installed' ? core.state.addon_tracked : null,
  );

  function deactivate(): void {
    core.deactivate();
    companion.clear();
  }

  const mutations = createRenoDxHostMutations({
    api,
    core,
    getAvailabilitySnapshot: () => availabilitySnapshot,
    getOutcome: () => outcome,
    getManualInstallHostKind: () => manualInstall?.host_kind ?? null,
    channelIsSupported,
    onChannelSwitched: (channel) => {
      selectedReshadeChannel = channel;
      availabilitySnapshot = {
        ...availabilitySnapshot,
        hostFacts: {
          ...availabilitySnapshot.hostFacts,
          channel: {
            ...availabilitySnapshot.hostFacts.channel,
            selected: channel,
            detected: channel,
          },
        },
      };
    },
    requireSafetyTokens,
    requireInstallSafetyTokens: options.requireInstallSafetyTokens,
    afterInstallLikeCommit: companion.afterInstallLikeCommit,
    afterCapabilityCommit: companion.afterCapabilityCommit,
  });
  const dlssFixMutations = createRenoDxDlssFixMutations({
    api,
    core,
    requireSafetyTokens,
    afterInstallLikeCommit: companion.afterInstallLikeCommit,
  });

  function setSelectedReshadeChannel(channel: ReshadeChannel): void {
    selectedReshadeChannel = channel;
  }

  async function applyEngineConfig(gameId: string): Promise<AddonMutationResult> {
    const apply = api.applyEngineConfig;
    if (!apply) {
      return 'skipped';
    }
    return core.runSidecarMutation(
      gameId,
      async () => {
        const tokens = await requireSafetyTokens?.(gameId, 'game');
        return tokens ? apply(gameId, tokens.gameContextToken) : apply(gameId);
      },
      { errorKey: 'gameDetails.renodx.engineConfigApplyError', safetyScope: 'game' },
    );
  }

  return mergeAddonApis(
    addonCoreApi(core),
    commonOutcomeApi(() => outcome),
    hostSnapshotApi(() => availabilitySnapshot),
    {
      deactivate,
      get reshadeChannel() {
        return currentHostChannel(availabilitySnapshot);
      },
      get engineConfig() {
        return availabilitySnapshot.engineConfig;
      },
      get reshadeStableSupported() {
        return availabilitySnapshot.reshadeStableSupported;
      },
      get selectedReshadeChannel() {
        return selectedReshadeChannel;
      },
      get renodxAddon() {
        return availabilitySnapshot.renodxAddon;
      },
      get installTorn() {
        return availabilitySnapshot.installTorn;
      },
      get manualInstall() {
        return manualInstall;
      },
      get isExternal() {
        return isExternal;
      },
      get isNativeHdr() {
        return isNativeHdr;
      },
      get hasUnsupportedSettings() {
        return hasUnsupportedSettings;
      },
      get externalUrl() {
        return externalUrl;
      },
      get externalMessage() {
        return externalMessage;
      },
      get externalFileInstallable() {
        return externalFileInstallable;
      },
      get externalConfidence() {
        return externalConfidence;
      },
      get confidence() {
        return outcome?.kind === 'installable' ? outcome.confidence : externalConfidence;
      },
      get genericProfile() {
        return genericProfile;
      },
      get profileId() {
        return profileId;
      },
      get guidance() {
        return guidance;
      },
      get launch() {
        return launch;
      },
      get vulkanLayer() {
        return vulkanLayer;
      },
      get vulkanUpdateDiagnostics() {
        return vulkanUpdateDiagnostics;
      },
      get addonTracked() {
        return addonTracked;
      },
      get dlssFix() {
        return dlssFix;
      },
      load: companion.load,
      retry: companion.retry,
      refreshAvailability: companion.refreshAvailability,
      checkForUpdates: companion.checkForUpdates,
      install: mutations.install,
      installFromFile: mutations.installFromFile,
      setSelectedReshadeChannel,
      switchChannel: mutations.switchChannel,
      update: mutations.update,
      uninstall: mutations.uninstall,
      installDlssFix: dlssFixMutations.installDlssFix,
      updateDlssFix: dlssFixMutations.updateDlssFix,
      retryDlssFixRecovery: dlssFixMutations.retryDlssFixRecovery,
      uninstallDlssFix: dlssFixMutations.uninstallDlssFix,
      applyEngineConfig,
    },
  );
}
