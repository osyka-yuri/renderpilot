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

/** Ordinary error feedback keeps Sonner's polite, auto-dismissing default urgency. */
export function publishErrorNotification(title: string, description?: string): string {
  return publishNotification({ severity: 'error', title, description });
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
