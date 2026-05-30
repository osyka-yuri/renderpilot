import { publishNotification } from './notification-center';
import type { NotificationSeverity } from './types';

export function publishSuccessNotification(title: string, description?: string): string {
  return publishTransientNotification('success', title, description);
}

export function publishInfoNotification(title: string, description?: string): string {
  return publishTransientNotification('info', title, description);
}

export function publishWarningNotification(title: string, description?: string): string {
  return publishNotification({ severity: 'warning', title, description });
}

/**
 * Error toasts default to `important: true` so they are not auto-dismissed
 * — the user needs to see what action failed and why.
 */
export function publishErrorNotification(title: string, description?: string): string {
  return publishNotification({ severity: 'error', title, description, important: true });
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
