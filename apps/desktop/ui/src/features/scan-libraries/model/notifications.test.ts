import { beforeEach, describe, expect, it } from 'vitest';
import { clearAllNotifications, getActiveNotifications } from '@shared/notifications';
import { t } from '@shared/i18n';
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
        title: t('notify.statusError'),
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
        title: t('notify.statusWarning'),
        description: t('scan.partialWarning', { count: 2 }),
        important: false,
      },
    ]);
  });
});
