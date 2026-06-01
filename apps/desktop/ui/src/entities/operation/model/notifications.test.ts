import { beforeEach, describe, expect, it } from 'vitest';
import { clearAllNotifications, getActiveNotifications } from '@shared/notifications';
import { t } from '@shared/i18n';
import {
  publishApplyCompletedNotification,
  publishRollbackCompletedNotification,
} from './notifications';

describe('operation notifications', () => {
  beforeEach(() => {
    clearAllNotifications();
  });

  it('publishes apply completion copy from operation semantics', () => {
    const notificationId = publishApplyCompletedNotification(2);

    expect(notificationId).toBe('notification-1');
    expect(getActiveNotifications()).toEqual([
      {
        id: 'notification-1',
        severity: 'success',
        title: t('notify.applyCompleted'),
        description: t('operation.filesUpdated.count', { count: 2 }),
        important: undefined,
      },
    ]);
  });

  it('publishes rollback completion copy from operation semantics', () => {
    const notificationId = publishRollbackCompletedNotification(0);

    expect(notificationId).toBe('notification-1');
    expect(getActiveNotifications()).toEqual([
      {
        id: 'notification-1',
        severity: 'success',
        title: t('notify.rollbackCompleted'),
        description: t('operation.filesRestored.none'),
        important: undefined,
      },
    ]);
  });
});
