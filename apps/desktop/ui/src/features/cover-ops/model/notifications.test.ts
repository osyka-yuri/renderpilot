import { beforeEach, describe, expect, it } from 'vitest';
import { clearAllNotifications, getActiveNotifications } from '@shared/notifications';
import { t } from '@shared/i18n';
import {
  publishCoverDownloadedNotification,
  publishCoverOperationErrorNotification,
  publishCoverPickerPreviewModeNotification,
  publishCoverRemovedNotification,
  publishCoverUpdatedNotification,
} from './notifications';

describe('cover notifications', () => {
  beforeEach(() => {
    clearAllNotifications();
  });

  it('publishes the custom cover success copy', () => {
    const notificationId = publishCoverUpdatedNotification();

    expect(notificationId).toBe('notification-1');
    expect(getActiveNotifications()).toEqual([
      {
        id: 'notification-1',
        severity: 'success',
        title: t('notify.coverUpdated.title'),
        description: t('notify.coverUpdated.body'),
        important: undefined,
      },
    ]);
  });

  it('publishes the cover download info copy', () => {
    const notificationId = publishCoverDownloadedNotification();

    expect(notificationId).toBe('notification-1');
    expect(getActiveNotifications()).toEqual([
      {
        id: 'notification-1',
        severity: 'info',
        title: t('notify.coverDownloaded.title'),
        description: t('notify.coverDownloaded.body'),
        important: undefined,
      },
    ]);
  });

  it('publishes the cover removal success copy', () => {
    const notificationId = publishCoverRemovedNotification();

    expect(notificationId).toBe('notification-1');
    expect(getActiveNotifications()).toEqual([
      {
        id: 'notification-1',
        severity: 'success',
        title: t('notify.coverRemoved.title'),
        description: t('notify.coverRemoved.body'),
        important: undefined,
      },
    ]);
  });

  it('publishes a status error for cover command failures', () => {
    const notificationId = publishCoverOperationErrorNotification('reload failed');

    expect(notificationId).toBe('desktop-status');
    expect(getActiveNotifications()).toEqual([
      {
        id: 'desktop-status',
        severity: 'error',
        title: t('notify.statusError'),
        description: 'reload failed',
        important: true,
      },
    ]);
  });

  it('publishes the preview-mode cover picker error copy', () => {
    const notificationId = publishCoverPickerPreviewModeNotification();

    expect(notificationId).toBe('desktop-status');
    expect(getActiveNotifications()).toEqual([
      {
        id: 'desktop-status',
        severity: 'error',
        title: t('notify.statusError'),
        description: t('notify.coverPickerPreview'),
        important: true,
      },
    ]);
  });
});
