import {
  formatOperationLabel,
  getCompletedDurationText,
  statusBadgeVariant,
  type OperationBadgeVariant,
  type OperationSummary,
} from '@entities/operation';
import { formatTimestamp } from '@shared/format';
import { t } from '@shared/i18n';

export type OperationHistoryDetails = {
  operations: readonly OperationSummary[];
};

export type OperationViewModel = {
  id: string;
  kindLabel: string;
  statusLabel: string;
  badgeVariant: OperationBadgeVariant;
  createdAtText: string;
  itemCount: number;
  completedDurationText: string | null;
  ariaLabel: string;
};

export function createOperationViewModel(operation: OperationSummary): OperationViewModel {
  const kindLabel = formatOperationLabel(operation.kind);
  const statusLabel = formatOperationLabel(operation.status);
  const createdAtText = formatTimestamp(operation.created_at);
  const badgeVariant = statusBadgeVariant(operation.status);

  return {
    id: operation.operation_id,
    kindLabel,
    statusLabel,
    badgeVariant,
    createdAtText,
    itemCount: operation.item_count,
    completedDurationText: getCompletedDurationText(
      operation.created_at,
      operation.completed_at ?? null,
    ),
    ariaLabel: t('operation.itemAria', {
      kind: kindLabel,
      status: statusLabel,
      createdAt: createdAtText,
    }),
  };
}
