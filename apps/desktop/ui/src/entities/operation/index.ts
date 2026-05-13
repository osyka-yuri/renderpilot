import {
  formatOperationLabel,
  formatRisk,
  riskTone,
  riskBadgeVariant,
  statusBadgeVariant,
  isRollbackableOperation,
  isPlanForGame,
  formatBackupSummary,
  formatRestoredFilesSummary,
  formatUpdatedFilesSummary,
  getCompletedDurationText,
  type OperationBadgeVariant,
} from './model/presenters';
import {
  publishApplyCompletedNotification,
  publishRollbackCompletedNotification,
} from './model/notifications';
import type {
  SwapPlan,
  ApplyOperationResult,
  RollbackOperationResult,
  OperationSummary,
  OperationHandler,
} from './model/types';
import { buildSwapPlan, applyOperationPlan, rollbackOperation } from './api/desktop';

export {
  formatOperationLabel,
  formatRisk,
  riskTone,
  riskBadgeVariant,
  statusBadgeVariant,
  isRollbackableOperation,
  isPlanForGame,
  formatBackupSummary,
  formatRestoredFilesSummary,
  formatUpdatedFilesSummary,
  getCompletedDurationText,
  publishApplyCompletedNotification,
  publishRollbackCompletedNotification,
  buildSwapPlan,
  applyOperationPlan,
  rollbackOperation,
};

export type {
  OperationBadgeVariant,
  SwapPlan,
  ApplyOperationResult,
  RollbackOperationResult,
  OperationSummary,
  OperationHandler,
};
