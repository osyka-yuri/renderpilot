import { humanizeToken } from '@shared/text';
import type { SwapPlan } from './types';

export type OperationBadgeVariant = 'outline' | 'secondary' | 'destructive';

const OPERATION_LABELS: Record<string, string> = {
  low: 'Low',
  medium: 'Medium',
  high: 'High',
  blocked: 'Blocked',
  unknown: 'Unknown',
  planned: 'Planned',
  completed: 'Completed',
  failed: 'Failed',
  rolled_back: 'Rolled Back',
  rollback_required: 'Rollback Required',
  replace_component: 'Replace Component',
};

export function formatOperationLabel(value?: string | null): string {
  if (!value) {
    return 'Unknown';
  }
  return OPERATION_LABELS[value] ?? humanizeToken(value);
}

export function formatRisk(value?: string | null): string {
  return formatOperationLabel(value);
}

export function riskTone(value?: string | null): 'low' | 'medium' | 'high' | 'blocked' | 'unknown' {
  switch (value) {
    case 'low':
      return 'low';
    case 'medium':
      return 'medium';
    case 'high':
      return 'high';
    case 'blocked':
      return 'blocked';
    default:
      return 'unknown';
  }
}

export function riskBadgeVariant(value?: string | null): OperationBadgeVariant {
  switch (riskTone(value)) {
    case 'low':
      return 'outline';
    case 'medium':
      return 'secondary';
    case 'high':
    case 'blocked':
      return 'destructive';
    default:
      return 'outline';
  }
}

export function statusBadgeVariant(value?: string | null): OperationBadgeVariant {
  switch (value) {
    case 'completed':
    case 'rolled_back':
      return 'secondary';
    case 'planned':
    case 'validating':
    case 'backup_created':
    case 'replacing':
      return 'outline';
    case 'rollback_required':
      return 'secondary';
    case 'failed':
    case 'blocked':
      return 'destructive';
    default:
      return 'outline';
  }
}

const COMPLETED_STATUS = 'completed';
const ROLLBACK_OPERATION_KIND = 'rollback_operation';
const BACKUP_STATUS_AVAILABLE = 'available';
const BACKUP_STATUS_PARTIAL = 'partial';

export function isRollbackableOperation(operation: {
  status: string;
  kind: string;
  backup_status: string;
}): boolean {
  return (
    operation.status === COMPLETED_STATUS &&
    operation.kind !== ROLLBACK_OPERATION_KIND &&
    isRollbackableBackupStatus(operation.backup_status)
  );
}

export function isRollbackableBackupStatus(status: string): boolean {
  return status === BACKUP_STATUS_AVAILABLE || status === BACKUP_STATUS_PARTIAL;
}

function areSameGameIds(left: string, right: string): boolean {
  return left.trim() === right.trim();
}

export function isPlanForGame(plan: SwapPlan | null, gameId: string): boolean {
  return plan !== null && areSameGameIds(plan.game_id, gameId);
}

export function formatBackupSummary(backupCount: number, backupStatus: string): string {
  return `${backupCount} (${formatOperationLabel(backupStatus)})`;
}

export function formatUpdatedFilesSummary(itemCount: number): string {
  return formatAffectedFilesSummary(itemCount, 'updated');
}

export function formatRestoredFilesSummary(itemCount: number): string {
  return formatAffectedFilesSummary(itemCount, 'restored');
}

export function getCompletedDurationText(
  createdAt: number,
  completedAt: number | null,
): string | null {
  if (completedAt === null) {
    return null;
  }

  const durationSeconds = Math.max(0, Math.round((completedAt - createdAt) / 1000));

  return `Completed in ${durationSeconds}s`;
}

function formatAffectedFilesSummary(itemCount: number, verb: 'updated' | 'restored'): string {
  if (itemCount === 0) {
    return verb === 'updated' ? 'No files were updated.' : 'No files were restored.';
  }

  if (itemCount === 1) {
    return `1 file ${verb}.`;
  }

  return `${itemCount} files ${verb}.`;
}
