import { publishSuccessNotification } from '@shared/notifications';
import { formatRestoredFilesSummary, formatUpdatedFilesSummary } from './presenters';

export function publishApplyCompletedNotification(itemCount: number): string {
  return publishSuccessNotification('Changes applied', formatUpdatedFilesSummary(itemCount));
}

export function publishRollbackCompletedNotification(itemCount: number): string {
  return publishSuccessNotification('Rollback completed', formatRestoredFilesSummary(itemCount));
}
