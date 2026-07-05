import {
  addonCoreApi,
  commonOutcomeApi,
  createAddonStore,
  hostSnapshotApi,
  mergeAddonApis,
} from '@entities/addon';

import { renodxApi, type RenoDxApi } from '../api/desktop';
import {
  availabilitySnapshotFromReport,
  currentHostChannel,
  defaultHostFacts,
  degradeUnsupportedStableChannel,
  type AvailabilitySnapshot,
} from './renodx-store-helpers';
import type {
  AvailabilityOutcome,
  AvailabilityReport,
  ManualFileInstall,
  MatchConfidence,
  RenoDxInstallState,
  RenoDxUpdateReport,
  ReshadeChannel,
  RiskAssessment,
  VulkanLayerReport,
} from './types';

/** Reactive store backing the RenoDX card for a single game. */
export type RenoDxStore = ReturnType<typeof createRenoDxStore>;

export type RenoDxStoreOptions = {
  api?: RenoDxApi;
  /**
   * Called after a successful install/uninstall changes whether this game
   * blocks peer add-ons. Peer refreshes are page-owned and best-effort.
   */
  onExclusivityChange?: (gameId: string) => void;
};

/**
 * Creates the RenoDX store. The backend API is injected so tests can drive the
 * store with fakes; production code uses the default Tauri-bound [`renodxApi`].
 */
export function createRenoDxStore(options: RenoDxStoreOptions = {}) {
  const api = options.api ?? renodxApi;
  const onExclusivityChange = options.onExclusivityChange;

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
  let dlssFixAvailable = $state(false);

  function applyAvailabilitySnapshot(
    report: Parameters<typeof availabilitySnapshotFromReport>[0],
    mode: 'resetSelection' | 'preserveSelection',
  ): void {
    const nextSnapshot = availabilitySnapshotFromReport(report);
    availabilitySnapshot = nextSnapshot;
    if (mode === 'resetSelection') {
      selectedReshadeChannel = selectedChannelFromSnapshot(nextSnapshot);
    } else if (!nextSnapshot.reshadeStableSupported && selectedReshadeChannel === 'stable') {
      selectedReshadeChannel = 'nightly';
    }
  }

  function selectedChannelFromSnapshot(snapshot: AvailabilitySnapshot): ReshadeChannel {
    const channel = currentHostChannel(snapshot) ?? snapshot.hostFacts.channel.effective;
    return degradeUnsupportedStableChannel(channel, snapshot.reshadeStableSupported);
  }

  const core = createAddonStore<RenoDxInstallState, RenoDxUpdateReport, AvailabilityReport>({
    api,
    messages: { loadFailed: 'gameDetails.renodx.loadFailed' },
    onExclusivityChange,
    applyLoadReport: (report) => {
      applyAvailabilitySnapshot(report, 'resetSelection');
      outcome = report.outcome;
      manualInstall = report.manual_install;
      vulkanLayer = report.vulkan_layer;
    },
    applyHostRefresh: (report) => {
      applyAvailabilitySnapshot(report, 'preserveSelection');
    },
    buildUpdateReportForInstall: (nextState) => {
      if (nextState.status !== 'installed') {
        return null;
      }
      return {
        addon: 'current',
        host: 'current',
        dlssFix: nextState.dlss_fix_installed ? 'current' : null,
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
  const externalLabelKey = $derived(outcome?.kind === 'external' ? outcome.label_key : null);
  const externalFileInstall = $derived(outcome?.kind === 'external' ? outcome.file_install : null);
  const externalFileInstallable = $derived(externalFileInstall !== null);
  const externalConfidence = $derived<MatchConfidence | null>(
    externalFileInstall?.confidence ?? null,
  );
  const externalRisk = $derived<RiskAssessment | null>(externalFileInstall?.risk ?? null);
  const externalNotes = $derived<string[]>(externalFileInstall?.notes_keys ?? []);
  const externalRequiresConfirmation = $derived(externalRisk?.severity === 'warn');
  const dlssFixUpdate = $derived(core.updateReport?.dlssFix ?? null);
  const vulkanUpdateDiagnostics = $derived(core.updateReport?.vulkan_diagnostics ?? []);
  const dlssFixInstalled = $derived(
    core.state?.status === 'installed' && core.state.dlss_fix_installed,
  );

  async function probeDlssFixAvailability(gameId: string, token: number): Promise<void> {
    try {
      const available = await api.dlssFixAvailability(gameId);
      if (core.isCurrentRequest(token)) {
        dlssFixAvailable = available;
      }
    } catch {
      if (core.isCurrentRequest(token)) {
        dlssFixAvailable = false;
      }
    }
  }

  async function maybeProbeDlssFix(gameId: string, token: number): Promise<void> {
    if (!core.isCurrentRequest(token)) {
      return;
    }
    if (core.state?.status !== 'installed' || core.state.dlss_fix_installed) {
      return;
    }
    if (core.updateReport?.dlssFix !== null) {
      return;
    }
    await probeDlssFixAvailability(gameId, token);
  }

  async function load(gameId: string): Promise<void> {
    dlssFixAvailable = false;
    await core.load(gameId);
    await maybeProbeDlssFix(gameId, core.requestToken);
  }

  async function checkForUpdates(gameId: string): Promise<void> {
    dlssFixAvailable = false;
    await core.checkForUpdates(gameId);
    await maybeProbeDlssFix(gameId, core.requestToken);
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

  async function install(
    gameId: string,
    channel: ReshadeChannel,
    confirmAnticheat: boolean,
  ): Promise<boolean> {
    return core.runBusyMutation(gameId, () => api.install(gameId, channel, confirmAnticheat), {
      errorKey: 'gameDetails.renodx.installError',
      onSuccess: async (token) => {
        selectedReshadeChannel = channel;
        await refreshVulkanLayerStatus(token);
        dlssFixAvailable = false;
        await maybeProbeDlssFix(gameId, token);
      },
      notifyExclusivity: true,
    });
  }

  async function installFromFile(
    gameId: string,
    filePath: string,
    channel: ReshadeChannel,
    confirmAnticheat: boolean,
  ): Promise<boolean> {
    return core.runBusyMutation(
      gameId,
      () => api.installFromFile(gameId, filePath, channel, confirmAnticheat),
      {
        errorKey: 'gameDetails.renodx.installError',
        onSuccess: async (token) => {
          selectedReshadeChannel = channel;
          await refreshVulkanLayerStatus(token);
          dlssFixAvailable = false;
          await maybeProbeDlssFix(gameId, token);
        },
        notifyExclusivity: true,
      },
    );
  }

  async function update(gameId: string): Promise<boolean> {
    return core.runBusyMutation(gameId, () => api.update(gameId), {
      errorKey: 'gameDetails.renodx.updateError',
      requireUpdateAvailable: true,
      onSuccess: async (token) => {
        dlssFixAvailable = false;
        await maybeProbeDlssFix(gameId, token);
      },
    });
  }

  async function switchChannel(gameId: string, channel: ReshadeChannel): Promise<boolean> {
    const action = availabilitySnapshot.actions.switch_channel;
    if (
      core.busy ||
      core.state?.status !== 'installed' ||
      core.state.host_kind !== 'proxy' ||
      action?.enabled !== true ||
      action.target_channel !== channel ||
      channel === currentHostChannel(availabilitySnapshot)
    ) {
      return false;
    }
    return core.runBusyMutation(gameId, () => api.switchChannel(gameId, channel), {
      errorKey: 'gameDetails.renodx.switchError',
      onSuccess: () => {
        selectedReshadeChannel = channel;
        availabilitySnapshot = {
          ...availabilitySnapshot,
          hostFacts: {
            ...availabilitySnapshot.hostFacts,
            channel: {
              ...availabilitySnapshot.hostFacts.channel,
              effective: channel,
              detected: channel,
            },
          },
        };
      },
    });
  }

  function setSelectedReshadeChannel(channel: ReshadeChannel): void {
    selectedReshadeChannel = degradeUnsupportedStableChannel(
      channel,
      availabilitySnapshot.reshadeStableSupported,
    );
  }

  async function uninstall(gameId: string): Promise<boolean> {
    return core.runBusyMutation(gameId, () => api.uninstall(gameId), {
      errorKey: 'gameDetails.renodx.uninstallError',
      clearDownloadProgress: false,
      notifyExclusivity: true,
      onSuccess: async (token) => {
        dlssFixAvailable = false;
        await maybeProbeDlssFix(gameId, token);
      },
    });
  }

  async function installDlssFix(gameId: string): Promise<boolean> {
    return core.runBusyMutation(gameId, () => api.installDlssFix(gameId), {
      errorKey: 'gameDetails.renodx.dlssFixInstallError',
      onSuccess: async (token) => {
        dlssFixAvailable = false;
        await maybeProbeDlssFix(gameId, token);
      },
    });
  }

  async function uninstallDlssFix(gameId: string): Promise<boolean> {
    return core.runBusyMutation(gameId, () => api.uninstallDlssFix(gameId), {
      errorKey: 'gameDetails.renodx.dlssFixRemoveError',
      clearDownloadProgress: false,
      onSuccess: async (token) => {
        dlssFixAvailable = false;
        await maybeProbeDlssFix(gameId, token);
      },
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
      get externalLabelKey() {
        return externalLabelKey;
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
      get externalNotes() {
        return externalNotes;
      },
      get externalRequiresConfirmation() {
        return externalRequiresConfirmation;
      },
      get vulkanLayer() {
        return vulkanLayer;
      },
      get dlssFixUpdate() {
        return dlssFixUpdate;
      },
      get vulkanUpdateDiagnostics() {
        return vulkanUpdateDiagnostics;
      },
      get dlssFixInstalled() {
        return dlssFixInstalled;
      },
      get dlssFixAvailable() {
        return dlssFixAvailable;
      },
      load,
      checkForUpdates,
      install,
      installFromFile,
      setSelectedReshadeChannel,
      switchChannel,
      update,
      uninstall,
      installDlssFix,
      uninstallDlssFix,
    },
  );
}
