import { formatPresentedError } from '@shared/error-presentation';
import { t } from '@shared/i18n';
import { publishPresentedErrorNotification } from '@shared/notifications';
import { CATALOG_SETTING_KEYS, getCatalogSetting, setCatalogSetting } from '@entities/settings';
import { clearDownloadProgress } from '@shared/lib';
import { renodxApi, type RenoDxApi } from '../api/desktop';
import { normalizeReshadeChannel } from './renodx-store-helpers';
import type { VulkanLayerDisplayState } from './reshade-presenters';
import type { ReshadeChannel } from '@entities/addon';

import type { VulkanLayerManagementReport } from './types';

export const VULKAN_LAYER_PROGRESS_ID = 'renodx:vulkan_layer';

export type VulkanLayerPrimaryAction = 'install' | 'update' | 'switch_channel' | 'repair';

export type VulkanLayerSettingsStore = ReturnType<typeof createVulkanLayerSettingsStore>;

type SettingsApi = Pick<
  RenoDxApi,
  'vulkanLayerManagementStatus' | 'applyVulkanLayer' | 'removeVulkanLayer'
>;

export function createVulkanLayerSettingsStore(
  api: SettingsApi = renodxApi,
  settings = { getCatalogSetting, setCatalogSetting },
) {
  let report = $state<VulkanLayerManagementReport | null>(null);
  let selectedChannel = $state<ReshadeChannel>('stable');
  let loading = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let requestId = 0;

  const layer = $derived(report?.layer ?? null);
  const stableSupported = $derived(report?.reshade_stable_supported ?? true);
  const activeChannel = $derived(report?.recorded_channel ?? report?.default_channel ?? null);
  const updateStatus = $derived(report?.update_status ?? null);
  const selectedChannelSupported = $derived(selectedChannel !== 'stable' || stableSupported);
  /**
   * The detection state as displayed: a conflict the backend already offers an
   * update action for reads as "needs repair" rather than a bare "conflict",
   * since the fix is a routine update, not a manual resolution.
   */
  const displayState = $derived<VulkanLayerDisplayState | null>(
    layer?.layer_detection === 'conflict' && layer.actions.update?.enabled
      ? 'needs_repair'
      : (layer?.layer_detection ?? null),
  );
  const primaryAction = $derived.by((): VulkanLayerPrimaryAction | null => {
    if (!selectedChannelSupported) {
      return null;
    }
    const actions = layer?.actions;
    if (!actions) {
      return null;
    }
    if (actions.install?.enabled) {
      return 'install';
    }
    if (actions.resolve_conflict?.enabled) {
      return 'repair';
    }
    if (activeChannel && selectedChannel !== activeChannel && actions.switch_channel?.enabled) {
      return 'switch_channel';
    }
    if (actions.update?.enabled && updateStatus === 'available') {
      return 'update';
    }
    return null;
  });
  const primaryActionDescriptor = $derived.by(() => {
    if (!primaryAction || !layer) {
      return undefined;
    }
    return primaryAction === 'repair'
      ? layer.actions.resolve_conflict
      : layer.actions[primaryAction];
  });

  function normalizeChannel(
    channel: string | null | undefined,
    fallback: ReshadeChannel,
  ): ReshadeChannel {
    return normalizeReshadeChannel(channel, fallback);
  }

  async function loadStoredChannel(fallback: ReshadeChannel): Promise<ReshadeChannel> {
    try {
      const stored = await settings.getCatalogSetting(CATALOG_SETTING_KEYS.RENODX_RESHADE_CHANNEL);
      return normalizeChannel(stored.value, fallback);
    } catch {
      return normalizeChannel(null, fallback);
    }
  }

  function applyReport(next: VulkanLayerManagementReport, preferred: ReshadeChannel | null): void {
    report = next;
    const fallback = next.recorded_channel ?? next.default_channel;
    selectedChannel = normalizeChannel(preferred, fallback);
  }

  async function load(): Promise<void> {
    const token = ++requestId;
    loading = true;
    error = null;
    try {
      const next = await api.vulkanLayerManagementStatus();
      const stored = await loadStoredChannel(next.recorded_channel ?? next.default_channel);
      if (token !== requestId) {
        return;
      }
      applyReport(next, stored);
    } catch (loadError) {
      if (token !== requestId) {
        return;
      }
      error = formatPresentedError(loadError);
      publishPresentedErrorNotification(t('settings.renodx.vulkan.loadError'), loadError);
    } finally {
      if (token === requestId) {
        loading = false;
      }
    }
  }

  async function setSelectedChannel(channel: ReshadeChannel): Promise<void> {
    selectedChannel = normalizeChannel(channel, selectedChannel);
    try {
      await settings.setCatalogSetting(
        CATALOG_SETTING_KEYS.RENODX_RESHADE_CHANNEL,
        selectedChannel,
      );
    } catch (saveError) {
      publishPresentedErrorNotification(t('settings.renodx.vulkan.saveError'), saveError);
    }
  }

  async function apply(): Promise<boolean> {
    if (busy || primaryAction === null || primaryActionDescriptor?.enabled !== true) {
      return false;
    }
    busy = true;
    clearDownloadProgress([VULKAN_LAYER_PROGRESS_ID]);
    try {
      await settings.setCatalogSetting(
        CATALOG_SETTING_KEYS.RENODX_RESHADE_CHANNEL,
        selectedChannel,
      );
      const next = await api.applyVulkanLayer(selectedChannel);
      applyReport(next, selectedChannel);
      return true;
    } catch (applyError) {
      publishPresentedErrorNotification(t('settings.renodx.vulkan.applyError'), applyError);
      return false;
    } finally {
      busy = false;
    }
  }

  async function remove(): Promise<boolean> {
    if (busy || layer?.actions.remove?.enabled !== true) {
      return false;
    }
    busy = true;
    try {
      await api.removeVulkanLayer();
      const next = await api.vulkanLayerManagementStatus();
      applyReport(next, selectedChannel);
      return true;
    } catch (removeError) {
      publishPresentedErrorNotification(
        t('gameDetails.renodx.vulkanLayer.removeError'),
        removeError,
      );
      return false;
    } finally {
      busy = false;
    }
  }

  return {
    get report() {
      return report;
    },
    get layer() {
      return layer;
    },
    get selectedChannel() {
      return selectedChannel;
    },
    get stableSupported() {
      return stableSupported;
    },
    get activeChannel() {
      return activeChannel;
    },
    get displayState() {
      return displayState;
    },
    get primaryAction() {
      return primaryAction;
    },
    get primaryActionDescriptor() {
      return primaryActionDescriptor;
    },
    get loading() {
      return loading;
    },
    get busy() {
      return busy;
    },
    get error() {
      return error;
    },
    get updateStatus() {
      return updateStatus;
    },
    load,
    setSelectedChannel,
    apply,
    remove,
  };
}
