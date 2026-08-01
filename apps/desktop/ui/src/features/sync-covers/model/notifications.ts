import { publishStatusNotification } from '@shared/notifications';
import { formatBackgroundCoverSyncError } from './background-cover-sync';

export function publishBackgroundCoverSyncFailureNotification(): string | null {
  return publishStatusNotification(formatBackgroundCoverSyncError(), 'error');
}

export function publishBackgroundCoverSyncIssueNotification(message: string): string | null {
  return publishStatusNotification(message, 'error');
}
