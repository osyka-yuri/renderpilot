/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import {
  clearAllNotifications,
  dismissNotification,
  getActiveNotifications,
  publishInfoNotification,
  publishStatusNotification,
} from '@shared/notifications';
import {
  dismissSonnerNotification,
  publishSonnerNotification,
} from './notification-adapter';
import NotificationsToaster from './notifications-toaster.svelte';

vi.mock('./notification-adapter', () => ({
  publishSonnerNotification: vi.fn(),
  dismissSonnerNotification: vi.fn(),
}));

vi.mock('@shared/ui', () => ({
  Toaster: function MockToaster() {
    return undefined;
  },
}));

const publishSonnerNotificationMock = vi.mocked(publishSonnerNotification);
const dismissSonnerNotificationMock = vi.mocked(dismissSonnerNotification);

describe('NotificationsToaster', () => {
  let target: HTMLDivElement;
  let renderedComponent: object | undefined;

  const mountToaster = () => {
    renderedComponent = mount(NotificationsToaster, { target });
    flushSync();

    return renderedComponent;
  };

  const unmountToaster = async () => {
    if (!renderedComponent) {
      return;
    }

    await unmount(renderedComponent);
    flushSync();

    renderedComponent = undefined;
  };

  beforeEach(() => {
    clearAllNotifications();
    vi.clearAllMocks();

    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(async () => {
    await unmountToaster();

    clearAllNotifications();
    target.remove();
  });

  it('replays active notifications when mounted', () => {
    publishStatusNotification('Missing file', 'error');
    publishInfoNotification('Cover downloaded', 'The artwork has been refreshed.');

    const activeNotifications = getActiveNotifications();

    mountToaster();

    expect(publishSonnerNotificationMock).toHaveBeenCalledTimes(
      activeNotifications.length,
    );

    expect(publishSonnerNotificationMock.mock.calls).toEqual(
      activeNotifications.map((notification) => [notification]),
    );
  });

  it('forwards publish and dismiss events to the Sonner adapter', () => {
    mountToaster();

    const notificationId = publishInfoNotification(
      'Cover downloaded',
      'The game artwork has been refreshed.',
    );

    expect(publishSonnerNotificationMock).toHaveBeenCalledTimes(1);
    expect(publishSonnerNotificationMock).toHaveBeenCalledWith({
      id: notificationId,
      severity: 'info',
      title: 'Cover downloaded',
      description: 'The game artwork has been refreshed.',
      important: undefined,
    });

    dismissNotification(notificationId);

    expect(dismissSonnerNotificationMock).toHaveBeenCalledTimes(1);
    expect(dismissSonnerNotificationMock).toHaveBeenCalledWith(notificationId);
  });

  it('stops forwarding notification events after unmount', async () => {
    mountToaster();

    await unmountToaster();

    const notificationId = publishInfoNotification('Cover downloaded');

    dismissNotification(notificationId);

    expect(publishSonnerNotificationMock).not.toHaveBeenCalled();
    expect(dismissSonnerNotificationMock).not.toHaveBeenCalled();
  });
});
