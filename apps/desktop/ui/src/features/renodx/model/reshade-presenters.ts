import type { MessageKey } from '@shared/i18n';

import type {
  ManagedReshadeHealth,
  RenoDxAddonState,
  ReshadeAddonSupport,
  ReshadeChannel,
  ReshadeHost,
  ReshadeHostAction,
  ReshadeHostOwnership,
  RiskSeverity,
} from './types';

export const ADDON_SUPPORT_LABEL = {
  none: 'gameDetails.renodx.host.addons.none',
  unknown: 'gameDetails.renodx.host.addons.unknown',
} satisfies Record<Exclude<ReshadeAddonSupport, 'full'>, MessageKey>;

export const HOST_ACTION_LABEL = {
  conflict: 'gameDetails.renodx.host.action.conflict',
  reinstall_with_addon_support: 'gameDetails.renodx.host.action.reinstall_with_addon_support',
  repair_host: 'gameDetails.renodx.host.action.repair_host',
  update_host: 'gameDetails.renodx.host.action.update_host',
} satisfies Record<Exclude<ReshadeHostAction, 'up_to_date'>, MessageKey>;

export const CHANNEL_LABEL = {
  stable: 'gameDetails.renodx.channel.stable',
  nightly: 'gameDetails.renodx.channel.nightly',
} satisfies Record<ReshadeChannel, MessageKey>;

export const MANAGED_HEALTH_LABEL = {
  missing: 'gameDetails.renodx.host.health.missing',
  conflicting: 'gameDetails.renodx.host.health.conflicting',
} satisfies Record<Exclude<ManagedReshadeHealth, 'healthy'>, MessageKey>;

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
  host,
  action,
  conflict,
  ownership,
}: {
  host: ReshadeHost;
  action: ReshadeHostAction;
  conflict: boolean;
  ownership: ReshadeHostOwnership;
}): ReshadeDescription {
  if (conflict) {
    return {
      kind: 'conflict',
      key: 'gameDetails.renodx.host.conflictMultiple',
    };
  }

  const parts: ReshadeDescriptionPart[] = [];
  if (host.status === 'present' && host.version) {
    parts.push({
      kind: 'version',
      key: 'gameDetails.renodx.host.version',
      version: host.version,
    });
  }
  if (ownership.kind === 'managed' && ownership.health !== 'healthy') {
    parts.push({
      kind: 'message',
      key: MANAGED_HEALTH_LABEL[ownership.health],
    });
  }
  if (host.status === 'present' && host.addon_support !== 'full') {
    parts.push({
      kind: 'message',
      key: ADDON_SUPPORT_LABEL[host.addon_support],
    });
  }
  if (action !== 'up_to_date') {
    parts.push({
      kind: 'message',
      key: HOST_ACTION_LABEL[action],
    });
  }

  return {
    kind: 'parts',
    fallbackKey: 'gameDetails.renodx.host.versionUnknown',
    parts,
  };
}

export function getReshadeSwitchTarget(channel: ReshadeChannel | null): ReshadeChannel | null {
  if (channel === 'stable') {
    return 'nightly';
  }
  if (channel === 'nightly') {
    return 'stable';
  }
  return null;
}

export function canSwitchReshadeChannel(
  ownership: ReshadeHostOwnership,
  target: ReshadeChannel | null,
): boolean {
  return ownership.kind === 'managed' && target !== null;
}

export function isReshadeSwitchDisabled({
  busy,
  target,
  stableSupported,
}: {
  busy: boolean;
  target: ReshadeChannel | null;
  stableSupported: boolean;
}): boolean {
  return busy || (target === 'stable' && !stableSupported);
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
