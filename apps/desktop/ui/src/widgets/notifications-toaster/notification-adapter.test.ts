import { beforeEach, describe, expect, it, vi } from 'vitest';
import { dismissNotification, type Notification } from '@shared/notifications';
import { toast } from 'svelte-sonner';
import { dismissSonnerNotification, publishSonnerNotification } from './notification-adapter';

vi.mock('@shared/notifications', () => ({
  dismissNotification: vi.fn(),
}));

vi.mock('svelte-sonner', () => ({
  toast: {
    error: vi.fn(),
    warning: vi.fn(),
    success: vi.fn(),
    info: vi.fn(),
    dismiss: vi.fn(),
  },
}));

describe('notification-adapter', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('routes error notifications to Sonner and syncs dismiss callbacks', () => {
    const notification: Notification = {
      id: 'desktop-status',
      severity: 'error',
      title: 'Error',
      description: 'Missing file',
      important: true,
    };

    publishSonnerNotification(notification);

    expect(toast.error).toHaveBeenCalledTimes(1);

    const [title, options] = vi.mocked(toast.error).mock.calls[0];

    expect(title).toBe('Error');
    expect(options).toMatchObject({
      id: 'desktop-status',
      description: 'Missing file',
      important: true,
    });

    options?.onDismiss?.({ id: notification.id } as never);
    options?.onAutoClose?.({ id: notification.id } as never);

    expect(dismissNotification).toHaveBeenCalledTimes(2);
    expect(dismissNotification).toHaveBeenNthCalledWith(1, 'desktop-status');
    expect(dismissNotification).toHaveBeenNthCalledWith(2, 'desktop-status');
  });

  it('uses the matching Sonner API for success, info, and warning notifications', () => {
    publishSonnerNotification({
      id: 'notification-1',
      severity: 'success',
      title: 'Changes applied',
      description: '2 files updated.',
    });
    publishSonnerNotification({
      id: 'notification-2',
      severity: 'info',
      title: 'Cover downloaded',
      description: 'The game artwork has been refreshed.',
    });
    publishSonnerNotification({
      id: 'desktop-status',
      severity: 'warning',
      title: 'Warning',
      description: 'Some folders could not be scanned.',
      important: false,
    });

    expect(toast.success).toHaveBeenCalledWith(
      'Changes applied',
      expect.objectContaining({ id: 'notification-1' }),
    );
    expect(toast.info).toHaveBeenCalledWith(
      'Cover downloaded',
      expect.objectContaining({ id: 'notification-2' }),
    );
    expect(toast.warning).toHaveBeenCalledWith(
      'Warning',
      expect.objectContaining({ id: 'desktop-status' }),
    );
  });

  it('dismisses Sonner notifications by id', () => {
    dismissSonnerNotification('notification-7');

    expect(toast.dismiss).toHaveBeenCalledWith('notification-7');
  });
});
