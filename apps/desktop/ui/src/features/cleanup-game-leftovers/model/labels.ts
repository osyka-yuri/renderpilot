import { t, type MessageKeyWithoutParams } from '@shared/i18n';
import type { CleanupOutcome, CleanupStep, LeftoverCategory, LeftoverIssueCode } from './types';

const categoryKeys = {
  vulkanRegistration: 'leftovers.category.vulkanRegistration',
  engineConfig: 'leftovers.category.engineConfig',
  optiscalerFile: 'leftovers.category.optiscalerFile',
  addonFile: 'leftovers.category.addonFile',
  componentFile: 'leftovers.category.componentFile',
  privateCustody: 'leftovers.category.privateCustody',
  directory: 'leftovers.category.directory',
} as const satisfies Record<LeftoverCategory, MessageKeyWithoutParams>;

const issueKeys = {
  unsupportedOwner: 'leftovers.issue.unsupportedOwner',
  unprovenOwnership: 'leftovers.issue.unprovenOwnership',
  modifiedFile: 'leftovers.issue.modifiedFile',
  privateOriginalCustody: 'leftovers.issue.privateOriginalCustody',
  foreignPending: 'leftovers.issue.foreignPending',
  pendingConflict: 'leftovers.issue.pendingConflict',
  unavailableTarget: 'leftovers.issue.unavailableTarget',
  nativeAuthorityUnavailable: 'leftovers.issue.nativeAuthorityUnavailable',
  activeOwnershipConflict: 'leftovers.issue.activeOwnershipConflict',
  staleIntent: 'leftovers.issue.staleIntent',
  operationFailed: 'leftovers.issue.operationFailed',
} as const satisfies Record<LeftoverIssueCode, MessageKeyWithoutParams>;

const stepKeys = {
  removed: 'leftovers.outcome.removed',
  released: 'leftovers.outcome.released',
  alreadyAbsent: 'leftovers.outcome.alreadyAbsent',
  recovered: 'leftovers.outcome.recovered',
  preservedForeign: 'leftovers.outcome.preservedForeign',
  blocked: 'leftovers.outcome.blocked',
  failed: 'leftovers.outcome.failed',
} as const satisfies Record<CleanupStep['outcome'], MessageKeyWithoutParams>;

const statusKeys = {
  complete: 'leftovers.status.complete',
  partial: 'leftovers.status.partial',
  blocked: 'leftovers.status.blocked',
  stale: 'leftovers.status.stale',
} as const satisfies Record<CleanupOutcome['status'], MessageKeyWithoutParams>;

export const categoryLabel = (category: LeftoverCategory): string => t(categoryKeys[category]);
export const issueLabel = (code: LeftoverIssueCode): string => t(issueKeys[code]);
export const stepLabel = (outcome: CleanupStep['outcome']): string => t(stepKeys[outcome]);
export const statusLabel = (status: CleanupOutcome['status']): string => t(statusKeys[status]);
