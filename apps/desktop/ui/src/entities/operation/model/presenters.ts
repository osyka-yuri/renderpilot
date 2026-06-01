import { humanizeToken } from '@shared/text';
import { t, type MessageKey } from '@shared/i18n';
import type { SwapPlan } from './types';

export type OperationBadgeVariant = 'outline' | 'secondary' | 'destructive';

const OPERATION_LABEL_KEYS: Partial<Record<string, MessageKey>> = {
  low: 'operation.label.low',
  medium: 'operation.label.medium',
  high: 'operation.label.high',
  blocked: 'operation.label.blocked',
  unknown: 'common.unknown',
  planned: 'operation.label.planned',
  completed: 'operation.label.completed',
  failed: 'operation.label.failed',
  rolled_back: 'operation.label.rolledBack',
  replace_component: 'operation.label.replaceComponent',
};

export function formatOperationLabel(value?: string | null): string {
  if (!value) {
    return t('common.unknown');
  }

  const key = OPERATION_LABEL_KEYS[value];

  return key ? t(key) : humanizeToken(value);
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
      return 'outline';
    case 'failed':
    case 'blocked':
      return 'destructive';
    default:
      return 'outline';
  }
}

function areSameGameIds(left: string, right: string): boolean {
  return left.trim() === right.trim();
}

export function isPlanForGame(plan: SwapPlan | null, gameId: string): boolean {
  return plan !== null && areSameGameIds(plan.game_id, gameId);
}

export function isPlanForComponent(plan: SwapPlan | null, componentId: string): boolean {
  return plan !== null && plan.component_id === componentId;
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

  return t('operation.duration', { seconds: durationSeconds });
}

function formatAffectedFilesSummary(itemCount: number, verb: 'updated' | 'restored'): string {
  if (itemCount === 0) {
    return t(verb === 'updated' ? 'operation.filesUpdated.none' : 'operation.filesRestored.none');
  }

  return t(verb === 'updated' ? 'operation.filesUpdated.count' : 'operation.filesRestored.count', {
    count: itemCount,
  });
}
