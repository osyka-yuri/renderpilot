import { publishNotification } from './notification-center';
import type { NotificationSeverity } from './types';

export function publishSuccessNotification(title: string, description?: string): string {
  return publishTransientNotification('success', title, description);
}

export function publishInfoNotification(title: string, description?: string): string {
  return publishTransientNotification('info', title, description);
}

function publishTransientNotification(
  severity: Extract<NotificationSeverity, 'success' | 'info'>,
  title: string,
  description?: string,
): string {
  return publishNotification({
    severity,
    title,
    description,
  });
}
