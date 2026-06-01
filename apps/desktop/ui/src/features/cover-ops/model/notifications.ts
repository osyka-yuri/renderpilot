import {
  publishInfoNotification,
  publishStatusNotification,
  publishSuccessNotification,
} from '@shared/notifications';
import { t } from '@shared/i18n';

export function publishCoverUpdatedNotification(): string {
  return publishSuccessNotification(t('notify.coverUpdated.title'), t('notify.coverUpdated.body'));
}

export function publishCoverDownloadedNotification(): string {
  return publishInfoNotification(
    t('notify.coverDownloaded.title'),
    t('notify.coverDownloaded.body'),
  );
}

export function publishCoverRemovedNotification(): string {
  return publishSuccessNotification(t('notify.coverRemoved.title'), t('notify.coverRemoved.body'));
}

export function publishCoverOperationErrorNotification(message: string): string | null {
  return publishStatusNotification(message, 'error');
}

export function publishCoverPickerPreviewModeNotification(): string | null {
  return publishStatusNotification(t('notify.coverPickerPreview'), 'error');
}
