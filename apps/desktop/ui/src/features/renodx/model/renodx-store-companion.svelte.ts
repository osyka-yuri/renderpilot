import type { createAddonStore } from '@entities/addon';

import type { RenoDxApi } from '../api/desktop';
import type {
  AvailabilityReport,
  DlssFixAvailability,
  RenoDxInstallState,
  RenoDxUpdateReport,
  VulkanLayerReport,
} from './types';

type RenoDxCore = Pick<
  ReturnType<typeof createAddonStore<RenoDxInstallState, RenoDxUpdateReport, AvailabilityReport>>,
  'load' | 'retry' | 'checkForUpdates' | 'requestToken' | 'loadError' | 'isCurrentRequest'
>;

export type RenoDxCompanionStoreOptions = {
  api: Pick<RenoDxApi, 'dlssFixAvailability' | 'vulkanLayerStatus'>;
  core: RenoDxCore;
  setSelectedChannel: (channel: Parameters<RenoDxApi['install']>[1]) => void;
  setVulkanLayer: (report: VulkanLayerReport) => void;
  onGameDetailsInvalidate?: (gameId: string) => void | Promise<void>;
};

/**
 * Owns the companion probes that are ordered around the shared add-on core.
 * Keeping these probes together makes their request-token guards and
 * post-commit ordering explicit without making the main store a second
 * mutation coordinator.
 */
export function createRenoDxCompanionStore(options: RenoDxCompanionStoreOptions) {
  const { api, core, setSelectedChannel, setVulkanLayer, onGameDetailsInvalidate } = options;
  let dlssFixAvailability = $state<DlssFixAvailability | null>(null);

  function clear(): void {
    dlssFixAvailability = null;
  }

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
    clear();
    const loading = core.load(gameId);
    const token = core.requestToken;
    await loading;
    if (core.isCurrentRequest(token) && !core.loadError) {
      await probeDlssFixAvailability(gameId, token);
    }
  }

  async function retry(gameId: string): Promise<void> {
    clear();
    const loading = core.retry(gameId);
    const token = core.requestToken;
    await loading;
    if (core.isCurrentRequest(token) && !core.loadError) {
      await probeDlssFixAvailability(gameId, token);
    }
  }

  async function checkForUpdates(gameId: string): Promise<void> {
    clear();
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
        setVulkanLayer(report);
      }
    } catch {
      // Best-effort: a failed layer-status refresh leaves the previous report.
    }
  }

  /**
   * Refreshes companion state after a successful install-like mutation.
   * The synthetic install report remains the source of freshness; these are
   * token-guarded advisory probes only.
   */
  async function afterInstallLikeCommit(
    gameId: string,
    token: number,
    channel?: Parameters<RenoDxApi['install']>[1],
  ): Promise<void> {
    if (!core.isCurrentRequest(token)) {
      return;
    }
    if (channel !== undefined) {
      setSelectedChannel(channel);
      await refreshVulkanLayerStatus(token);
    }
    if (!core.isCurrentRequest(token)) {
      return;
    }
    clear();
    await probeDlssFixAvailability(gameId, token);
  }

  async function afterCapabilityCommit(
    gameId: string,
    token: number,
    channel?: Parameters<RenoDxApi['install']>[1],
  ): Promise<void> {
    await afterInstallLikeCommit(gameId, token, channel);
    if (core.isCurrentRequest(token)) {
      await onGameDetailsInvalidate?.(gameId);
    }
  }

  return {
    get dlssFixAvailability() {
      return dlssFixAvailability;
    },
    load,
    retry,
    checkForUpdates,
    clear,
    afterInstallLikeCommit,
    afterCapabilityCommit,
  };
}
