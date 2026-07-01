import type { MessageKey } from '@shared/i18n';

import type {
  ActionDescriptor,
  HostAddonSupport,
  HostDetection,
  HostFacts,
  HostUpdateStatus,
  LayerDiagnosticReason,
  RenoDxAddonState,
  ReshadeChannel,
  RiskSeverity,
  VulkanLayerDetection,
  VulkanLoaderVisibility,
} from './types';

export const ADDON_SUPPORT_LABEL = {
  limited: 'gameDetails.renodx.host.addons.none',
  unknown: 'gameDetails.renodx.host.addons.unknown',
} satisfies Record<Exclude<HostAddonSupport, 'full'>, MessageKey>;

export const HOST_UPDATE_STATUS_LABEL = {
  update_available: 'gameDetails.renodx.host.action.update_host',
  repair_available: 'gameDetails.renodx.host.action.repair_host',
  unknown_needs_validation: 'gameDetails.renodx.fresh.validationRequired',
  channel_mismatch: 'gameDetails.renodx.fresh.channelMismatch',
} satisfies Record<Exclude<HostUpdateStatus, 'current'>, MessageKey>;

export const CHANNEL_LABEL = {
  stable: 'gameDetails.renodx.channel.stable',
  nightly: 'gameDetails.renodx.channel.nightly',
} satisfies Record<ReshadeChannel, MessageKey>;

export type VulkanLayerDisplayState = VulkanLayerDetection | 'needs_repair';

export const VULKAN_LAYER_STATE_LABEL = {
  not_installed: 'gameDetails.renodx.vulkanLayer.state.not_installed',
  installed: 'gameDetails.renodx.vulkanLayer.state.installed',
  installed_disabled: 'gameDetails.renodx.vulkanLayer.state.installed_disabled',
  external_read_only: 'gameDetails.renodx.vulkanLayer.state.external_read_only',
  conflict: 'gameDetails.renodx.vulkanLayer.state.conflict',
  needs_repair: 'gameDetails.renodx.vulkanLayer.state.needs_repair',
  unsupported: 'gameDetails.renodx.vulkanLayer.state.unsupported',
} satisfies Record<VulkanLayerDisplayState, MessageKey>;

export const VULKAN_LAYER_PRIMARY_ACTION_LABEL = {
  install: 'gameDetails.renodx.vulkanLayer.action.install',
  update: 'gameDetails.renodx.vulkanLayer.action.update',
  switch_channel: 'gameDetails.renodx.vulkanLayer.action.switch_channel',
  repair: 'gameDetails.renodx.vulkanLayer.action.repair',
} satisfies Record<'install' | 'update' | 'switch_channel' | 'repair', MessageKey>;

export const VULKAN_LOADER_VISIBILITY_NOTE = {
  hkcu_not_visible_when_elevated:
    'gameDetails.renodx.vulkanLayer.diagnostic.hkcu_not_visible_when_elevated',
  ambiguous: 'gameDetails.renodx.vulkanLayer.diagnostic.ambiguous_loader_visibility',
} satisfies Partial<Record<VulkanLoaderVisibility, MessageKey>>;

export type ReshadeDescriptionPart =
  | {
      kind: 'version';
      key: 'gameDetails.renodx.host.version';
      version: string;
    }
  | {
      kind: 'message';
      key: MessageKey;
    };

export type ReshadeDescription =
  | {
      kind: 'conflict';
      key: 'gameDetails.renodx.host.conflictMultiple';
    }
  | {
      kind: 'parts';
      fallbackKey: 'gameDetails.renodx.host.versionUnknown';
      parts: ReshadeDescriptionPart[];
    };

export function getReshadeDescription({
  detection,
  facts,
}: {
  detection: HostDetection;
  facts: HostFacts;
}): ReshadeDescription {
  if (detection === 'conflict') {
    return {
      kind: 'conflict',
      key: 'gameDetails.renodx.host.conflictMultiple',
    };
  }

  const parts: ReshadeDescriptionPart[] = [];
  if (detection === 'present' && facts.version) {
    parts.push({
      kind: 'version',
      key: 'gameDetails.renodx.host.version',
      version: facts.version,
    });
  }
  if (detection === 'present' && facts.addon_support !== 'full') {
    parts.push({
      kind: 'message',
      key: ADDON_SUPPORT_LABEL[facts.addon_support],
    });
  }
  if (facts.update_status !== 'current') {
    parts.push({
      kind: 'message',
      key: HOST_UPDATE_STATUS_LABEL[facts.update_status],
    });
  }

  return {
    kind: 'parts',
    fallbackKey: 'gameDetails.renodx.host.versionUnknown',
    parts,
  };
}

export type HostVersionDescription =
  | { kind: 'version'; key: 'gameDetails.renodx.host.version'; version: string }
  | { kind: 'unknown'; key: 'gameDetails.renodx.host.versionUnknown' };

/**
 * The version-or-unknown host description for surfaces that only show a
 * version (no add-on-support/update-status context), such as the shared
 * Vulkan layer settings panel. Shares message keys with the version part of
 * {@link getReshadeDescription} so both surfaces read identically.
 */
export function hostVersionDescription(version: string | null | undefined): HostVersionDescription {
  return version
    ? { kind: 'version', key: 'gameDetails.renodx.host.version', version }
    : { kind: 'unknown', key: 'gameDetails.renodx.host.versionUnknown' };
}

/**
 * The humanized disabled-reason message for a backend-authored action, when
 * it's disabled with a reason — `undefined` otherwise. Shared by every panel
 * that surfaces why an install/update/repair action is unavailable.
 */
export function actionDisabledMessage(action: ActionDescriptor | undefined): string | undefined {
  return action?.enabled === false && action.disabled_reason
    ? humanizeMessageKey(action.disabled_reason)
    : undefined;
}

export function getAddonDescriptionKey(
  addon: RenoDxAddonState | null,
  addonTracked: boolean | null,
): MessageKey {
  if (addon?.enabled_by_config === false) {
    return 'gameDetails.renodx.component.addonDisabled';
  }
  if (addonTracked === false) {
    return 'gameDetails.renodx.component.addonFileInstall';
  }
  return 'gameDetails.renodx.component.addonDesc';
}

/**
 * The severity-based fallback message key for an install risk, shown when the
 * backend's `message_key` is not present in the i18n catalog.
 */
export function riskFallbackKey(severity: RiskSeverity): MessageKey {
  switch (severity) {
    case 'block':
      return 'gameDetails.renodx.riskBlocked';
    case 'warn':
      return 'gameDetails.renodx.riskWarn';
    default:
      return 'gameDetails.renodx.riskSafe';
  }
}

/**
 * Humanizes an i18n key for display when it is not in the catalog: drops the
 * dotted namespace and turns underscores into spaces (`a.b.foo_bar` → `foo bar`).
 * Used as the fallback for backend-provided note/requirement keys.
 */
export function humanizeMessageKey(key: string): string {
  return key.replace(/^.*\./, '').replace(/_/g, ' ');
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
} satisfies Record<LayerDiagnosticReason, MessageKey>;
