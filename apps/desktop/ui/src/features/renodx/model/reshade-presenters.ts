import { t, type MessageKeyWithoutParams } from '@shared/i18n';
import { ADDON_DISPLAY_NAME } from '@shared/model';
import {
  createReshadePresenters,
  type HostDetection,
  type HostFacts,
  type ReshadeChannel,
  type RiskAssessment,
} from '@entities/addon';

import type {
  LayerDiagnosticReason,
  RenoDxAddonState,
  VulkanLayerDetection,
  VulkanLoaderVisibility,
} from './types';

export const CHANNEL_LABEL = {
  stable: 'gameDetails.renodx.channel.stable',
  nightly: 'gameDetails.renodx.channel.nightly',
} satisfies Record<ReshadeChannel, MessageKeyWithoutParams>;

export type VulkanLayerDisplayState = VulkanLayerDetection | 'needs_repair';

export const VULKAN_LAYER_STATE_LABEL = {
  not_installed: 'gameDetails.renodx.vulkanLayer.state.not_installed',
  installed: 'gameDetails.renodx.vulkanLayer.state.installed',
  installed_disabled: 'gameDetails.renodx.vulkanLayer.state.installed_disabled',
  external_read_only: 'gameDetails.renodx.vulkanLayer.state.external_read_only',
  conflict: 'gameDetails.renodx.vulkanLayer.state.conflict',
  needs_repair: 'gameDetails.renodx.vulkanLayer.state.needs_repair',
  unsupported: 'gameDetails.renodx.vulkanLayer.state.unsupported',
} satisfies Record<VulkanLayerDisplayState, MessageKeyWithoutParams>;

export const VULKAN_LAYER_PRIMARY_ACTION_LABEL = {
  install: 'gameDetails.renodx.vulkanLayer.action.install',
  update: 'gameDetails.renodx.vulkanLayer.action.update',
  switch_channel: 'gameDetails.renodx.vulkanLayer.action.switch_channel',
  repair: 'gameDetails.renodx.vulkanLayer.action.repair',
} satisfies Record<'install' | 'update' | 'switch_channel' | 'repair', MessageKeyWithoutParams>;

export const VULKAN_LOADER_VISIBILITY_NOTE = {
  hkcu_not_visible_when_elevated:
    'gameDetails.renodx.vulkanLayer.diagnostic.hkcu_not_visible_when_elevated',
  ambiguous: 'gameDetails.renodx.vulkanLayer.diagnostic.ambiguous_loader_visibility',
} satisfies Partial<Record<VulkanLoaderVisibility, MessageKeyWithoutParams>>;

const presenters = createReshadePresenters('gameDetails.renodx', ADDON_DISPLAY_NAME.renodx);

/**
 * Renders an install-risk message: the backend's own `message_key` when it's
 * in the catalog, else the severity-based fallback — either way interpolated
 * with RenoDX's display name (the risk copy is addon-agnostic).
 */
export function riskMessage(risk: RiskAssessment): string {
  return presenters.riskMessage(risk);
}

/** Structured host description → single i18n string (tool keys via factory). */
export function describeReshadeHost(input: { detection: HostDetection; facts: HostFacts }): string {
  return presenters.describeHost(input);
}

/** Managed RenderPilot layer: installed or installed-but-disabled in registry. */
export function isManagedVulkanLayer(detection: VulkanLayerDetection | null | undefined): boolean {
  return detection === 'installed' || detection === 'installed_disabled';
}

/**
 * Host version label for the shared Vulkan layer settings card.
 * `null` when detection is not a managed install (no placeholder/spacer text).
 * Shares message keys with the host version part of {@link describeReshadeHost}.
 */
export function vulkanLayerHostDescription(
  detection: VulkanLayerDetection | null | undefined,
  version: string | null | undefined,
): string | null {
  if (!isManagedVulkanLayer(detection)) {
    return null;
  }
  return version
    ? t('gameDetails.renodx.host.version', { version })
    : t('gameDetails.renodx.host.versionUnknown');
}

/**
 * “Check for updates” is meaningful once we have a real detection that is not
 * empty or platform-unsupported.
 */
export function canCheckVulkanLayerUpdates(
  detection: VulkanLayerDetection | null | undefined,
): boolean {
  return detection != null && detection !== 'not_installed' && detection !== 'unsupported';
}

export function getAddonDescriptionKey(
  addon: RenoDxAddonState | null,
  addonTracked: boolean | null,
): MessageKeyWithoutParams {
  if (addon?.enabled_by_config === false) {
    return 'gameDetails.renodx.component.addonDisabled';
  }
  if (addonTracked === false) {
    return 'gameDetails.renodx.component.addonFileInstall';
  }
  return 'gameDetails.renodx.component.addonDesc';
}

/**
 * i18n keys for every `LayerDiagnosticReason` the backend can emit. Shared by
 * the Vulkan layer panel (detection diagnostics) and the installed panel
 * (update-check digest-mismatch diagnostics) so both render the same reason
 * labels.
 */
export const VULKAN_DIAGNOSTIC_LABEL = {
  external_layer_detected: 'gameDetails.renodx.vulkanLayer.diagnostic.external_layer_detected',
  duplicate_layer_manifest: 'gameDetails.renodx.vulkanLayer.diagnostic.duplicate_layer_manifest',
  ambiguous_loader_visibility:
    'gameDetails.renodx.vulkanLayer.diagnostic.ambiguous_loader_visibility',
  missing_layer_dll: 'gameDetails.renodx.vulkanLayer.diagnostic.missing_layer_dll',
  unreadable_dll: 'gameDetails.renodx.vulkanLayer.diagnostic.unreadable_dll',
  missing_manifest: 'gameDetails.renodx.vulkanLayer.diagnostic.missing_manifest',
  registry_missing: 'gameDetails.renodx.vulkanLayer.diagnostic.registry_missing',
  registry_disabled: 'gameDetails.renodx.vulkanLayer.diagnostic.registry_disabled',
  unsupported_architecture: 'gameDetails.renodx.vulkanLayer.diagnostic.unsupported_architecture',
  hkcu_not_visible_when_elevated:
    'gameDetails.renodx.vulkanLayer.diagnostic.hkcu_not_visible_when_elevated',
  manifest_malformed: 'gameDetails.renodx.vulkanLayer.diagnostic.manifest_malformed',
  registry_scope_not_writable:
    'gameDetails.renodx.vulkanLayer.diagnostic.registry_scope_not_writable',
  permission_denied: 'gameDetails.renodx.vulkanLayer.diagnostic.permission_denied',
  backend_validation_failed: 'gameDetails.renodx.vulkanLayer.diagnostic.backend_validation_failed',
  hash_mismatch: 'gameDetails.renodx.vulkanLayer.diagnostic.hash_mismatch',
  db_only_fallback: 'gameDetails.renodx.vulkanLayer.diagnostic.db_only_fallback',
} satisfies Record<LayerDiagnosticReason, MessageKeyWithoutParams>;
