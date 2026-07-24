import { beforeEach, describe, expect, it } from 'vitest';
import { clearAllNotifications, getActiveNotifications } from '@shared/notifications';
import { t } from '@shared/i18n';
import {
  publishApplyCompletedNotification,
  publishRollbackCompletedNotification,
} from './notifications';

describe('operation notifications', () => {
  beforeEach(() => {
    clearAllNotifications();
  });

  it('publishes apply completion copy from operation semantics', () => {
    const notificationId = publishApplyCompletedNotification(2);

    expect(notificationId).toBe('notification-1');
    expect(getActiveNotifications()).toEqual([
      {
        id: 'notification-1',
        severity: 'success',
        title: t('notify.applyCompleted'),
        description: t('operation.filesUpdated.count', { count: 2 }),
        important: undefined,
      },
    ]);
  });

  it('publishes rollback completion copy from operation semantics', () => {
    const notificationId = publishRollbackCompletedNotification(0);

    expect(notificationId).toBe('notification-1');
    expect(getActiveNotifications()).toEqual([
      {
        id: 'notification-1',
        severity: 'success',
        title: t('notify.rollbackCompleted'),
        description: t('operation.filesRestored.none'),
        important: undefined,
      },
    ]);
  });

  it('publishes exactly one EXE-focused completion notification for the first patch', () => {
    const notificationId = publishApplyCompletedNotification(2, {
      kind: 'patch',
      executable_path: 'C:/Game/game.exe',
      original_sdk_version: 606,
      from_sdk_version: 606,
      to_sdk_version: 619,
    });

    expect(notificationId).toBe('notification-1');
    expect(getActiveNotifications()).toEqual([
      {
        id: 'notification-1',
        severity: 'success',
        title: t('gameDetails.d3d12.action.patch', { from: 606, to: 619 }),
        description: 'C:/Game/game.exe',
        important: undefined,
      },
    ]);
  });

  it('publishes one generic completion notification when an existing patch is updated', () => {
    const notificationId = publishApplyCompletedNotification(2, {
      kind: 'patch',
      executable_path: 'C:/Game/game.exe',
      original_sdk_version: 606,
      from_sdk_version: 619,
      to_sdk_version: 618,
    });

    expect(notificationId).toBe('notification-1');
    expect(getActiveNotifications()).toEqual([
      {
        id: 'notification-1',
        severity: 'success',
        title: t('notify.applyCompleted'),
        description: t('operation.filesUpdated.count', { count: 2 }),
        important: undefined,
      },
    ]);
  });

  it('publishes exactly one EXE-focused completion notification when a swap restores the EXE', () => {
    const notificationId = publishApplyCompletedNotification(2, {
      kind: 'restore',
      executable_path: 'C:/Game/game.exe',
      original_sdk_version: 606,
      from_sdk_version: 619,
      to_sdk_version: 606,
    });

    expect(notificationId).toBe('notification-1');
    expect(getActiveNotifications()).toEqual([
      {
        id: 'notification-1',
        severity: 'success',
        title: t('gameDetails.d3d12.action.restore', { from: 619, to: 606 }),
        description: 'C:/Game/game.exe',
        important: undefined,
      },
    ]);
  });
});
