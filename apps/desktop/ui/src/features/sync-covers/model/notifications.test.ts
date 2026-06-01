import { beforeEach, describe, expect, it } from 'vitest';
import { clearAllNotifications, getActiveNotifications } from '@shared/notifications';
import { t } from '@shared/i18n';
import {
  publishBackgroundCoverSyncFailureNotification,
  publishBackgroundCoverSyncIssueNotification,
} from './notifications';

describe('sync-covers notifications', () => {
  beforeEach(() => {
    clearAllNotifications();
  });

  it('publishes catastrophic background sync failures as status errors', () => {
    const notificationId = publishBackgroundCoverSyncFailureNotification(
      new Error('network failure'),
    );

    expect(notificationId).toBe('desktop-status');
    expect(getActiveNotifications()).toEqual([
      {
        id: 'desktop-status',
        severity: 'error',
        title: t('notify.statusError'),
        description: t('coverSync.failed'),
        important: true,
      },
    ]);
  });

  it('publishes recoverable background sync issues as status errors', () => {
    const notificationId = publishBackgroundCoverSyncIssueNotification(
      'Could not download 2 covers. Try Refresh Libraries.',
    );

    expect(notificationId).toBe('desktop-status');
    expect(getActiveNotifications()).toEqual([
      {
        id: 'desktop-status',
        severity: 'error',
        title: t('notify.statusError'),
        description: 'Could not download 2 covers. Try Refresh Libraries.',
        important: true,
      },
    ]);
  });
});
