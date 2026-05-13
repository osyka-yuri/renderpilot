import { beforeEach, describe, expect, it } from 'vitest';
import { clearAllNotifications, getActiveNotifications } from '@shared/notifications';
import {
  publishAutomaticLibraryScanFailedNotification,
  publishPartialLibraryScanWarning,
} from './notifications';

describe('scan-libraries notifications', () => {
  beforeEach(() => {
    clearAllNotifications();
  });

  it('publishes the automatic scan failure as a status error', () => {
    const notificationId = publishAutomaticLibraryScanFailedNotification(
      'Automatic library scan failed; your game list was still refreshed. Disk error.',
    );

    expect(notificationId).toBe('desktop-status');
    expect(getActiveNotifications()).toEqual([
      {
        id: 'desktop-status',
        severity: 'error',
        title: 'Needs attention',
        description:
          'Automatic library scan failed; your game list was still refreshed. Disk error.',
        important: true,
      },
    ]);
  });

  it('publishes the partial scan warning from scan semantics', () => {
    const notificationId = publishPartialLibraryScanWarning(2);

    expect(notificationId).toBe('desktop-status');
    expect(getActiveNotifications()).toEqual([
      {
        id: 'desktop-status',
        severity: 'warning',
        title: 'Review warning',
        description:
          'Some game libraries could not be scanned (2 roots failed). Check logs for details.',
        important: false,
      },
    ]);
  });
});