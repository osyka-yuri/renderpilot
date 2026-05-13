import { beforeEach, describe, expect, it } from 'vitest';
import {
  clearAllNotifications,
  getActiveNotifications,
  subscribeToNotificationEvents,
} from './notification-center';
import {
  clearStatusNotification,
  publishCommandErrorNotification,
  publishStatusNotification,
  STATUS_NOTIFICATION_ID,
} from './notification-status';
import type { NotificationEvent } from './types';

describe('notification-status', () => {
  beforeEach(() => {
    clearAllNotifications();
  });

  it('publishes a keyed status notification', () => {
    const { events, unsubscribe } = captureNotificationEvents();

    const notificationId = publishStatusNotification('  Something went wrong  ', 'error');

    expect(notificationId).toBe(STATUS_NOTIFICATION_ID);
    expect(getActiveNotifications()).toEqual([
      {
        id: STATUS_NOTIFICATION_ID,
        severity: 'error',
        title: 'Needs attention',
        description: 'Something went wrong',
        important: true,
      },
    ]);
    expect(events).toEqual([
      {
        type: 'published',
        notification: {
          id: STATUS_NOTIFICATION_ID,
          severity: 'error',
          title: 'Needs attention',
          description: 'Something went wrong',
          important: true,
        },
      },
    ]);

    unsubscribe();
  });

  it('dismisses the status notification when the message becomes empty', () => {
    const { events, unsubscribe } = captureNotificationEvents();

    publishStatusNotification('Missing file', 'warning');
    publishStatusNotification('   ', 'warning');

    expect(getActiveNotifications()).toEqual([]);
    expect(events[events.length - 1]).toEqual({ type: 'dismissed', id: STATUS_NOTIFICATION_ID });

    unsubscribe();
  });

  it('publishes command errors through the normalized status flow', () => {
    publishCommandErrorNotification({
      code: 'catalog_partial_scan',
      severity: 'warning',
      messageKey: 'warnings.catalog_partial_scan',
      details: 'Some folders could not be scanned.',
      suggestedActions: [],
    });

    expect(getActiveNotifications()).toEqual([
      {
        id: STATUS_NOTIFICATION_ID,
        severity: 'warning',
        title: 'Review warning',
        description: 'Some folders could not be scanned.',
        important: false,
      },
    ]);
  });

  it('clears the active status notification explicitly', () => {
    publishStatusNotification('Some folders could not be scanned.', 'warning');

    clearStatusNotification();

    expect(getActiveNotifications()).toEqual([]);
  });
});

function captureNotificationEvents(): {
  events: NotificationEvent[];
  unsubscribe: () => void;
} {
  const events: NotificationEvent[] = [];
  const unsubscribe = subscribeToNotificationEvents((event) => {
    events.push(event);
  });

  return { events, unsubscribe };
}
