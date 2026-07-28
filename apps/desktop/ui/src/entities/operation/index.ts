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

export { applySwap, planRollback, planSwap, rollbackComponent } from './api/desktop';

export type { OperationBadgeVariant } from './model/presenters';

export type {
  SwapPlan,
  SwapPlanBlocker,
  RollbackPlan,
  ApplySwapResult,
  RollbackComponentResult,
  ExecutedD3d12ExecutableAction,
  OperationMetadata,
  OperationSummary,
} from './model/types';
