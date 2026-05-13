import { publishStatusNotification } from '@shared/notifications';
import { formatBackgroundCoverSyncError } from './background-cover-sync';

export function publishBackgroundCoverSyncFailureNotification(error: unknown): string | null {
  return publishStatusNotification(formatBackgroundCoverSyncError(error), 'error');
}

export function publishBackgroundCoverSyncIssueNotification(message: string): string | null {
  return publishStatusNotification(message, 'error');
}
