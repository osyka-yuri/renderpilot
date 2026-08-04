import {
  t,
  translateExternalMessage,
  type MessageKeyForParams,
  type MessageKeyWithoutParams,
} from '@shared/i18n';

import type {
  ActionDescriptor,
  HostAddonSupport,
  HostDetection,
  HostFacts,
  HostUpdateStatus,
  RiskAssessment,
  RiskSeverity,
} from './types';
import { toolMessageKey as toolKey, type ToolI18nPrefix } from './tool-message-key';

type DateMessageKey = MessageKeyForParams<Readonly<{ date: string | number }>>;
type TimeMessageKey = MessageKeyForParams<Readonly<{ time: string | number }>>;
type VersionMessageKey = MessageKeyForParams<Readonly<{ version: string | number }>>;

export type AddonInstallableLabels = {
  installAction: MessageKeyWithoutParams;
  installing: MessageKeyWithoutParams;
  confidenceLabel: MessageKeyWithoutParams;
  hostCustomBuild: MessageKeyWithoutParams;
  hostConflictBlocksInstall: MessageKeyWithoutParams;
  fullAddonWarning: MessageKeyWithoutParams;
  confirmTitle: MessageKeyWithoutParams;
  confirmBody: MessageKeyWithoutParams;
  confirmAccept: MessageKeyWithoutParams;
};

export type AddonInstalledLabels = {
  statusLabel: MessageKeyWithoutParams;
  statusInstalled: MessageKeyWithoutParams;
  freshnessLabel: MessageKeyWithoutParams;
  addonDated: DateMessageKey;
  installedOn: DateMessageKey;
  lastChecked: TimeMessageKey;
  lastCheckedNever: MessageKeyWithoutParams;
  fullAddonWarning: MessageKeyWithoutParams;
  componentReshade: MessageKeyWithoutParams;
  componentAddon: MessageKeyWithoutParams;
  checking: MessageKeyWithoutParams;
  actionCheckUpdates: MessageKeyWithoutParams;
  updating: MessageKeyWithoutParams;
  actionUpdate: MessageKeyWithoutParams;
  actionRepair: MessageKeyWithoutParams;
  actionUninstall: MessageKeyWithoutParams;
  uninstallConfirmTitle: MessageKeyWithoutParams;
  uninstallConfirmBody: MessageKeyWithoutParams;
  uninstallConfirmAction: MessageKeyWithoutParams;
};

export type HostDescriptionPart =
  | { kind: 'version'; key: VersionMessageKey; version: string }
  | { kind: 'message'; key: MessageKeyWithoutParams };

export type HostDescription =
  | { kind: 'conflict'; key: MessageKeyWithoutParams }
  | { kind: 'parts'; fallbackKey: MessageKeyWithoutParams; parts: HostDescriptionPart[] };

/** The tool-specific i18n keys {@link getHostDescription} renders with. */
export type HostDescriptionKeys = {
  versionKey: VersionMessageKey;
  versionUnknownKey: MessageKeyWithoutParams;
  conflictKey: MessageKeyWithoutParams;
  customBuildKey: MessageKeyWithoutParams;
  addonSupportLabel: Record<Exclude<HostAddonSupport, 'full'>, MessageKeyWithoutParams>;
  hostUpdateStatusLabel: Record<Exclude<HostUpdateStatus, 'current'>, MessageKeyWithoutParams>;
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

/** Severity-based fallback keys when the backend risk `message_key` is missing. */
type RiskFallbackKeys = {
  warn: MessageKeyWithoutParams;
  safe: MessageKeyWithoutParams;
};

function riskFallbackKey(severity: RiskSeverity, keys: RiskFallbackKeys): MessageKeyWithoutParams {
  switch (severity) {
    case 'warn':
      return keys.warn;
    default:
      return keys.safe;
  }
}

/**
 * Humanizes an action disabled-reason key when it is not in the catalog: drops
 * the dotted namespace and turns underscores into spaces (`a.b.foo_bar` → `foo bar`).
 */
function humanizeMessageKey(key: string): string {
  return key.replace(/^.*\./, '').replaceAll('_', ' ');
}

/**
 * Shared host i18n key maps for a tool's `gameDetails.<tool>` prefix.
 * Tool presenters pass these into {@link getHostDescription}.
 */
function createHostLabelMaps(prefix: ToolI18nPrefix): {
  addonSupportLabel: Record<'limited' | 'unknown', MessageKeyWithoutParams>;
  hostUpdateStatusLabel: Record<
    'update_available' | 'repair_available' | 'unknown_needs_validation' | 'channel_mismatch',
    MessageKeyWithoutParams
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

  const describeHost = (input: { detection: HostDetection; facts: HostFacts }): string =>
    formatHostDescription(getReshadeDescription(input));

  const riskMessage = (risk: RiskAssessment): string => {
    const fallback = riskFallbackKey(risk.severity, hostLabels.riskFallback);
    return translateExternalMessage({
      key: risk.message_key,
      fallback: t(fallback),
      params: { addonName },
    });
  };

  return {
    getReshadeDescription,
    describeHost,
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

/**
 * Shared installable-view i18n keys for a tool's `gameDetails.<tool>` prefix.
 * Confirm body/accept stay on the shared `gameDetails.addon.*` namespace.
 */
export function createInstallableLabels(prefix: ToolI18nPrefix): AddonInstallableLabels {
  return {
    installAction: toolKey(prefix, 'actionInstall'),
    installing: toolKey(prefix, 'installing'),
    confidenceLabel: toolKey(prefix, 'confidenceLabel'),
    hostCustomBuild: toolKey(prefix, 'host.customBuild'),
    hostConflictBlocksInstall: toolKey(prefix, 'host.conflictBlocksInstall'),
    fullAddonWarning: 'gameDetails.addon.fullAddonWarning',
    confirmTitle: toolKey(prefix, 'confirmTitle'),
    confirmBody: 'gameDetails.addon.confirmBody',
    confirmAccept: 'gameDetails.addon.confirmAccept',
  };
}

/** Confidence badge labels for a tool's `gameDetails.<tool>` prefix. */
export function createConfidenceLabelKeys(
  prefix: ToolI18nPrefix,
): Record<'verified' | 'experimental' | 'untested', MessageKeyWithoutParams> {
  return {
    verified: toolKey(prefix, 'confidenceVerified'),
    experimental: toolKey(prefix, 'confidenceExperimental'),
    untested: toolKey(prefix, 'confidenceUntested'),
  };
}
