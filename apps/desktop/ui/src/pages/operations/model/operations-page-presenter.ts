import {
  formatOperationLabel,
  formatBackupSummary,
  getCompletedDurationText,
  isRollbackableOperation,
  statusTone,
  type OperationSummary,
} from '@entities/operation';
import { formatTimestamp } from '@shared/utils';

export type OperationHistoryDetails = {
  operations: readonly OperationSummary[];
};

export type OperationViewModel = {
  id: string;
  kindLabel: string;
  statusLabel: string;
  tone: ReturnType<typeof statusTone>;
  createdAtText: string;
  itemCount: number;
  backupSummary: string;
  isBusy: boolean;
  canRollback: boolean;
  isRollbackDisabled: boolean;
  rollbackLabel: string;
  completedDurationText: string | null;
  ariaLabel: string;
};

export function createOperationViewModel(
  operation: OperationSummary,
  options: {
    busyOperationId?: string | null;
    isInteractionBusy?: boolean;
    hasRollbackHandler?: boolean;
  } = {},
): OperationViewModel {
  const id = operation.operation_id;
  const kindLabel = formatOperationLabel(operation.kind);
  const statusLabel = formatOperationLabel(operation.status);
  const createdAtText = formatTimestamp(operation.created_at);
  const isBusy = options.busyOperationId === id;
  const canRollback = Boolean(options.hasRollbackHandler) && isRollbackableOperation(operation);

  return {
    id,
    kindLabel,
    statusLabel,
    tone: statusTone(operation.status),
    createdAtText,
    itemCount: operation.item_count,
    backupSummary: formatBackupSummary(operation.backup_count, operation.backup_status),
    isBusy,
    canRollback,
    isRollbackDisabled: Boolean(options.isInteractionBusy),
    rollbackLabel: isBusy ? 'Rolling back...' : 'Rollback',
    completedDurationText: getCompletedDurationText(
      operation.created_at,
      operation.completed_at ?? null,
    ),
    ariaLabel: `${kindLabel}, ${statusLabel}, created ${createdAtText}`,
  };
}
