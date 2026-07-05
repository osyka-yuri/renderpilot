import { t, translateKey, type MessageKey } from '@shared/i18n';

import type {
  ActionDescriptor,
  HostAddonSupport,
  HostDetection,
  HostFacts,
  HostUpdateStatus,
  RiskAssessment,
  RiskSeverity,
} from './types';

/** i18n key prefix for tool-specific freshness / status copy. */
export type ToolI18nPrefix = 'gameDetails.renodx';

export type AddonInstallableLabels = {
  installAction: MessageKey;
  installing: MessageKey;
  confidenceLabel: MessageKey;
  hostCustomBuild: MessageKey;
  hostConflictBlocksInstall: MessageKey;
  fullAddonWarning: MessageKey;
  confirmTitle: MessageKey;
  confirmBody: MessageKey;
  confirmAccept: MessageKey;
};

export type AddonInstalledLabels = {
  statusLabel: MessageKey;
  statusInstalled: MessageKey;
  freshnessLabel: MessageKey;
  addonDated: MessageKey;
  installedOn: MessageKey;
  lastChecked: MessageKey;
  lastCheckedNever: MessageKey;
  fullAddonWarning: MessageKey;
  componentReshade: MessageKey;
  componentAddon: MessageKey;
  checking: MessageKey;
  actionCheckUpdates: MessageKey;
  updating: MessageKey;
  actionUpdate: MessageKey;
  actionRepair: MessageKey;
  actionUninstall: MessageKey;
  uninstallConfirmTitle: MessageKey;
  uninstallConfirmBody: MessageKey;
  uninstallConfirmAction: MessageKey;
};

export type HostDescriptionPart =
  { kind: 'version'; key: MessageKey; version: string } | { kind: 'message'; key: MessageKey };

export type HostDescription =
  | { kind: 'conflict'; key: MessageKey }
  | { kind: 'parts'; fallbackKey: MessageKey; parts: HostDescriptionPart[] };

/** The tool-specific i18n keys {@link getHostDescription} renders with. */
export type HostDescriptionKeys = {
  versionKey: MessageKey;
  versionUnknownKey: MessageKey;
  conflictKey: MessageKey;
  customBuildKey: MessageKey;
  addonSupportLabel: Record<Exclude<HostAddonSupport, 'full'>, MessageKey>;
  hostUpdateStatusLabel: Record<Exclude<HostUpdateStatus, 'current'>, MessageKey>;
};

/**
 * Describes the detected ReShade host for display: a plain conflict message,
 * or a fallback-vs-parts breakdown (version / add-on-support / update-status).
 * Identical across every tool; only the i18n keys in `keys` differ.
 */
function getHostDescription(
  detection: HostDetection,
  facts: HostFacts,
  keys: HostDescriptionKeys,
): HostDescription {
  // A recognized custom build (e.g. GShade) is reported as a conflict at the
  // backend (the slot is never safe to write to), but it isn't a problem to
  // resolve — say so plainly instead of the generic conflict/update wording.
  if (facts.is_custom_build) {
    return {
      kind: 'parts',
      fallbackKey: keys.versionUnknownKey,
      parts: [{ kind: 'message', key: keys.customBuildKey }],
    };
  }
  if (detection === 'conflict') {
    return { kind: 'conflict', key: keys.conflictKey };
  }

  const parts: HostDescriptionPart[] = [];
  if (detection === 'present' && facts.version) {
    parts.push({ kind: 'version', key: keys.versionKey, version: facts.version });
  }
  if (detection === 'present' && facts.addon_support !== 'full') {
    parts.push({ kind: 'message', key: keys.addonSupportLabel[facts.addon_support] });
  }
  if (facts.update_status !== 'current') {
    parts.push({ kind: 'message', key: keys.hostUpdateStatusLabel[facts.update_status] });
  }

  return { kind: 'parts', fallbackKey: keys.versionUnknownKey, parts };
}

/**
 * The humanized disabled-reason message for a backend-authored action, when
 * it's disabled with a reason — `undefined` otherwise.
 */
export function actionDisabledMessage(action: ActionDescriptor | undefined): string | undefined {
  return action?.enabled === false && action.disabled_reason
    ? humanizeMessageKey(action.disabled_reason)
    : undefined;
}

/** The tool-specific i18n keys {@link riskFallbackKey} chooses between. */
export type RiskFallbackKeys = {
  warn: MessageKey;
  safe: MessageKey;
};

/**
 * The severity-based fallback message key for an install risk, shown when the
 * backend's `message_key` is not present in the i18n catalog.
 */
function riskFallbackKey(severity: RiskSeverity, keys: RiskFallbackKeys): MessageKey {
  switch (severity) {
    case 'warn':
      return keys.warn;
    default:
      return keys.safe;
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

/** Catalog keys are mirrored under both tool prefixes; cast keeps MessageKey strict. */
function toolKey(prefix: ToolI18nPrefix, rest: string): MessageKey {
  return `${prefix}.${rest}` as MessageKey;
}

/**
 * Shared host i18n key maps for a tool's `gameDetails.<tool>` prefix.
 * Tool presenters pass these into {@link getHostDescription} / {@link riskFallbackKey}.
 */
function createHostLabelMaps(prefix: ToolI18nPrefix): {
  addonSupportLabel: Record<'limited' | 'unknown', MessageKey>;
  hostUpdateStatusLabel: Record<
    'update_available' | 'repair_available' | 'unknown_needs_validation' | 'channel_mismatch',
    MessageKey
  >;
  riskFallback: RiskFallbackKeys;
  descriptionKeys: Pick<
    HostDescriptionKeys,
    'versionKey' | 'versionUnknownKey' | 'conflictKey' | 'customBuildKey'
  >;
} {
  return {
    addonSupportLabel: {
      limited: toolKey(prefix, 'host.addons.none'),
      unknown: toolKey(prefix, 'host.addons.unknown'),
    },
    hostUpdateStatusLabel: {
      update_available: toolKey(prefix, 'host.action.update_host'),
      repair_available: toolKey(prefix, 'host.action.repair_host'),
      unknown_needs_validation: toolKey(prefix, 'fresh.validationRequired'),
      channel_mismatch: toolKey(prefix, 'fresh.channelMismatch'),
    },
    riskFallback: {
      warn: 'gameDetails.addon.riskWarn',
      safe: 'gameDetails.addon.riskSafe',
    },
    descriptionKeys: {
      versionKey: toolKey(prefix, 'host.version'),
      versionUnknownKey: toolKey(prefix, 'host.versionUnknown'),
      conflictKey: toolKey(prefix, 'host.conflictMultiple'),
      customBuildKey: toolKey(prefix, 'host.customBuild'),
    },
  };
}

/**
 * Renders a structured {@link HostDescription} into a single i18n string.
 * Conflict descriptions get a plain translation; parts are joined with ' · ',
 * falling back to the description's `fallbackKey` when there are no parts.
 * Tool-agnostic — only the keys already stored on the description matter.
 */
export function formatHostDescription(description: HostDescription): string {
  if (description.kind === 'conflict') {
    return t(description.key);
  }
  const parts = description.parts.map((part) =>
    part.kind === 'version' ? t(part.key, { version: part.version }) : t(part.key),
  );
  return parts.length > 0 ? parts.join(' · ') : t(description.fallbackKey);
}

/**
 * Everything a ReShade-hosted tool's presenter module derives from its
 * `gameDetails.<tool>` prefix and display name: the structured host description,
 * a composed string form of that description, and the install-risk message
 * (backend `message_key`, or the severity-based fallback — either way
 * interpolated with `addonName`, since the risk copy is addon-agnostic).
 */
export function createReshadePresenters(
  prefix: ToolI18nPrefix,
  addonName: string,
): {
  getReshadeDescription: (input: { detection: HostDetection; facts: HostFacts }) => HostDescription;
  describeHost: (input: { detection: HostDetection; facts: HostFacts }) => string;
  riskFallbackKey: (severity: RiskSeverity) => MessageKey;
  riskMessage: (risk: RiskAssessment) => string;
} {
  const hostLabels = createHostLabelMaps(prefix);

  const getReshadeDescription = ({
    detection,
    facts,
  }: {
    detection: HostDetection;
    facts: HostFacts;
  }): HostDescription => {
    return getHostDescription(detection, facts, {
      ...hostLabels.descriptionKeys,
      addonSupportLabel: hostLabels.addonSupportLabel,
      hostUpdateStatusLabel: hostLabels.hostUpdateStatusLabel,
    });
  };

  const toolRiskFallbackKey = (severity: RiskSeverity): MessageKey => {
    return riskFallbackKey(severity, hostLabels.riskFallback);
  };

  const describeHost = (input: { detection: HostDetection; facts: HostFacts }): string =>
    formatHostDescription(getReshadeDescription(input));

  const riskMessage = (risk: RiskAssessment): string => {
    return translateKey(risk.message_key, t(toolRiskFallbackKey(risk.severity)), { addonName });
  };

  return {
    getReshadeDescription,
    describeHost,
    riskFallbackKey: toolRiskFallbackKey,
    riskMessage,
  };
}

/**
 * Shared installed-panel i18n keys for a tool's `gameDetails.<tool>` prefix.
 * Tool panels layer only tool-specific extras on top of this shape.
 */
export function createInstalledLabels(prefix: ToolI18nPrefix): AddonInstalledLabels {
  return {
    statusLabel: toolKey(prefix, 'status.label'),
    statusInstalled: toolKey(prefix, 'statusInstalled'),
    freshnessLabel: toolKey(prefix, 'fresh.label'),
    addonDated: toolKey(prefix, 'addonDated'),
    installedOn: toolKey(prefix, 'installedOn'),
    lastChecked: toolKey(prefix, 'lastChecked'),
    lastCheckedNever: toolKey(prefix, 'lastCheckedNever'),
    fullAddonWarning: 'gameDetails.addon.fullAddonWarning',
    componentReshade: toolKey(prefix, 'component.reshade'),
    componentAddon: toolKey(prefix, 'component.addon'),
    checking: toolKey(prefix, 'fresh.checking'),
    actionCheckUpdates: toolKey(prefix, 'actionCheckUpdates'),
    updating: toolKey(prefix, 'updating'),
    actionUpdate: toolKey(prefix, 'actionUpdate'),
    actionRepair: toolKey(prefix, 'actionRepair'),
    actionUninstall: toolKey(prefix, 'actionUninstall'),
    uninstallConfirmTitle: toolKey(prefix, 'uninstallConfirmTitle'),
    uninstallConfirmBody: toolKey(prefix, 'uninstallConfirmBody'),
    uninstallConfirmAction: toolKey(prefix, 'uninstallConfirmAction'),
  };
}
