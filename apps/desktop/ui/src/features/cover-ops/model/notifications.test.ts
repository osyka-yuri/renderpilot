import { beforeEach, describe, expect, it } from 'vitest';
import { clearAllNotifications, getActiveNotifications } from '@shared/notifications';
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
        title: 'Cover updated',
        description: 'The custom artwork has been saved.',
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
        title: 'Cover downloaded',
        description: 'The game artwork has been refreshed.',
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
        title: 'Cover removed',
        description: 'The game now uses the default artwork.',
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
        title: 'Needs attention',
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
        title: 'Needs attention',
        description: 'Choosing a cover file requires the desktop app.',
        important: true,
      },
    ]);
  });
});