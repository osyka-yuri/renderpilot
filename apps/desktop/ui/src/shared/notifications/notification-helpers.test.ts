import { beforeEach, describe, expect, it } from 'vitest';
import { clearAllNotifications, getActiveNotifications } from './notification-center';
import {
  formatError,
  publishInfoNotification,
  publishSuccessNotification,
} from './notification-helpers';

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

describe('formatError', () => {
  it('extracts the message property from Error instances', () => {
    const error = new Error('something broke');

    expect(formatError(error)).toBe('something broke');
  });

  it('returns the string unchanged for string input', () => {
    expect(formatError('plain text')).toBe('plain text');
  });

  it('stringifies non-Error, non-string values', () => {
    expect(formatError(42)).toBe('42');
    expect(formatError(null)).toBe('null');
    expect(formatError(undefined)).toBe('undefined');
  });

  it('handles Error subclasses correctly', () => {
    expect(formatError(new RangeError('out of range'))).toBe('out of range');
    expect(formatError(new TypeError('wrong type'))).toBe('wrong type');
  });
});
