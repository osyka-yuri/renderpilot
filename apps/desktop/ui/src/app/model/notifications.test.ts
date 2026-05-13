import { beforeEach, describe, expect, it } from 'vitest';
import { clearAllNotifications, getActiveNotifications } from '@shared/notifications';
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
        title: 'Needs attention',
        description:
          'The selected operation plan is no longer current. Rebuild the plan before applying it.',
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
        title: 'Needs attention',
        description: 'Catalog returned game details without a stable identifier.',
        important: true,
      },
    ]);
  });
});
