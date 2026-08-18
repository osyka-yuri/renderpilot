import {
  addonCoreApi,
  commonOutcomeApi,
  createAddonStore,
  hostSnapshotApi,
  mergeAddonApis,
  type AddonMutationResult,
  type MatchConfidence,
  type ReshadeChannel,
  type RiskAssessment,
} from '@entities/addon';

import { renodxApi, type RenoDxApi } from '../api/desktop';
import {
  availabilitySnapshotFromReport,
  currentHostChannel,
  defaultHostFacts,
  type AvailabilitySnapshot,
} from './renodx-store-helpers';
import type {
  AvailabilityOutcome,
  AvailabilityReport,
  DlssFixAvailability,
  ManualFileInstall,
  RenoDxInstallState,
  RenoDxUpdateReport,
  VulkanLayerReport,
} from './types';
import { presentDlssFix } from './dlss-fix-presentation';

/** Reactive store backing the RenoDX card for a single game. */
export type RenoDxStore = ReturnType<typeof createRenoDxStore>;

export type RenoDxStoreOptions = {
  api?: RenoDxApi;
  /**
   * Called after a successful install/uninstall changes whether this game
   * blocks Luma. Peer refreshes are page-owned and best-effort.
   */
  onExclusivityChange?: (gameId: string) => void;
  /** Refreshes the details capability projection after install/uninstall. */
  onGameDetailsInvalidate?: (gameId: string) => void | Promise<void>;
};

/**
 * Creates the RenoDX store. The backend API is injected so tests can drive the
 * store with fakes; production code uses the default Tauri-bound [`renodxApi`].
 */
export function createRenoDxStore(options: RenoDxStoreOptions = {}) {
  const api = options.api ?? renodxApi;
  const onExclusivityChange = options.onExclusivityChange;
  const onGameDetailsInvalidate = options.onGameDetailsInvalidate;

  let availabilitySnapshot = $state<AvailabilitySnapshot>({
    hostDetection: 'absent',
    hostFacts: defaultHostFacts(),
    actions: {},
    reshadeStableSupported: true,
    renodxAddon: null,
  });
  let selectedReshadeChannel = $state<ReshadeChannel>('stable');
  let outcome = $state<AvailabilityOutcome | null>(null);
  let manualInstall = $state<ManualFileInstall | null>(null);
  let vulkanLayer = $state<VulkanLayerReport | null>(null);
  let dlssFixAvailability = $state<DlssFixAvailability | null>(null);

  function applyAvailabilitySnapshot(
    report: Parameters<typeof availabilitySnapshotFromReport>[0],
    mode: 'resetSelection' | 'preserveSelection',
  ): void {
    const nextSnapshot = availabilitySnapshotFromReport(report);
    availabilitySnapshot = nextSnapshot;
    if (mode === 'resetSelection') {
      selectedReshadeChannel = selectedChannelFromSnapshot(nextSnapshot);
    }
  }

  function selectedChannelFromSnapshot(snapshot: AvailabilitySnapshot): ReshadeChannel {
    return snapshot.hostFacts.channel.selected;
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
    // Keep the synthetic install report; companion probes live in afterCommit.
    postMutationProbe: 'never',
    applyLoadReport: (report) => {
      applyAvailabilitySnapshot(report, 'resetSelection');
      outcome = report.outcome;
      manualInstall = report.manual_install;
      vulkanLayer = report.vulkan_layer;
    },
    applyHostRefresh: (report) => {
      applyAvailabilitySnapshot(report, 'preserveSelection');
      // Keep outcome / manual install / vulkan layer in sync after mutations
      // (load-only path already assigns these via applyLoadReport).
      outcome = report.outcome;
      manualInstall = report.manual_install;
      vulkanLayer = report.vulkan_layer;
    },
    resetToolState: (_gameId) => {
      availabilitySnapshot = {
        hostDetection: 'absent',
        hostFacts: defaultHostFacts(),
        actions: {},
        reshadeStableSupported: true,
        renodxAddon: null,
      };
      selectedReshadeChannel = 'stable';
      outcome = null;
      manualInstall = null;
      vulkanLayer = null;
      dlssFixAvailability = null;
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

  const isExternal = $derived(outcome?.kind === 'external');
  const isNativeHdr = $derived(outcome?.kind === 'native_hdr');
  const externalUrl = $derived(outcome?.kind === 'external' ? outcome.url : null);
  const externalMessage = $derived(outcome?.kind === 'external' ? outcome.message : null);
  const externalFileInstall = $derived(outcome?.kind === 'external' ? outcome.file_install : null);
  const externalFileInstallable = $derived(externalFileInstall !== null);
  const externalConfidence = $derived<MatchConfidence | null>(
    externalFileInstall?.confidence ?? null,
  );
  const externalRisk = $derived<RiskAssessment | null>(externalFileInstall?.risk ?? null);
  const externalRequiresConfirmation = $derived(externalRisk?.severity === 'warn');
  const genericProfile = $derived(
    outcome?.kind === 'installable' ? (outcome.generic_profile ?? null) : null,
  );
  const vulkanUpdateDiagnostics = $derived(core.updateReport?.vulkan_diagnostics ?? []);
  const dlssFix = $derived(
    presentDlssFix({
      availability: dlssFixAvailability,
      fallbackEvidencePresent:
        core.state?.status === 'installed' && core.state.dlss_fix_evidence_present,
      updateStatus: core.updateReport?.dlssFix ?? null,
    }),
  );
  const addonTracked = $derived(
    core.state?.status === 'installed' ? core.state.addon_tracked : null,
  );

  async function probeDlssFixAvailability(gameId: string, token: number): Promise<void> {
    try {
      const availability = await api.dlssFixAvailability(gameId);
      if (core.isCurrentRequest(token)) {
        dlssFixAvailability = availability;
      }
    } catch {
      if (core.isCurrentRequest(token)) {
        dlssFixAvailability = null;
      }
    }
  }

  async function load(gameId: string): Promise<void> {
    dlssFixAvailability = null;
    const loading = core.load(gameId);
    const token = core.requestToken;
    await loading;
    if (core.isCurrentRequest(token) && !core.loadError) {
      await probeDlssFixAvailability(gameId, token);
    }
  }

  async function retry(gameId: string): Promise<void> {
    dlssFixAvailability = null;
    const loading = core.retry(gameId);
    const token = core.requestToken;
    await loading;
    if (core.isCurrentRequest(token) && !core.loadError) {
      await probeDlssFixAvailability(gameId, token);
    }
  }

  async function checkForUpdates(gameId: string): Promise<void> {
    dlssFixAvailability = null;
    const checking = core.checkForUpdates(gameId);
    const token = core.requestToken;
    await checking;
    if (core.isCurrentRequest(token)) {
      await probeDlssFixAvailability(gameId, token);
    }
  }

  async function refreshVulkanLayerStatus(token: number): Promise<void> {
    try {
      const report = await api.vulkanLayerStatus();
      if (core.isCurrentRequest(token)) {
        vulkanLayer = report;
      }
    } catch {
      // Best-effort: a failed layer-status refresh leaves the previous report.
    }
  }

  /**
   * Post-commit companion refresh for install-like mutations.
   * Freshness stays on the synthetic install report (`postMutationProbe: 'never'`);
   * Luma uses store-level `postMutationProbe: 'passive'` for advisory checks.
   */
  async function afterInstallLikeCommit(
    gameId: string,
    token: number,
    channel?: ReshadeChannel,
  ): Promise<void> {
    if (!core.isCurrentRequest(token)) {
      return;
    }
    if (channel !== undefined) {
      selectedReshadeChannel = channel;
      await refreshVulkanLayerStatus(token);
    }
    if (!core.isCurrentRequest(token)) {
      return;
    }
    dlssFixAvailability = null;
    await probeDlssFixAvailability(gameId, token);
  }

  async function afterCapabilityCommit(
    gameId: string,
    token: number,
    channel?: ReshadeChannel,
  ): Promise<void> {
    await afterInstallLikeCommit(gameId, token, channel);
    if (core.isCurrentRequest(token)) {
      await onGameDetailsInvalidate?.(gameId);
    }
  }

  async function install(
    gameId: string,
    channel: ReshadeChannel,
    confirmAnticheat: boolean,
  ): Promise<AddonMutationResult> {
    if (!channelIsSupported(channel)) {
      return 'skipped';
    }
    return core.runBusyMutation(gameId, () => api.install(gameId, channel, confirmAnticheat), {
      errorKey: 'gameDetails.renodx.installError',
      afterCommit: (token) => afterCapabilityCommit(gameId, token, channel),
      notifyExclusivity: true,
    });
  }

  async function installFromFile(
    gameId: string,
    filePath: string,
    channel: ReshadeChannel,
    confirmAnticheat: boolean,
  ): Promise<AddonMutationResult> {
    if (!channelIsSupported(channel)) {
      return 'skipped';
    }
    return core.runBusyMutation(
      gameId,
      () => api.installFromFile(gameId, filePath, channel, confirmAnticheat),
      {
        errorKey: 'gameDetails.renodx.installError',
        afterCommit: (token) => afterCapabilityCommit(gameId, token, channel),
        notifyExclusivity: true,
      },
    );
  }

  async function update(gameId: string): Promise<AddonMutationResult> {
    return core.runBusyMutation(gameId, () => api.update(gameId), {
      errorKey: 'gameDetails.renodx.updateError',
      requireUpdateAvailable: true,
      afterCommit: (token) => afterInstallLikeCommit(gameId, token),
    });
  }

  async function switchChannel(
    gameId: string,
    channel: ReshadeChannel,
  ): Promise<AddonMutationResult> {
    const action = availabilitySnapshot.actions.switch_channel;
    if (
      core.busy ||
      core.state?.status !== 'installed' ||
      core.state.host_kind !== 'proxy' ||
      action?.enabled !== true ||
      action.target_channel !== channel ||
      channel === currentHostChannel(availabilitySnapshot)
    ) {
      return 'skipped';
    }
    return core.runBusyMutation(gameId, () => api.switchChannel(gameId, channel), {
      errorKey: 'gameDetails.renodx.switchError',
      afterCommit: () => {
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
    });
  }

  function setSelectedReshadeChannel(channel: ReshadeChannel): void {
    selectedReshadeChannel = channel;
  }

  async function uninstall(gameId: string): Promise<AddonMutationResult> {
    return core.runBusyMutation(gameId, () => api.uninstall(gameId), {
      errorKey: 'gameDetails.renodx.uninstallError',
      clearDownloadProgress: false,
      notifyExclusivity: true,
      afterCommit: (token) => afterCapabilityCommit(gameId, token),
    });
  }

  async function installDlssFix(gameId: string): Promise<AddonMutationResult> {
    return core.runBusyMutation(gameId, () => api.installDlssFix(gameId), {
      errorKey: 'gameDetails.renodx.dlssFixInstallError',
      afterCommit: (token) => afterInstallLikeCommit(gameId, token),
    });
  }

  async function uninstallDlssFix(gameId: string): Promise<AddonMutationResult> {
    return core.runBusyMutation(gameId, () => api.uninstallDlssFix(gameId), {
      errorKey: 'gameDetails.renodx.dlssFixRemoveError',
      clearDownloadProgress: false,
      afterCommit: (token) => afterInstallLikeCommit(gameId, token),
    });
  }

  async function updateDlssFix(gameId: string): Promise<AddonMutationResult> {
    return core.runBusyMutation(gameId, () => api.updateDlssFix(gameId), {
      errorKey: 'gameDetails.renodx.dlssFixInstallError',
      afterCommit: (token) => afterInstallLikeCommit(gameId, token),
    });
  }

  async function retryDlssFixRecovery(gameId: string): Promise<AddonMutationResult> {
    return core.runBusyMutation(gameId, () => api.retryDlssFixRecovery(gameId), {
      errorKey: 'gameDetails.renodx.dlssFixInstallError',
      clearDownloadProgress: false,
      afterCommit: (token) => afterInstallLikeCommit(gameId, token),
    });
  }

  return mergeAddonApis(
    addonCoreApi(core),
    commonOutcomeApi(() => outcome),
    hostSnapshotApi(() => availabilitySnapshot),
    {
      get reshadeChannel() {
        return currentHostChannel(availabilitySnapshot);
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
      get manualInstall() {
        return manualInstall;
      },
      get isExternal() {
        return isExternal;
      },
      get isNativeHdr() {
        return isNativeHdr;
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
      get externalRisk() {
        return externalRisk;
      },
      get externalRequiresConfirmation() {
        return externalRequiresConfirmation;
      },
      get genericProfile() {
        return genericProfile;
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
      load,
      retry,
      checkForUpdates,
      install,
      installFromFile,
      setSelectedReshadeChannel,
      switchChannel,
      update,
      uninstall,
      installDlssFix,
      updateDlssFix,
      retryDlssFixRecovery,
      uninstallDlssFix,
    },
  );
}
