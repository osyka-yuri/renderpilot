import { beforeEach, describe, expect, it } from 'vitest';
import { clearAllNotifications, getActiveNotifications } from '@shared/notifications';
import { t } from '@shared/i18n';
import {
  publishAutomaticLibraryScanFailedNotification,
  publishAddGameWarnings,
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

  it('surfaces add-game warnings and the recovery bundle path', () => {
    publishAddGameWarnings({
      gameId: 'game:test',
      effectiveRoot: 'C:/Games/Test',
      disposition: 'updated',
      rootAuthority: 'user_confirmed',
      detectedLibraryCount: 1,
      consolidatedGameIds: ['legacy:child'],
      recoveryBundlePath: 'C:/catalog/recovery/test.bundle',
      warnings: [
        {
          code: 'legacy_cards_consolidated',
          message: 'Backend fallback.',
          parameters: { count: 1 },
        },
      ],
    });

    expect(getActiveNotifications()).toEqual([
      expect.objectContaining({
        severity: 'warning',
        description: `${t('addGame.warning.legacyCardsConsolidated', {
          count: 1,
        })}\n${t('addGame.warning.recoveryBundleFallback', {
          path: 'C:/catalog/recovery/test.bundle',
        })}`,
      }),
    ]);
  });

  it('preserves the backend fallback for forward-compatible warning codes', () => {
    publishAddGameWarnings({
      gameId: 'game:test',
      effectiveRoot: 'C:/Games/Test',
      disposition: 'updated',
      rootAuthority: 'user_confirmed',
      detectedLibraryCount: 1,
      consolidatedGameIds: [],
      recoveryBundlePath: null,
      warnings: [{ code: 'future_warning', message: 'A warning from a newer backend.' }],
    });

    expect(getActiveNotifications()).toEqual([
      expect.objectContaining({
        severity: 'warning',
        description: 'A warning from a newer backend.',
      }),
    ]);
  });
});
