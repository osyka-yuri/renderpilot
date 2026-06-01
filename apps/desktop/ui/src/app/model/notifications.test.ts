import { beforeEach, describe, expect, it } from 'vitest';
import { clearAllNotifications, getActiveNotifications } from '@shared/notifications';
import { t } from '@shared/i18n';
import {
  publishMissingStableGameDetailsNotification,
  publishStalePlanNotification,
} from './notifications';

describe('app notifications', () => {
  beforeEach(() => {
    clearAllNotifications();
  });

  it('publishes the stale plan status error copy', () => {
    const notificationId = publishStalePlanNotification();

    expect(notificationId).toBe('desktop-status');
    expect(getActiveNotifications()).toEqual([
      {
        id: 'desktop-status',
        severity: 'error',
        title: t('notify.statusError'),
        description: t('notify.stalePlan'),
        important: true,
      },
    ]);
  });

  it('publishes the missing stable game details status error copy', () => {
    const notificationId = publishMissingStableGameDetailsNotification();

    expect(notificationId).toBe('desktop-status');
    expect(getActiveNotifications()).toEqual([
      {
        id: 'desktop-status',
        severity: 'error',
        title: t('notify.statusError'),
        description: t('notify.missingStableGameId'),
        important: true,
      },
    ]);
  });
});
