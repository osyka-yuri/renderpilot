import { publishSuccessNotification } from '@shared/notifications';
import { t } from '@shared/i18n';
import { formatRestoredFilesSummary, formatUpdatedFilesSummary } from './presenters';

export function publishApplyCompletedNotification(itemCount: number): string {
  return publishSuccessNotification(
    t('notify.applyCompleted'),
    formatUpdatedFilesSummary(itemCount),
  );
}

export function publishRollbackCompletedNotification(itemCount: number): string {
  return publishSuccessNotification(
    t('notify.rollbackCompleted'),
    formatRestoredFilesSummary(itemCount),
  );
}
