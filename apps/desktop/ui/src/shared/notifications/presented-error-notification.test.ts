import { beforeEach, describe, expect, it } from 'vitest';
import { DesktopCommandError } from '@shared/errors';
import { t } from '@shared/i18n';

import { clearAllNotifications, getActiveNotifications } from './notification-center';
import {
  getPresentedErrorNotificationContent,
  publishPresentedErrorNotification,
} from './presented-error-notification';

describe('presented-error-notification', () => {
  beforeEach(() => {
    clearAllNotifications();
  });

  it('presents access-denied backend errors with their recovery action', () => {
    const content = getPresentedErrorNotificationContent(
      DesktopCommandError.fromDto({ code: 'access_denied' }),
    );

    expect(content).toEqual({
      severity: 'error',
      description: t('user_message.access_denied'),
      details: [t('suggested_action.inspect_logs')],
    });
  });

  it('keeps recovery paths as separately rendered details', () => {
    publishPresentedErrorNotification(
      'Could not remove game',
      DesktopCommandError.fromDto({
        code: 'managed_cleanup_ambiguous',
        recoveryBundlePath: 'C:/Recovery/catalog-bundle',
      }),
    );

    expect(getActiveNotifications()).toEqual([
      expect.objectContaining({
        severity: 'error',
        title: 'Could not remove game',
        description: t('user_message.managed_cleanup_ambiguous'),
        details: [
          t('suggested_action.reload_game_details'),
          t('error.recoveryBundlePath', { path: 'C:/Recovery/catalog-bundle' }),
        ],
        important: true,
      }),
    ]);
  });

  it('never exposes unknown codes or arbitrary backend fields', () => {
    const content = getPresentedErrorNotificationContent({
      code: 'future_backend_code',
      details: 'PRIVATE C:/Users/name/token',
    });

    expect(content.description).toBe(t('error.unexpectedClient'));
    expect(JSON.stringify(content)).not.toContain('PRIVATE');
    expect(JSON.stringify(content)).not.toContain('future_backend_code');
  });
});
