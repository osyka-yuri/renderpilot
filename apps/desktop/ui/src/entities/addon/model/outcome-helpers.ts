import type {
  AddonKind,
  CatalogMessage,
  IncompatibilityReason,
  MatchConfidence,
  RiskAssessment,
} from './types';

/** Outcome kinds shared by every add-on tool's availability preview. */
export type CommonAvailabilityOutcome =
  | {
      kind: 'installable';
      confidence: MatchConfidence;
      risk: RiskAssessment;
    }
  | { kind: 'incompatible'; reason: IncompatibilityReason }
  | { kind: 'blacklisted'; message: CatalogMessage }
  | { kind: 'unsupported' }
  | { kind: 'blocked_by_other_addon'; other_kind: AddonKind; unmanaged: boolean }
  | { kind: 'unmanaged_present' };

export type CommonOutcomeFields = {
  isInstallable: boolean;
  isIncompatible: boolean;
  isBlacklisted: boolean;
  isUnsupported: boolean;
  isBlockedByOtherAddon: boolean;
  isUnmanagedPresent: boolean;
  otherAddonKind: AddonKind | null;
  otherAddonUnmanaged: boolean;
  confidence: MatchConfidence | null;
  blacklistMessage: CatalogMessage | null;
  risk: RiskAssessment | null;
  requiresConfirmation: boolean;
};

const EMPTY_COMMON_OUTCOME: CommonOutcomeFields = {
  isInstallable: false,
  isIncompatible: false,
  isBlacklisted: false,
  isUnsupported: false,
  isBlockedByOtherAddon: false,
  isUnmanagedPresent: false,
  otherAddonKind: null,
  otherAddonUnmanaged: false,
  confidence: null,
  blacklistMessage: null,
  risk: null,
  requiresConfirmation: false,
};

/** Whether a tool-specific union member uses the shared outcome contract. */
export function isCommonAvailabilityOutcome(outcome: {
  kind: string;
}): outcome is CommonAvailabilityOutcome {
  switch (outcome.kind) {
    case 'installable':
    case 'incompatible':
    case 'blacklisted':
    case 'unsupported':
    case 'blocked_by_other_addon':
    case 'unmanaged_present':
      return true;
    default:
      return false;
  }
}

/** Maps an already typed shared outcome to the common card fields. */
export function deriveCommonOutcomeFields(
  outcome: CommonAvailabilityOutcome | null | undefined,
): CommonOutcomeFields {
  if (!outcome) {
    return EMPTY_COMMON_OUTCOME;
  }

  switch (outcome.kind) {
    case 'installable': {
      const risk = outcome.risk;
      return {
        ...EMPTY_COMMON_OUTCOME,
        isInstallable: true,
        confidence: outcome.confidence,
        risk,
        requiresConfirmation: risk.severity === 'warn',
      };
    }
    case 'incompatible':
      return { ...EMPTY_COMMON_OUTCOME, isIncompatible: true };
    case 'blacklisted':
      return {
        ...EMPTY_COMMON_OUTCOME,
        isBlacklisted: true,
        blacklistMessage: outcome.message,
      };
    case 'unsupported':
      return { ...EMPTY_COMMON_OUTCOME, isUnsupported: true };
    case 'blocked_by_other_addon': {
      return {
        ...EMPTY_COMMON_OUTCOME,
        isBlockedByOtherAddon: true,
        otherAddonKind: outcome.other_kind,
        otherAddonUnmanaged: outcome.unmanaged,
      };
    }
    case 'unmanaged_present':
      return { ...EMPTY_COMMON_OUTCOME, isUnmanagedPresent: true };
    default:
      return EMPTY_COMMON_OUTCOME;
  }
}
