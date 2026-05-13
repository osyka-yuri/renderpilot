import { toast, type ExternalToast } from 'svelte-sonner';
import { dismissNotification, type Notification } from '@shared/notifications';

export function publishSonnerNotification(notification: Notification): void {
  const options: ExternalToast = {
    id: notification.id,
    description: notification.description,
    important: notification.important,
    onDismiss: () => {
      dismissNotification(notification.id);
    },
    onAutoClose: () => {
      dismissNotification(notification.id);
    },
  };

  switch (notification.severity) {
    case 'error':
      toast.error(notification.title, options);
      return;
    case 'warning':
      toast.warning(notification.title, options);
      return;
    case 'success':
      toast.success(notification.title, options);
      return;
    case 'info':
      toast.info(notification.title, options);
      return;
  }
}

export function dismissSonnerNotification(notificationId: string): void {
  toast.dismiss(notificationId);
}
