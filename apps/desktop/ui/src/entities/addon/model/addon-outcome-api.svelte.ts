import { deriveCommonOutcomeFields, isCommonAvailabilityOutcome } from './outcome-helpers';

/**
 * Live getters for the outcome-derived fields every tool store forwards from
 * `deriveCommonOutcomeFields`. Takes a thunk (not the outcome itself) so the
 * internal `$derived` keeps tracking the store's own `$state` outcome —
 * mirrors how `addonCoreApi` stays live over `AddonStoreCore`.
 */
export function commonOutcomeApi<O extends { kind: string }>(outcome: () => O | null) {
  const common = $derived.by(() => {
    const current = outcome();
    return deriveCommonOutcomeFields(
      current && isCommonAvailabilityOutcome(current) ? current : null,
    );
  });

  return {
    get outcome(): O | null {
      return outcome();
    },
    get isInstallable() {
      return common.isInstallable;
    },
    get isBlacklisted() {
      return common.isBlacklisted;
    },
    get isUnsupported() {
      return common.isUnsupported;
    },
    get isIncompatible() {
      return common.isIncompatible;
    },
    get isBlockedByOtherAddon() {
      return common.isBlockedByOtherAddon;
    },
    get isUnmanagedPresent() {
      return common.isUnmanagedPresent;
    },
    get otherAddonKind() {
      return common.otherAddonKind;
    },
    get otherAddonUnmanaged() {
      return common.otherAddonUnmanaged;
    },
    get confidence() {
      return common.confidence;
    },
    get blacklistMessage() {
      return common.blacklistMessage;
    },
  };
}

/** Live getters over a tool store's ReShade-host availability snapshot. */
export function hostSnapshotApi<
  S extends { hostDetection: unknown; hostFacts: unknown; actions: unknown },
>(snapshot: () => S) {
  return {
    get hostDetection(): S['hostDetection'] {
      return snapshot().hostDetection;
    },
    get hostFacts(): S['hostFacts'] {
      return snapshot().hostFacts;
    },
    get hostActions(): S['actions'] {
      return snapshot().actions;
    },
  };
}
