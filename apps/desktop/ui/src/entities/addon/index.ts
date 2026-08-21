export { getCardView } from './model/card-view';
export type { BaseCardView } from './model/card-view';

export {
  createConfidenceLabelKeys,
  createInstallableLabels,
  createInstalledLabels,
  createReshadePresenters,
  formatHostDescription,
} from './model/presenters';
export type { HostDescription } from './model/presenters';

export { createAddonStore } from './model/create-addon-store.svelte';
export {
  isMutationFailure,
  isMutationSuccess,
  type AddonMutationResult,
  type CheckUpdateKind,
  type PostMutationProbe,
} from './model/busy-mutation';

export { addonCoreApi, mergeAddonApis } from './model/addon-core-api';
export { commonOutcomeApi, hostSnapshotApi } from './model/addon-outcome-api.svelte';

export { createExclusiveAddonStores } from './model/create-exclusive-addon-stores';

export { defaultHostFacts, mapAvailabilitySnapshot } from './model/store-helpers';

export { isReshadeChannel } from './model/types';
export type { CommonAvailabilityOutcome } from './model/outcome-helpers';

export type {
  ActionDescriptor,
  CatalogMessage,
  HostActions,
  HostDetection,
  HostFacts,
  MatchConfidence,
  MutationSafetyScope,
  ReshadeChannel,
  MutationSafetyTokens,
  UpdateStatus,
} from './model/types';

export {
  AddonAttribution,
  AddonBlockedMessage,
  AddonCardShell,
  AddonComponentRow,
  AddonConfidenceBadge,
  AddonFieldLabel,
  AddonInstallableView,
  AddonInstalledPanel,
  AddonStateMessage,
  AddonToolStatusBadge,
} from './ui';
