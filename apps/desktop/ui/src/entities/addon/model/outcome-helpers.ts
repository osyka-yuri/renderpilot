import type { AddonKind, IncompatibilityReason, MatchConfidence, RiskAssessment } from './types';

/** Outcome kinds shared by every add-on tool's availability preview. */
export type CommonAvailabilityOutcome =
  | {
      kind: 'installable';
      confidence: MatchConfidence;
      risk: RiskAssessment;
      notes_keys: string[];
    }
  | { kind: 'incompatible'; reason: IncompatibilityReason }
  | { kind: 'blacklisted'; reason: string | null }
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
  notesKeys: string[];
  blacklistReason: string | null;
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
  notesKeys: [],
  blacklistReason: null,
  risk: null,
  requiresConfirmation: false,
};

/** Maps a tool outcome (or a superset with extra kinds) to shared card fields. */
export function deriveCommonOutcomeFields(
  // Accept tool-specific supersets (extra fields like `confidence`, `url`, `other_kind`)
  // without forcing casts at call sites. Excess property check is bypassed via intersection.
  outcome: ({ kind: string } & Record<string, unknown>) | null | undefined,
): CommonOutcomeFields {
  if (!outcome) {
    return EMPTY_COMMON_OUTCOME;
  }

  switch (outcome.kind) {
    case 'installable': {
      const installable = outcome as Extract<CommonAvailabilityOutcome, { kind: 'installable' }>;
      return {
        ...EMPTY_COMMON_OUTCOME,
        isInstallable: true,
        confidence: installable.confidence,
        notesKeys: installable.notes_keys,
        risk: installable.risk,
        requiresConfirmation: installable.risk.severity === 'warn',
      };
    }
    case 'incompatible':
      return { ...EMPTY_COMMON_OUTCOME, isIncompatible: true };
    case 'blacklisted': {
      const blacklisted = outcome as Extract<CommonAvailabilityOutcome, { kind: 'blacklisted' }>;
      return {
        ...EMPTY_COMMON_OUTCOME,
        isBlacklisted: true,
        blacklistReason: blacklisted.reason,
      };
    }
    case 'unsupported':
      return { ...EMPTY_COMMON_OUTCOME, isUnsupported: true };
    case 'blocked_by_other_addon': {
      const blocked = outcome as Extract<
        CommonAvailabilityOutcome,
        { kind: 'blocked_by_other_addon' }
      >;
      return {
        ...EMPTY_COMMON_OUTCOME,
        isBlockedByOtherAddon: true,
        otherAddonKind: blocked.other_kind,
        otherAddonUnmanaged: blocked.unmanaged,
      };
    }
    case 'unmanaged_present':
      return { ...EMPTY_COMMON_OUTCOME, isUnmanagedPresent: true };
    default:
      return EMPTY_COMMON_OUTCOME;
  }
}
