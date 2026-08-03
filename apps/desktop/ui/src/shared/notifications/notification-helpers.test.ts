import { beforeEach, describe, expect, it } from 'vitest';
import { clearAllNotifications, getActiveNotifications } from './notification-center';
import { publishInfoNotification, publishSuccessNotification } from './notification-helpers';

describe('notification-helpers', () => {
  beforeEach(() => {
    clearAllNotifications();
  });

  it('publishes transient success notifications with generated ids', () => {
    const notificationId = publishSuccessNotification(
      '  Changes applied  ',
      '  2 files updated.  ',
    );

    expect(notificationId).toBe('notification-1');
    expect(getActiveNotifications()).toEqual([
      {
        id: 'notification-1',
        severity: 'success',
        title: 'Changes applied',
        description: '2 files updated.',
        important: undefined,
      },
    ]);
  });

  it('publishes transient info notifications with generated ids', () => {
    const notificationId = publishInfoNotification('  Cover downloaded  ', '   ');

    expect(notificationId).toBe('notification-1');
    expect(getActiveNotifications()).toEqual([
      {
        id: 'notification-1',
        severity: 'info',
        title: 'Cover downloaded',
        description: undefined,
        important: undefined,
      },
    ]);
  });

  it('rejects empty transient titles', () => {
    expect(() => publishSuccessNotification('   ')).toThrow(
      new RangeError('Notification title must not be empty.'),
    );
  });
});
