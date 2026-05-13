import { beforeEach, describe, expect, it } from 'vitest';
import {
  clearAllNotifications,
  dismissNotification,
  getActiveNotifications,
  publishNotification,
  subscribeToNotificationEvents,
} from './notification-center';
import type { NotificationEvent } from './types';

describe('notification-center', () => {
  beforeEach(() => {
    clearAllNotifications();
  });

  it('publishes a notification and emits a published event', () => {
    const { events, unsubscribe } = captureNotificationEvents();

    const notificationId = publishNotification({
      severity: 'error',
      title: 'Something went wrong',
      description: 'Disk full.',
      id: 'custom-id',
    });

    expect(notificationId).toBe('custom-id');
    expect(getActiveNotifications()).toEqual([
      {
        id: 'custom-id',
        severity: 'error',
        title: 'Something went wrong',
        description: 'Disk full.',
        important: undefined,
      },
    ]);
    expect(events).toEqual([
      {
        type: 'published',
        notification: {
          id: 'custom-id',
          severity: 'error',
          title: 'Something went wrong',
          description: 'Disk full.',
          important: undefined,
        },
      },
    ]);

    unsubscribe();
  });

  it('dismisses a published notification and emits a dismissed event', () => {
    publishNotification({ severity: 'info', title: 'Info' });
    const { events, unsubscribe } = captureNotificationEvents();

    dismissNotification('notification-1');

    expect(getActiveNotifications()).toEqual([]);
    expect(events).toEqual([{ type: 'dismissed', id: 'notification-1' }]);

    unsubscribe();
  });

  it('clears all notifications and resets the id counter', () => {
    publishNotification({ severity: 'error', title: 'A' });
    publishNotification({ severity: 'warning', title: 'B' });

    clearAllNotifications();

    expect(getActiveNotifications()).toEqual([]);

    const idAfterClear = publishNotification({ severity: 'info', title: 'C' });
    expect(idAfterClear).toBe('notification-1');
  });

  it('rejects empty titles', () => {
    expect(() => publishNotification({ severity: 'success', title: '   ' })).toThrow(
      new RangeError('Notification title must not be empty.'),
    );
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