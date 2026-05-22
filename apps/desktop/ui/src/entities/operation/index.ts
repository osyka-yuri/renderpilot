export {
  formatOperationLabel,
  formatRisk,
  riskTone,
  riskBadgeVariant,
  statusBadgeVariant,
  isPlanForGame,
  isPlanForComponent,
  formatRestoredFilesSummary,
  formatUpdatedFilesSummary,
  getCompletedDurationText,
} from './model/presenters';

export {
  publishApplyCompletedNotification,
  publishRollbackCompletedNotification,
} from './model/notifications';

export { applySwap, rollbackComponent } from './api/desktop';

export type { OperationBadgeVariant } from './model/presenters';

export type {
  SwapPlan,
  ApplySwapResult,
  RollbackComponentResult,
  OperationSummary,
} from './model/types';
